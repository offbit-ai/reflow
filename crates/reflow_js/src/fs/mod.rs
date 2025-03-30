pub mod virtual_fs;
pub mod permissions;

use boa_engine::{Context, JsValue, JsError, JsResult, object::ObjectInitializer, property::Attribute};
use crate::runtime::{Extension, ExtensionError};
use crate::security::PermissionManager;
use crate::async_runtime::{AsyncRuntime, Task};
use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use std::path::{Path, PathBuf};
use std::fs;
use std::io::{self, Read, Write};
use std::time::SystemTime;

pub use virtual_fs::VirtualFileSystem;
pub use permissions::FileSystemPermissions;

/// File system module
#[derive(Debug)]
pub struct FileSystemModule {
    /// File system permissions
    permissions: FileSystemPermissions,
    
    /// Virtual file system (for browser environments)
    virtual_fs: Option<Arc<Mutex<VirtualFileSystem>>>,
    
    /// Async runtime
    async_runtime: Option<Arc<AsyncRuntime>>,
    
    /// Base directory
    base_dir: PathBuf,
}

impl FileSystemModule {
    /// Create a new file system module
    pub fn new(permissions: FileSystemPermissions, use_virtual_fs: bool) -> Self {
        let virtual_fs = if use_virtual_fs {
            Some(Arc::new(Mutex::new(VirtualFileSystem::new())))
        } else {
            None
        };
        
        FileSystemModule {
            permissions,
            virtual_fs,
            async_runtime: None,
            base_dir: PathBuf::from("."),
        }
    }
    
    /// Set the async runtime
    pub fn set_async_runtime(&mut self, async_runtime: Arc<AsyncRuntime>) {
        self.async_runtime = Some(async_runtime);
    }
    
    /// Set the base directory
    pub fn set_base_dir(&mut self, base_dir: PathBuf) {
        self.base_dir = base_dir;
    }
    
