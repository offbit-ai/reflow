pub mod runtime;
pub mod security;
pub mod async_runtime;
pub mod console;
pub mod timers;
pub mod fs;
pub mod networking;
pub mod modules;

// Re-export the main components
pub use runtime::{JsRuntime, RuntimeConfig, Extension, ExtensionError};
pub use security::{PermissionManager, Permission, PermissionScope, PermissionError, PermissionResult, FileSystemPermissionScope, NetworkPermissionScope};
pub use async_runtime::{AsyncRuntime, Task, Promise, PromiseResolver};
pub use console::{ConsoleModule, ConsoleOutputType};
pub use timers::TimerModule;
pub use fs::{FileSystemModule, FileSystemPermissions, VirtualFileSystem};
pub use networking::{FetchModule, WebSocketModule, NetworkConfig};
pub use modules::{ModuleSystem, Module, ModuleType, ModuleCache};

/// Version of the library
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Create a new JavaScript runtime with default configuration
pub fn create_runtime() -> JsRuntime {
    JsRuntime::new(RuntimeConfig::default())
}

/// Create a new JavaScript runtime with custom configuration
pub fn create_runtime_with_config(config: RuntimeConfig) -> JsRuntime {
    JsRuntime::new(config)
}

/// Evaluate JavaScript code
pub fn eval(code: &str) -> Result<String, String> {
    let runtime = create_runtime();
    
    match runtime.eval(code) {
        Ok(value) => Ok(value.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

/// Evaluate JavaScript code with custom configuration
pub fn eval_with_config(code: &str, config: RuntimeConfig) -> Result<String, String> {
    let runtime = create_runtime_with_config(config);
    
    match runtime.eval(code) {
        Ok(value) => Ok(value.to_string()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_eval_simple() {
        let result = eval("1 + 2").unwrap();
        assert_eq!(result, "3");
    }
    
    #[test]
    fn test_eval_object() {
        let result = eval("({a: 1, b: 2})").unwrap();
        assert_eq!(result, "[object Object]");
    }
    
    #[test]
    fn test_eval_array() {
        let result = eval("[1, 2, 3]").unwrap();
        assert_eq!(result, "1,2,3");
    }
    
    #[test]
    fn test_eval_function() {
        let result = eval("(function() { return 42; })()").unwrap();
        assert_eq!(result, "42");
    }
    
    #[test]
    fn test_eval_error() {
        let result = eval("throw new Error('test')");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_runtime_config() {
        let config = RuntimeConfig {
            enable_console: true,
            enable_timers: true,
            ..Default::default()
        };
        
        let runtime = create_runtime_with_config(config);
        let result = runtime.eval("console.log('test'); 42").unwrap();
        assert_eq!(result.to_string(), "42");
    }
}
