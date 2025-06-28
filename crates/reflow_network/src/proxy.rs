use parking_lot::Mutex;

use crate::{
    actor::{Actor, ActorContext, ActorLoad, MemoryState, Port},
    bridge::NetworkBridge,
    message::Message,
};
use std::{collections::HashMap, sync::Arc};

/// Proxy actor that represents a remote actor in the local network
pub struct RemoteActorProxy {
    remote_network_id: String,
    remote_actor_id: String,
    bridge: Arc<NetworkBridge>,
    inports: Port, 
    outports: Port,
    load: Arc<Mutex<ActorLoad>>
}

impl RemoteActorProxy {
    pub fn new(
        remote_network_id: String,
        remote_actor_id: String,
        bridge: Arc<NetworkBridge>,
    ) -> Self {
        RemoteActorProxy {
            remote_network_id,
            remote_actor_id,
            bridge,
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load:  Arc::new(Mutex::new(ActorLoad::new(0)))
        }
    }
}

impl Actor for RemoteActorProxy {
    fn get_behavior(&self) -> crate::actor::ActorBehavior {
        let bridge = self.bridge.clone();
        let remote_network_id = self.remote_network_id.clone();
        let remote_actor_id = self.remote_actor_id.clone();
        Box::new(move |context| {
            let payload = context.get_payload().clone();
            let bridge = bridge.clone();
            let remote_network_id = remote_network_id.clone();
            let remote_actor_id = remote_actor_id.clone();
            Box::pin(async move {
                let mut result = HashMap::new();
                
                // Forward all input messages to remote actor
                for (port, message) in payload.iter() {
                    tracing::info!("[PROXY] Forwarding message to {}::{} on port {}", 
                        remote_network_id, remote_actor_id, port);
                    
                    match bridge.send_remote_message(
                        &remote_network_id,
                        &remote_actor_id,
                        port,
                        message.clone(),
                    ).await {
                        Ok(_) => {
                            tracing::info!("[PROXY] ✅ Successfully sent message to remote actor");
                            // Create a response indicating successful forwarding
                            result.insert(
                                "status".to_string(), 
                                Message::String(Arc::new("forwarded".to_string()))
                            );
                        }
                        Err(e) => {
                            tracing::error!("[PROXY] ❌ Failed to send message to remote actor: {}", e);
                            result.insert(
                                "error".to_string(), 
                                Message::String(Arc::new(format!("Failed to forward: {}", e)))
                            );
                        }
                    }
                }

                Ok(result)
            })
        })
    }

    fn get_inports(&self) -> Port {
       self.inports.clone()
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

     fn load_count(&self) -> Arc<parking_lot::Mutex<ActorLoad>> {
       self.load.clone()
    }

    fn create_process(
        &self,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        use futures::StreamExt;

        let behavior = self.get_behavior();
        let (_, receiver) = self.get_inports();
        let outports = self.get_outports();
        let load = self.load_count();
        

        Box::pin(async move {
           
            loop {
                if let Some(packet) = receiver.clone().stream().next().await {
                    // Run the behavior function
                        let context = ActorContext::new(
                            packet,
                            outports.clone(),
                            Arc::new(Mutex::new(MemoryState::default())),
                            HashMap::new(),
                            load.clone(),
                        );

                        if let Ok(result) = behavior(context).await {
                            if !result.is_empty() {
                                let _ = outports
                                    .0
                                    .send(result)
                                    .expect("Expected to send message via outport");
                                load.lock().reset();
                            }
                        }
                }
            }
        })
    }
}
