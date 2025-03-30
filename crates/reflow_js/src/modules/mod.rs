use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use boa_engine::{Context, JsValue, JsError, JsResult, NativeFunction};
use crate::runtime::{Extension, ExtensionError};
use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

pub mod resolver;
pub mod commonjs;
pub mod esm;

pub use resolver::{ModuleResolver, ModuleResolutionError, NodeModuleResolver};
pub use commonjs::CommonJsModule;
pub use esm::EsModule;

/// Module type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
    /// CommonJS module
    CommonJs,
    
    /// ES module
    EsModule,
    
    /// Native module
    Native,
}

/// Module trait
pub trait Module: Send + Sync {
    /// Get the module type
    fn module_type(&self) -> ModuleType;
    
    /// Get the module path
    fn path(&self) -> &Path;
    
    /// Get the module exports
    fn exports(&self) -> JsResult<JsValue>;
    
    /// Evaluate the module
    fn evaluate(&self, context: &mut Context) -> JsResult<JsValue>;
}

/// Module cache
#[derive(Debug)]
pub struct ModuleCache {
    /// Cached modules
    modules: RwLock<HashMap<PathBuf, Arc<dyn Module>>>,
}

impl ModuleCache {
    /// Create a new module cache
    pub fn new() -> Self {
        ModuleCache {
            modules: RwLock::new(HashMap::new()),
        }
    }
    
    /// Get a module from the cache
    pub fn get(&self, path: &Path) -> Option<Arc<dyn Module>> {
        let modules = self.modules.read().unwrap();
        modules.get(path).cloned()
    }
    
    /// Add a module to the cache
    pub fn add(&self, module: Arc<dyn Module>) {
        let path = module.path().to_path_buf();
        let mut modules = self.modules.write().unwrap();
        modules.insert(path, module);
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        let mut modules = self.modules.write().unwrap();
        modules.clear();
    }
    
    /// Check if a module is in the cache
    pub fn contains(&self, path: &Path) -> bool {
        let modules = self.modules.read().unwrap();
        modules.contains_key(path)
    }
    
    /// Remove a module from the cache
    pub fn remove(&self, path: &Path) -> Option<Arc<dyn Module>> {
        let mut modules = self.modules.write().unwrap();
        modules.remove(path)
    }
}

impl Default for ModuleCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Module system
#[derive(Debug)]
pub struct ModuleSystem {
    /// Base directory for module resolution
    base_dir: PathBuf,
    
    /// Module cache
    cache: ModuleCache,
    
    /// Module resolvers
    resolvers: Vec<Box<dyn ModuleResolver>>,
}

impl ModuleSystem {
    /// Create a new module system
    pub fn new(base_dir: PathBuf) -> Self {
        let mut resolvers: Vec<Box<dyn ModuleResolver>> = Vec::new();
        resolvers.push(Box::new(NodeModuleResolver::new()));
        
        ModuleSystem {
            base_dir,
            cache: ModuleCache::new(),
            resolvers,
        }
    }
    
    /// Add a module resolver
    pub fn add_resolver(&mut self, resolver: Box<dyn ModuleResolver>) {
        self.resolvers.push(resolver);
    }
    
    /// Resolve a module specifier to a file path
    pub fn resolve(&self, specifier: &str, referrer: Option<&Path>) -> Result<PathBuf, ModuleResolutionError> {
        for resolver in &self.resolvers {
            if let Ok(path) = resolver.resolve(specifier, referrer, &self.base_dir) {
                return Ok(path);
            }
        }
        
        Err(ModuleResolutionError::ModuleNotFound(specifier.to_string()))
    }
    
    /// Load a module
    pub fn load(&self, path: &Path, source: &str) -> Result<Arc<dyn Module>, JsError> {
        // Check if the module is already loaded
        if let Some(module) = self.cache.get(path) {
            return Ok(module);
        }
        
        // Determine the module type based on the file extension
        let module: Arc<dyn Module> = if let Some(extension) = path.extension() {
            if extension == "mjs" {
                // ES module
                Arc::new(EsModule::new(path.to_path_buf(), source.to_string()))
            } else {
                // CommonJS module
                Arc::new(CommonJsModule::new(path.to_path_buf(), source.to_string()))
            }
        } else {
            // Default to CommonJS
            Arc::new(CommonJsModule::new(path.to_path_buf(), source.to_string()))
        };
        
        // Add the module to the cache
        self.cache.add(module.clone());
        
        Ok(module)
    }
    
    /// Require a module
    pub fn require(&self, specifier: &str, referrer: Option<&Path>, context: &mut Context) -> Result<JsValue, JsError> {
        // Resolve the module
        let path = self.resolve(specifier, referrer)
            .map_err(|e| JsError::from_opaque(e.to_string().into()))?;
        
        // Check if the module is already loaded
        if let Some(module) = self.cache.get(&path) {
            return module.exports();
        }
        
        // Load the module source
        let source = std::fs::read_to_string(&path)
            .map_err(|e| JsError::from_opaque(format!("Failed to read module: {}", e).into()))?;
        
        // Load the module
        let module = self.load(&path, &source)?;
        
        // Evaluate the module
        module.evaluate(context)
    }
    
    /// Import a module
    pub async fn import(&self, specifier: &str, referrer: Option<&Path>, context: &mut Context) -> Result<JsValue, JsError> {
        // Resolve the module
        let path = self.resolve(specifier, referrer)
            .map_err(|e| JsError::from_opaque(e.to_string().into()))?;
        
        // Check if the module is already loaded
        if let Some(module) = self.cache.get(&path) {
            return module.exports();
        }
        
        // Load the module source
        let source = std::fs::read_to_string(&path)
            .map_err(|e| JsError::from_opaque(format!("Failed to read module: {}", e).into()))?;
        
        // Load the module
        let module = self.load(&path, &source)?;
        
        // Evaluate the module
        module.evaluate(context)
    }
}

#[async_trait]
impl Extension for ModuleSystem {
    fn name(&self) -> &str {
        "module_system"
    }
    
    async fn initialize(&self, context: &mut Context) -> Result<(), ExtensionError> {
        // Create the require function
        let require_fn = {
            let module_system = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                if args.is_empty() {
                    return Err(JsError::from_opaque("require requires a module specifier".into()));
                }
                
                let specifier = args[0].to_string(ctx)?;
                
                module_system.require(&specifier, None, ctx)
            }
        };
        
        // Register the require function
        let global = context.global_object();
        global.set("require", require_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        Ok(())
    }
}

impl Clone for ModuleSystem {
    fn clone(&self) -> Self {
        ModuleSystem {
            base_dir: self.base_dir.clone(),
            cache: ModuleCache::new(),
            resolvers: self.resolvers.iter().map(|r| r.clone_box()).collect(),
        }
    }
}
