use crate::async_runtime::{AsyncRuntime, Task};
use crate::state::{GlobalState, TimerCallbackId, store_js_value, get_js_value, remove_js_value};
use boa_engine::object::FunctionObjectBuilder;
use boa_engine::{
    native_function::NativeFunction, object::ObjectInitializer, Context, JsError, JsResult, JsValue,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::{Duration, Instant};

/// Timer ID
type TimerId = u32;

/// Timer event
enum TimerEvent {
    /// Execute a timeout
    Timeout(TimerId),
    
    /// Execute an interval
    Interval(TimerId),
}

/// Timer
#[derive(Clone)]
struct Timer {
    /// Timer ID
    id: TimerId,

    /// Callback ID
    callback_id: u32,

    /// Interval
    interval: Option<Duration>,

    /// Next execution time
    next_execution: Instant,
}

// Global storage for timers module
static mut TIMERS_MODULE: Option<Arc<TimersModule>> = None;

/// Timers module
pub struct TimersModule {
    /// Async runtime
    async_runtime: Arc<AsyncRuntime>,

    /// Timers
    timers: Arc<Mutex<HashMap<TimerId, Timer>>>,

    /// Next timer ID
    next_timer_id: Arc<Mutex<TimerId>>,
    
    /// Next callback ID
    next_callback_id: Arc<Mutex<u32>>,
    
    /// Timer event sender
    event_sender: Sender<TimerEvent>,
    
    /// Timer event receiver
    event_receiver: Arc<Mutex<Receiver<TimerEvent>>>,
    
    /// Global state
    state: GlobalState,
}

impl TimersModule {
    /// Create a new timers module
    pub fn new(async_runtime: Arc<AsyncRuntime>) -> Self {
        // Create a channel for timer events
        let (sender, receiver) = channel();
        
        let module = TimersModule {
            async_runtime,
            timers: Arc::new(Mutex::new(HashMap::new())),
            next_timer_id: Arc::new(Mutex::new(1)),
            next_callback_id: Arc::new(Mutex::new(1)),
            event_sender: sender,
            event_receiver: Arc::new(Mutex::new(receiver)),
            state: GlobalState::new(),
        };
        
        // Store the timers module in the global storage
        unsafe {
            TIMERS_MODULE = Some(Arc::new(module.clone()));
        }
        
        // Start the timer event processor
        let module_clone = module.clone();
        thread::spawn(move || {
            module_clone.process_timer_events();
        });
        
        module
    }
    
    /// Process timer events
    fn process_timer_events(&self) {
        let receiver = self.event_receiver.lock().unwrap();
        
        while let Ok(event) = receiver.recv() {
            match event {
                TimerEvent::Timeout(timer_id) => {
                    // Schedule a task to execute the timer
                    self.schedule_timer_task(timer_id, true);
                }
                TimerEvent::Interval(timer_id) => {
                    // Schedule a task to execute the timer
                    self.schedule_timer_task(timer_id, false);
                }
            }
        }
    }
    
    /// Schedule a task to execute a timer
    fn schedule_timer_task(&self, timer_id: TimerId, is_timeout: bool) {
        // Get the timer
        let timer_opt = {
            let mut timers = self.timers.lock().unwrap();
            if is_timeout {
                timers.remove(&timer_id)
            } else {
                timers.get(&timer_id).cloned()
            }
        };
        
        if let Some(timer) = timer_opt {
            // Get the callback ID
            let callback_id = timer.callback_id;
            
            // Create a task that will execute the timer
            let task = Task::new(move |ctx| {
                // Get the callback from thread-local storage
                if let Some(callback) = get_js_value(callback_id) {
                    // Call the callback
                    let undefined = JsValue::undefined();
                    let args = [];
                    if let Some(callback_obj) = callback.as_object() {
                        callback_obj.call(&undefined, &args, ctx)
                    } else {
                        Ok(JsValue::undefined())
                    }
                } else {
                    Ok(JsValue::undefined())
                }
            });
            
            // Schedule the task
            self.async_runtime.schedule_task(task);
        }
    }
    
    /// Get the timers module from the global storage
    fn get_instance() -> Arc<TimersModule> {
        unsafe {
            TIMERS_MODULE.as_ref().unwrap().clone()
        }
    }

    /// Register the timers module with a JavaScript context
    pub fn register(&self, context: &mut Context) -> JsResult<()> {
        // Add the setTimeout function
        let global = context.global_object();
        global.set(
            "setTimeout",
            FunctionObjectBuilder::new(context, NativeFunction::from_fn_ptr(Self::set_timeout))
                .build(),
            true,
            context,
        )?;

        // Add the clearTimeout function
        global.set(
            "clearTimeout",
            FunctionObjectBuilder::new(context, NativeFunction::from_fn_ptr(Self::clear_timeout))
                .build(),
            true,
            context,
        )?;

        // Add the setInterval function
        global.set(
            "setInterval",
            FunctionObjectBuilder::new(context, NativeFunction::from_fn_ptr(Self::set_interval))
                .build(),
            true,
            context,
        )?;

        // Add the clearInterval function
        global.set(
            "clearInterval",
            FunctionObjectBuilder::new(context, NativeFunction::from_fn_ptr(Self::clear_interval))
                .build(),
            true,
            context,
        )?;

        Ok(())
    }

    /// Set timeout
    fn set_timeout(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque(
                "setTimeout requires at least one argument".into(),
            ));
        }

        let callback = args[0].clone();
        if !callback.is_callable() {
            return Err(JsError::from_opaque(
                "setTimeout callback must be a function".into(),
            ));
        }

        let delay = if args.len() > 1 {
            args[1].to_number(ctx)? as u64
        } else {
            0
        };

        // Get the timers module from the global storage
        let timers_module = Self::get_instance();

        // Create a new callback ID
        let callback_id = {
            let mut next_callback_id = timers_module.next_callback_id.lock().unwrap();
            let id = *next_callback_id;
            *next_callback_id = next_callback_id.wrapping_add(1);
            id
        };

        // Store the callback in thread-local storage
        store_js_value(callback_id, callback);

        // Store the callback ID in the state
        timers_module.state.set(TimerCallbackId(callback_id));

        // Create a new timer
        let timer_id = {
            let mut next_timer_id = timers_module.next_timer_id.lock().unwrap();
            let id = *next_timer_id;
            *next_timer_id = next_timer_id.wrapping_add(1);
            id
        };

        let timer = Timer {
            id: timer_id,
            callback_id,
            interval: None,
            next_execution: Instant::now() + Duration::from_millis(delay),
        };

        // Add the timer to the timers map
        {
            let mut timers = timers_module.timers.lock().unwrap();
            timers.insert(timer_id, timer);
        }

        // Schedule the timer
        let event_sender = timers_module.event_sender.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(delay));
            let _ = event_sender.send(TimerEvent::Timeout(timer_id));
        });

        Ok(JsValue::from(timer_id))
    }

    /// Clear timeout
    fn clear_timeout(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Ok(JsValue::undefined());
        }

        let timer_id = args[0].to_number(ctx)? as u32;

        // Get the timers module from the global storage
        let timers_module = Self::get_instance();

        // Remove the timer from the timers map
        let callback_id = {
            let mut timers = timers_module.timers.lock().unwrap();
            timers.remove(&timer_id).map(|timer| timer.callback_id)
        };

        // Remove the callback from thread-local storage
        if let Some(callback_id) = callback_id {
            remove_js_value(callback_id);
        }

        Ok(JsValue::undefined())
    }

    /// Set interval
    fn set_interval(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque(
                "setInterval requires at least one argument".into(),
            ));
        }

        let callback = args[0].clone();
        if !callback.is_callable() {
            return Err(JsError::from_opaque(
                "setInterval callback must be a function".into(),
            ));
        }

        let delay = if args.len() > 1 {
            args[1].to_number(ctx)? as u64
        } else {
            0
        };

        // Get the timers module from the global storage
        let timers_module = Self::get_instance();

        // Create a new callback ID
        let callback_id = {
            let mut next_callback_id = timers_module.next_callback_id.lock().unwrap();
            let id = *next_callback_id;
            *next_callback_id = next_callback_id.wrapping_add(1);
            id
        };

        // Store the callback in thread-local storage
        store_js_value(callback_id, callback);

        // Store the callback ID in the state
        timers_module.state.set(TimerCallbackId(callback_id));

        // Create a new timer
        let timer_id = {
            let mut next_timer_id = timers_module.next_timer_id.lock().unwrap();
            let id = *next_timer_id;
            *next_timer_id = next_timer_id.wrapping_add(1);
            id
        };

        let timer = Timer {
            id: timer_id,
            callback_id,
            interval: Some(Duration::from_millis(delay)),
            next_execution: Instant::now() + Duration::from_millis(delay),
        };

        // Add the timer to the timers map
        {
            let mut timers = timers_module.timers.lock().unwrap();
            timers.insert(timer_id, timer);
        }

        // Schedule the timer
        let event_sender = timers_module.event_sender.clone();
        let timers_clone = timers_module.timers.clone();
        thread::spawn(move || {
            let mut running = true;
            while running {
                thread::sleep(Duration::from_millis(delay));
                
                // Check if the timer still exists
                let timer_exists = {
                    let timers = timers_clone.lock().unwrap();
                    timers.contains_key(&timer_id)
                };
                
                if !timer_exists {
                    running = false;
                    continue;
                }
                
                // Send the timer event
                if event_sender.send(TimerEvent::Interval(timer_id)).is_err() {
                    running = false;
                }
            }
        });

        Ok(JsValue::from(timer_id))
    }

    /// Clear interval
    fn clear_interval(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Ok(JsValue::undefined());
        }

        let timer_id = args[0].to_number(ctx)? as u32;

        // Get the timers module from the global storage
        let timers_module = Self::get_instance();

        // Remove the timer from the timers map
        let callback_id = {
            let mut timers = timers_module.timers.lock().unwrap();
            timers.remove(&timer_id).map(|timer| timer.callback_id)
        };

        // Remove the callback from thread-local storage
        if let Some(callback_id) = callback_id {
            remove_js_value(callback_id);
        }

        Ok(JsValue::undefined())
    }
}

impl Clone for TimersModule {
    fn clone(&self) -> Self {
        TimersModule {
            async_runtime: self.async_runtime.clone(),
            timers: self.timers.clone(),
            next_timer_id: self.next_timer_id.clone(),
            next_callback_id: self.next_callback_id.clone(),
            event_sender: self.event_sender.clone(),
            event_receiver: self.event_receiver.clone(),
            state: self.state.clone(),
        }
    }
}
