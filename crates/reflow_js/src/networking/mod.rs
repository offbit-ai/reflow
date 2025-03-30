use boa_engine::object::{NativeObject, ObjectInitializer};
use boa_engine::{property::Attribute, Context, JsError, JsResult, JsValue};
use crate::security::{PermissionManager, Permission};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

pub mod fetch;
pub mod websocket;

/// Network configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Allowed hosts
    pub allowed_hosts: Arc<Mutex<HashSet<String>>>,
    
    /// Default allowed
    pub default_allowed: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        NetworkConfig {
            allowed_hosts: Arc::new(Mutex::new(HashSet::new())),
            default_allowed: false,
        }
    }
}

/// Networking module
pub struct NetworkingModule {
    /// Permission manager
    permissions: PermissionManager,
    
    /// Network configuration
    config: NetworkConfig,
}

impl NetworkingModule {
    /// Create a new networking module
    pub fn new(permissions: PermissionManager) -> Self {
        NetworkingModule {
            permissions,
            config: NetworkConfig::default(),
        }
    }
    
    /// Create a new networking module with configuration
    pub fn with_config(permissions: PermissionManager, config: NetworkConfig) -> Self {
        NetworkingModule {
            permissions,
            config,
        }
    }
    
    /// Register the networking module with a JavaScript context
    pub fn register(&self, context: &mut Context) -> JsResult<()> {
        // Register the fetch API
        self.register_fetch(context)?;
        
        // Register the WebSocket API
        self.register_websocket(context)?;
        
        Ok(())
    }
    
    /// Register the fetch API
    fn register_fetch(&self, context: &mut Context) -> JsResult<()> {
        // Create the fetch object
        let fetch = fetch::FetchModule::new(self.permissions.clone(), self.config.clone());

        // Register the fetch object with the context
        context.register_global_property("fetch", fetch, Attribute::default())?;
        Ok(())
    }
    
    /// Register the WebSocket API
    fn register_websocket(&self, context: &mut Context) -> JsResult<()> {
        // Create the WebSocket object
        let websocket = websocket::WebSocketModule::new(self.permissions.clone(), self.config.clone());
        // Register the WebSocket object with the context
        context.register_global_property("WebSocket", JsValue::from(websocket), Attribute::default())?;
        Ok(())
    }
    
    /// Get the network configuration
    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }
    
    /// Set the network configuration
    pub fn set_config(&mut self, config: NetworkConfig) {
        self.config = config;
    }
    
    /// Add an allowed host
    pub fn add_allowed_host(&self, host: &str) {
        let mut allowed_hosts = self.config.allowed_hosts.lock().unwrap();
        allowed_hosts.insert(host.to_string());
    }
    
    /// Remove an allowed host
    pub fn remove_allowed_host(&self, host: &str) {
        let mut allowed_hosts = self.config.allowed_hosts.lock().unwrap();
        allowed_hosts.remove(host);
    }
    
    /// Check if a host is allowed
    pub fn is_host_allowed(&self, host: &str) -> bool {
        // Check if network access is allowed
        if !self.permissions.is_granted(Permission::NetworkAccess) {
            return false;
        }
        
        // Check if the host is allowed
        let allowed_hosts = self.config.allowed_hosts.lock().unwrap();
        if allowed_hosts.contains(host) {
            return true;
        }
        
        // Check if all hosts are allowed by default
        self.config.default_allowed
    }
}
