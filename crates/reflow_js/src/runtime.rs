use boa_engine::{Context, JsValue, JsError, JsResult, Source, Realm};
use crate::security::PermissionManager;
use crate::async_runtime::AsyncRuntime;
use crate::console::ConsoleModule;
use crate::timers::TimerModule;
use crate::fs::{FileSystemModule, FileSystemPermissions};
use crate::networking::{FetchModule, WebSocketModule, NetworkConfig};
use crate::modules::ModuleSystem;
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::runtime::Runtime as TokioRuntime;

/// Extension trait
#[async_trait]
pub trait Extension: Send + Sync {
    /// Get the extension name
    fn name(&self) -> &str;
    
    /// Initialize the extension
    async fn initialize(&self, context: &mut Context) -> Result<(), ExtensionError>;
}

/// Extension error
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    /// Extension initialization failed
    #[error("Extension initialization failed: {0}")]
    InitializationFailed(String),
    
    /// Extension already loaded
    #[error("Extension already loaded: {0}")]
    AlreadyLoaded(String),
    
    /// Extension not found
    #[error("Extension not found: {0}")]
    NotFound(String),
    
    /// Other error
    #[error("Extension error: {0}")]
    Other(String),
}

/// Runtime configuration
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    /// Enable network
    pub enable_network: bool,
    
    /// Enable filesystem
    pub enable_filesystem: bool,
    
    /// Enable timers
    pub enable_timers: bool,
    
    /// Enable console
    pub enable_console: bool,
    
    /// Enable modules
    pub enable_modules: bool,
    
    /// Memory limit
    pub memory_limit: Option<usize>,
    
    /// Execution timeout
    pub execution_timeout: Option<Duration>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        RuntimeConfig {
            enable_network: false,
            enable_filesystem: false,
            enable_timers: true,
            enable_console: true,
            enable_modules: false,
            memory_limit: None,
            execution_timeout: None,
        }
    }
}

/// JavaScript runtime
pub struct JsRuntime {
    /// Boa context
    context: Arc<Mutex<Context>>,
    
    /// Runtime configuration
    config: RuntimeConfig,
    
    /// Extensions
    extensions: Arc<Mutex<Vec<Box<dyn Extension>>>>,
    
    /// Permission manager
    permissions: Arc<PermissionManager>,
    
    /// Async runtime
    async_runtime: Arc<AsyncRuntime>,
    
    /// Tokio runtime
    tokio_runtime: Arc<TokioRuntime>,
}

impl JsRuntime {
    /// Create a new JavaScript runtime
    pub fn new(config: RuntimeConfig) -> Self {
        // Create the Boa context
        let context = Context::default();
        
        // Create the permission manager
        let permissions = Arc::new(PermissionManager::new());
        
        // Create the async runtime
        let async_runtime = Arc::new(AsyncRuntime::new());
        
        // Create the Tokio runtime
        let tokio_runtime = Arc::new(
            TokioRuntime::new().expect("Failed to create Tokio runtime")
        );
        
        let mut runtime = JsRuntime {
            context: Arc::new(Mutex::new(context)),
            config,
            extensions: Arc::new(Mutex::new(Vec::new())),
            permissions,
            async_runtime,
            tokio_runtime,
        };
        
        // Initialize the runtime
        runtime.initialize();
        
        runtime
    }
    
    /// Initialize the runtime
    fn initialize(&mut self) {
        // Register the async runtime with the context
        let context = &mut self.context.lock().unwrap();
        self.async_runtime.register_with_context(context).expect("Failed to register async runtime");
        
        // Load the default extensions
        if self.config.enable_console {
            let console = ConsoleModule::new();
            self.tokio_runtime.block_on(async {
                self.load_extension(Box::new(console)).await.expect("Failed to load console module");
            });
        }
        
        if self.config.enable_timers {
            let mut timers = TimerModule::new();
            timers.set_async_runtime(self.async_runtime.clone());
            self.tokio_runtime.block_on(async {
                self.load_extension(Box::new(timers)).await.expect("Failed to load timers module");
            });
        }
        
        if self.config.enable_filesystem {
            let fs_permissions = FileSystemPermissions::default();
            let mut fs = FileSystemModule::new(fs_permissions, false);
            fs.set_async_runtime(self.async_runtime.clone());
            self.tokio_runtime.block_on(async {
                self.load_extension(Box::new(fs)).await.expect("Failed to load filesystem module");
            });
        }
        
        if self.config.enable_network {
            let network_config = NetworkConfig::default();
            
            let mut fetch = FetchModule::new(network_config.clone());
            fetch.set_async_runtime(self.async_runtime.clone());
            self.tokio_runtime.block_on(async {
                self.load_extension(Box::new(fetch)).await.expect("Failed to load fetch module");
            });
            
            let mut websocket = WebSocketModule::new(network_config);
            websocket.set_async_runtime(self.async_runtime.clone());
            self.tokio_runtime.block_on(async {
                self.load_extension(Box::new(websocket)).await.expect("Failed to load websocket module");
            });
        }
        
        if self.config.enable_modules {
            let module_system = ModuleSystem::new(PathBuf::from("."));
            self.tokio_runtime.block_on(async {
                self.load_extension(Box::new(module_system)).await.expect("Failed to load module system");
            });
        }
    }
    
    /// Load an extension
    pub async fn load_extension(&mut self, extension: Box<dyn Extension>) -> Result<(), ExtensionError> {
        // Check if the extension is already loaded
        let extensions = self.extensions.lock().unwrap();
        for ext in extensions.iter() {
            if ext.name() == extension.name() {
                return Err(ExtensionError::AlreadyLoaded(extension.name().to_string()));
            }
        }
        drop(extensions);
        
        // Initialize the extension
        let context = &mut self.context.lock().unwrap();
        extension.initialize(context).await?;
        
        // Add the extension to the list
        let mut extensions = self.extensions.lock().unwrap();
        extensions.push(extension);
        
        Ok(())
    }
    
    /// Evaluate JavaScript code
    pub fn eval(&self, code: &str) -> JsResult<JsValue> {
        let context = &mut self.context.lock().unwrap();
        let result = context.eval(Source::from_bytes(code));
        
        // Process the async runtime tasks
        self.async_runtime.process_tasks(context)?;
        
        result
    }
    
    /// Evaluate JavaScript code asynchronously
    pub async fn eval_async(&self, code: &str) -> JsResult<JsValue> {
        let result = self.eval(code)?;
        
        // Wait for a short time to allow async tasks to complete
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Process the async runtime tasks
        let context = &mut self.context.lock().unwrap();
        self.async_runtime.process_tasks(context)?;
        
        Ok(result)
    }
    
    /// Register a global function
    pub fn register_global_function<F>(&mut self, name: &str, function: F) -> JsResult<()>
    where
        F: Fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue> + Send + Sync + 'static,
    {
        let context = &mut self.context.lock().unwrap();
        let global = context.global_object();
        
        global.set(name, function, true, context)?;
        
        Ok(())
    }
    
    /// Get the permission manager
    pub fn permissions(&self) -> &PermissionManager {
        &self.permissions
    }
    
    /// Get a mutable reference to the permission manager
    pub fn permissions_mut(&mut self) -> &mut PermissionManager {
        Arc::get_mut(&mut self.permissions).expect("Failed to get mutable reference to permission manager")
    }
    
    /// Get the async runtime
    pub fn async_runtime(&self) -> &AsyncRuntime {
        &self.async_runtime
    }
    
    /// Get the Tokio runtime
    pub fn tokio_runtime(&self) -> &TokioRuntime {
        &self.tokio_runtime
    }
}
