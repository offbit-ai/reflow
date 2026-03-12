//! Peer mesh — wraps `DistributedNetwork` for peer-to-peer actor federation.
//!
//! This module is activated when Zeal assigns subgraphs to this node via
//! `subgraph.assign` and `peer.connect` ZIP commands. It manages:
//! - Peer-to-peer connections between Reflow nodes
//! - Remote actor proxy setup for cross-node edges
//! - Bridge lifecycle tied to execution lifetime

use std::collections::HashMap;

use anyhow::Result;
use log::{info, warn};
use reflow_network::distributed_network::{DistributedConfig, DistributedNetwork};
use tokio::sync::RwLock;

/// Manages the distributed network mesh for this Reflow node.
///
/// Created lazily when the first `subgraph.assign` command arrives from Zeal.
/// Each distributed execution gets its own `DistributedNetwork` instance.
pub struct PeerMesh {
    /// Active distributed networks keyed by execution_id.
    networks: RwLock<HashMap<String, DistributedNetwork>>,
    /// This node's ID (used as network_id in the bridge).
    node_id: String,
    /// Default bind address for peer connections.
    bind_address: String,
    /// Default bind port (auto-incremented per execution).
    base_port: u16,
}

impl PeerMesh {
    pub fn new(node_id: String, bind_address: String, base_port: u16) -> Self {
        Self {
            networks: RwLock::new(HashMap::new()),
            node_id,
            bind_address,
            base_port,
        }
    }

    /// Create a distributed network for an execution and connect to a peer.
    ///
    /// Called when Zeal sends a `peer.connect` command. If the execution
    /// doesn't have a network yet, one is created.
    pub async fn connect_peer(
        &self,
        execution_id: &str,
        peer_id: &str,
        peer_address: &str,
    ) -> Result<()> {
        let mut networks = self.networks.write().await;

        if !networks.contains_key(execution_id) {
            // Create a new distributed network for this execution
            let port = self.base_port + networks.len() as u16;
            let config = DistributedConfig {
                network_id: self.node_id.clone(),
                instance_id: format!("{}_{}", self.node_id, execution_id),
                bind_address: self.bind_address.clone(),
                bind_port: port,
                discovery_endpoints: vec![],
                auth_token: None,
                max_connections: 64,
                local_network_config: Default::default(),
                heartbeat_interval_ms: 5000,
            };

            let dist_net = DistributedNetwork::new(config).await?;
            networks.insert(execution_id.to_string(), dist_net);

            info!(
                "Created distributed network for execution {} on port {}",
                execution_id, port
            );
        }

        let dist_net = networks.get(execution_id).unwrap();

        // Connect to the peer
        dist_net.connect_to_network(peer_address).await?;
        drop(networks); // Release write lock
        info!(
            "Connected to peer {} at {} for execution {}",
            peer_id, peer_address, execution_id
        );

        Ok(())
    }

    /// Register a remote actor proxy for cross-node communication.
    pub async fn register_remote_actor(
        &self,
        execution_id: &str,
        actor_id: &str,
        remote_network_id: &str,
    ) -> Result<()> {
        let networks = self.networks.read().await;
        if let Some(dist_net) = networks.get(execution_id) {
            dist_net
                .register_remote_actor(actor_id, remote_network_id)
                .await?;
            info!(
                "Registered remote actor {}@{} for execution {}",
                actor_id, remote_network_id, execution_id
            );
        } else {
            warn!(
                "No distributed network for execution {} — cannot register remote actor",
                execution_id
            );
        }
        Ok(())
    }

    /// Shut down the distributed network for a completed execution.
    pub async fn teardown_execution(&self, execution_id: &str) {
        let mut networks = self.networks.write().await;
        if let Some(mut dist_net) = networks.remove(execution_id) {
            if let Err(e) = dist_net.shutdown().await {
                warn!(
                    "Error shutting down distributed network for {}: {}",
                    execution_id, e
                );
            }
            info!(
                "Torn down distributed network for execution {}",
                execution_id
            );
        }
    }
}
