use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;

use futures::StreamExt;
use reflow_tracing_protocol::client::TracingIntegration;

use crate::actor::{Actor, ActorBehavior, ActorConfig, ActorLoad, Port};
use crate::connector::{ConnectionPoint, Connector};
use crate::graph::types::GraphExport;
use crate::message::Message;
use crate::network::{Network, NetworkConfig};

/// A lightweight actor that bridges an inner network's outport to the
/// SubgraphActor's external outport channel. Registered as a node inside the
/// inner network and connected via a standard Connector.
struct OutportBridge {
    /// The external sender — messages forwarded here appear on the SubgraphActor's outport
    external_sender: flume::Sender<HashMap<String, Message>>,
    /// The external port name used as the HashMap key when forwarding
    external_port_name: String,
    /// The inner port name to extract from incoming packets
    inner_port_name: String,
    inports: Port,
    outports: Port,
    load: Arc<ActorLoad>,
}

impl OutportBridge {
    fn new(
        external_sender: flume::Sender<HashMap<String, Message>>,
        external_port_name: String,
        inner_port_name: String,
    ) -> Self {
        OutportBridge {
            external_sender,
            external_port_name,
            inner_port_name,
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ActorLoad::new(0)),
        }
    }
}

impl Actor for OutportBridge {
    fn get_behavior(&self) -> ActorBehavior {
        // Routing is handled in create_process; behavior is a no-op.
        Box::new(|_ctx| Box::pin(async { Ok(HashMap::new()) }))
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn load_count(&self) -> Arc<ActorLoad> {
        self.load.clone()
    }

    fn create_instance(&self) -> Arc<dyn Actor> {
        Arc::new(Self::new(
            self.external_sender.clone(),
            self.external_port_name.clone(),
            self.inner_port_name.clone(),
        ))
    }

    fn create_process(
        &self,
        _config: ActorConfig,
        _tracing: Option<TracingIntegration>,
    ) -> Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        let receiver = self.inports.1.clone();
        let external_sender = self.external_sender.clone();
        let ext_port = self.external_port_name.clone();
        let inner_port = self.inner_port_name.clone();

        Box::pin(async move {
            while let Some(mut packet) = receiver.clone().stream().next().await {
                if let Some(msg) = packet.remove(&inner_port) {
                    let out = HashMap::from([(ext_port.clone(), msg)]);
                    if external_sender.send(out).is_err() {
                        break;
                    }
                }
            }
        })
    }
}

/// A SubgraphActor wraps an inner `Network` and exposes it as a single Actor
/// in the parent network. Messages route through inport/outport boundary maps
/// derived from `GraphExport.inports` / `GraphExport.outports`.
///
/// Outbound routing uses `OutportBridge` actors registered inside the inner
/// network, connected via standard `Connector`s — the same pattern used for
/// all other inter-actor communication.
///
/// **Note**: The connector architecture uses competing consumers (flume MPMC).
/// Inner actors whose ports are mapped to external outports should not also
/// have internal connections from those same ports, as messages would be
/// split between consumers rather than broadcast.
pub struct SubgraphActor {
    /// The inner network this subgraph wraps
    inner_network: Arc<parking_lot::Mutex<Network>>,
    /// Original graph export, retained so the runtime can instantiate a
    /// fresh isolated copy for each parent node that uses this subgraph.
    graph_export: Option<GraphExport>,
    /// Actor templates required by the graph export.
    actor_templates: HashMap<String, Arc<dyn Actor>>,
    /// Maps external inport name → (inner_actor_id, inner_port_name)
    inport_map: HashMap<String, (String, String)>,
    /// Maps external outport name → (inner_actor_id, inner_port_name)
    outport_map: HashMap<String, (String, String)>,
    /// External inports — parent network sends messages here
    inports: Port,
    /// External outports — parent network reads messages from here
    outports: Port,
    /// Load counter
    load: Arc<ActorLoad>,
    /// Cancellation signal — send `true` to stop the inbound routing loop
    shutdown_tx: Arc<tokio::sync::watch::Sender<bool>>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
}

