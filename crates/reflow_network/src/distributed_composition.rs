//! Distributed Graph Composition
//!
//! Bridges the graph composition system (multi_graph) with the distributed network layer,
//! enabling graphs to be composed across network boundaries with automatic proxy actor creation.
//!
//! Key concepts:
//! - **RemoteGraphSource**: Load a graph from a connected remote network instance
//! - **DistributedNamespace**: Three-level namespace `network_id/namespace/process`
//! - **Cross-network connections**: Automatically bridged via RemoteActorProxy actors
//! - **Execution targets**: Metadata declaring which network should execute a subgraph

use crate::{
    bridge::NetworkBridge,
    distributed_network::DistributedNetwork,
    graph::types::{GraphConnection, GraphEdge, GraphExport, GraphNode},
    multi_graph::{
        CompositionConnection, CompositionEndpoint, GraphComposition, GraphLoader, GraphSource,
        GraphComposer, GraphMetadata, NamespaceConflictPolicy,
    },
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Configuration for a graph that should be loaded from or executed on a remote network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteGraphConfig {
    /// The network_id of the remote network that owns this graph
    pub network_id: String,
    /// The graph source on the remote side (typically a name or path)
    pub graph_name: String,
    /// If set, this graph should be executed on the specified remote network.
    /// If None, the graph is loaded remotely but executed locally.
    pub execution_target: Option<String>,
}

/// A cross-network composition connection that specifies full distributed endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedConnection {
    pub from: DistributedEndpoint,
    pub to: DistributedEndpoint,
    pub metadata: Option<HashMap<String, Value>>,
}

/// A fully qualified endpoint in the distributed namespace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedEndpoint {
    /// The network that hosts this process. None means local.
    pub network_id: Option<String>,
    /// The namespace/process qualified name (e.g., "data_pipeline/transformer")
    pub process: String,
    /// The port name
    pub port: String,
    pub index: Option<usize>,
}

impl DistributedEndpoint {
    /// Returns the fully qualified name: "network_id/namespace/process" or "namespace/process" if local.
    pub fn qualified_name(&self) -> String {
        match &self.network_id {
            Some(nid) => format!("{}/{}", nid, self.process),
            None => self.process.clone(),
        }
    }

    /// Returns true if this endpoint refers to a remote network.
    pub fn is_remote(&self) -> bool {
        self.network_id.is_some()
    }
}

/// Describes a complete distributed graph composition.
///
/// Extends the local `GraphComposition` with:
/// - Remote graph sources that can be fetched from other network instances
/// - Cross-network connections that will be automatically bridged via proxies
/// - Execution target metadata per subgraph
#[derive(Debug, Clone)]
pub struct DistributedGraphComposition {
    /// Local graph sources (composed in-process)
    pub local_sources: Vec<GraphSource>,

    /// Remote graph configs (fetched from remote networks)
    pub remote_sources: Vec<RemoteGraphConfig>,

    /// Local connections (within the same network)
    pub local_connections: Vec<CompositionConnection>,

    /// Cross-network connections (will create proxy actors at boundaries)
    pub distributed_connections: Vec<DistributedConnection>,

    /// Composed graph properties
    pub properties: HashMap<String, Value>,

    /// Execution targets: graph_name -> network_id where the graph should run
    pub execution_targets: HashMap<String, String>,
}

/// Resolves distributed endpoints into a unified namespace.
///
/// Naming convention: `{network_id}/{namespace}/{process}`
/// - Local processes: `local/{namespace}/{process}`
/// - Remote processes: `{remote_network_id}/{namespace}/{process}`
pub struct DistributedNamespaceResolver {
    local_network_id: String,
    /// Maps fully qualified names to their network location
    process_locations: HashMap<String, ProcessLocation>,
}

#[derive(Debug, Clone)]
pub struct ProcessLocation {
    pub network_id: String,
    pub namespace: String,
    pub process_name: String,
    pub is_local: bool,
}

