use crate::fs::FileSystemModule;
use crate::security::PermissionManager;
use boa_engine::object::FunctionObjectBuilder;
use boa_engine::property::Attribute;
use boa_engine::{
    native_function::NativeFunction, object::ObjectInitializer, Context, JsError, JsResult,
    JsValue, Source,
};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

pub mod commonjs;
pub mod esm;
pub mod resolver;

use resolver::{ModuleResolutionError, ModuleResolver, NodeModuleResolver};

/// Module type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
    /// CommonJS module
    CommonJs,

    /// ES module
    EsModule,
}

/// Module trait
pub trait Module {
    /// Get the module type
    fn module_type(&self) -> ModuleType;

    /// Get the module path
    fn path(&self) -> &Path;

    /// Evaluate the module
    fn evaluate(&self, context: &mut Context) -> JsResult<JsValue>;

    /// Get the exports object
    fn exports(&self) -> JsResult<JsValue>;
}

/// Module cache
pub struct ModuleCache {
    /// Cached modules
    modules: Mutex<HashMap<PathBuf, Box<dyn Module>>>,
}

impl fmt::Debug for ModuleCache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ModuleCache")
            .field("modules_count", &self.modules.lock().unwrap().len())
            .finish()
    }
}

impl ModuleCache {
    /// Create a new module cache
    pub fn new() -> Self {
        ModuleCache {
            modules: Mutex::new(HashMap::new()),
        }
    }

    /// Get a module from the cache
    pub fn get(&self, path: &Path) -> Option<Box<dyn Module>> {
        let modules = self.modules.lock().unwrap();
        modules.get(path).map(|module| {
            // Clone the module
            let module_type = module.module_type();
            let path = module.path().to_path_buf();

            match module_type {
                ModuleType::CommonJs => {
                    // Read the source code
                    let source = fs::read_to_string(&path).unwrap_or_default();
                    Box::new(commonjs::CommonJsModule::new(path, source)) as Box<dyn Module>
                }
                ModuleType::EsModule => {
                    // Read the source code
                    let source = fs::read_to_string(&path).unwrap_or_default();
                    Box::new(esm::EsModule::new(path, source)) as Box<dyn Module>
                }
            }
        })
    }

    /// Add a module to the cache
    pub fn add(&self, module: Box<dyn Module>) {
        let mut modules = self.modules.lock().unwrap();
        modules.insert(module.path().to_path_buf(), module);
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut modules = self.modules.lock().unwrap();
        modules.clear();
    }
}

impl Default for ModuleCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Module system
pub struct ModuleSystem {
    /// Permission manager
    permissions: PermissionManager,

    /// File system module
    fs: Arc<FileSystemModule>,

    /// Module cache
    cache: ModuleCache,

    /// Module resolvers
    resolvers: Arc<Mutex<Vec<Box<dyn ModuleResolver>>>>,

    /// Base directory
    base_dir: PathBuf,
}

// Global storage for module system data
static mut MODULE_CACHE: Option<Arc<ModuleCache>> = None;
static mut MODULE_RESOLVERS: Option<Arc<Mutex<Vec<Box<dyn ModuleResolver>>>>> = None;
static mut BASE_DIR: Option<Arc<PathBuf>> = None;
static mut FS_MODULE: Option<Arc<FileSystemModule>> = None;

