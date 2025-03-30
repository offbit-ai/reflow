use boa_engine::{Context, JsValue, JsError, JsResult, object::ObjectInitializer};
use std::sync::{Arc, Mutex};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::task::{Poll, Context as TaskContext, Waker};
use futures::future::BoxFuture;
use tokio::sync::oneshot;

/// Task type for the async runtime
pub type TaskFn = Box<dyn FnOnce(&mut Context) -> JsResult<JsValue> + Send>;

/// Task for the async runtime
pub struct Task {
    /// Task function
    function: TaskFn,
}

impl Task {
    /// Create a new task
    pub fn new<F>(function: F) -> Self
    where
        F: FnOnce(&mut Context) -> JsResult<JsValue> + Send + 'static,
    {
        Task {
            function: Box::new(function),
        }
    }
    
    /// Execute the task
    pub fn execute(self, context: &mut Context) -> JsResult<JsValue> {
        (self.function)(context)
    }
}

/// Promise resolver
pub struct PromiseResolver {
    /// Promise ID
    id: u32,
    
    /// Async runtime
    runtime: Arc<AsyncRuntime>,
}

impl PromiseResolver {
    /// Create a new promise resolver
    pub(crate) fn new(id: u32, runtime: Arc<AsyncRuntime>) -> Self {
        PromiseResolver { id, runtime }
    }
    
    /// Resolve the promise
    pub fn resolve<F>(&self, value_fn: F)
    where
        F: FnOnce(JsValue) -> JsResult<JsValue> + Send + 'static,
    {
        self.runtime.resolve_promise(self.id, value_fn);
    }
    
    /// Reject the promise
    pub fn reject<F>(&self, reason_fn: F)
    where
        F: FnOnce(JsValue) -> JsResult<JsValue> + Send + 'static,
    {
        self.runtime.reject_promise(self.id, reason_fn);
    }
}

/// Promise
pub struct Promise {
    /// Promise ID
    id: u32,
    
    /// Async runtime
    runtime: Arc<AsyncRuntime>,
    
    /// Promise value
    value: oneshot::Receiver<JsResult<JsValue>>,
}

impl Promise {
    /// Create a new promise
    pub(crate) fn new(id: u32, runtime: Arc<AsyncRuntime>, value: oneshot::Receiver<JsResult<JsValue>>) -> Self {
        Promise { id, runtime, value }
    }
    
    /// Get the promise ID
    pub fn id(&self) -> u32 {
        self.id
    }
    
    /// Wait for the promise to resolve
    pub async fn await_value(self) -> JsResult<JsValue> {
        match self.value.await {
            Ok(result) => result,
            Err(_) => Err(JsError::from_opaque("Promise was cancelled".into())),
        }
    }
}

/// Promise state
enum PromiseState {
    /// Pending
    Pending(
        Option<Box<dyn FnOnce(JsValue) -> JsResult<JsValue> + Send>>,
        Option<Box<dyn FnOnce(JsValue) -> JsResult<JsValue> + Send>>,
    ),
    
    /// Fulfilled
    Fulfilled(JsValue),
    
    /// Rejected
    Rejected(JsValue),
}

/// Async runtime
pub struct AsyncRuntime {
    /// Task queue
    task_queue: Arc<Mutex<VecDeque<Task>>>,
    
    /// Promise registry
    promises: Arc<Mutex<HashMap<u32, PromiseState>>>,
    
    /// Next promise ID
    next_promise_id: Arc<Mutex<u32>>,
    
    /// Microtask queue
    microtask_queue: Arc<Mutex<VecDeque<Box<dyn FnOnce(JsValue) -> JsResult<JsValue> + Send>>>>,
}