impl DistributedNamespaceResolver {
    pub fn new(local_network_id: &str) -> Self {
        DistributedNamespaceResolver {
            local_network_id: local_network_id.to_string(),
            process_locations: HashMap::new(),
        }
    }

    /// Register a local graph's processes under the distributed namespace.
    pub fn register_local_graph(&mut self, namespace: &str, graph: &GraphExport) {
        for (process_name, _) in &graph.processes {
            let qualified = format!("{}/{}/{}", self.local_network_id, namespace, process_name);
            self.process_locations.insert(
                qualified,
                ProcessLocation {
                    network_id: self.local_network_id.clone(),
                    namespace: namespace.to_string(),
                    process_name: process_name.clone(),
                    is_local: true,
                },
            );
        }
    }

    /// Register a remote graph's processes under the distributed namespace.
    pub fn register_remote_graph(&mut self, network_id: &str, namespace: &str, graph: &GraphExport) {
        for (process_name, _) in &graph.processes {
            let qualified = format!("{}/{}/{}", network_id, namespace, process_name);
            self.process_locations.insert(
                qualified,
                ProcessLocation {
                    network_id: network_id.to_string(),
                    namespace: namespace.to_string(),
                    process_name: process_name.clone(),
                    is_local: false,
                },
            );
        }
    }

    /// Resolve a distributed endpoint to its process location.
    pub fn resolve(&self, endpoint: &DistributedEndpoint) -> Option<&ProcessLocation> {
        let qualified = endpoint.qualified_name();
        self.process_locations.get(&qualified)
    }

    /// Find all connections that cross network boundaries (require proxy actors).
    pub fn find_cross_network_connections(
        &self,
        connections: &[DistributedConnection],
    ) -> Vec<CrossNetworkEdge> {
        let mut edges = Vec::new();

        for conn in connections {
            let from_location = self.resolve(&conn.from);
            let to_location = self.resolve(&conn.to);

            match (from_location, to_location) {
                (Some(from_loc), Some(to_loc)) if from_loc.network_id != to_loc.network_id => {
                    edges.push(CrossNetworkEdge {
                        from_network: from_loc.network_id.clone(),
                        to_network: to_loc.network_id.clone(),
                        from_process: conn.from.process.clone(),
                        to_process: conn.to.process.clone(),
                        from_port: conn.from.port.clone(),
                        to_port: conn.to.port.clone(),
                        metadata: conn.metadata.clone(),
                    });
                }
                _ => {}
            }
        }

        edges
    }
}

/// Represents a connection that crosses network boundaries and needs a proxy actor.
#[derive(Debug, Clone)]
pub struct CrossNetworkEdge {
    pub from_network: String,
    pub to_network: String,
    pub from_process: String,
    pub to_process: String,
    pub from_port: String,
    pub to_port: String,
    pub metadata: Option<HashMap<String, Value>>,
}

impl CrossNetworkEdge {
    /// Generate the proxy actor name that will bridge this connection.
    /// The proxy lives on the sender's network and forwards to the receiver's network.
    pub fn proxy_actor_name(&self) -> String {
        format!("{}@{}", self.to_process, self.to_network)
    }
}