// Function pointers for module system operations
fn require_fn(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "require requires a module specifier".into(),
        ));
    }

    // Get the module specifier
    let specifier = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the current module path
    let current_module_path: Option<PathBuf> = None;

    // Get the module system data from the global storage
    let cache = unsafe { MODULE_CACHE.as_ref().unwrap().clone() };
    let resolvers = unsafe { MODULE_RESOLVERS.as_ref().unwrap().clone() };
    let base_dir = unsafe { BASE_DIR.as_ref().unwrap().clone() };
    let fs = unsafe { FS_MODULE.as_ref().unwrap().clone() };

    // Resolve the module
    let mut resolved_path = None;
    let mut resolution_error = None;

    if let Ok(resolver) = resolvers.lock() {
        for resolver in resolver.iter() {
            match resolver.resolve(&specifier, current_module_path.as_deref(), &base_dir) {
                Ok(path) => {
                    resolved_path = Some(path);
                    break;
                }
                Err(err) => {
                    resolution_error = Some(err);
                }
            }
        }
    }

    let module_path = match resolved_path {
        Some(path) => path,
        None => {
            return Err(JsError::from_opaque(
                format!(
                    "Cannot find module '{}'{}",
                    specifier,
                    resolution_error
                        .map(|e| format!(": {}", e))
                        .unwrap_or_default()
                )
                .into(),
            ));
        }
    };

    // Check if the module is already cached
    if let Some(module) = cache.get(&module_path) {
        return module.evaluate(ctx);
    }

    // Read the module source
    let source = match fs::read_to_string(&module_path) {
        Ok(source) => source,
        Err(err) => {
            return Err(JsError::from_opaque(
                format!("Error reading module '{}': {}", module_path.display(), err).into(),
            ));
        }
    };

    // Create the module
    let module = Box::new(commonjs::CommonJsModule::new(module_path.clone(), source));

    // Add the module to the cache
    cache.add(module);

    // Get the module from the cache
    let module = cache.get(&module_path).unwrap();

    // Evaluate the module
    let result = module.evaluate(ctx);

    result
}

fn import_fn(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "import requires a module specifier".into(),
        ));
    }

    // Get the module specifier
    let specifier = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the current module path
    let current_module_path: Option<PathBuf> = None;

    // Get the module system data from the global storage
    let cache = unsafe { MODULE_CACHE.as_ref().unwrap().clone() };
    let resolvers = unsafe { MODULE_RESOLVERS.as_ref().unwrap().clone() };
    let base_dir = unsafe { BASE_DIR.as_ref().unwrap().clone() };
    let fs = unsafe { FS_MODULE.as_ref().unwrap().clone() };

    // Create a promise object
    let promise_obj = ObjectInitializer::new(ctx).build();

    // Resolve the module
    let mut resolved_path = None;
    let mut resolution_error = None;

    if let Ok(resolver) = resolvers.lock() {
        for resolver in resolver.iter() {
            match resolver.resolve(&specifier, current_module_path.as_deref(), &base_dir) {
                Ok(path) => {
                    resolved_path = Some(path);
                    break;
                }
                Err(err) => {
                    resolution_error = Some(err);
                }
            }
        }
    }

    let module_path = match resolved_path {
        Some(path) => path,
        None => {
            let error = JsError::from_opaque(
                format!(
                    "Cannot find module '{}'{}",
                    specifier,
                    resolution_error
                        .map(|e| format!(": {}", e))
                        .unwrap_or_default()
                )
                .into(),
            );
            return Err(error);
        }
    };

    // Read the module source
    let source = match fs::read_to_string(&module_path) {
        Ok(source) => source,
        Err(err) => {
            let error = JsError::from_opaque(
                format!("Error reading module '{}': {}", module_path.display(), err).into(),
            );
            return Err(error);
        }
    };

    // Create the module
    let module = Box::new(esm::EsModule::new(module_path.clone(), source));

    // Add the module to the cache
    cache.add(module);

    // Get the module from the cache
    let module = cache.get(&module_path).unwrap();

    // Evaluate the module
    match module.evaluate(ctx) {
        Ok(exports) => {
            promise_obj.set("__value", exports, true, ctx)?;
            promise_obj.set("__state", "fulfilled", true, ctx)?;
        }
        Err(err) => {
            promise_obj.set("__reason", err.to_string(), true, ctx)?;
            promise_obj.set("__state", "rejected", true, ctx)?;
        }
    }

    Ok(promise_obj.into())
}

