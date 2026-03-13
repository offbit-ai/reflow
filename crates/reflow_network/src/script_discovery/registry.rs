use super::factory::{
    ActorFactory, DynASBActorFactory, JavaScriptActorFactory, PythonActorFactory,
    ScriptActorFactory,
};
use super::types::*;
use crate::actor::Actor;
use crate::websocket_rpc::{DynASBClient, DynASBConfig, DeploymentStatus, DynASBFunction};
use anyhow::Result;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{debug, info};

/// Component type indicator
#[derive(Debug, Clone)]
pub enum ComponentType {
    Native,
    Wasm,
    Script(ScriptRuntime),
    /// Remote script actor deployed and executed on dynASB microVMs
    DynASB,
}

/// Unified component registry for all actor types
pub struct ComponentRegistry {
    /// Native Rust actors (stored as Arc for sharing)
    native_actors: HashMap<String, Arc<dyn Actor>>,

    /// WASM actors
    // wasm_actors: HashMap<String, WasmActorFactory>,

    /// Script actors (includes both embedded and dynASB-backed)
    script_actors: HashMap<String, Arc<dyn ActorFactory>>,

    /// Component type index
    component_index: HashMap<String, ComponentType>,

    /// Shared dynASB client for remote script deployment
    dynasb_client: Option<Arc<tokio::sync::Mutex<DynASBClient>>>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            native_actors: HashMap::new(),
            script_actors: HashMap::new(),
            component_index: HashMap::new(),
            dynasb_client: None,
        }
    }

    /// Configure the dynASB backend for remote script execution
    pub fn set_dynasb_config(&mut self, config: DynASBConfig) {
        self.dynasb_client = Some(Arc::new(tokio::sync::Mutex::new(
            DynASBClient::new(config),
        )));
    }

    /// Register a script actor for remote execution on dynASB.
    /// The script source is read and deployed when the actor is instantiated.
    pub fn register_dynasb_actor(
        &mut self,
        name: &str,
        metadata: DiscoveredScriptActor,
    ) -> Result<()> {
        let client = self.dynasb_client.clone()
            .ok_or_else(|| anyhow::anyhow!(
                "dynASB not configured. Call set_dynasb_config() first."
            ))?;

        let factory: Arc<dyn ActorFactory> = Arc::new(
            DynASBActorFactory::new(metadata, client),
        );

        self.script_actors.insert(name.to_string(), factory);
        self.component_index
            .insert(name.to_string(), ComponentType::DynASB);

        info!("Registered dynASB script actor: {}", name);
        Ok(())
    }

    /// Register a script actor for embedded execution (Python/JavaScript)
    pub fn register_script_actor(
        &mut self,
        name: &str,
        metadata: DiscoveredScriptActor,
    ) -> Result<()> {
        let runtime = metadata.runtime;

        let factory: Arc<dyn ActorFactory> = match runtime {
            ScriptRuntime::Python => Arc::new(PythonActorFactory::new(metadata)?),
            ScriptRuntime::JavaScript => Arc::new(JavaScriptActorFactory::new(metadata)?),
        };

        self.script_actors.insert(name.to_string(), factory);
        self.component_index
            .insert(name.to_string(), ComponentType::Script(runtime));

        info!("Registered script actor: {} ({:?})", name, runtime);
        Ok(())
    }

    /// Register a native actor
    pub fn register_native_actor(&mut self, name: &str, actor: Arc<dyn Actor>) -> Result<()> {
        self.native_actors.insert(name.to_string(), actor);
        self.component_index
            .insert(name.to_string(), ComponentType::Native);

        info!("Registered native actor: {}", name);
        Ok(())
    }

    /// Get an actor instance — creates via factory for script/dynASB actors
    pub async fn get_actor(&self, name: &str) -> Result<Arc<dyn Actor>> {
        match self.component_index.get(name) {
            Some(ComponentType::Native) => {
                self.native_actors
                    .get(name)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Native actor not found: {}", name))
            }
            Some(ComponentType::Script(_)) | Some(ComponentType::DynASB) => {
                // Create actor instance via factory (deploys to dynASB if DynASB type)
                let factory = self.script_actors.get(name)
                    .ok_or_else(|| anyhow::anyhow!("Script actor factory not found: {}", name))?;
                let actor = factory.create_instance().await?;
                Ok(Arc::from(actor))
            }
            Some(ComponentType::Wasm) => {
                Err(anyhow::anyhow!("WASM actor support not yet implemented"))
            }
            None => Err(anyhow::anyhow!("Component not found: {}", name)),
        }
    }

    /// Check if a component exists
    pub fn has_component(&self, name: &str) -> bool {
        self.component_index.contains_key(name)
    }

    /// Get component type
    pub fn get_component_type(&self, name: &str) -> Option<&ComponentType> {
        self.component_index.get(name)
    }

    /// List all registered components
    pub fn list_components(&self) -> Vec<ComponentInfo> {
        self.component_index
            .iter()
            .map(|(name, comp_type)| ComponentInfo {
                name: name.clone(),
                component_type: comp_type.clone(),
            })
            .collect()
    }

    /// Get script actor metadata
    pub fn get_script_metadata(&self, name: &str) -> Option<&DiscoveredScriptActor> {
        self.script_actors
            .get(name)
            .map(|factory| factory.get_metadata())
    }

    /// Get total component count
    pub fn total_count(&self) -> usize {
        self.component_index.len()
    }

    /// Query deployment status for a dynASB-backed component.
    ///
    /// Delegates to `DynASBClient.deployed_functions()` — the single source
    /// of truth for deployment metadata. Returns `None` for non-dynASB components
    /// or if dynASB is not configured.
    pub async fn get_deployment_status(&self, name: &str) -> Option<DynASBFunction> {
        if !matches!(self.component_index.get(name), Some(ComponentType::DynASB)) {
            return None;
        }

        let client = self.dynasb_client.as_ref()?;
        let guard = client.lock().await;
        guard.deployed_functions()
            .values()
            .find(|f| f.name == name)
            .cloned()
    }

    /// Run a health check for a dynASB-backed component and return updated status.
    pub async fn check_deployment_health(&self, name: &str) -> Option<DeploymentStatus> {
        if !matches!(self.component_index.get(name), Some(ComponentType::DynASB)) {
            return None;
        }

        let client = self.dynasb_client.as_ref()?;
        let mut guard = client.lock().await;

        // Find the function_id for this component name
        let function_id = guard.deployed_functions()
            .values()
            .find(|f| f.name == name)
            .map(|f| f.function_id.clone())?;

        guard.health_check(&function_id).await.ok()
    }

    /// Get deployment metadata for a dynASB component as a flat map,
    /// suitable for merging into `GraphNode.metadata`.
    pub async fn get_deployment_metadata(&self, name: &str) -> Option<HashMap<String, serde_json::Value>> {
        if !matches!(self.component_index.get(name), Some(ComponentType::DynASB)) {
            return None;
        }

        let client = self.dynasb_client.as_ref()?;
        let guard = client.lock().await;

        let function_id = guard.deployed_functions()
            .values()
            .find(|f| f.name == name)
            .map(|f| f.function_id.clone())?;

        Some(guard.deployment_metadata(&function_id))
    }

    /// Get count by type
    pub fn count_by_type(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();

        for comp_type in self.component_index.values() {
            let key = match comp_type {
                ComponentType::Native => "native",
                ComponentType::Wasm => "wasm",
                ComponentType::Script(ScriptRuntime::Python) => "python",
                ComponentType::Script(ScriptRuntime::JavaScript) => "javascript",
                ComponentType::DynASB => "dynasb",
            };

            *counts.entry(key.to_string()).or_insert(0) += 1;
        }

        counts
    }
}

