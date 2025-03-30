use std::path::{Path, PathBuf};
use std::fs;
use std::io;
use std::fmt;

/// Module resolution error
#[derive(Debug)]
pub enum ModuleResolutionError {
    /// Module not found
    ModuleNotFound(String),
    
    /// I/O error
    IoError(io::Error),
    
    /// Invalid module
    InvalidModule(String),
}

impl fmt::Display for ModuleResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModuleResolutionError::ModuleNotFound(module) => write!(f, "Module not found: {}", module),
            ModuleResolutionError::IoError(err) => write!(f, "I/O error: {}", err),
            ModuleResolutionError::InvalidModule(reason) => write!(f, "Invalid module: {}", reason),
        }
    }
}

impl std::error::Error for ModuleResolutionError {}

impl From<io::Error> for ModuleResolutionError {
    fn from(err: io::Error) -> Self {
        ModuleResolutionError::IoError(err)
    }
}

/// Module resolver trait
pub trait ModuleResolver: Send + Sync {
    /// Resolve a module specifier to a file path
    fn resolve(&self, specifier: &str, referrer: Option<&Path>, base_dir: &Path) -> Result<PathBuf, ModuleResolutionError>;
    
    /// Clone the resolver
    fn clone_box(&self) -> Box<dyn ModuleResolver>;
}

/// Node.js-style module resolver
#[derive(Debug, Clone)]
pub struct NodeModuleResolver {
    /// Extensions to try when resolving modules
    extensions: Vec<String>,
}

impl NodeModuleResolver {
    /// Create a new Node.js-style module resolver
    pub fn new() -> Self {
        NodeModuleResolver {
            extensions: vec![".js".to_string(), ".json".to_string()],
        }
    }
    
    /// Add an extension to try when resolving modules
    pub fn add_extension(&mut self, extension: String) {
        self.extensions.push(extension);
    }
    
    /// Check if a path exists and is a file
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
    
    /// Check if a path exists and is a directory
    fn is_directory(&self, path: &Path) -> bool {
        path.is_dir()
    }
    
    /// Resolve a file
    fn resolve_file(&self, path: &Path) -> Option<PathBuf> {
        // Check if the path exists as is
        if self.is_file(path) {
            return Some(path.to_path_buf());
        }
        
        // Try adding extensions
        for ext in &self.extensions {
            let path_with_ext = path.with_extension(ext.trim_start_matches('.'));
            if self.is_file(&path_with_ext) {
                return Some(path_with_ext);
            }
        }
        
        None
    }
    
    /// Resolve a directory
    fn resolve_directory(&self, path: &Path) -> Option<PathBuf> {
        // Check if the directory exists
        if !self.is_directory(path) {
            return None;
        }
        
        // Try package.json
        let package_json_path = path.join("package.json");
        if self.is_file(&package_json_path) {
            if let Ok(content) = fs::read_to_string(&package_json_path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(main) = json.get("main").and_then(|v| v.as_str()) {
                        let main_path = path.join(main);
                        if let Some(resolved) = self.resolve_file(&main_path) {
                            return Some(resolved);
                        }
                    }
                }
            }
        }
        
        // Try index files
        for ext in &self.extensions {
            let index_path = path.join(format!("index{}", ext));
            if self.is_file(&index_path) {
                return Some(index_path);
            }
        }
        
        None
    }
    
    /// Resolve a module in node_modules
    fn resolve_node_modules(&self, specifier: &str, start_dir: &Path) -> Option<PathBuf> {
        let mut dir = start_dir.to_path_buf();
        
        loop {
            let node_modules_dir = dir.join("node_modules");
            if self.is_directory(&node_modules_dir) {
                let module_path = node_modules_dir.join(specifier);
                
                // Try as a file
                if let Some(resolved) = self.resolve_file(&module_path) {
                    return Some(resolved);
                }
                
                // Try as a directory
                if let Some(resolved) = self.resolve_directory(&module_path) {
                    return Some(resolved);
                }
            }
            
            // Go up one directory
            if !dir.pop() {
                break;
            }
        }
        
        None
    }
}

impl ModuleResolver for NodeModuleResolver {
    fn resolve(&self, specifier: &str, referrer: Option<&Path>, base_dir: &Path) -> Result<PathBuf, ModuleResolutionError> {
        // Handle built-in modules
        if specifier.starts_with('@') || specifier.starts_with('.') || specifier.starts_with('/') {
            // Relative or absolute path
            let path = if specifier.starts_with('.') {
                // Relative path
                if let Some(referrer) = referrer {
                    if let Some(parent) = referrer.parent() {
                        parent.join(specifier)
                    } else {
                        base_dir.join(specifier)
                    }
                } else {
                    base_dir.join(specifier)
                }
            } else if specifier.starts_with('/') {
                // Absolute path
                PathBuf::from(specifier)
            } else {
                // Scoped package
                if let Some(referrer) = referrer {
                    if let Some(parent) = referrer.parent() {
                        if let Some(resolved) = self.resolve_node_modules(specifier, parent) {
                            return Ok(resolved);
                        }
                    }
                }
                
                if let Some(resolved) = self.resolve_node_modules(specifier, base_dir) {
                    return Ok(resolved);
                }
                
                return Err(ModuleResolutionError::ModuleNotFound(specifier.to_string()));
            };
            
            // Try as a file
            if let Some(resolved) = self.resolve_file(&path) {
                return Ok(resolved);
            }
            
            // Try as a directory
            if let Some(resolved) = self.resolve_directory(&path) {
                return Ok(resolved);
            }
            
            Err(ModuleResolutionError::ModuleNotFound(specifier.to_string()))
        } else {
            // Bare specifier (e.g. 'lodash')
            if let Some(referrer) = referrer {
                if let Some(parent) = referrer.parent() {
                    if let Some(resolved) = self.resolve_node_modules(specifier, parent) {
                        return Ok(resolved);
                    }
                }
            }
            
            if let Some(resolved) = self.resolve_node_modules(specifier, base_dir) {
                return Ok(resolved);
            }
            
            Err(ModuleResolutionError::ModuleNotFound(specifier.to_string()))
        }
    }
    
    fn clone_box(&self) -> Box<dyn ModuleResolver> {
        Box::new(self.clone())
    }
}

impl Default for NodeModuleResolver {
    fn default() -> Self {
        Self::new()
    }
}