    /// Resolve a path
    fn resolve_path(&self, path: &str) -> PathBuf {
        if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.base_dir.join(path)
        }
    }
    
    /// Read a file synchronously
    fn read_file_sync(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("readFileSync requires a path"));
        }
        
        let path = args[0].to_string(context)?;
        let encoding = if args.len() > 1 {
            Some(args[1].to_string(context)?)
        } else {
            None
        };
        
        let path = self.resolve_path(&path);
        
        // Check permissions
        self.permissions.check_read(&path)
            .map_err(|e| JsError::from_opaque(e.to_string().into()))?;
        
        // Read the file
        if let Some(virtual_fs) = &self.virtual_fs {
            // Use virtual file system
            let virtual_fs = virtual_fs.lock().unwrap();
            let content = virtual_fs.read_file(&path)
                .map_err(|e| JsError::from_opaque(format!("Failed to read file: {}", e).into()))?;
            
            if let Some(encoding) = encoding {
                if encoding.to_lowercase() == "utf8" || encoding.to_lowercase() == "utf-8" {
                    let text = String::from_utf8(content.clone())
                        .map_err(|e| JsError::from_opaque(format!("Failed to decode file: {}", e).into()))?;
                    
                    return Ok(JsValue::from(text));
                }
            }
            
            // Return as buffer
            let buffer = context.create_object();
            // TODO: Create a proper Buffer object
            buffer.set("data", content.len(), true, context)?;
            
            Ok(buffer.into())
        } else {
            // Use real file system
            let mut file = fs::File::open(&path)
                .map_err(|e| JsError::from_opaque(format!("Failed to open file: {}", e).into()))?;
            
            let mut content = Vec::new();
            file.read_to_end(&mut content)
                .map_err(|e| JsError::from_opaque(format!("Failed to read file: {}", e).into()))?;
            
            if let Some(encoding) = encoding {
                if encoding.to_lowercase() == "utf8" || encoding.to_lowercase() == "utf-8" {
                    let text = String::from_utf8(content.clone())
                        .map_err(|e| JsError::from_opaque(format!("Failed to decode file: {}", e).into()))?;
                    
                    return Ok(JsValue::from(text));
                }
            }
            
            // Return as buffer
            let buffer = context.create_object();
            // TODO: Create a proper Buffer object
            buffer.set("data", content.len(), true, context)?;
            
            Ok(buffer.into())
        }
    }
    
    /// Write a file synchronously
    fn write_file_sync(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.len() < 2 {
            return Err(JsError::from_opaque("writeFileSync requires a path and data".into()));
        }
        
        let path = args[0].to_string(context)?;
        let data = args[1].to_string(context)?;
        
        let path = self.resolve_path(&path);
        
        // Check permissions
        self.permissions.check_write(&path)
            .map_err(|e| JsError::from_opaque(e.to_string().into()))?;
        
        // Write the file
        if let Some(virtual_fs) = &self.virtual_fs {
            // Use virtual file system
            let mut virtual_fs = virtual_fs.lock().unwrap();
            virtual_fs.write_file(&path, data.as_bytes())
                .map_err(|e| JsError::from_opaque(format!("Failed to write file: {}", e).into()))?;
        } else {
            // Use real file system
            // Create parent directories if they don't exist
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| JsError::from_opaque(format!("Failed to create directories: {}", e).into()))?;
            }
            
            // Write the file
            let mut file = fs::File::create(&path)
                .map_err(|e| JsError::from_opaque(format!("Failed to create file: {}", e).into()))?;
            
            file.write_all(data.as_bytes())
                .map_err(|e| JsError::from_opaque(format!("Failed to write file: {}", e).into()))?;
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Check if a file exists
    fn exists_sync(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("existsSync requires a path".into()));
        }
        
        let path = args[0].to_string(context)?;
        let path = self.resolve_path(&path);
        
        // Check if the file exists
        let exists = if let Some(virtual_fs) = &self.virtual_fs {
            // Use virtual file system
            let virtual_fs = virtual_fs.lock().unwrap();
            virtual_fs.exists(&path)
        } else {
            // Use real file system
            path.exists()
        };
        
        Ok(JsValue::from(exists))
    }
    
    /// Create a directory
    fn mkdir_sync(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("mkdirSync requires a path".into()));
        }
        
        let path = args[0].to_string(context)?;
        let recursive = if args.len() > 1 && args[1].is_object() {
            let options = args[1].as_object().unwrap();
            if let Ok(recursive) = options.get_property("recursive", context) {
                recursive.as_boolean().unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        
        let path = self.resolve_path(&path);
        
        // Check permissions
        self.permissions.check_directory(&path)
            .map_err(|e| JsError::from_opaque(e.to_string()))?;
        
        // Create the directory
        if let Some(virtual_fs) = &self.virtual_fs {
            // Use virtual file system
            let mut virtual_fs = virtual_fs.lock().unwrap();
            if recursive {
                virtual_fs.create_dir_all(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to create directories: {}", e).into()))?;
            } else {
                virtual_fs.create_dir(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to create directory: {}", e).into()))?;
            }
        } else {
            // Use real file system
            if recursive {
                fs::create_dir_all(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to create directories: {}", e).into()))?;
            } else {
                fs::create_dir(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to create directory: {}", e).into()))?;
            }
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Read a directory
    fn read_dir_sync(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("readdirSync requires a path"));
        }
        
        let path = args[0].to_string(context)?;
        let path = self.resolve_path(&path);
        
        // Check permissions
        self.permissions.check_directory(&path)
            .map_err(|e| JsError::from_opaque(e.to_string().into()))?;
        
        // Read the directory
        let entries = if let Some(virtual_fs) = &self.virtual_fs {
            // Use virtual file system
            let virtual_fs = virtual_fs.lock().unwrap();
            virtual_fs.read_dir(&path)
                .map_err(|e| JsError::from_opaque(format!("Failed to read directory: {}", e).into()))?
        } else {
            // Use real file system
            let entries = fs::read_dir(&path)
                .map_err(|e| JsError::from_opaque(format!("Failed to read directory: {}", e).into()))?;
            
            let mut result = Vec::new();
            for entry in entries {
                let entry = entry.map_err(|e| JsError::from_opaque(format!("Failed to read directory entry: {}", e).into()))?;
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy().to_string();
                result.push(file_name);
            }
            
            result
        };
        
        // Create an array of entries
        let array = context.create_array();
        for (i, entry) in entries.iter().enumerate() {
            array.set(i as u32, entry, true, context)?;
        }
        
        Ok(array.into())
    }
    
    /// Remove a file
    fn unlink_sync(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("unlinkSync requires a path".into()));
        }
        
        let path = args[0].to_string(context)?;
        let path = self.resolve_path(&path);
        
        // Check permissions
        self.permissions.check_write(&path)
            .map_err(|e| JsError::from_opaque(e.to_string().into()))?;
        
        // Remove the file
        if let Some(virtual_fs) = &self.virtual_fs {
            // Use virtual file system
            let mut virtual_fs = virtual_fs.lock().unwrap();
            virtual_fs.remove_file(&path)
                .map_err(|e| JsError::from_opaque(format!("Failed to remove file: {}", e).into()))?;
        } else {
            // Use real file system
            fs::remove_file(&path)
                .map_err(|e| JsError::from_opaque(format!("Failed to remove file: {}", e).into()))?;
        }
        
        Ok(JsValue::undefined())
    }
    
    /// Remove a directory
    fn rmdir_sync(&self, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        if args.is_empty() {
            return Err(JsError::from_opaque("rmdirSync requires a path"));
        }
        
        let path = args[0].to_string(context)?;
        let recursive = if args.len() > 1 && args[1].is_object() {
            let options = args[1].as_object().unwrap();
            if let Ok(recursive) = options.get_property("recursive", context) {
                recursive.as_boolean().unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };
        
        let path = self.resolve_path(&path);
        
        // Check permissions
        self.permissions.check_directory(&path)
            .map_err(|e| JsError::from_opaque(e.to_string()))?;
        
        // Remove the directory
        if let Some(virtual_fs) = &self.virtual_fs {
            // Use virtual file system
            let mut virtual_fs = virtual_fs.lock().unwrap();
            if recursive {
                virtual_fs.remove_dir_all(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to remove directory: {}", e).into()))?;
            } else {
                virtual_fs.remove_dir(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to remove directory: {}", e).into()))?;
            }
        } else {
            // Use real file system
            if recursive {
                fs::remove_dir_all(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to remove directory: {}", e).into()))?;
            } else {
                fs::remove_dir(&path)
                    .map_err(|e| JsError::from_opaque(format!("Failed to remove directory: {}", e).into()))?;
            }
        }
        
        Ok(JsValue::undefined())
    }
}

#[async_trait]
impl Extension for FileSystemModule {
    fn name(&self) -> &str {
        "fs"
    }
    
    async fn initialize(&self, context: &mut Context) -> Result<(), ExtensionError> {
        // Create the fs object
        let fs_obj = context.create_object();
        
        // Create the readFileSync function
        let read_file_sync_fn = {
            let fs_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fs_module.read_file_sync(args, ctx)
            }
        };
        
        // Create the writeFileSync function
        let write_file_sync_fn = {
            let fs_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fs_module.write_file_sync(args, ctx)
            }
        };
        
        // Create the existsSync function
        let exists_sync_fn = {
            let fs_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fs_module.exists_sync(args, ctx)
            }
        };
        
        // Create the mkdirSync function
        let mkdir_sync_fn = {
            let fs_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fs_module.mkdir_sync(args, ctx)
            }
        };
        
        // Create the readdirSync function
        let read_dir_sync_fn = {
            let fs_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fs_module.read_dir_sync(args, ctx)
            }
        };
        
        // Create the unlinkSync function
        let unlink_sync_fn = {
            let fs_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fs_module.unlink_sync(args, ctx)
            }
        };
        
        // Create the rmdirSync function
        let rmdir_sync_fn = {
            let fs_module = self.clone();
            move |_this: &JsValue, args: &[JsValue], ctx: &mut Context| {
                fs_module.rmdir_sync(args, ctx)
            }
        };
        
        // Add the functions to the fs object
        fs_obj.set("readFileSync", read_file_sync_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        fs_obj.set("writeFileSync", write_file_sync_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        fs_obj.set("existsSync", exists_sync_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        fs_obj.set("mkdirSync", mkdir_sync_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        fs_obj.set("readdirSync", read_dir_sync_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        fs_obj.set("unlinkSync", unlink_sync_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        fs_obj.set("rmdirSync", rmdir_sync_fn, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        // Add the fs object to the global object
        let global = context.global_object();
        global.set("fs", fs_obj, true, context)
            .map_err(|e| ExtensionError::InitializationFailed(e.to_string()))?;
        
        Ok(())
    }
}

impl Clone for FileSystemModule {
    fn clone(&self) -> Self {
        FileSystemModule {
            permissions: self.permissions.clone(),
            virtual_fs: self.virtual_fs.clone(),
            async_runtime: self.async_runtime.clone(),
            base_dir: self.base_dir.clone(),
        }
    }
}
