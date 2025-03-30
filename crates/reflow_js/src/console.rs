use boa_engine::{Context, JsValue, JsError, JsResult, NativeFunction};
use crate::runtime::{Extension, ExtensionError};
use crate::security::PermissionManager;
use async_trait::async_trait;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

/// Console output type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleOutputType {
    /// Standard output
    Stdout,
    
    /// Standard error
    Stderr,
}

/// Console module
#[derive(Debug)]
pub struct ConsoleModule {
    /// Output type for log, info, and debug
    pub log_output: ConsoleOutputType,
    
    /// Output type for warn and error
    pub error_output: ConsoleOutputType,
    
    /// Whether to include timestamps
    pub include_timestamps: bool,
    
    /// Custom stdout writer
    pub stdout: Option<Arc<Mutex<dyn Write + Send>>>,
    
    /// Custom stderr writer
    pub stderr: Option<Arc<Mutex<dyn Write + Send>>>,
}

impl ConsoleModule {
    /// Create a new console module
    pub fn new() -> Self {
        ConsoleModule {
            log_output: ConsoleOutputType::Stdout,
            error_output: ConsoleOutputType::Stderr,
            include_timestamps: false,
            stdout: None,
            stderr: None,
        }
    }
    
    /// Set the output type for log, info, and debug
    pub fn set_log_output(&mut self, output_type: ConsoleOutputType) {
        self.log_output = output_type;
    }
    
    /// Set the output type for warn and error
    pub fn set_error_output(&mut self, output_type: ConsoleOutputType) {
        self.error_output = output_type;
    }
    
    /// Set whether to include timestamps
    pub fn set_include_timestamps(&mut self, include_timestamps: bool) {
        self.include_timestamps = include_timestamps;
    }
    
    /// Set a custom stdout writer
    pub fn set_stdout(&mut self, stdout: Arc<Mutex<dyn Write + Send>>) {
        self.stdout = Some(stdout);
    }
    
    /// Set a custom stderr writer
    pub fn set_stderr(&mut self, stderr: Arc<Mutex<dyn Write + Send>>) {
        self.stderr = Some(stderr);
    }
    
    /// Write to stdout
    fn write_stdout(&self, message: &str) -> io::Result<()> {
        if let Some(stdout) = &self.stdout {
            let mut stdout = stdout.lock().unwrap();
            writeln!(stdout, "{}", message)?;
        } else {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            writeln!(handle, "{}", message)?;
        }
        
        Ok(())
    }
    
    /// Write to stderr
    fn write_stderr(&self, message: &str) -> io::Result<()> {
        if let Some(stderr) = &self.stderr {
            let mut stderr = stderr.lock().unwrap();
            writeln!(stderr, "{}", message)?;
        } else {
            let stderr = io::stderr();
            let mut handle = stderr.lock();
            writeln!(handle, "{}", message)?;
        }
        
        Ok(())
    }
    
    /// Format a message
    fn format_message(&self, args: &[JsValue], context: &mut Context) -> JsResult<String> {
        let mut result = String::new();
        
        if self.include_timestamps {
            let now = chrono::Local::now();
            result.push_str(&format!("[{}] ", now.format("%Y-%m-%d %H:%M:%S%.3f")));
        }
        
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                result.push(' ');
            }
            
            result.push_str(&arg.to_string(context)?);
        }
        
        Ok(result)
    }
    
    /// Log a message
    fn log(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let message = self.format_message(args, context)?;
        
        match self.log_output {
            ConsoleOutputType::Stdout => self.write_stdout(&message)
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stdout: {}", e).into()))?,
            ConsoleOutputType::Stderr => self.write_stderr(&message)
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stderr: {}", e).into()))?,
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Log an info message
    fn info(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let message = self.format_message(args, context)?;
        
        match self.log_output {
            ConsoleOutputType::Stdout => self.write_stdout(&format!("INFO: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stdout: {}", e).into()))?,
            ConsoleOutputType::Stderr => self.write_stderr(&format!("INFO: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stderr: {}", e).into()))?,
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Log a warning message
    fn warn(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let message = self.format_message(args, context)?;
        
        match self.error_output {
            ConsoleOutputType::Stdout => self.write_stdout(&format!("WARN: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stdout: {}", e).into()))?,
            ConsoleOutputType::Stderr => self.write_stderr(&format!("WARN: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stderr: {}", e).into()))?,
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Log an error message
    fn error(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let message = self.format_message(args, context)?;
        
        match self.error_output {
            ConsoleOutputType::Stdout => self.write_stdout(&format!("ERROR: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stdout: {}", e).into()))?,
            ConsoleOutputType::Stderr => self.write_stderr(&format!("ERROR: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stderr: {}", e).into()))?,
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Log a debug message
    fn debug(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let message = self.format_message(args, context)?;
        
        match self.log_output {
            ConsoleOutputType::Stdout => self.write_stdout(&format!("DEBUG: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stdout: {}", e).into()))?,
            ConsoleOutputType::Stderr => self.write_stderr(&format!("DEBUG: {}", message))
                .map_err(|e| JsError::from_opaque(format!("Failed to write to stderr: {}", e).into()))?,
        }
        
        Ok(JsValue::undefined())
    }
}

#[async_trait]
impl Extension for ConsoleModule {
    fn name(&self) -> &str {
        "console"
    }
    
    async fn initialize(&self, context: &mut Context) -> Result<(), ExtensionError> {
        // Create the console object
        let console = context.create_object();
        
        // Create the log function
        let log_fn = {
            let console_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                console_module.log(args, ctx)
            }
        };
        
        // Create the info function
        let info_fn = {
            let console_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                console_module.info(args, ctx)
            }
        };
        
        // Create the warn function
        let warn_fn = {
            let console_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                console_module.warn(args, ctx)
            }
        };
        
        // Create the error function
        let error_fn = {
            let console_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                console_module.error(args, ctx)
            }
        };
        
        // Create the debug function
        let debug_fn = {
            let console_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                console_module.debug(args, ctx)
            }
        };
        
        // Add the functions to the console object
        console.set("log", log_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        console.set("info", info_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        console.set("warn", warn_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        console.set("error", error_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        console.set("debug", debug_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        // Add the console object to the global object
        let global = context.global_object();
        global.set("console", console, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        Ok(())
    }
}

impl Clone for ConsoleModule {
    fn clone(&self) -> Self {
        ConsoleModule {
            log_output: self.log_output,
            error_output: self.error_output,
            include_timestamps: self.include_timestamps,
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
        }
    }
}

impl Default for ConsoleModule {
    fn default() -> Self {
        Self::new()
    }
}