impl ModuleSystem {
    /// Create a new module system
    pub fn new(
        permissions: PermissionManager,
        fs: Arc<FileSystemModule>,
        base_dir: PathBuf,
    ) -> Self {
        let mut resolvers: Vec<Box<dyn ModuleResolver>> = Vec::new();
        resolvers.push(Box::new(NodeModuleResolver::new()));

        let cache = ModuleCache::new();
        let resolvers = Arc::new(Mutex::new(resolvers));

        // Store the module system data in the global storage
        unsafe {
            MODULE_CACHE = Some(Arc::new(cache.clone()));
            MODULE_RESOLVERS = Some(resolvers.clone());
            BASE_DIR = Some(Arc::new(base_dir.clone()));
            FS_MODULE = Some(fs.clone());
        }

        ModuleSystem {
            permissions,
            fs,
            cache,
            resolvers,
            base_dir,
        }
    }

    /// Register the module system with a JavaScript context
    pub fn register(&self, context: &mut Context) -> JsResult<()> {
        // Register the CommonJS module system
        self.register_commonjs(context)?;

        // Register the ES module system
        self.register_esm(context)?;

        Ok(())
    }

    /// Register the CommonJS module system
    fn register_commonjs(&self, context: &mut Context) -> JsResult<()> {
        // Create the require function
        let require = NativeFunction::from_fn_ptr(require_fn);
        let js_fn = FunctionObjectBuilder::new(context, require).build();
        // Add the require function to the global object
        let global = context.global_object();
        global.set("require", js_fn, true, context)?;

        Ok(())
    }

    /// Register the ES module system
    fn register_esm(&self, context: &mut Context) -> JsResult<()> {
        // Create the import function
        let import = NativeFunction::from_fn_ptr(import_fn);

        let js_fn = FunctionObjectBuilder::new(context, import).build();

        // Add the import function to the global object
        let global = context.global_object();
        global.set("import", js_fn, true, context)?;

        Ok(())
    }

    /// Add a module resolver
    pub fn add_resolver(&mut self, resolver: Box<dyn ModuleResolver>) {
        if let Ok(mut resolvers) = self.resolvers.lock() {
            resolvers.push(resolver);
        }
    }

    /// Get the module cache
    pub fn cache(&self) -> &ModuleCache {
        &self.cache
    }

    /// Clear the module cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }

    /// Require a module
    pub fn require(&self, specifier: &str, context: &mut Context) -> JsResult<JsValue> {
        // Create the require arguments
        let args = [JsValue::from(specifier)];

        // Get the require function from the global object
        let global = context.global_object();
        let require = global.get("require", context)?;

        // Call the require function
        if let Some(require_obj) = require.as_object() {
            require_obj.call(&JsValue::undefined(), &args, context)
        } else {
            Err(JsError::from_opaque("require is not a function".into()))
        }
    }

    /// Import a module
    pub fn import(&self, specifier: &str, context: &mut Context) -> JsResult<JsValue> {
        // Create the import arguments
        let args = [JsValue::from(specifier)];

        // Get the import function from the global object
        let global = context.global_object();
        let import = global.get("import", context)?;

        // Call the import function
        if let Some(import_obj) = import.as_object() {
            import_obj.call(&JsValue::undefined(), &args, context)
        } else {
            Err(JsError::from_opaque("import is not a function".into()))
        }
    }
}

impl Clone for ModuleCache {
    fn clone(&self) -> Self {
        // Create a new cache
        let mut new_cache = ModuleCache::new();

        // Copy the modules
        let modules = self.modules.lock().unwrap();
        for (path, module) in modules.iter() {
            let module_type = module.module_type();
            let path = module.path().to_path_buf();

            match module_type {
                ModuleType::CommonJs => {
                    // Read the source code
                    let source = fs::read_to_string(&path).unwrap_or_default();
                    new_cache.add(Box::new(commonjs::CommonJsModule::new(path, source)));
                }
                ModuleType::EsModule => {
                    // Read the source code
                    let source = fs::read_to_string(&path).unwrap_or_default();
                    new_cache.add(Box::new(esm::EsModule::new(path, source)));
                }
            }
        }

        new_cache
    }
}
