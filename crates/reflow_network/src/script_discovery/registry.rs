use super::types::*;
use super::factory::{ActorFactory, ScriptActorFactory, PythonActorFactory, JavaScriptActorFactory};
use crate::actor::Actor;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::{info, debug};

/// Component type indicator
#[derive(Debug, Clone)]
pub enum ComponentType {
    Native,
    Wasm,
    Script(ScriptRuntime),
}

/// Unified component registry for all actor types
pub struct ComponentRegistry {
    /// Native Rust actors (stored as Arc for sharing)
    native_actors: HashMap<String, Arc<dyn Actor>>,
    
    /// WASM actors
    // wasm_actors: HashMap<String, WasmActorFactory>,
    
    /// Script actors
    script_actors: HashMap<String, Arc<dyn ActorFactory>>,
    
    /// Component type index
    component_index: HashMap<String, ComponentType>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            native_actors: HashMap::new(),
            script_actors: HashMap::new(),
            component_index: HashMap::new(),
        }
    }
    
    /// Register a script actor
    pub fn register_script_actor(
        &mut self,
        name: &str,
        metadata: DiscoveredScriptActor,
    ) -> Result<()> {
        let runtime = metadata.runtime.clone();
        
        // Create appropriate factory based on runtime
        let factory: Arc<dyn ActorFactory> = match runtime {
            ScriptRuntime::Python => {
                Arc::new(PythonActorFactory::new(metadata)?)
            }
            ScriptRuntime::JavaScript => {
                Arc::new(JavaScriptActorFactory::new(metadata)?)
            }
        };
        
        self.script_actors.insert(name.to_string(), factory);
        self.component_index.insert(
            name.to_string(),
            ComponentType::Script(runtime),
        );
        
        info!("Registered script actor: {} ({:?})", name, runtime);
        Ok(())
    }
    
    /// Register a native actor
    pub fn register_native_actor(
        &mut self,
        name: &str,
        actor: Arc<dyn Actor>,
    ) -> Result<()> {
        self.native_actors.insert(name.to_string(), actor);
        self.component_index.insert(
            name.to_string(),
            ComponentType::Native,
        );
        
        info!("Registered native actor: {}", name);
        Ok(())
    }
    
    /// Get an actor instance (returns Arc for native actors)
    pub async fn get_actor(&self, name: &str) -> Result<Arc<dyn Actor>> {
        match self.component_index.get(name) {
            Some(ComponentType::Native) => {
                // Return the Arc directly (no cloning needed)
                self.native_actors
                    .get(name)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("Native actor not found: {}", name))
            }
            Some(ComponentType::Script(_)) => {
                // Script actors need to be created through factories
                // For now, return error as script actors don't implement Actor trait yet
                Err(anyhow::anyhow!("Script actor instantiation not yet implemented"))
            }
            Some(ComponentType::Wasm) => {
                // TODO: Create WASM actor instance
                Err(anyhow::anyhow!("WASM actor support not yet implemented"))
            }
            None => {
                Err(anyhow::anyhow!("Component not found: {}", name))
            }
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
    
    /// Get count by type
    pub fn count_by_type(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        
        for comp_type in self.component_index.values() {
            let key = match comp_type {
                ComponentType::Native => "native",
                ComponentType::Wasm => "wasm",
                ComponentType::Script(ScriptRuntime::Python) => "python",
                ComponentType::Script(ScriptRuntime::JavaScript) => "javascript",
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
        self.registered_actors.write().insert(
            component_name.clone(),
            registered,
        );
        
        // Update indices
        self.component_index
            .write()
            .entry(component_name.clone())
            .or_insert_with(Vec::new)
            .push(namespace.clone());
        
        self.namespace_index
            .write()
            .entry(namespace)
            .or_insert_with(Vec::new)
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
    
    /// Create an actor instance
    /// Note: Script actors don't implement Actor trait yet, so this returns an error
    pub async fn create_instance(&self, name: &str) -> Result<Arc<dyn Actor>> {
        let registered = self.get(name)
            .ok_or_else(|| anyhow::anyhow!("Actor not registered: {}", name))?;
        
        // Increment instantiation count
        registered.instantiation_count.fetch_add(1, Ordering::Relaxed);
        
        // Script actors need proper Actor trait implementation
        Err(anyhow::anyhow!("Script actor instantiation not yet implemented - WebSocketScriptActor doesn't implement Actor trait"))
    }
}