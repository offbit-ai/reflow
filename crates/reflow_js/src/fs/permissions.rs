use std::path::{Path, PathBuf};
use crate::security::{Permission, PermissionManager, PermissionState};
use std::sync::Arc;

/// File system permission scope
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FileSystemPermissionScope {
    /// Read permission
    Read(PathBuf),
    
    /// Write permission
    Write(PathBuf),
}

/// File system permission error
#[derive(Debug, Clone)]
pub enum FileSystemPermissionError {
    /// Permission denied
    PermissionDenied(String),
    
    /// Invalid path
    InvalidPath(String),
    
    /// Other error
    Other(String),
}

impl std::fmt::Display for FileSystemPermissionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileSystemPermissionError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            FileSystemPermissionError::InvalidPath(msg) => write!(f, "Invalid path: {}", msg),
            FileSystemPermissionError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for FileSystemPermissionError {}

/// File system permission result
pub type FileSystemPermissionResult<T> = Result<T, FileSystemPermissionError>;

/// File system permissions
pub struct FileSystemPermissions {
    /// Permission manager
    permission_manager: PermissionManager,
    
    /// Root directory
    root_dir: PathBuf,
}

impl FileSystemPermissions {
    /// Create a new file system permissions
    pub fn new(permission_manager: PermissionManager, root_dir: PathBuf) -> Self {
        FileSystemPermissions {
            permission_manager,
            root_dir,
        }
    }
    
    /// Check if a path is allowed for reading
    pub fn check_read_permission(&self, path: &Path) -> FileSystemPermissionResult<()> {
        // Check if the path is within the root directory
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root_dir.join(path)
        };
        
        // Canonicalize the path
        let canonical_path = match absolute_path.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                return Err(FileSystemPermissionError::InvalidPath(
                    format!("Failed to canonicalize path: {}", path.display())
                ));
            }
        };
        
        // Check if the path is within the root directory
        if !canonical_path.starts_with(&self.root_dir) {
            return Err(FileSystemPermissionError::PermissionDenied(
                format!("Path is outside of the root directory: {}", path.display())
            ));
        }
        
        // Check if the permission is granted
        if !self.permission_manager.is_granted(Permission::FileSystemRead) {
            return Err(FileSystemPermissionError::PermissionDenied(
                format!("Read permission denied for path: {}", path.display())
            ));
        }
        
        Ok(())
    }
    
    /// Check if a path is allowed for writing
    pub fn check_write_permission(&self, path: &Path) -> FileSystemPermissionResult<()> {
        // Check if the path is within the root directory
        let absolute_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root_dir.join(path)
        };
        
        // Canonicalize the path
        let canonical_path = match absolute_path.canonicalize() {
            Ok(path) => path,
            Err(_) => {
                return Err(FileSystemPermissionError::InvalidPath(
                    format!("Failed to canonicalize path: {}", path.display())
                ));
            }
        };
        
        // Check if the path is within the root directory
        if !canonical_path.starts_with(&self.root_dir) {
            return Err(FileSystemPermissionError::PermissionDenied(
                format!("Path is outside of the root directory: {}", path.display())
            ));
        }
        
        // Check if the permission is granted
        if !self.permission_manager.is_granted(Permission::FileSystemWrite) {
            return Err(FileSystemPermissionError::PermissionDenied(
                format!("Write permission denied for path: {}", path.display())
            ));
        }
        
        Ok(())
    }
    
    /// Normalize a path
    pub fn normalize_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root_dir.join(path)
        }
    }
    
    /// Get the root directory
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }
}
