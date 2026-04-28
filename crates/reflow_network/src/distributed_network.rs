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
    actor::message::Message,
    bridge::NetworkBridge,
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
        self.local_network.clone().write().start()?;

        Ok(())
    }

    /// Resolve component specifications on the local network.
    /// See [`Network::resolve_components`] for details.
    #[allow(clippy::await_holding_lock)]
    pub async fn resolve_components(
        &self,
        registry: &crate::script_discovery::registry::ComponentRegistry,
    ) -> Result<(), anyhow::Error> {
        self.local_network
            .write()
            .resolve_components(registry)
            .await
    }

    pub async fn register_remote_actor(
        &self,
        actor_id: &str,
        remote_network_id: &str,
    ) -> Result<(), anyhow::Error> {
        self.register_remote_actor_with_capabilities(actor_id, remote_network_id, None)
            .await
    }

    pub async fn register_remote_actor_with_capabilities(
        &self,
        actor_id: &str,
        remote_network_id: &str,
        capabilities: Option<Vec<String>>,
    ) -> Result<(), anyhow::Error> {
        // Register with router
        self.bridge
            .register_remote_actor(actor_id, remote_network_id, capabilities)
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
            let proxy_arc: Arc<dyn crate::actor::Actor> = Arc::new(proxy);
            network.register_actor_arc(&proxy_name, Arc::clone(&proxy_arc))?;

            // Add as a node and start the proxy actor process
            network.add_node(
                &proxy_name,
                &proxy_name,
                Some(HashMap::from([(
                    "remote_actor_proxy".to_string(),
                    serde_json::Value::Bool(true),
                )])),
            )?;

            // Track in `initialized_actors` so `send_to_actor` and the
            // connector machinery can find the proxy. Without this the
            // proxy is reachable via direct `bridge.send_remote_message`
            // but not via the local-network paths that callers expect
            // (a proxy is meant to be wired into the graph just like
            // any other node).
            network
                .initialized_actors
                .insert(proxy_name.clone(), Arc::clone(&proxy_arc));

            // Start the proxy actor process
            let actor_config =
                ActorConfig::from_node(network.nodes.get(&proxy_name).cloned().unwrap())?;
            let process =
                proxy_arc.create_process(actor_config, network.tracing_integration.clone());
            tokio::spawn(process);
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
        source_actor: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        self.bridge
            .send_remote_message(network_id, actor_id, port, message, source_actor)
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
        metadata: Option<HashMap<String, Value>>,
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