impl SubgraphActor {
    /// Create a SubgraphActor from a `GraphExport` and a set of pre-registered actors.
    ///
    /// The `actors` map must contain entries for every `component` referenced by
    /// `graph_export.processes`. Inport/outport boundary maps are built from
    /// `graph_export.inports` and `graph_export.outports`.
    ///
    /// For each outport mapping, an `OutportBridge` actor is automatically
    /// registered inside the inner network and connected via a standard Connector.
    pub fn from_graph_export(
        graph_export: &GraphExport,
        actors: HashMap<String, Arc<dyn Actor>>,
    ) -> Result<Self, anyhow::Error> {
        let mut inner_network = Network::new(NetworkConfig::default());
        let actor_templates = actors.clone();

        // Register actors (components) in the inner network
        for (name, actor) in actors {
            inner_network.register_actor_arc(&name, actor)?;
        }

        // Add nodes (processes) from the graph export
        for (id, node) in &graph_export.processes {
            inner_network.add_node(id, &node.component, node.metadata.clone())?;
        }

        // Add internal connections
        for conn in &graph_export.connections {
            inner_network.add_connection(Connector::new(
                ConnectionPoint::new(
                    &conn.from.node_id,
                    &conn.from.port_id,
                    conn.data.clone().map(Message::from),
                ),
                ConnectionPoint::new(&conn.to.node_id, &conn.to.port_id, None),
            ));
        }

        // Build inport map: external port name → (inner actor id, inner port name)
        let mut inport_map = HashMap::new();
        for (ext_port, edge) in &graph_export.inports {
            inport_map.insert(
                ext_port.clone(),
                (edge.node_id.clone(), edge.port_name.clone()),
            );
        }

        // Build outport map and register OutportBridge actors inside the inner network
        let outports: Port = flume::unbounded();
        let mut outport_map = HashMap::new();

        for (ext_port, edge) in &graph_export.outports {
            outport_map.insert(
                ext_port.clone(),
                (edge.node_id.clone(), edge.port_name.clone()),
            );

            // Create a bridge actor that forwards to the external outport sender
            let bridge =
                OutportBridge::new(outports.0.clone(), ext_port.clone(), edge.port_name.clone());

            let bridge_component = format!("__outport_bridge_{}", ext_port);
            let bridge_node_id = format!("__outport_bridge_{}", ext_port);

            inner_network.register_actor_arc(&bridge_component, Arc::new(bridge))?;
            inner_network.add_node(&bridge_node_id, &bridge_component, None)?;

            // Connect inner actor's outport → bridge's inport via standard Connector
            inner_network.add_connection(Connector::new(
                ConnectionPoint::new(&edge.node_id, &edge.port_id, None),
                ConnectionPoint::new(&bridge_node_id, &edge.port_name, None),
            ));
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        Ok(SubgraphActor {
            inner_network: Arc::new(parking_lot::Mutex::new(inner_network)),
            graph_export: Some(graph_export.clone()),
            actor_templates,
            inport_map,
            outport_map,
            inports: flume::unbounded(),
            outports,
            load: Arc::new(ActorLoad::new(0)),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        })
    }

    /// Create a SubgraphActor from a pre-built inner network and explicit port maps.
    pub fn new(
        inner_network: Network,
        inport_map: HashMap<String, (String, String)>,
        outport_map: HashMap<String, (String, String)>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

        SubgraphActor {
            inner_network: Arc::new(parking_lot::Mutex::new(inner_network)),
            graph_export: None,
            actor_templates: HashMap::new(),
            inport_map,
            outport_map,
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ActorLoad::new(0)),
            shutdown_tx: Arc::new(shutdown_tx),
            shutdown_rx,
        }
    }

    /// Get a reference to the inner network (locked).
    pub fn inner_network(&self) -> &Arc<parking_lot::Mutex<Network>> {
        &self.inner_network
    }

