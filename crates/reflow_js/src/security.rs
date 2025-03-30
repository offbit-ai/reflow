use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::fmt;

/// Permission result
pub type PermissionResult<T> = Result<T, PermissionError>;

/// Permission error
#[derive(Debug, Clone)]
pub enum PermissionError {
    /// Permission denied
    Denied(String),
    
    /// Permission already exists
    AlreadyExists(String),
    
    /// Permission not found
    NotFound(String),
    
    /// Other error
    Other(String),
}

impl fmt::Display for PermissionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PermissionError::Denied(msg) => write!(f, "Permission denied: {}", msg),
            PermissionError::AlreadyExists(msg) => write!(f, "Permission already exists: {}", msg),
            PermissionError::NotFound(msg) => write!(f, "Permission not found: {}", msg),
            PermissionError::Other(msg) => write!(f, "Permission error: {}", msg),
        }
    }
}

impl std::error::Error for PermissionError {}

/// File system permission scope
#[derive(Debug, Clone)]
pub enum FileSystemPermissionScope {
    /// All files and directories
    All,
    
    /// Specific directories
    Directories(Vec<PathBuf>),
    
    /// Specific files
    Files(Vec<PathBuf>),
}

/// Network permission scope
#[derive(Debug, Clone)]
pub enum NetworkPermissionScope {
    /// All hosts
    All,
    
    /// Specific hosts
    Hosts(Vec<String>),
}

/// Permission scope
#[derive(Debug, Clone)]
pub enum PermissionScope {
    /// File system permission
    FileSystem(FileSystemPermissionScope),
    
    /// Network permission
    Network(NetworkPermissionScope),
    
    /// Process permission
    Process,
    
    /// Environment permission
    Environment,
}

/// Permission
#[derive(Debug, Clone)]
pub struct Permission {
    /// Permission name
    pub name: String,
    
    /// Permission scope
    pub scope: PermissionScope,
    
    /// Whether the permission is granted
    pub granted: bool,
}

impl Permission {
    /// Create a new permission
    pub fn new(name: String, scope: PermissionScope, granted: bool) -> Self {
        Permission {
            name,
            scope,
            granted,
        }
    }
}

/// Permission manager
#[derive(Debug, Clone)]
pub struct PermissionManager {
    /// Permissions
    permissions: Arc<Mutex<HashMap<String, Permission>>>,
}

impl PermissionManager {
    /// Create a new permission manager
    pub fn new() -> Self {
        PermissionManager {
            permissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Add a permission
    pub fn add_permission(&self, permission: Permission) -> PermissionResult<()> {
        let mut permissions = self.permissions.lock().unwrap();
        
        if permissions.contains_key(&permission.name) {
            return Err(PermissionError::AlreadyExists(format!("Permission '{}' already exists", permission.name)));
        }
        
        permissions.insert(permission.name.clone(), permission);
        
        Ok(())
    }
    
    /// Remove a permission
    pub fn remove_permission(&self, name: &str) -> PermissionResult<()> {
        let mut permissions = self.permissions.lock().unwrap();
        
        if !permissions.contains_key(name) {
            return Err(PermissionError::NotFound(format!("Permission '{}' not found", name)));
        }
        
        permissions.remove(name);
        
        Ok(())
    }
    
    /// Get a permission
    pub fn get_permission(&self, name: &str) -> PermissionResult<Permission> {
        let permissions = self.permissions.lock().unwrap();
        
        if let Some(permission) = permissions.get(name) {
            Ok(permission.clone())
        } else {
            Err(PermissionError::NotFound(format!("Permission '{}' not found", name)))
        }
    }
    
    /// Check if a permission is granted
    pub fn is_granted(&self, name: &str) -> bool {
        let permissions = self.permissions.lock().unwrap();
        
        if let Some(permission) = permissions.get(name) {
            permission.granted
        } else {
            false
        }
    }
    
    /// Grant a permission
    pub fn grant_permission(&self, name: &str) -> PermissionResult<()> {
        let mut permissions = self.permissions.lock().unwrap();
        
        if let Some(permission) = permissions.get_mut(name) {
            permission.granted = true;
            Ok(())
        } else {
            Err(PermissionError::NotFound(format!("Permission '{}' not found", name)))
        }
    }
    
    /// Revoke a permission
    pub fn revoke_permission(&self, name: &str) -> PermissionResult<()> {
        let mut permissions = self.permissions.lock().unwrap();
        
        if let Some(permission) = permissions.get_mut(name) {
            permission.granted = false;
            Ok(())
        } else {
            Err(PermissionError::NotFound(format!("Permission '{}' not found", name)))
        }
    }
    
    /// Check if a file system operation is allowed
    pub fn check_fs_permission(&self, path: &Path, operation: &str) -> PermissionResult<()> {
        let permissions = self.permissions.lock().unwrap();
        
        // Check if there's a file system permission
        for (_, permission) in permissions.iter() {
            if !permission.granted {
                continue;
            }
            
            match &permission.scope {
                PermissionScope::FileSystem(scope) => {
                    match scope {
                        FileSystemPermissionScope::All => {
                            return Ok(());
                        },
                        FileSystemPermissionScope::Directories(dirs) => {
                            for dir in dirs {
                                if path.starts_with(dir) {
                                    return Ok(());
                                }
                            }
                        },
                        FileSystemPermissionScope::Files(files) => {
                            for file in files {
                                if path == file {
                                    return Ok(());
                                }
                            }
                        },
                    }
                },
                _ => {},
            }
        }
        
        Err(PermissionError::Denied(format!("File system operation '{}' on '{}' is not allowed", operation, path.display())))
    }
    
    /// Check if a network operation is allowed
    pub fn check_network_permission(&self, host: &str, operation: &str) -> PermissionResult<()> {
        let permissions = self.permissions.lock().unwrap();
        
        // Check if there's a network permission
        for (_, permission) in permissions.iter() {
            if !permission.granted {
                continue;
            }
            
            match &permission.scope {
                PermissionScope::Network(scope) => {
                    match scope {
                        NetworkPermissionScope::All => {
                            return Ok(());
                        },
                        NetworkPermissionScope::Hosts(hosts) => {
                            for allowed_host in hosts {
                                if host.ends_with(allowed_host) {
                                    return Ok(());
                                }
                            }
                        },
                    }
                },
                _ => {},
            }
        }
        
        Err(PermissionError::Denied(format!("Network operation '{}' to '{}' is not allowed", operation, host)))
    }
    
    /// Check if a process operation is allowed
    pub fn check_process_permission(&self, operation: &str) -> PermissionResult<()> {
        let permissions = self.permissions.lock().unwrap();
        
        // Check if there's a process permission
        for (_, permission) in permissions.iter() {
            if !permission.granted {
                continue;
            }
            
            match &permission.scope {
                PermissionScope::Process => {
                    return Ok(());
                },
                _ => {},
            }
        }
        
        Err(PermissionError::Denied(format!("Process operation '{}' is not allowed", operation)))
    }
    
    /// Check if an environment operation is allowed
    pub fn check_environment_permission(&self, operation: &str) -> PermissionResult<()> {
        let permissions = self.permissions.lock().unwrap();
        
        // Check if there's an environment permission
        for (_, permission) in permissions.iter() {
            if !permission.granted {
                continue;
            }
            
            match &permission.scope {
                PermissionScope::Environment => {
                    return Ok(());
                },
                _ => {},
            }
        }
        
        Err(PermissionError::Denied(format!("Environment operation '{}' is not allowed", operation)))
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}
