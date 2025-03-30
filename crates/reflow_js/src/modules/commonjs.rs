use boa_engine::Source;
use boa_engine::{
    Context, JsValue, JsError, JsResult, object::ObjectInitializer,
    property::Attribute, native_function::NativeFunction,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use crate::modules::{Module, ModuleType};
use std::fs;

/// CommonJS module
#[derive(Debug)]
pub struct CommonJsModule {
    /// Module path
    path: PathBuf,
    
    /// Module source code
    source: String,
    
    /// Module evaluated flag
    evaluated: Arc<Mutex<bool>>,
}

impl CommonJsModule {
    /// Create a new CommonJS module
    pub fn new(path: PathBuf, source: String) -> Self {
        CommonJsModule {
            path,
            source,
            evaluated: Arc::new(Mutex::new(false)),
        }
    }
    
    /// Create a new CommonJS module from a file
    pub fn from_file(path: PathBuf) -> Result<Self, std::io::Error> {
        let source = fs::read_to_string(&path)?;
        Ok(Self::new(path, source))
    }
    
    /// Create the module wrapper
    fn create_wrapper(&self, context: &mut Context) -> JsResult<JsValue> {
        // Create the exports object
        let exports = ObjectInitializer::new(context).build();
        
        // Create the module object
        let module = ObjectInitializer::new(context)
            .property("exports", exports.clone(), Attribute::all())
            .build();
        
        // Create the require function
        let require = {
            // Get the require function from the global object
            let global = context.global_object();
            global.get("require", context)?
        };
        
        // Create the wrapper function
        let wrapper_source = format!(
            "(function(exports, require, module, __filename, __dirname) {{\n{}\n}})",
            self.source
        );
        
        // Evaluate the wrapper function
        let wrapper = context.eval(Source::from_bytes(&wrapper_source))?;
        
        // Create the __filename and __dirname values
        let filename = JsValue::from(self.path.to_string_lossy().to_string());
        let dirname = if let Some(parent) = self.path.parent() {
            JsValue::from(parent.to_string_lossy().to_string())
        } else {
            JsValue::from(".")
        };
        
        // Call the wrapper function with the module context
        let args = [
            exports.clone().into(),
            require,
            module.clone().into(),
            filename,
            dirname,
        ];
        
        let _result = if let Some(wrapper_obj) = wrapper.as_object() {
            wrapper_obj.call(&JsValue::undefined(), &args, context)?
        } else {
            return Err(JsError::from_opaque("Wrapper is not a function".into()));
        };
        
        // Get the exports from the module object
        let module_exports = module.get("exports", context)?;
        
        // Mark the module as evaluated
        *self.evaluated.lock().unwrap() = true;
        
        Ok(module_exports)
    }
}

impl Module for CommonJsModule {
    fn module_type(&self) -> ModuleType {
        ModuleType::CommonJs
    }
    
    fn path(&self) -> &Path {
        &self.path
    }
    
    fn exports(&self) -> JsResult<JsValue> {
        // We can't actually return the exports here since we don't store them
        // Instead, we'll return an error indicating that the module needs to be evaluated
        Err(JsError::from_opaque("Module exports can only be accessed during evaluation".into()))
    }
    
    fn evaluate(&self, context: &mut Context) -> JsResult<JsValue> {
        // Check if the module has already been evaluated
        if *self.evaluated.lock().unwrap() {
            // Re-evaluate the module to get the exports
            // This is not ideal, but it's a workaround for the thread safety issue
            return self.create_wrapper(context);
        }
        
        // Evaluate the module
        self.create_wrapper(context)
    }
}
