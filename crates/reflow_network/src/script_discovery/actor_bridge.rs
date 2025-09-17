use super::script_actor_trait::ScriptActor;
use super::types::PortDefinition;
use crate::actor::{Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, Port, MemoryState};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use std::pin::Pin;
use futures::Future;
use tracing::{debug, error, warn};

/// Bridge that adapts a ScriptActor to the native Actor trait
/// This allows script actors to be used in the network alongside native actors
pub struct ScriptActorBridge {
    script_actor: Arc<Mutex<Box<dyn ScriptActor>>>,
    metadata: super::types::ScriptActorMetadata,
    component_name: String,
    inports: Vec<PortDefinition>,
    outports: Vec<PortDefinition>,
}

impl ScriptActorBridge {
    pub fn new(script_actor: Box<dyn ScriptActor>, component_name: String) -> Self {
        let metadata = script_actor.get_metadata();
        let inports = script_actor.get_inports();
        let outports = script_actor.get_outports();
        
        Self {
            script_actor: Arc::new(Mutex::new(script_actor)),
            metadata,
            component_name,
            inports,
            outports,
        }
    }
}

impl Actor for ScriptActorBridge {
    fn get_behavior(&self) -> ActorBehavior {
        let script_actor = self.script_actor.clone();
        let component_name = self.component_name.clone();
        
        Box::new(move |mut context: ActorContext| {
            let script_actor = script_actor.clone();
            let component_name = component_name.clone();
            
            Box::pin(async move {
                debug!("Processing message in script actor: {}", component_name);
                
                // Get the input messages from context
                let inputs = context.payload;
                
                // Lock and process through the script actor
                let result = {
                    let mut actor_guard = script_actor.lock();
                    actor_guard.process(inputs).await
                };
                
                match result {
                    Ok(outputs) => {
                        // Send outputs through the context ports if any
                        if !outputs.is_empty() {
                            if let Err(e) = context.outports.0.send(outputs.clone()).await {
                                error!("Failed to send outputs: {}", e);
                                return Err(e.into());
                            }
                        }
                        // Return empty since we already sent through outports
                        Ok(HashMap::new())
                    }
                    Err(e) => {
                        error!("Script actor {} failed: {}", component_name, e);
                        Err(e)
                    }
                }
            })
        })
    }
    
    fn get_inports(&self) -> Port {
        // Create a port channel
        let (tx, rx) = flume::unbounded();
        (tx, rx)
    }
    
    fn get_outports(&self) -> Port {
        // Create a port channel
        let (tx, rx) = flume::unbounded();
        (tx, rx)
    }
    
    fn load_count(&self) -> Arc<parking_lot::Mutex<ActorLoad>> {
        Arc::new(parking_lot::Mutex::new(ActorLoad::new(0)))
    }
    
    fn create_process(
        &self,
        config: ActorConfig,
        _tracing_integration: Option<crate::tracing::TracingIntegration>,
    ) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let script_actor = self.script_actor.clone();
        let component_name = self.component_name.clone();
        let behavior = self.get_behavior();
        let inports = self.get_inports();
        let outports = self.get_outports();
        let load_count = self.load_count();
        
        Box::pin(async move {
            debug!("Starting script actor process: {}", component_name);
            
            // Initialize the script actor
            let init_result = {
                let mut actor_guard = script_actor.lock();
                actor_guard.initialize().await
            };
            if let Err(e) = init_result {
                error!("Failed to initialize script actor {}: {}", component_name, e);
                return;
            }
            
            // Main message processing loop
            loop {
                // Wait for incoming messages
                if let Ok(messages) = inports.1.recv_async().await {
                    // Increment load count
                    load_count.lock().inc();
                    
                    // Create context
                    let context = ActorContext::new(
                        messages,
                        outports.clone(),
                        Arc::new(parking_lot::Mutex::new(MemoryState::default())),
                        config.clone(),
                        load_count.clone(),
                    );
                    
                    // Process through behavior
                    if let Err(e) = behavior(context).await {
                        error!("Script actor {} processing failed: {}", component_name, e);
                    }
                    
                    // Decrement load count
                    load_count.lock().dec();
                }
            }
        })
    }
    
    fn shutdown(&self) {
        let script_actor = self.script_actor.clone();
        let component_name = self.component_name.clone();
        
        // Run cleanup in a blocking context
        let runtime = tokio::runtime::Handle::current();
        runtime.spawn(async move {
            // Try to cleanup the actor
            let cleanup_result = {
                let mut actor_guard = script_actor.lock();
                actor_guard.cleanup().await
            };
            
            match cleanup_result {
                Ok(()) => debug!("Script actor {} cleanup completed", component_name),
                Err(e) => warn!("Script actor {} cleanup failed: {}", component_name, e),
            }
        });
    }
}