    /// Get the inport map.
    pub fn inport_map(&self) -> &HashMap<String, (String, String)> {
        &self.inport_map
    }

    /// Get the outport map.
    pub fn outport_map(&self) -> &HashMap<String, (String, String)> {
        &self.outport_map
    }
}

impl Actor for SubgraphActor {
    fn get_behavior(&self) -> ActorBehavior {
        // SubgraphActor doesn't use a simple behavior function — routing is
        // handled entirely in create_process. Return a no-op behavior.
        Box::new(|_context| Box::pin(async { Ok(HashMap::new()) }))
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn load_count(&self) -> Arc<ActorLoad> {
        self.load.clone()
    }

    fn create_instance(&self) -> Arc<dyn Actor> {
        if let Some(graph_export) = &self.graph_export {
            return Arc::new(
                SubgraphActor::from_graph_export(graph_export, self.actor_templates.clone())
                    .expect("Failed to clone subgraph actor from graph export"),
            );
        }

        Arc::new(SubgraphActor::new(
            self.inner_network.lock().clone(),
            self.inport_map.clone(),
            self.outport_map.clone(),
        ))
    }

    fn create_process(
        &self,
        _config: ActorConfig,
        _tracing_integration: Option<TracingIntegration>,
    ) -> Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        let inner_network = self.inner_network.clone();
        let inport_map = self.inport_map.clone();
        let inport_receiver = self.inports.1.clone();
        let load = self.load.clone();
        let mut shutdown_rx = self.shutdown_rx.clone();

        Box::pin(async move {
            // 1. Start the inner network (initializes actors, connectors, IIPs).
            //    OutportBridge actors are started as part of this — their connectors
            //    handle outbound routing automatically.
            {
                let mut network = inner_network.lock();
                if let Err(e) = network.start() {
                    tracing::error!("[SUBGRAPH] Failed to start inner network: {}", e);
                    return;
                }
            }

            // 2. Inbound routing loop: receive on external inports and route
            //    to the appropriate inner actor via send_to_actor.
            let mut inport_stream = inport_receiver.stream();
            loop {
                tokio::select! {
                    biased;
                    _ = shutdown_rx.changed() => {
                        break;
                    }
                    packet = inport_stream.next() => {
                        match packet {
                            Some(pkt) => {
                                load.inc();
                                for (port_name, message) in pkt {
                                    if let Some((inner_actor_id, inner_port)) = inport_map.get(&port_name) {
                                        let network = inner_network.lock();
                                        if let Err(e) = network.send_to_actor(inner_actor_id, inner_port, message) {
                                            tracing::error!(
                                                "[SUBGRAPH] Failed to route inport '{}' to {}:{} — {}",
                                                port_name, inner_actor_id, inner_port, e
                                            );
                                        }
                                    } else {
                                        tracing::warn!("[SUBGRAPH] No inport mapping for port '{}'", port_name);
                                    }
                                }
                                load.dec();
                            }
                            None => break,
                        }
                    }
                }
            }
        })
    }

    fn shutdown(&self) {
        // Signal the inbound routing loop to stop
        let _ = self.shutdown_tx.send(true);

        // Shutdown the inner network (cascades to inner actors, bridges, and processes)
        let network = self.inner_network.lock();
        network.shutdown();
    }

    fn cleanup(&self) {
        let _ = self.shutdown_tx.send(true);
    }
}

