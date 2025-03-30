use boa_engine::{
    Context, JsValue, JsError, JsResult, object::ObjectInitializer,
    property::Attribute,
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use crate::modules::Module;
use crate::modules::ModuleType;

/// CommonJS module
#[derive(Debug)]
pub struct CommonJsModule {
    /// Module path
    path: PathBuf,
    
    /// Module source code
    source: String,
    
    /// Module exports
    exports: Arc<Mutex<Option<JsValue>>>,
}

impl CommonJsModule {
    /// Create a new CommonJS module
    pub fn new(path: PathBuf, source: String) -> Self {
        CommonJsModule {
            path,
            source,
            exports: Arc::new(Mutex::new(None)),
        }
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
            // In a real implementation, this would use the module system to resolve and load modules
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                if args.is_empty() {
                    return Err(JsError::from_opaque("require requires a module specifier".into()));
                }
                
                let specifier = match args[0].as_string() {
                    Some(s) => s.to_std_string_escaped(),
                    None => return Err(JsError::from_opaque("require requires a string module specifier".into())),
                };
                
                // In a real implementation, this would resolve and load the module
                // For now, just return an empty object
                let mock_exports = ObjectInitializer::new(ctx).build();
                
                Ok(mock_exports.into())
            }
        };
        
        // Create the require function object
        let require_obj = ObjectInitializer::new(context)
            .function("require", 1, require)
            .build();
        
        // Create the wrapper function
        let wrapper_source = format!(
            "(function(exports, require, module, __filename, __dirname) {{\n{}\n}})",
            self.source
        );
        
        // Evaluate the wrapper function
        let wrapper = context.eval(wrapper_source)?;
        
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
            require_obj.into(),
            module.into(),
            filename,
            dirname,
        ];
        
        let _result = context.call(&wrapper, &JsValue::undefined(), &args)?;
        
        // Store the exports
        *self.exports.lock().unwrap() = Some(exports.clone().into());
        
        Ok(exports.into())
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
        if let Some(exports) = &*self.exports.lock().unwrap() {
            Ok(exports.clone())
        } else {
            Err(JsError::from_opaque("Module not evaluated".into()))
        }
    }
    
    fn evaluate(&self, context: &mut Context) -> JsResult<JsValue> {
        // Check if the module has already been evaluated
        if let Some(exports) = &*self.exports.lock().unwrap() {
            return Ok(exports.clone());
        }
        
        // Evaluate the module
        self.create_wrapper(context)
    }
}