impl AsyncRuntime {
    /// Create a new async runtime
    pub fn new() -> Self {
        AsyncRuntime {
            task_queue: Arc::new(Mutex::new(VecDeque::new())),
            promises: Arc::new(Mutex::new(HashMap::new())),
            next_promise_id: Arc::new(Mutex::new(1)),
            microtask_queue: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
    
    /// Schedule a task
    pub fn schedule_task(&self, task: Task) {
        let mut queue = self.task_queue.lock().unwrap();
        queue.push_back(task);
    }
    
    /// Create a promise
    pub fn create_promise(&self) -> (Promise, PromiseResolver) {
        // Get the next promise ID
        let id = {
            let mut next_id = self.next_promise_id.lock().unwrap();
            let id = *next_id;
            *next_id = next_id.wrapping_add(1);
            id
        };
        
        // Create the promise state
        let (sender, receiver) = oneshot::channel();
        
        // Create the promise
        let promise = Promise::new(id, self.clone(), receiver);
        
        // Create the resolver
        let resolver = PromiseResolver::new(id, self.clone());
        
        // Register the promise
        let mut promises = self.promises.lock().unwrap();
        promises.insert(id, PromiseState::Pending(None, None));
        
        (promise, resolver)
    }
    
    /// Resolve a promise
    pub fn resolve_promise<F>(&self, id: u32, value_fn: F)
    where
        F: FnOnce(JsValue) -> JsResult<JsValue> + Send + 'static,
    {
        let mut promises = self.promises.lock().unwrap();
        
        if let Some(state) = promises.get_mut(&id) {
            match state {
                PromiseState::Pending(resolve_fn, _) => {
                    // Replace the resolve function
                    *resolve_fn = Some(Box::new(value_fn));
                },
                _ => {
                    // Promise already resolved or rejected
                },
            }
        }
    }
    
    /// Reject a promise
    pub fn reject_promise<F>(&self, id: u32, reason_fn: F)
    where
        F: FnOnce(JsValue) -> JsResult<JsValue> + Send + 'static,
    {
        let mut promises = self.promises.lock().unwrap();
        
        if let Some(state) = promises.get_mut(&id) {
            match state {
                PromiseState::Pending(_, reject_fn) => {
                    // Replace the reject function
                    *reject_fn = Some(Box::new(reason_fn));
                },
                _ => {
                    // Promise already resolved or rejected
                },
            }
        }
    }
    
    /// Process the task queue
    pub fn process_tasks(&self, context: &mut Context) -> JsResult<()> {
        // Process the task queue
        loop {
            // Get the next task
            let task = {
                let mut queue = self.task_queue.lock().unwrap();
                queue.pop_front()
            };
            
            // Execute the task
            if let Some(task) = task {
                task.execute(context)?;
            } else {
                break;
            }
        }
        
        // Process the microtask queue
        loop {
            // Get the next microtask
            let microtask = {
                let mut queue = self.microtask_queue.lock().unwrap();
                queue.pop_front()
            };
            
            // Execute the microtask
            if let Some(microtask) = microtask {
                microtask(JsValue::undefined())?;
            } else {
                break;
            }
        }
        
        Ok(())
    }
    
    /// Register the async runtime with a JavaScript context
    pub fn register_with_context(&self, context: &mut Context) -> JsResult<()> {
        // Create the Promise constructor
        let promise_constructor = {
            let async_runtime = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                // Create a new promise
                let (promise, resolver) = async_runtime.create_promise();
                
                // Create the promise object
                let promise_obj = ctx.create_object();
                
                // Store the promise ID
                promise_obj.set("__id", promise.id(), true, ctx)?;
                
                // Create the executor function
                if !args.is_empty() && args[0].is_function() {
                    let executor = args[0].clone();
                    
                    // Create the resolve function
                    let resolve_fn = {
                        let resolver = resolver.clone();
                        move |_this: &JsValue, args: &[JsValue], _ctx: &mut Context| {
                            let value = if args.is_empty() {
                                JsValue::undefined()
                            } else {
                                args[0].clone()
                            };
                            
                            resolver.resolve(move |_| Ok(value));
                            
                            Ok(JsValue::undefined())
                        }
                    };
                    
                    // Create the reject function
                    let reject_fn = {
                        let resolver = resolver;
                        move |_this: &JsValue, args: &[JsValue], _ctx: &mut Context| {
                            let reason = if args.is_empty() {
                                JsValue::undefined()
                            } else {
                                args[0].clone()
                            };
                            
                            resolver.reject(move |_| Ok(reason));
                            
                            Ok(JsValue::undefined())
                        }
                    };
                    
                    // Call the executor function
                    let resolve_obj = ObjectInitializer::new(ctx)
                        .function(resolve_fn)
                        .build();
                    
                    let reject_obj = ObjectInitializer::new(ctx)
                        .function(reject_fn)
                        .build();
                    
                    let args = [resolve_obj.into(), reject_obj.into()];
                    ctx.call(&executor, &JsValue::undefined(), &args)?;
                }
                
                Ok(promise_obj.into())
            }
        };
        
        // Create the Promise constructor object
        let promise_obj = ObjectInitializer::new(context)
            .constructor(promise_constructor)
            .build();
        
        // Add the Promise constructor to the global object
        let global = context.global_object();
        global.set("Promise", promise_obj, true, context)?;
        
        Ok(())
    }
}

impl Clone for AsyncRuntime {
    fn clone(&self) -> Self {
        AsyncRuntime {
            task_queue: self.task_queue.clone(),
            promises: self.promises.clone(),
            next_promise_id: self.next_promise_id.clone(),
            microtask_queue: self.microtask_queue.clone(),
        }
    }
}

impl Default for AsyncRuntime {
    fn default() -> Self {
        Self::new()
    }
}
