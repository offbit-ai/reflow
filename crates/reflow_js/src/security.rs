use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// Permission type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Permission {
    /// File system read permission
    FileSystemRead,
    
    /// File system write permission
    FileSystemWrite,
    
    /// Network access permission
    NetworkAccess,
    
    /// System command execution permission
    SystemCommand,
}

/// Permission state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    /// Permission granted
    Granted,
    
    /// Permission denied
    Denied,
    
    /// Permission prompt
    Prompt,
}

impl Default for PermissionState {
    fn default() -> Self {
        PermissionState::Prompt
    }
}

/// Permission manager
#[derive(Debug, Clone)]
pub struct PermissionManager {
    /// Permissions
    permissions: Arc<Mutex<HashMap<Permission, PermissionState>>>,
}

impl PermissionManager {
    /// Create a new permission manager
    pub fn new() -> Self {
        PermissionManager {
            permissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Set permission
    pub fn set_permission(&self, permission: Permission, state: PermissionState) {
        let mut permissions = self.permissions.lock().unwrap();
        permissions.insert(permission, state);
    }
    
    /// Get permission
    pub fn get_permission(&self, permission: Permission) -> PermissionState {
        let permissions = self.permissions.lock().unwrap();
        *permissions.get(&permission).unwrap_or(&PermissionState::default())
    }
    
    /// Check if permission is granted
    pub fn is_granted(&self, permission: Permission) -> bool {
        self.get_permission(permission) == PermissionState::Granted
    }
    
    /// Check if permission is denied
    pub fn is_denied(&self, permission: Permission) -> bool {
        self.get_permission(permission) == PermissionState::Denied
    }
    
    /// Check if permission requires prompt
    pub fn requires_prompt(&self, permission: Permission) -> bool {
        self.get_permission(permission) == PermissionState::Prompt
    }
    
    /// Reset all permissions
    pub fn reset(&self) {
        let mut permissions = self.permissions.lock().unwrap();
        permissions.clear();
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}
