use super::types::*;
use crate::actor::Actor;
use crate::websocket_rpc::{WebSocketRpcClient, WebSocketScriptActor};
use crate::redis_state::RedisActorState;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use parking_lot::RwLock;

/// Factory for creating script actor instances
#[async_trait]
pub trait ActorFactory: Send + Sync {
    /// Create a new actor instance
    async fn create_instance(&self) -> Result<Box<dyn Actor>>;
    
    /// Get actor metadata
    fn get_metadata(&self) -> &DiscoveredScriptActor;
}

/// Script actor factory implementation
pub struct ScriptActorFactory {
    pub metadata: DiscoveredScriptActor,
    pub websocket_url: String,
    pub redis_url: String,
}

impl ScriptActorFactory {
    pub fn new(metadata: DiscoveredScriptActor) -> Result<Self> {
        Ok(Self {
            metadata,
            websocket_url: std::env::var("WEBSOCKET_URL")
                .unwrap_or_else(|_| "ws://localhost:8080".to_string()),
            redis_url: std::env::var("REDIS_URL")
                .unwrap_or_else(|_| "redis://localhost:6379".to_string()),
        })
    }
    
    pub fn with_urls(
        metadata: DiscoveredScriptActor,
        websocket_url: String,
        redis_url: String,
    ) -> Self {
        Self {
            metadata,
            websocket_url,
            redis_url,
        }
    }
}

#[async_trait]
impl ActorFactory for ScriptActorFactory {
    async fn create_instance(&self) -> Result<Box<dyn Actor>> {
        // Create WebSocket RPC client
        let rpc_client = Arc::new(WebSocketRpcClient::new(self.websocket_url.clone()));
        
        // Note: We don't connect here as the connection should be managed
        // by the actor itself during initialization
        
        // Create WebSocketScriptActor which implements Actor trait
        let actor = WebSocketScriptActor::new(
            self.metadata.clone(),
            rpc_client,
            self.redis_url.clone(),
        ).await;
        
        Ok(Box::new(actor))
    }
    
    fn get_metadata(&self) -> &DiscoveredScriptActor {
        &self.metadata
    }
}

/// Python-specific actor factory
pub struct PythonActorFactory {
    base_factory: ScriptActorFactory,
    python_path: Option<String>,
}

impl PythonActorFactory {
    pub fn new(metadata: DiscoveredScriptActor) -> Result<Self> {
        Ok(Self {
            base_factory: ScriptActorFactory::new(metadata)?,
            python_path: std::env::var("PYTHON_PATH").ok(),
        })
    }
}

#[async_trait]
impl ActorFactory for PythonActorFactory {
    async fn create_instance(&self) -> Result<Box<dyn Actor>> {
        // Python-specific initialization
        self.base_factory.create_instance().await
    }
    
    fn get_metadata(&self) -> &DiscoveredScriptActor {
        self.base_factory.get_metadata()
    }
}

/// JavaScript-specific actor factory
pub struct JavaScriptActorFactory {
    base_factory: ScriptActorFactory,
    node_path: Option<String>,
}

impl JavaScriptActorFactory {
    pub fn new(metadata: DiscoveredScriptActor) -> Result<Self> {
        Ok(Self {
            base_factory: ScriptActorFactory::new(metadata)?,
            node_path: std::env::var("NODE_PATH").ok(),
        })
    }
}

#[async_trait]
impl ActorFactory for JavaScriptActorFactory {
    async fn create_instance(&self) -> Result<Box<dyn Actor>> {
        // JavaScript-specific initialization
        self.base_factory.create_instance().await
    }
    
    fn get_metadata(&self) -> &DiscoveredScriptActor {
        self.base_factory.get_metadata()
    }
}