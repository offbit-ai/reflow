use std::path::{Path, PathBuf};
use crate::security::{PermissionManager, Permission, PermissionScope, PermissionResult, FileSystemPermissionScope};

/// File system permissions
#[derive(Debug, Clone)]
pub struct FileSystemPermissions {
    /// Allow read operations
    pub allow_read: bool,
    
    /// Allow write operations
    pub allow_write: bool,
    
    /// Allow directory operations
    pub allow_directory: bool,
    
    /// Allowed paths
    pub allowed_paths: Vec<PathBuf>,
}

impl FileSystemPermissions {
    /// Create new file system permissions
    pub fn new(allow_read: bool, allow_write: bool, allow_directory: bool, allowed_paths: Vec<PathBuf>) -> Self {
        FileSystemPermissions {
            allow_read,
            allow_write,
            allow_directory,
            allowed_paths,
        }
    }
    
    /// Check if a path is allowed
    pub fn is_path_allowed(&self, path: &Path) -> bool {
        // If no paths are specified, nothing is allowed
        if self.allowed_paths.is_empty() {
            return false;
        }
        
        // Check if the path is in the allowed paths
        for allowed_path in &self.allowed_paths {
            if path.starts_with(allowed_path) {
                return true;
            }
        }
        
        false
    }
    
    /// Check if read operations are allowed for a path
    pub fn check_read(&self, path: &Path) -> PermissionResult<()> {
        if !self.allow_read {
            return Err(crate::security::PermissionError::Denied(
                format!("Read operations are not allowed: {}", path.display())
            ));
        }
        
        if !self.is_path_allowed(path) {
            return Err(crate::security::PermissionError::Denied(
                format!("Path is not allowed for reading: {}", path.display())
            ));
        }
        
        Ok(())
    }
    
    /// Check if write operations are allowed for a path
    pub fn check_write(&self, path: &Path) -> PermissionResult<()> {
        if !self.allow_write {
            return Err(crate::security::PermissionError::Denied(
                format!("Write operations are not allowed: {}", path.display())
            ));
        }
        
        if !self.is_path_allowed(path) {
            return Err(crate::security::PermissionError::Denied(
                format!("Path is not allowed for writing: {}", path.display())
            ));
        }
        
        Ok(())
    }
    
    /// Check if directory operations are allowed for a path
    pub fn check_directory(&self, path: &Path) -> PermissionResult<()> {
        if !self.allow_directory {
            return Err(crate::security::PermissionError::Denied(
                format!("Directory operations are not allowed: {}", path.display())
            ));
        }
        
        if !self.is_path_allowed(path) {
            return Err(crate::security::PermissionError::Denied(
                format!("Path is not allowed for directory operations: {}", path.display())
            ));
        }
        
        Ok(())
    }
    
    /// Register permissions with the permission manager
    pub fn register_with_manager(&self, manager: &PermissionManager) -> PermissionResult<()> {
        // Create file system permission
        let scope = match self.allowed_paths.is_empty() {
            true => FileSystemPermissionScope::All,
            false => FileSystemPermissionScope::Directories(self.allowed_paths.clone()),
        };
        
        let permission = Permission::new(
            "fs".to_string(),
            PermissionScope::FileSystem(scope),
            true,
        );
        
        // Add permission to manager
        manager.add_permission(permission)
    }
    
    /// Check if two permission scopes are compatible
    pub fn is_compatible(scope1: &FileSystemPermissionScope, scope2: &FileSystemPermissionScope) -> bool {
        match (scope1, scope2) {
            (FileSystemPermissionScope::All, _) => true,
            (_, FileSystemPermissionScope::All) => true,
            (FileSystemPermissionScope::Directories(dirs1), FileSystemPermissionScope::Directories(dirs2)) => {
                // Check if any directory in dirs1 is a parent of any directory in dirs2
                for dir1 in dirs1 {
                    for dir2 in dirs2 {
                        if dir2.starts_with(dir1) {
                            return true;
                        }
                    }
                }
                false
            },
            (FileSystemPermissionScope::Directories(dirs), FileSystemPermissionScope::Files(files)) => {
                // Check if any file is in any directory
                for dir in dirs {
                    for file in files {
                        if file.starts_with(dir) {
                            return true;
                        }
                    }
                }
                false
            },
            (FileSystemPermissionScope::Files(files), FileSystemPermissionScope::Directories(dirs)) => {
                // Check if any file is in any directory
                for file in files {
                    for dir in dirs {
                        if file.starts_with(dir) {
                            return true;
                        }
                    }
                }
                false
            },
            (FileSystemPermissionScope::Files(files1), FileSystemPermissionScope::Files(files2)) => {
                // Check if any file is the same
                for file1 in files1 {
                    for file2 in files2 {
                        if file1 == file2 {
                            return true;
                        }
                    }
                }
                false
            },
        }
    }
}

impl Default for FileSystemPermissions {
    fn default() -> Self {
        FileSystemPermissions {
            allow_read: true,
            allow_write: false,
            allow_directory: true,
            allowed_paths: vec![],
        }
    }
}
