use boa_engine::{Context, JsValue, JsError, JsResult, object::ObjectInitializer, native_function::NativeFunction};
use std::io::{self, Write};
use std::time::{Instant, Duration};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

// Global storage for timers
static mut TIMERS: Option<Arc<Mutex<HashMap<String, Instant>>>> = None;

/// Console module
pub struct ConsoleModule {
    /// Timers
    timers: Arc<Mutex<HashMap<String, Instant>>>,
}

impl ConsoleModule {
    /// Create a new console module
    pub fn new() -> Self {
        let timers = Arc::new(Mutex::new(HashMap::new()));
        
        // Store the timers in the global storage
        unsafe {
            TIMERS = Some(timers.clone());
        }
        
        ConsoleModule {
            timers,
        }
    }
    
    /// Register the console module with a JavaScript context
    pub fn register(&self, context: &mut Context) -> JsResult<()> {
        // Create the console object
        let console_obj = ObjectInitializer::new(context)
            .function(NativeFunction::from_fn_ptr(Self::log), "log", 1)
            .function(NativeFunction::from_fn_ptr(Self::warn), "warn", 1)
            .function(NativeFunction::from_fn_ptr(Self::info), "info", 1)
            .function(NativeFunction::from_fn_ptr(Self::error), "error", 1)
            .function(NativeFunction::from_fn_ptr(Self::debug), "debug", 1)
            .function(NativeFunction::from_fn_ptr(Self::time), "time", 1)
            .function(NativeFunction::from_fn_ptr(Self::time_end), "timeEnd", 1)
            .build();
        
        // Add the console object to the global object
        let global = context.global_object();
        global.set("console", console_obj, true, context)?;
        
        Ok(())
    }
    
    /// Log method
    fn log(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        Self::print_args("LOG", args, ctx, &mut io::stdout())?;
        Ok(JsValue::undefined())
    }
    
    /// Info method
    fn info(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        Self::print_args("INFO", args, ctx, &mut io::stdout())?;
        Ok(JsValue::undefined())
    }
    
    /// Warn method
    fn warn(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        Self::print_args("WARN", args, ctx, &mut io::stderr())?;
        Ok(JsValue::undefined())
    }
    
    /// Error method
    fn error(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        Self::print_args("ERROR", args, ctx, &mut io::stderr())?;
        Ok(JsValue::undefined())
    }
    
    /// Debug method
    fn debug(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        Self::print_args("DEBUG", args, ctx, &mut io::stdout())?;
        Ok(JsValue::undefined())
    }
    
    /// Time method
    fn time(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let label = if args.is_empty() {
            "default".to_string()
        } else {
            args[0].to_string(ctx)?.to_std_string_escaped()
        };
        
        // Get the timers from the global storage
        let timers = unsafe {
            TIMERS.as_ref().unwrap().clone()
        };
        
        // Start the timer
        let mut timers = timers.lock().unwrap();
        timers.insert(label.clone(), Instant::now());
        
        println!("Timer '{}' started", label);
        
        Ok(JsValue::undefined())
    }
    
    /// TimeEnd method
    fn time_end(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
        let label = if args.is_empty() {
            "default".to_string()
        } else {
            args[0].to_string(ctx)?.to_std_string_escaped()
        };
        
        // Get the timers from the global storage
        let timers = unsafe {
            TIMERS.as_ref().unwrap().clone()
        };
        
        // End the timer
        let mut timers = timers.lock().unwrap();
        if let Some(start) = timers.remove(&label) {
            let elapsed = start.elapsed();
            println!("Timer '{}' ended: {:?}", label, elapsed);
        } else {
            println!("Timer '{}' does not exist", label);
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Print arguments
    fn print_args(
        level: &str,
        args: &[JsValue],
        ctx: &mut Context,
        output: &mut dyn Write,
    ) -> JsResult<()> {
        write!(output, "[{}]", level).map_err(|e| JsError::from_opaque(format!("Failed to write to output: {}", e).into()))?;
        
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                write!(output, " ").map_err(|e| JsError::from_opaque(format!("Failed to write to output: {}", e).into()))?;
            }
            
            let str_value = arg.to_string(ctx)?.to_std_string_escaped();
            write!(output, " {}", str_value).map_err(|e| JsError::from_opaque(format!("Failed to write to output: {}", e).into()))?;
        }
        
        writeln!(output).map_err(|e| JsError::from_opaque(format!("Failed to write to output: {}", e).into()))?;
        output.flush().map_err(|e| JsError::from_opaque(format!("Failed to flush output: {}", e).into()))?;
        
        Ok(())
    }
}

impl Default for ConsoleModule {
    fn default() -> Self {
        Self::new()
    }
}