/// Plans the execution of a distributed graph composition.
///
/// Given a `DistributedGraphComposition`, this produces:
/// 1. A local `GraphComposition` for each network (with proxy actors at boundaries)
/// 2. A list of proxy actors that need to be created
/// 3. A mapping of which graphs run where
#[derive(Debug)]
pub struct DistributedCompositionPlan {
    /// The local composition to execute on this network
    pub local_composition: GraphComposition,
    /// Proxy actors to create (proxy_name -> remote_network_id, remote_actor_id)
    pub proxy_actors: Vec<ProxyActorSpec>,
    /// Cross-network edges that the proxies will bridge
    pub cross_network_edges: Vec<CrossNetworkEdge>,
    /// Which graphs are delegated to remote networks for execution
    pub remote_executions: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ProxyActorSpec {
    /// Local name for the proxy actor
    pub proxy_name: String,
    /// The remote network this proxy forwards to
    pub remote_network_id: String,
    /// The remote actor this proxy represents
    pub remote_actor_id: String,
}

/// Plans a distributed composition without executing it.
///
/// This is a pure planning step that determines what needs to happen on each network.
/// It derives cross-network edges directly from the distributed connections (which carry
/// explicit network_id on each endpoint) without requiring graph registration.
pub fn plan_distributed_composition(
    composition: &DistributedGraphComposition,
    local_network_id: &str,
) -> DistributedCompositionPlan {
    // Derive cross-network edges directly from distributed connections
    let mut cross_network_edges = Vec::new();
    for conn in &composition.distributed_connections {
        let from_net = conn.from.network_id.as_deref().unwrap_or(local_network_id);
        let to_net = conn.to.network_id.as_deref().unwrap_or(local_network_id);

        if from_net != to_net {
            cross_network_edges.push(CrossNetworkEdge {
                from_network: from_net.to_string(),
                to_network: to_net.to_string(),
                from_process: conn.from.process.clone(),
                to_process: conn.to.process.clone(),
                from_port: conn.from.port.clone(),
                to_port: conn.to.port.clone(),
                metadata: conn.metadata.clone(),
            });
        }
    }

    // Determine proxy actors needed on the local network
    let mut proxy_actors = Vec::new();
    let mut seen_proxies = std::collections::HashSet::new();

    for edge in &cross_network_edges {
        // If the source is local and destination is remote, we need a proxy on the local side
        if edge.from_network == local_network_id && edge.to_network != local_network_id {
            let proxy_name = edge.proxy_actor_name();
            if seen_proxies.insert(proxy_name.clone()) {
                proxy_actors.push(ProxyActorSpec {
                    proxy_name,
                    remote_network_id: edge.to_network.clone(),
                    remote_actor_id: edge.to_process.clone(),
                });
            }
        }
    }

    // Build the local composition: only local sources + connections that stay local
    // Cross-network connections are rewritten to point to local proxy actors
    let mut local_connections = composition.local_connections.clone();

    for edge in &cross_network_edges {
        if edge.from_network == local_network_id {
            // Rewrite: from_process:from_port -> proxy_actor:from_port
            local_connections.push(CompositionConnection {
                from: CompositionEndpoint {
                    process: edge.from_process.clone(),
                    port: edge.from_port.clone(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: edge.proxy_actor_name(),
                    port: edge.to_port.clone(),
                    index: None,
                },
                metadata: edge.metadata.clone(),
            });
        }
    }

    // Determine which graphs are delegated to remote execution
    let remote_executions = composition.execution_targets.clone();

    let local_composition = GraphComposition {
        sources: composition.local_sources.clone(),
        connections: local_connections,
        shared_resources: vec![],
        properties: composition.properties.clone(),
        case_sensitive: None,
        metadata: None,
    };

    DistributedCompositionPlan {
        local_composition,
        proxy_actors,
        cross_network_edges,
        remote_executions,
    }
}

/// Executes a distributed composition plan on a DistributedNetwork.
///
/// This:
/// 1. Creates proxy actors for cross-network edges
/// 2. Composes and starts the local graph
pub async fn execute_distributed_plan(
    network: &DistributedNetwork,
    plan: &DistributedCompositionPlan,
) -> Result<()> {
    // Step 1: Create proxy actors for cross-network edges
    for proxy_spec in &plan.proxy_actors {
        tracing::info!(
            "Creating proxy actor '{}' -> {}::{}",
            proxy_spec.proxy_name,
            proxy_spec.remote_network_id,
            proxy_spec.remote_actor_id
        );
        network
            .register_remote_actor(&proxy_spec.remote_actor_id, &proxy_spec.remote_network_id)
            .await?;
    }

    // Step 2: The local composition can now be composed and executed
    // by the caller using GraphComposer with plan.local_composition
    tracing::info!(
        "Distributed plan executed: {} proxy actors created, {} cross-network edges",
        plan.proxy_actors.len(),
        plan.cross_network_edges.len(),
    );

    Ok(())
}