#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::actor::{ActorContext, MemoryState};
    use crate::graph::types::{GraphConnection, GraphEdge, GraphNode};

    /// Simple pass-through actor: receives on "in", sends the same message on "out".
    /// Uses the default `create_process` via ActorProcess — no boilerplate loop.
    struct PassthroughActor {
        inports: Port,
        outports: Port,
        load: Arc<ActorLoad>,
    }

    impl PassthroughActor {
        fn new() -> Self {
            Self {
                inports: flume::unbounded(),
                outports: flume::unbounded(),
                load: Arc::new(ActorLoad::new(0)),
            }
        }
    }

    impl Actor for PassthroughActor {
        fn get_behavior(&self) -> ActorBehavior {
            Box::new(|context| {
                let payload = context.get_payload().clone();
                Box::pin(async move {
                    if let Some(msg) = payload.get("in") {
                        let result = HashMap::from([("out".to_string(), msg.clone())]);
                        return Ok(result);
                    }
                    Ok(HashMap::new())
                })
            })
        }

        fn get_inports(&self) -> Port {
            self.inports.clone()
        }

        fn get_outports(&self) -> Port {
            self.outports.clone()
        }

        fn load_count(&self) -> Arc<ActorLoad> {
            self.load.clone()
        }

        fn create_instance(&self) -> Arc<dyn Actor> {
            Arc::new(Self::new())
        }

        fn inport_names(&self) -> Vec<String> {
            vec!["in".into()]
        }

        fn outport_names(&self) -> Vec<String> {
            vec!["out".into()]
        }

        // create_process() uses the default trait impl via ActorProcess
    }

    /// Build a simple subgraph: one inner PassthroughActor.
    /// External inport "input" → inner actor "pass" port "in"
    /// Inner actor "pass" port "out" → external outport "output"
    fn build_simple_subgraph() -> SubgraphActor {
        let graph_export = GraphExport {
            processes: HashMap::from([(
                "pass".to_string(),
                GraphNode {
                    id: "pass".to_string(),
                    component: "Passthrough".to_string(),
                    metadata: None,
                },
            )]),
            inports: HashMap::from([(
                "input".to_string(),
                GraphEdge {
                    node_id: "pass".to_string(),
                    port_name: "in".to_string(),
                    port_id: "in".to_string(),
                    ..Default::default()
                },
            )]),
            outports: HashMap::from([(
                "output".to_string(),
                GraphEdge {
                    node_id: "pass".to_string(),
                    port_name: "out".to_string(),
                    port_id: "out".to_string(),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };

        let actors: HashMap<String, Arc<dyn Actor>> = HashMap::from([(
            "Passthrough".to_string(),
            Arc::new(PassthroughActor::new()) as Arc<dyn Actor>,
        )]);

        SubgraphActor::from_graph_export(&graph_export, actors).expect("Failed to build subgraph")
    }

    #[tokio::test]
    async fn test_subgraph_actor_creation() {
        let subgraph = build_simple_subgraph();

        assert_eq!(subgraph.inport_map().len(), 1);
        assert_eq!(subgraph.outport_map().len(), 1);
        assert!(subgraph.inport_map().contains_key("input"));
        assert!(subgraph.outport_map().contains_key("output"));

        let (inner_actor, inner_port) = &subgraph.inport_map()["input"];
        assert_eq!(inner_actor, "pass");
        assert_eq!(inner_port, "in");

        let (inner_actor, inner_port) = &subgraph.outport_map()["output"];
        assert_eq!(inner_actor, "pass");
        assert_eq!(inner_port, "out");
    }

    #[tokio::test]
    async fn test_subgraph_in_parent_network() {
        let subgraph = build_simple_subgraph();
        let mut parent = Network::new(NetworkConfig::default());

        parent
            .register_actor_arc("SubgraphComponent", Arc::new(subgraph))
            .unwrap();
        parent
            .add_node("sub_node", "SubgraphComponent", None)
            .unwrap();

        parent.start().unwrap();

        let msg = Message::from(serde_json::json!({"hello": "world"}));
        parent
            .send_to_actor("sub_node", "input", msg.clone())
            .unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let sub_actor = parent.initialized_actors.get("sub_node").unwrap();
        let outport_receiver = sub_actor.get_outports().1;

        match outport_receiver.try_recv() {
            Ok(result) => {
                assert!(
                    result.contains_key("output"),
                    "Expected 'output' port in result"
                );
                let output_msg = &result["output"];
                let expected: serde_json::Value = serde_json::json!({"hello": "world"});
                let actual: serde_json::Value = output_msg.clone().into();
                assert_eq!(actual, expected);
            }
            Err(_) => {
                panic!("Expected a message on the subgraph outport but got none");
            }
        }

        parent.shutdown();
    }

    #[tokio::test]
    async fn test_subgraph_chained_actors() {
        let graph_export = GraphExport {
            processes: HashMap::from([
                (
                    "actor_a".to_string(),
                    GraphNode {
                        id: "actor_a".to_string(),
                        component: "PassthroughA".to_string(),
                        metadata: None,
                    },
                ),
                (
                    "actor_b".to_string(),
                    GraphNode {
                        id: "actor_b".to_string(),
                        component: "PassthroughB".to_string(),
                        metadata: None,
                    },
                ),
            ]),
            connections: vec![GraphConnection {
                from: GraphEdge {
                    node_id: "actor_a".to_string(),
                    port_name: "out".to_string(),
                    port_id: "out".to_string(),
                    ..Default::default()
                },
                to: GraphEdge {
                    node_id: "actor_b".to_string(),
                    port_name: "in".to_string(),
                    port_id: "in".to_string(),
                    ..Default::default()
                },
                metadata: None,
                data: None,
            }],
            inports: HashMap::from([(
                "input".to_string(),
                GraphEdge {
                    node_id: "actor_a".to_string(),
                    port_name: "in".to_string(),
                    port_id: "in".to_string(),
                    ..Default::default()
                },
            )]),
            outports: HashMap::from([(
                "output".to_string(),
                GraphEdge {
                    node_id: "actor_b".to_string(),
                    port_name: "out".to_string(),
                    port_id: "out".to_string(),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };

        let actors: HashMap<String, Arc<dyn Actor>> = HashMap::from([
            (
                "PassthroughA".to_string(),
                Arc::new(PassthroughActor::new()) as Arc<dyn Actor>,
            ),
            (
                "PassthroughB".to_string(),
                Arc::new(PassthroughActor::new()) as Arc<dyn Actor>,
            ),
        ]);

        let subgraph = SubgraphActor::from_graph_export(&graph_export, actors).unwrap();

        let mut parent = Network::new(NetworkConfig::default());
        parent
            .register_actor_arc("ChainedSubgraph", Arc::new(subgraph))
            .unwrap();
        parent
            .add_node("chain_node", "ChainedSubgraph", None)
            .unwrap();
        parent.start().unwrap();

        let msg = Message::from(serde_json::json!(42));
        parent.send_to_actor("chain_node", "input", msg).unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let sub_actor = parent.initialized_actors.get("chain_node").unwrap();
        let outport_receiver = sub_actor.get_outports().1;

        match outport_receiver.try_recv() {
            Ok(result) => {
                assert!(result.contains_key("output"));
                let actual: serde_json::Value = result["output"].clone().into();
                assert_eq!(actual, serde_json::json!(42));
            }
            Err(_) => {
                panic!("Expected message on chained subgraph outport");
            }
        }

        parent.shutdown();
    }

    #[tokio::test]
    async fn test_subgraph_shutdown_cleans_up() {
        let subgraph = build_simple_subgraph();
        let mut parent = Network::new(NetworkConfig::default());

        parent
            .register_actor_arc("SubgraphComponent", Arc::new(subgraph))
            .unwrap();
        parent
            .add_node("sub_node", "SubgraphComponent", None)
            .unwrap();

        parent.start().unwrap();

        // Send a message to verify it's working
        let msg = Message::from(serde_json::json!("test"));
        parent.send_to_actor("sub_node", "input", msg).unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Shutdown should complete without hanging
        parent.shutdown();

        // Verify load is zero after shutdown
        let sub_actor = parent.actors.get("SubgraphComponent").unwrap();
        assert_eq!(sub_actor.load_count().get(), 0);
    }
}
