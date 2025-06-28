use crate::{bridge::RemoteConnection, message::Message, network::Network};
use anyhow::Result;
use futures::SinkExt;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, pin::Pin, sync::Arc};

#[derive(Clone)]
pub struct MessageRouter {
    remote_actor_registry: Arc<RwLock<HashMap<String, RemoteActorInfo>>>,
    connection_pool: Arc<RwLock<HashMap<String, RemoteConnection>>>,
    local_network: Arc<RwLock<Option<Arc<RwLock<Network>>>>>,
    local_network_id: Arc<RwLock<String>>,
}

unsafe impl Sync for MessageRouter {}
unsafe impl Send for MessageRouter {}

#[derive(Debug, Clone)]
pub struct RemoteActorInfo {
    pub actor_id: String,
    pub network_id: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteMessage {
    pub message_id: String,
    pub source_network: String,
    pub source_actor: String,
    pub target_network: String,
    pub target_actor: String,
    pub target_port: String,
    pub payload: Message,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl MessageRouter {
    pub fn new() -> Self {
        MessageRouter {
            remote_actor_registry: Arc::new(RwLock::new(HashMap::new())),
            connection_pool: Arc::new(RwLock::new(HashMap::new())),
            local_network: Arc::new(RwLock::new(None)),
            local_network_id: Arc::new(RwLock::new("local".to_string())),
        }
    }

    pub fn with_connection_pool(connections: Arc<RwLock<HashMap<String, RemoteConnection>>>) -> Self {
        MessageRouter {
            remote_actor_registry: Arc::new(RwLock::new(HashMap::new())),
            connection_pool: connections,
            local_network: Arc::new(RwLock::new(None)),
            local_network_id: Arc::new(RwLock::new("local".to_string())),
        }
    }

    pub fn set_local_network(&self, network: Arc<RwLock<Network>>, network_id: String) {
        *self.local_network.write() = Some(network);
        *self.local_network_id.write() = network_id;
    }

    pub fn set_connection_pool(&self, connections: Arc<RwLock<HashMap<String, RemoteConnection>>>) {
        // Note: connection_pool is already shared via Arc, so we'll reference it directly
        // This method is kept for API compatibility but doesn't need to reassign
    }

    pub async fn route_message(
        &self,
        network_id: &str,
        actor_id: &str,
        port: &str,
        message: Message,
    ) -> Result<()> {
        let source_network =  self.get_local_network_id().await?;
        
        tracing::info!("📨 ROUTER: Routing message from {} to {}::{} on port {}", 
                      source_network, network_id, actor_id, port);
        
        // Create remote message
        let remote_message = RemoteMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            source_network,
            source_actor: "".to_string(), // TODO: Get from context
            target_network: network_id.to_string(),
            target_actor: actor_id.to_string(),
            target_port: port.to_string(),
            payload: message,
            timestamp: chrono::Utc::now(),
        };

        // Find connection for target network
        let connection = {
            let connections = self.connection_pool.read();
            tracing::info!("🔍 ROUTER: Available connections: {:?}", 
                          connections.keys().collect::<Vec<_>>());
            connections.get(network_id).cloned()
        };
        
        if let Some(connection) = connection {
            tracing::info!("✅ ROUTER: Found connection for network {}, sending message {}", 
                          network_id, remote_message.message_id);
            match self.send_over_connection(&connection, remote_message).await {
                Ok(_) => {
                    tracing::info!("✅ ROUTER: Successfully sent message over connection");
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("❌ ROUTER: Failed to send message over connection: {}", e);
                    Err(e)
                }
            }
        } else {
            tracing::error!("❌ ROUTER: No connection to network: {}", network_id);
            return Err(anyhow::anyhow!("No connection to network: {}", network_id))
        }
    }

    pub async fn handle_incoming_message(
        &self,
        message: RemoteMessage,
    ) -> Result<(), anyhow::Error> {
        // Route to local network
        tracing::info!(
            "🎯 ROUTER: Routing message from {} to local actor: {} port: {}",
            message.source_network,
            message.target_actor,
            message.target_port
        );

        // Send message to local network
        let local_network_guard = self.local_network.read();
        if let Some(ref local_network_arc) = *local_network_guard {
            let network = local_network_arc.read();
            
            tracing::info!("🔍 ROUTER: Sending to local network, available actors: {:?}", 
                          network.actors.keys().collect::<Vec<_>>());
            tracing::info!("🔍 ROUTER: Available nodes: {:?}", 
                          network.nodes.keys().collect::<Vec<_>>());
            
            match network.send_to_actor(&message.target_actor, &message.target_port, message.payload) {
                Ok(_) => {
                    tracing::info!("✅ ROUTER: Successfully routed message to local actor {}", message.target_actor);
                }
                Err(e) => {
                    tracing::error!("❌ ROUTER: Failed to route message to local actor {}: {}", message.target_actor, e);
                    return Err(e);
                }
            }
        } else {
            tracing::error!("❌ ROUTER: No local network configured");
            return Err(anyhow::anyhow!("No local network configured"));
        }

        Ok(())
    }

    async fn send_over_connection(
        &self,
        connection: &RemoteConnection,
        message: RemoteMessage,
    ) -> Result<()> {
        tracing::info!("🔗 ROUTER: Serializing message {}", message.message_id);
        let serialized = match serde_json::to_string(&message) {
            Ok(s) => {
                tracing::info!("✅ ROUTER: Serialized message {} bytes", s.len());
                s
            }
            Err(e) => {
                tracing::error!("❌ ROUTER: Failed to serialize message: {}", e);
                return Err(e.into());
            }
        };

        tracing::info!("📡 ROUTER: Sending message over WebSocket to {}", connection.network_id);
        
        // Send over WebSocket using the ConnectionWebSocket's send method
        match connection.websocket.send(tokio_tungstenite::tungstenite::Message::Text(
            serialized.into(),
        )).await {
            Ok(_) => {
                tracing::info!("✅ ROUTER: Successfully sent message {} over WebSocket", message.message_id);
                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ ROUTER: Failed to send message over WebSocket: {}", e);
                Err(e.into())
            }
        }
    }

    async fn get_local_network_id(&self) -> Result<String> {
        Ok(self.local_network_id.read().clone())
    }

    pub async fn register_remote_actor(
        &self,
        actor_id: &str,
        remote_network_id: &str,
    ) -> Result<(), anyhow::Error> {
        let remote_info = RemoteActorInfo {
            actor_id: actor_id.to_string(),
            network_id: remote_network_id.to_string(),
            capabilities: vec!["actor_messaging".to_string()], // TODO: Get from discovery
        };
        
        self.remote_actor_registry
            .write()
            .insert(actor_id.to_string(), remote_info);
            
        tracing::info!("Registered remote actor {} from network {}", actor_id, remote_network_id);
        Ok(())
    }
}
