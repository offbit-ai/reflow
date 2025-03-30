use boa_engine::{Context, JsValue, JsError, JsResult, NativeFunction};
use crate::runtime::{Extension, ExtensionError};
use crate::security::PermissionManager;
use crate::async_runtime::{AsyncRuntime, Task};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Timer module
#[derive(Debug)]
pub struct TimerModule {
    /// Async runtime
    async_runtime: Option<Arc<AsyncRuntime>>,
    
    /// Timeout IDs
    timeout_ids: Arc<Mutex<HashMap<u32, tokio::task::JoinHandle<()>>>>,
    
    /// Interval IDs
    interval_ids: Arc<Mutex<HashMap<u32, tokio::task::JoinHandle<()>>>>,
    
    /// Next timer ID
    next_id: Arc<Mutex<u32>>,
}

impl TimerModule {
    /// Create a new timer module
    pub fn new() -> Self {
        TimerModule {
            async_runtime: None,
            timeout_ids: Arc::new(Mutex::new(HashMap::new())),
            interval_ids: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }
    
    /// Set the async runtime
    pub fn set_async_runtime(&mut self, async_runtime: Arc<AsyncRuntime>) {
        self.async_runtime = Some(async_runtime);
    }
    
    /// Get the next timer ID
    fn next_id(&self) -> u32 {
        let mut id = self.next_id.lock().unwrap();
        let current = *id;
        *id = id.wrapping_add(1);
        current
    }
    
    /// Create a timeout
    fn set_timeout(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.len() < 2 {
            return Err(JsError::from_opaque("setTimeout requires at least 2 arguments".into()));
        }
        
        let callback = args[0].clone();
        if !callback.is_function() {
            return Err(JsError::from_opaque("setTimeout requires a function as the first argument".into()));
        }
        
        let delay_ms = args[1].to_number(context)?;
        let delay = Duration::from_millis(delay_ms as u64);
        
        // Get the arguments to pass to the callback
        let callback_args = if args.len() > 2 {
            args[2..].to_vec()
        } else {
            vec![]
        };
        
        // Get the async runtime
        let async_runtime = match &self.async_runtime {
            Some(runtime) => runtime.clone(),
            None => return Err(JsError::from_opaque("Async runtime not available".into())),
        };
        
        // Get the timer ID
        let id = self.next_id();
        
        // Create the timeout task
        let timeout_ids = self.timeout_ids.clone();
        let handle = tokio::spawn(async move {
            // Wait for the specified delay
            sleep(delay).await;
            
            // Create a task to execute the callback
            let task = Task::new(move |ctx| {
                // Call the callback
                let result = ctx.call(&callback, &JsValue::undefined(), &callback_args);
                
                // Remove the timeout ID
                let mut ids = timeout_ids.lock().unwrap();
                ids.remove(&id);
                
                result
            });
            
            // Schedule the task
            async_runtime.schedule_task(task);
        });
        
        // Store the timeout ID
        let mut ids = self.timeout_ids.lock().unwrap();
        ids.insert(id, handle);
        
        // Return the timeout ID
        Ok(JsValue::from(id))
    }
    
    /// Clear a timeout
    fn clear_timeout(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("clearTimeout requires a timeout ID".into()));
        }
        
        let id = args[0].to_number(context)? as u32;
        
        // Remove the timeout ID
        let mut ids = self.timeout_ids.lock().unwrap();
        if let Some(handle) = ids.remove(&id) {
            // Cancel the timeout
            handle.abort();
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Create an interval
    fn set_interval(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.len() < 2 {
            return Err(JsError::from_opaque("setInterval requires at least 2 arguments".into()));
        }
        
        let callback = args[0].clone();
        if !callback.is_function() {
            return Err(JsError::from_opaque("setInterval requires a function as the first argument".into()));
        }
        
        let delay_ms = args[1].to_number(context)?;
        let delay = Duration::from_millis(delay_ms as u64);
        
        // Get the arguments to pass to the callback
        let callback_args = if args.len() > 2 {
            args[2..].to_vec()
        } else {
            vec![]
        };
        
        // Get the async runtime
        let async_runtime = match &self.async_runtime {
            Some(runtime) => runtime.clone(),
            None => return Err(JsError::from_opaque("Async runtime not available".into())),
        };
        
        // Get the timer ID
        let id = self.next_id();
        
        // Create the interval task
        let handle = tokio::spawn(async move {
            loop {
                // Wait for the specified delay
                sleep(delay).await;
                
                // Create a task to execute the callback
                let callback_clone = callback.clone();
                let callback_args_clone = callback_args.clone();
                let task = Task::new(move |ctx| {
                    // Call the callback
                    ctx.call(&callback_clone, &JsValue::undefined(), &callback_args_clone)
                });
                
                // Schedule the task
                async_runtime.schedule_task(task);
            }
        });
        
        // Store the interval ID
        let mut ids = self.interval_ids.lock().unwrap();
        ids.insert(id, handle);
        
        // Return the interval ID
        Ok(JsValue::from(id))
    }
    
    /// Clear an interval
    fn clear_interval(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("clearInterval requires an interval ID".into()));
        }
        
        let id = args[0].to_number(context)? as u32;
        
        // Remove the interval ID
        let mut ids = self.interval_ids.lock().unwrap();
        if let Some(handle) = ids.remove(&id) {
            // Cancel the interval
            handle.abort();
        }
        
        Ok(JsValue::undefined())
    }
}

#[async_trait]
impl Extension for TimerModule {
    fn name(&self) -> &str {
        "timers"
    }
    
    async fn initialize(&self, context: &mut Context) -> Result<(), ExtensionError> {
        // Create the setTimeout function
        let set_timeout_fn = {
            let timer_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                timer_module.set_timeout(args, ctx)
            }
        };
        
        // Create the clearTimeout function
        let clear_timeout_fn = {
            let timer_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                timer_module.clear_timeout(args, ctx)
            }
        };
        
        // Create the setInterval function
        let set_interval_fn = {
            let timer_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                timer_module.set_interval(args, ctx)
            }
        };
        
        // Create the clearInterval function
        let clear_interval_fn = {
            let timer_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                timer_module.clear_interval(args, ctx)
            }
        };
        
        // Add the functions to the global object
        let global = context.global_object();
        
        global.set("setTimeout", set_timeout_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        global.set("clearTimeout", clear_timeout_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        global.set("setInterval", set_interval_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        global.set("clearInterval", clear_interval_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        Ok(())
    }
}

impl Clone for TimerModule {
    fn clone(&self) -> Self {
        TimerModule {
            async_runtime: self.async_runtime.clone(),
            timeout_ids: self.timeout_ids.clone(),
            interval_ids: self.interval_ids.clone(),
            next_id: self.next_id.clone(),
        }
    }
}

impl Default for TimerModule {
    fn default() -> Self {
        Self::new()
    }
}
