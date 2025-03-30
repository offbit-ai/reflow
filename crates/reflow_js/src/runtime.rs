use boa_engine::{Context, JsValue, JsError, JsResult, Source};
use std::sync::{Arc, Mutex};
use crate::async_runtime::AsyncRuntime;
use crate::console::ConsoleModule;
use crate::timers::TimersModule;
use crate::fs::FileSystemModule;
use crate::networking::NetworkingModule;
use crate::modules::ModuleSystem;
use crate::security::PermissionManager;

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Enable console module
    pub enable_console: bool,
    
    /// Enable timers module
    pub enable_timers: bool,
    
    /// Enable filesystem module
    pub enable_filesystem: bool,
    pub enable_vfs: bool,
    
    /// Enable network module
    pub enable_network: bool,
    
    /// Enable module system
    pub enable_modules: bool,
    
    /// Memory limit in bytes
    pub memory_limit: Option<usize>,
    
    /// Execution timeout in milliseconds
    pub execution_timeout: Option<u64>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            enable_console: true,
            enable_timers: true,
            enable_filesystem: false,
            enable_network: false,
            enable_modules: true,
            memory_limit: None,
            execution_timeout: None,
            enable_vfs: false,
        }
    }
}

/// JavaScript runtime
pub struct JsRuntime {
    /// Boa context
    context: Arc<Mutex<Context<'static>>>,
    
    /// Runtime configuration
    config: RuntimeConfig,
    
    /// Async runtime
    async_runtime: Arc<AsyncRuntime>,
    
    /// Permission manager
    permissions: PermissionManager,
}

impl JsRuntime {
    /// Create a new JavaScript runtime
    pub fn new(config: RuntimeConfig) -> Self {
        // Create a new Boa context
        let context = Context::default();
        
        // Create the async runtime
        let async_runtime = AsyncRuntime::new();
        
        // Create the permission manager
        let permissions = PermissionManager::new();
        
        // Create the runtime
        let mut runtime = JsRuntime {
            context: Arc::new(Mutex::new(context)),
            config,
            async_runtime: Arc::new(async_runtime),
            permissions,
        };
        
        // Initialize the runtime
        runtime.initialize();
        
        runtime
    }
    
    /// Initialize the runtime
    fn initialize(&mut self) {
        let mut context = self.context.lock().unwrap();
        
        // Register the async runtime
        self.async_runtime.register_with_context(&mut context).unwrap();
        
        // Register the console module
        if self.config.enable_console {
            let console = ConsoleModule::new();
            console.register(&mut context).unwrap();
        }
        
        // Register the timers module
        if self.config.enable_timers {
            let timers = TimersModule::new(self.async_runtime.clone());
            timers.register(&mut context).unwrap();
        }
        
        // Register the filesystem module
        if self.config.enable_filesystem {
            let fs = FileSystemModule::new(self.permissions.clone());
            fs.register(&mut context).unwrap();
        }
        
        // Register the networking module
        if self.config.enable_network {
            let networking = NetworkingModule::new(self.permissions.clone());
            networking.register(&mut context).unwrap();
        }
        
        // Register the module system
        if self.config.enable_modules {
            let modules = ModuleSystem::new(self.permissions.clone(), Arc::new(FileSystemModule::new(self.permissions.clone())), "/".into());
            modules.register(&mut context).unwrap();
        }
    }
    
    /// Evaluate JavaScript code
    pub fn eval(&self, code: &str) -> JsResult<JsValue> {
        let mut context = self.context.lock().unwrap();
        let source = Source::from_bytes(code);
        let result = context.eval(source)?;
        
        // Process any pending tasks
        self.async_runtime.process_tasks(&mut context)?;
        
        Ok(result)
    }
    
    /// Evaluate JavaScript code asynchronously
    pub async fn eval_async(&self, code: &str) -> JsResult<JsValue> {
        let result = self.eval(code)?;
        
        // Process any pending tasks
        let mut context = self.context.lock().unwrap();
        self.async_runtime.process_tasks(&mut context)?;
        
        Ok(result)
    }
    
    /// Get the Boa context
    pub fn context(&self) -> Arc<Mutex<Context<'static>>> {
        self.context.clone()
    }
    
    /// Get the async runtime
    pub fn async_runtime(&self) -> Arc<AsyncRuntime> {
        self.async_runtime.clone()
    }
    
    /// Get the permission manager
    pub fn permissions(&self) -> PermissionManager {
        self.permissions.clone()
    }
}
