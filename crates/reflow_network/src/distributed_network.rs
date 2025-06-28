// ┌─────────────────────────────────────────────────────────────────────┐
// │                    Distributed Reflow Network                      │
// ├─────────────────────────────────────────────────────────────────────┤
// │  Instance A (Server)           │  Instance B (Client)               │
// │ ┌─────────────────────────────┐ │ ┌─────────────────────────────────┐ │
// │ │ Local Network               │ │ │ Local Network                   │ │
// │ │ ├─ Actor A1 ─┐              │ │ │ ├─ Actor B1 ─┐                  │ │
// │ │ ├─ Actor A2 ─┤              │ │ │ ├─ Actor B2 ─┤                  │ │
// │ │ └─ Actor A3 ─┘              │ │ │ └─ Actor B3 ─┘                  │ │
// │ └─────────────────────────────┘ │ └─────────────────────────────────┘ │
// │            │                    │                    │                │
// │ ┌─────────────────────────────┐ │ ┌─────────────────────────────────┐ │
// │ │ Network Bridge              │◄─┤ │ Network Bridge                  │ │
// │ │ ├─ Discovery Service        │ │ │ ├─ Discovery Service             │ │
// │ │ ├─ Message Router           │ │ │ ├─ Message Router                │ │
// │ │ ├─ Connection Manager       │ │ │ ├─ Connection Manager            │ │
// │ │ └─ Remote Actor Proxy       │ │ │ └─ Remote Actor Proxy            │ │
// │ └─────────────────────────────┘ │ └─────────────────────────────────┘ │
// │            │                    │                    │                │
// │ ┌─────────────────────────────┐ │ ┌─────────────────────────────────┐ │
// │ │ Transport Layer             │◄─┤ │ Transport Layer                 │ │
// │ │ ├─ WebSocket/TCP Server     │ │ │ ├─ WebSocket/TCP Client          │ │
// │ │ ├─ Protocol Handler         │ │ │ ├─ Protocol Handler              │ │
// │ │ └─ Serialization            │ │ │ └─ Serialization                 │ │
// │ └─────────────────────────────┘ │ └─────────────────────────────────┘ │
// └─────────────────────────────────────────────────────────────────────┘

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    actor::ActorConfig,
    bridge::NetworkBridge,
    message::Message,
    network::{Network, NetworkConfig},
};

#[derive(Clone)]
pub struct DistributedNetwork {
    // Existing local network
    local_network: Arc<RwLock<Network>>,

    // Network bridge for distributed communication
    bridge: Arc<NetworkBridge>,

    // Network identity and configuration
    config: DistributedConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConfig {
    pub network_id: String,
    pub instance_id: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub discovery_endpoints: Vec<String>,
    pub auth_token: Option<String>,
    pub max_connections: usize,
    pub heartbeat_interval_ms: u64,
    pub local_network_config: NetworkConfig,
}

impl DistributedNetwork {
    pub async fn new(config: DistributedConfig) -> Result<Self, anyhow::Error> {
        let local_network = Arc::new(RwLock::new(Network::new(
            config.local_network_config.clone(),
        )));
        let bridge = Arc::new(NetworkBridge::new(config.clone()).await?);

        Ok(DistributedNetwork {
            local_network,
            bridge,
            config,
        })
    }

    pub async fn start(&mut self) -> Result<(), anyhow::Error> {
       

        // Start distributed bridge
        self.bridge.start(self.local_network.clone()).await?;

         // Start local network
        self.local_network.clone().write().start().await?;

        Ok(())
    }

    pub async fn register_remote_actor(
        &self,
        actor_id: &str,
        remote_network_id: &str,
    ) -> Result<(), anyhow::Error> {
        // Register with router
        self.bridge
            .register_remote_actor(actor_id, remote_network_id)
            .await?;

        // Create proxy actor in local network
        let proxy = crate::proxy::RemoteActorProxy::new(
            remote_network_id.to_string(),
            actor_id.to_string(),
            self.bridge.clone(),
        );

        // Register proxy in local network with a unique name
        let proxy_name = format!("{}@{}", actor_id, remote_network_id);
        {
            let mut network = self.local_network.write();
            network.register_actor(&proxy_name, proxy)?;

            // Add as a node and start the proxy actor process
            network.add_node(&proxy_name, &proxy_name, Some(HashMap::from([
                ("remote_actor_proxy".to_string(), serde_json::Value::Bool(true)),
            ])))?;

            // Start the proxy actor process
            if let Some(actor_impl) = network.actors.get(&proxy_name) {
                let actor_config =
                    ActorConfig::from_node(network.nodes.get(&proxy_name).cloned().unwrap())?;
                let process = actor_impl.create_process(actor_config);
                tokio::spawn(process);
            }
        }

        tracing::info!(
            "Created and started proxy actor '{}' for remote actor '{}' in network '{}'",
            proxy_name,
            actor_id,
            remote_network_id
        );

        Ok(())
    }

    pub async fn send_to_remote_actor(
        &self,
        network_id: &str,
        actor_id: &str,
        port: &str,
        message: Message,
    ) -> Result<(), anyhow::Error> {
        self.bridge
            .send_remote_message(network_id, actor_id, port, message)
            .await
    }

    pub async fn connect_to_network(&self, endpoint: &str) -> Result<(), anyhow::Error> {
        self.bridge.connect_to_network(endpoint).await
    }

    pub fn get_local_network(&self) -> Arc<RwLock<Network>> {
        self.local_network.clone()
    }

    /// Register a local actor with the distributed network
    pub fn register_local_actor<T: crate::actor::Actor + 'static>(
        &self,
        actor_id: &str,
        actor: T,
        metadata: Option<HashMap<String, Value>>
    ) -> Result<(), anyhow::Error> {
        let mut network = self.local_network.write();
        // Register the actor
        network.register_actor(actor_id, actor)?;

        // Add as a node in the network and start it
        network.add_node(actor_id, actor_id, metadata)?;

        Ok(())
    }

    /// Get network configuration
    pub fn get_config(&self) -> &DistributedConfig {
        &self.config
    }

    /// Shutdown the distributed network
    pub async fn shutdown(&mut self) -> Result<(), anyhow::Error> {
        tracing::info!(
            "Shutting down distributed network: {}",
            self.config.network_id
        );

        // Shutdown bridge first
        self.bridge.shutdown().await?;

        // Shutdown local network
        self.local_network.write().shutdown();

        Ok(())
    }
}