/// Component information
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub name: String,
    pub component_type: ComponentType,
}

/// Registered actor information
#[derive(Clone)]
pub struct RegisteredActor {
    pub discovery_info: DiscoveredScriptActor,
    pub factory: Arc<dyn ActorFactory>,
    pub registration_time: chrono::DateTime<chrono::Utc>,
    pub instantiation_count: Arc<AtomicUsize>,
}

/// Actor registry for managing discovered actors
pub struct ActorRegistry {
    registered_actors: Arc<RwLock<HashMap<String, RegisteredActor>>>,
    component_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
    namespace_index: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl Default for ActorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorRegistry {
    pub fn new() -> Self {
        Self {
            registered_actors: Arc::new(RwLock::new(HashMap::new())),
            component_index: Arc::new(RwLock::new(HashMap::new())),
            namespace_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a discovered actor
    pub fn register(&mut self, actor: DiscoveredScriptActor) -> Result<()> {
        let component_name = actor.component.clone();
        let namespace = actor.workspace_metadata.namespace.clone();

        // Create factory
        let factory = Arc::new(ScriptActorFactory::new(actor.clone())?);

        // Create registered actor
        let registered = RegisteredActor {
            discovery_info: actor,
            factory,
            registration_time: chrono::Utc::now(),
            instantiation_count: Arc::new(AtomicUsize::new(0)),
        };

        // Store in registry
        self.registered_actors
            .write()
            .insert(component_name.clone(), registered);

        // Update indices
        self.component_index
            .write()
            .entry(component_name.clone())
            .or_default()
            .push(namespace.clone());

        self.namespace_index
            .write()
            .entry(namespace)
            .or_default()
            .push(component_name.clone());

        debug!("Registered actor: {}", component_name);
        Ok(())
    }

    /// Get a registered actor
    pub fn get(&self, name: &str) -> Option<RegisteredActor> {
        self.registered_actors.read().get(name).cloned()
    }

    /// List actors in a namespace
    pub fn list_by_namespace(&self, namespace: &str) -> Vec<String> {
        self.namespace_index
            .read()
            .get(namespace)
            .cloned()
            .unwrap_or_default()
    }

    /// Create an actor instance via its factory
    pub async fn create_instance(&self, name: &str) -> Result<Arc<dyn Actor>> {
        let registered = self
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Actor not registered: {}", name))?;

        registered
            .instantiation_count
            .fetch_add(1, Ordering::Relaxed);

        let actor = registered.factory.create_instance().await?;
        Ok(Arc::from(actor))
    }
}
