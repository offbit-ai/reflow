use super::types::*;
use crate::actor::Actor;
use crate::websocket_rpc::{DynASBClient, DynASBConfig, WebSocketRpcClient, WebSocketScriptActor};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::info;

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
        let actor =
            WebSocketScriptActor::new(self.metadata.clone(), rpc_client, self.redis_url.clone())
                .await;

        Ok(Box::new(actor))
    }

    fn get_metadata(&self) -> &DiscoveredScriptActor {
        &self.metadata
    }
}

/// Python-specific actor factory
pub struct PythonActorFactory {
    base_factory: ScriptActorFactory,
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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

/// dynASB actor factory — deploys script code to a dynASB instance on create,
/// returns a WebSocketScriptActor that communicates via JSON-RPC 2.0.
///
/// When a graph references a script component:
///   1. Factory reads the script source from disk
///   2. Deploys to dynASB via REST (code + deps → microVM)
///   3. Returns a WebSocketScriptActor pointing at dynASB's WebSocket
///   4. Actor receives messages via the normal reflow routing
///   5. On cleanup, function is undeployed from dynASB
pub struct DynASBActorFactory {
    metadata: DiscoveredScriptActor,
    client: Arc<Mutex<DynASBClient>>,
}

impl DynASBActorFactory {
    pub fn new(metadata: DiscoveredScriptActor, client: Arc<Mutex<DynASBClient>>) -> Self {
        Self { metadata, client }
    }

    /// Create a factory with a new DynASBClient from config
    pub fn with_config(metadata: DiscoveredScriptActor, config: DynASBConfig) -> Self {
        Self {
            metadata,
            client: Arc::new(Mutex::new(DynASBClient::new(config))),
        }
    }
}

#[async_trait]
impl ActorFactory for DynASBActorFactory {
    async fn create_instance(&self) -> Result<Box<dyn Actor>> {
        let metadata = &self.metadata;

        // Read script source from the discovered file path
        let code = std::fs::read_to_string(&metadata.file_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read script source at {:?}: {}",
                metadata.file_path,
                e
            )
        })?;

        // Map ScriptRuntime to dynASB runtime string
        let runtime_str = match metadata.runtime {
            ScriptRuntime::Python => "python",
            ScriptRuntime::JavaScript => "nodejs",
        };

        // Convert dependencies list to package manifest map
        let dependencies: Option<HashMap<String, String>> =
            if metadata.workspace_metadata.dependencies.is_empty() {
                None
            } else {
                Some(
                    metadata
                        .workspace_metadata
                        .dependencies
                        .iter()
                        .map(|dep| (dep.clone(), "*".to_string()))
                        .collect(),
                )
            };

        let timeout = metadata.workspace_metadata.runtime_requirements.timeout;

        // Deploy to dynASB
        let mut client = self.client.lock().await;
        let func = client
            .deploy(
                &metadata.component,
                runtime_str,
                &code,
                &metadata.component, // handler = component name
                dependencies,
                Some(timeout),
            )
            .await?;

        info!(
            "Deployed script actor '{}' to dynASB as function '{}'",
            metadata.component, func.function_id
        );

        // Wait for the function to become ready before handing back the actor.
        // Status is tracked on DynASBClient.deployed — the ComponentRegistry
        // can query it via DynASBClient::deployed_functions() / health_check().
        client
            .wait_until_ready(
                &func.function_id,
                Duration::from_secs(timeout as u64),
                Duration::from_millis(500),
            )
            .await?;

        // Create actor connected to this deployed function
        let actor = client.create_actor(&func, metadata.clone()).await?;

        Ok(Box::new(actor))
    }

    fn get_metadata(&self) -> &DiscoveredScriptActor {
        &self.metadata
    }
}
