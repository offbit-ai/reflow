use boa_engine::{
    native_function::NativeFunction, object::ObjectInitializer, Context, JsError, JsResult, JsValue,
};
use futures::future::BoxFuture;
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll, Waker};
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
#[derive(Clone)]
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
    pub(crate) fn new(
        id: u32,
        runtime: Arc<AsyncRuntime>,
        value: oneshot::Receiver<JsResult<JsValue>>,
    ) -> Self {
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
pub enum PromiseState {
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

impl fmt::Debug for PromiseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PromiseState::Pending(resolve, reject) => f
                .debug_tuple("Pending")
                .field(&resolve.is_some())
                .field(&reject.is_some())
                .finish(),
            PromiseState::Fulfilled(value) => f.debug_tuple("Fulfilled").field(value).finish(),
            PromiseState::Rejected(reason) => f.debug_tuple("Rejected").field(reason).finish(),
        }
    }
}

// We need to implement Send and Sync for PromiseState manually
// since the compiler can't automatically derive them
unsafe impl Send for PromiseState {}
unsafe impl Sync for PromiseState {}

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

impl fmt::Debug for AsyncRuntime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AsyncRuntime")
            .field("task_queue_size", &self.task_queue.lock().unwrap().len())
            .field("promises_count", &self.promises.lock().unwrap().len())
            .field("next_promise_id", &self.next_promise_id.lock().unwrap())
            .field(
                "microtask_queue_size",
                &self.microtask_queue.lock().unwrap().len(),
            )
            .finish()
    }
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
        let promise = Promise::new(id, Arc::new(self.clone()), receiver);

        // Create the resolver
        let resolver = PromiseResolver::new(id, Arc::new(self.clone()));

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
                }
                _ => {
                    // Promise already resolved or rejected
                }
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
                }
                _ => {
                    // Promise already resolved or rejected
                }
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
        // Define the Promise constructor function
        fn promise_constructor(
            this: &JsValue,
            args: &[JsValue],
            ctx: &mut Context,
        ) -> JsResult<JsValue> {
            // Create a new promise object
            let promise_obj = ObjectInitializer::new(ctx).build();

            // If there's an executor function, call it with resolve and reject functions
            if !args.is_empty() && args[0].is_callable() {
                let executor = args[0].clone();

                // Define the resolve function
                fn resolve_fn(
                    this: &JsValue,
                    args: &[JsValue],
                    ctx: &mut Context,
                ) -> JsResult<JsValue> {
                    // For now, just return undefined
                    Ok(JsValue::undefined())
                }

                // Define the reject function
                fn reject_fn(
                    this: &JsValue,
                    args: &[JsValue],
                    ctx: &mut Context,
                ) -> JsResult<JsValue> {
                    // For now, just return undefined
                    Ok(JsValue::undefined())
                }

                // Create function objects for resolve and reject
                let resolve_obj = ObjectInitializer::new(ctx)
                    .function(NativeFunction::from_fn_ptr(resolve_fn), "resolve", 1)
                    .build();

                let reject_obj = ObjectInitializer::new(ctx)
                    .function(NativeFunction::from_fn_ptr(reject_fn), "reject", 1)
                    .build();

                // Call the executor function
                let args = [resolve_obj.into(), reject_obj.into()];
                let _ = executor
                    .as_object()
                    .unwrap()
                    .call(&JsValue::undefined(), &args, ctx);
            }

            Ok(promise_obj.into())
        }

        // Define Promise.resolve
        fn promise_resolve(
            this: &JsValue,
            args: &[JsValue],
            ctx: &mut Context,
        ) -> JsResult<JsValue> {
            // Create a new promise that is already resolved with the given value
            let promise_obj = ObjectInitializer::new(ctx).build();

            // Set the promise state to fulfilled
            promise_obj.set("__state", "fulfilled", true, ctx)?;

            // Set the promise value
            let value = if args.is_empty() {
                JsValue::undefined()
            } else {
                args[0].clone()
            };

            promise_obj.set("__value", value, true, ctx)?;

            Ok(promise_obj.into())
        }

        // Define Promise.reject
        fn promise_reject(
            this: &JsValue,
            args: &[JsValue],
            ctx: &mut Context,
        ) -> JsResult<JsValue> {
            // Create a new promise that is already rejected with the given reason
            let promise_obj = ObjectInitializer::new(ctx).build();

            // Set the promise state to rejected
            promise_obj.set("__state", "rejected", true, ctx)?;

            // Set the promise reason
            let reason = if args.is_empty() {
                JsValue::undefined()
            } else {
                args[0].clone()
            };

            promise_obj.set("__reason", reason, true, ctx)?;

            Ok(promise_obj.into())
        }

        // Create the Promise constructor object
        let promise_obj = ObjectInitializer::new(context)
            .function(
                NativeFunction::from_fn_ptr(promise_constructor),
                "Promise",
                1,
            )
            .function(NativeFunction::from_fn_ptr(promise_resolve), "resolve", 1)
            .function(NativeFunction::from_fn_ptr(promise_reject), "resolve", 1)
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
