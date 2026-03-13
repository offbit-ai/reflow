//! Execution engine — owns Network instances and drives workflow lifecycle.
//!
//! This is the core of a Reflow node. It:
//! - Creates an isolated `Network` per workflow execution
//! - Registers actors from `reflow_components`
//! - Translates low-level [`NetworkEvent`]s into rich [`EngineEvent`]s
//! - Manages execution state (queued → running → completed/failed/cancelled)

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use log::{debug, error, info};
use reflow_network::connector::{ConnectionPoint, Connector, InitialPacket};
use reflow_network::graph::Graph;
use reflow_network::graph::types::GraphExport;
use reflow_network::network::{Network, NetworkConfig, NetworkEvent};
use serde::{Deserialize, Serialize};

use crate::zeal_converter::{ZealWorkflow, convert_zeal_to_graph_export};

// ============================================================================
// Engine Events — internal event stream consumed by zip_session / trace / REST
// ============================================================================

/// Internal events emitted by the engine during execution.
/// These are translated into ZIP events, trace submissions, or REST responses
/// by the respective consumer modules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineEvent {
    pub workflow_id: String,
    pub execution_id: String,
    pub event_type: EngineEventType,
    pub timestamp: u64,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EngineEventType {
    Started,
    /// A node/actor began executing.
    NodeExecuting {
        node_id: String,
        component: String,
    },
    /// A node/actor completed successfully.
    ActorCompleted {
        actor_id: String,
        component: String,
        /// Execution duration in milliseconds (from ActorStarted → ActorCompleted).
        duration_ms: Option<u64>,
        /// Serialized output size in bytes.
        output_size: Option<u64>,
        /// IDs of outbound connections carrying results.
        output_connections: Vec<String>,
    },
    /// A node/actor failed.
    ActorFailed {
        actor_id: String,
        component: String,
        error: String,
        output_connections: Vec<String>,
    },
    /// A message was sent between actors.
    MessageSent {
        from_node: String,
        from_port: String,
        to_node: String,
        to_port: String,
        /// Serialized message size in bytes.
        size: usize,
    },
    NetworkIdle,
    /// Execution completed. Includes aggregate stats.
    Completed {
        duration_ms: u64,
        nodes_executed: u32,
        nodes_failed: u32,
    },
    Failed {
        error: String,
        duration_ms: Option<u64>,
    },
}

// ============================================================================
// Execution State
// ============================================================================

#[derive(Clone, Serialize, Deserialize)]
pub struct ExecutionState {
    pub id: String,
    pub status: ExecutionStatus,
    #[serde(skip)]
    pub start_time: Option<Instant>,
    #[serde(skip)]
    pub end_time: Option<Instant>,
    pub result: Option<ExecutionResult>,
    #[serde(skip)]
    pub network_handle: Option<Arc<tokio::sync::Mutex<Network>>>,
    #[serde(skip)]
    pub event_receiver: Option<flume::Receiver<EngineEvent>>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            id: String::new(),
            status: ExecutionStatus::Queued,
            start_time: Some(Instant::now()),
            end_time: None,
            result: None,
            network_handle: None,
            event_receiver: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    pub execution_id: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub results: serde_json::Value,
    pub errors: Option<Vec<String>>,
    pub trace_session_id: Option<String>,
}

// ============================================================================
// Execution Engine
// ============================================================================

/// The core execution engine that manages workflow lifecycle.
///
/// Each `ExecutionEngine` represents a single Reflow node. It maintains
/// a registry of available actors and a map of active executions.
#[derive(Clone)]
pub struct ExecutionEngine {
    executions: Arc<DashMap<String, ExecutionState>>,
}

impl Default for ExecutionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ExecutionEngine {
    pub fn new() -> Self {
        Self {
            executions: Arc::new(DashMap::new()),
        }
    }

    /// Start a workflow execution in the background.
    /// Returns the execution ID immediately.
    pub async fn start_execution(
        &self,
        graph_json: serde_json::Value,
        input: serde_json::Value,
        execution_id: String,
        workflow_id: String,
    ) -> Result<(String, flume::Receiver<EngineEvent>)> {
        info!(
            "Starting execution: {} (workflow: {})",
            execution_id, workflow_id
        );

        let (event_tx, event_rx) = flume::unbounded::<EngineEvent>();

        let initial_state = ExecutionState {
            id: execution_id.clone(),
            status: ExecutionStatus::Queued,
            start_time: Some(Instant::now()),
            end_time: None,
            result: None,
            network_handle: None,
            event_receiver: Some(event_rx.clone()),
        };
        self.executions.insert(execution_id.clone(), initial_state);

        let executions = self.executions.clone();
        let eid = execution_id.clone();
        let wid = workflow_id.clone();

        tokio::spawn(async move {
            Self::run_execution(eid, wid, graph_json, input, executions, event_tx).await;
        });

        Ok((execution_id, event_rx))
    }

    /// Start a Zeal workflow — converts from Zeal format then executes.
    pub async fn start_zeal_execution(
        &self,
        zeal_workflow: ZealWorkflow,
        input: serde_json::Value,
    ) -> Result<(String, flume::Receiver<EngineEvent>)> {
        let graph_export = convert_zeal_to_graph_export(&zeal_workflow)?;
        let graph_json = serde_json::to_value(graph_export)?;

        let execution_id = format!("zeal_exec_{}", uuid::Uuid::new_v4());
        let workflow_id = zeal_workflow.id.clone();

        self.start_execution(graph_json, input, execution_id, workflow_id)
            .await
    }

    /// Get the current state of an execution.
    pub fn get_execution(&self, execution_id: &str) -> Option<ExecutionState> {
        self.executions.get(execution_id).map(|e| e.clone())
    }

    /// Cancel a running execution.
    pub async fn cancel_execution(&self, execution_id: &str) -> Result<()> {
        if let Some(mut state) = self.executions.get_mut(execution_id) {
            state.status = ExecutionStatus::Cancelled;
            state.end_time = Some(Instant::now());

            if let Some(network_handle) = &state.network_handle {
                let network = network_handle.lock().await;
                network.shutdown();
                info!("Cancelled execution: {} (network shutdown)", execution_id);
            }

            Ok(())
        } else {
            Err(anyhow!("Execution not found: {}", execution_id))
        }
    }

    /// Get the network event receiver for direct subscription.
    pub async fn get_network_receiver(
        &self,
        execution_id: &str,
    ) -> Option<flume::Receiver<NetworkEvent>> {
        if let Some(state) = self.executions.get(execution_id)
            && let Some(handle) = &state.network_handle
        {
            let network = handle.lock().await;
            return Some(network.get_event_receiver());
        }
        None
    }

    // ── Private ──────────────────────────────────────────────────

    /// Background execution worker.
    ///
    /// Starts the network, then drains its `NetworkEvent` stream and
    /// translates each event into an `EngineEvent` with timing, output
    /// size, and connection metadata. Waits for `NetworkIdle` or
    /// `NetworkShutdown` before marking the execution as complete.
    async fn run_execution(
        execution_id: String,
        workflow_id: String,
        graph_json: serde_json::Value,
        _input: serde_json::Value,
        executions: Arc<DashMap<String, ExecutionState>>,
        event_tx: flume::Sender<EngineEvent>,
    ) {
        let exec_start = Instant::now();

        // Mark running
        if let Some(mut state) = executions.get_mut(&execution_id) {
            state.status = ExecutionStatus::Running;
        }

        // Emit started event
        let _ = event_tx.send(EngineEvent {
            workflow_id: workflow_id.clone(),
            execution_id: execution_id.clone(),
            event_type: EngineEventType::Started,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
            data: serde_json::json!({}),
        });

        // Build the network and obtain the event receiver BEFORE wrapping in Arc
        let (network, net_event_rx, graph) = match Self::build_network(&graph_json) {
            Ok(result) => result,
            Err(e) => {
                let error_msg = format!("Network startup failed: {}", e);
                error!("{}", error_msg);
                let _ = event_tx.send(EngineEvent {
                    workflow_id: workflow_id.clone(),
                    execution_id: execution_id.clone(),
                    event_type: EngineEventType::Failed {
                        error: error_msg,
                        duration_ms: Some(exec_start.elapsed().as_millis() as u64),
                    },
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    data: serde_json::Value::Null,
                });
                Self::finalize_execution(
                    &executions,
                    &execution_id,
                    false,
                    serde_json::Value::Null,
                    vec![e.to_string()],
                );
                return;
            }
        };

        // Store the network handle for cancel/status queries
        if let Some(mut state) = executions.get_mut(&execution_id) {
            state.network_handle = Some(Arc::new(tokio::sync::Mutex::new(network)));
        }

        info!("Network started for execution: {}", execution_id);

        // Build a lookup of outbound connections per node for enriching events.
        // Map: node_id → vec of "connection_id" (from_port→to_node:to_port).
        let mut outbound_connections: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &graph.connections {
            outbound_connections
                .entry(edge.from.node_id.clone())
                .or_default()
                .push(format!(
                    "{}:{}→{}:{}",
                    edge.from.node_id, edge.from.port_id, edge.to.node_id, edge.to.port_id
                ));
        }

        // ── Drain NetworkEvents → EngineEvents ──────────────────
        let mut actor_start_times: HashMap<String, Instant> = HashMap::new();
        let mut nodes_completed: u32 = 0;
        let mut nodes_failed: u32 = 0;

        while let Ok(net_event) = net_event_rx.recv_async().await {
            let now_ms = chrono::Utc::now().timestamp_millis() as u64;

            match net_event {
                NetworkEvent::ActorStarted {
                    actor_id,
                    component,
                    ..
                } => {
                    actor_start_times.insert(actor_id.clone(), Instant::now());
                    let _ = event_tx.send(EngineEvent {
                        workflow_id: workflow_id.clone(),
                        execution_id: execution_id.clone(),
                        event_type: EngineEventType::NodeExecuting {
                            node_id: actor_id,
                            component,
                        },
                        timestamp: now_ms,
                        data: serde_json::json!({}),
                    });
                }

                NetworkEvent::ActorCompleted {
                    actor_id,
                    component,
                    outputs,
                    ..
                } => {
                    nodes_completed += 1;
                    let duration_ms = actor_start_times
                        .remove(&actor_id)
                        .map(|t| t.elapsed().as_millis() as u64);
                    let output_size = outputs
                        .as_ref()
                        .and_then(|v| serde_json::to_vec(v).ok())
                        .map(|b| b.len() as u64);
                    let conns = outbound_connections
                        .get(&actor_id)
                        .cloned()
                        .unwrap_or_default();

                    let _ = event_tx.send(EngineEvent {
                        workflow_id: workflow_id.clone(),
                        execution_id: execution_id.clone(),
                        event_type: EngineEventType::ActorCompleted {
                            actor_id: actor_id.clone(),
                            component,
                            duration_ms,
                            output_size,
                            output_connections: conns,
                        },
                        timestamp: now_ms,
                        data: outputs.unwrap_or(serde_json::Value::Null),
                    });
                }

                NetworkEvent::ActorFailed {
                    actor_id,
                    component,
                    error,
                    ..
                } => {
                    nodes_failed += 1;
                    actor_start_times.remove(&actor_id);
                    let conns = outbound_connections
                        .get(&actor_id)
                        .cloned()
                        .unwrap_or_default();

                    let _ = event_tx.send(EngineEvent {
                        workflow_id: workflow_id.clone(),
                        execution_id: execution_id.clone(),
                        event_type: EngineEventType::ActorFailed {
                            actor_id: actor_id.clone(),
                            component,
                            error,
                            output_connections: conns,
                        },
                        timestamp: now_ms,
                        data: serde_json::Value::Null,
                    });
                }

                NetworkEvent::MessageSent {
                    from_actor,
                    from_port,
                    to_actor,
                    to_port,
                    message,
                    ..
                } => {
                    let size = serde_json::to_vec(&message).map(|b| b.len()).unwrap_or(0);
                    let _ = event_tx.send(EngineEvent {
                        workflow_id: workflow_id.clone(),
                        execution_id: execution_id.clone(),
                        event_type: EngineEventType::MessageSent {
                            from_node: from_actor,
                            from_port,
                            to_node: to_actor,
                            to_port,
                            size,
                        },
                        timestamp: now_ms,
                        data: serde_json::Value::Null,
                    });
                }

                NetworkEvent::NetworkIdle { .. } => {
                    debug!("Network idle for execution: {}", execution_id);
                    let _ = event_tx.send(EngineEvent {
                        workflow_id: workflow_id.clone(),
                        execution_id: execution_id.clone(),
                        event_type: EngineEventType::NetworkIdle,
                        timestamp: now_ms,
                        data: serde_json::json!({}),
                    });
                    // NetworkIdle means all actors finished — treat as completion.
                    break;
                }

                NetworkEvent::NetworkShutdown { .. } => {
                    info!("Network shutdown for execution: {}", execution_id);
                    break;
                }

                // ActorEmit, MessageReceived — informational, skip for now
                _ => {}
            }
        }

        // ── Emit completion ─────────────────────────────────────
        let duration_ms = exec_start.elapsed().as_millis() as u64;
        let success = nodes_failed == 0;

        if success {
            let _ = event_tx.send(EngineEvent {
                workflow_id: workflow_id.clone(),
                execution_id: execution_id.clone(),
                event_type: EngineEventType::Completed {
                    duration_ms,
                    nodes_executed: nodes_completed,
                    nodes_failed: 0,
                },
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                data: serde_json::json!({
                    "duration_ms": duration_ms,
                    "nodes_executed": nodes_completed,
                }),
            });
        } else {
            let _ = event_tx.send(EngineEvent {
                workflow_id: workflow_id.clone(),
                execution_id: execution_id.clone(),
                event_type: EngineEventType::Completed {
                    duration_ms,
                    nodes_executed: nodes_completed + nodes_failed,
                    nodes_failed,
                },
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                data: serde_json::json!({
                    "duration_ms": duration_ms,
                    "nodes_executed": nodes_completed + nodes_failed,
                    "nodes_failed": nodes_failed,
                }),
            });
        }

        let final_result = serde_json::json!({
            "status": if success { "completed" } else { "completed_with_errors" },
            "execution_id": execution_id,
            "workflow_id": workflow_id,
            "duration_ms": duration_ms,
            "nodes_executed": nodes_completed + nodes_failed,
            "nodes_failed": nodes_failed,
        });

        Self::finalize_execution(&executions, &execution_id, success, final_result, vec![]);
    }

    /// Build a Network from graph JSON, register actors, start it,
    /// and return the network + its event receiver + parsed graph.
    fn build_network(
        graph_json: &serde_json::Value,
    ) -> Result<(Network, flume::Receiver<NetworkEvent>, Graph)> {
        let graph_export: GraphExport = serde_json::from_value(graph_json.clone())
            .map_err(|e| anyhow!("Failed to parse graph JSON: {}", e))?;
        let graph = Graph::load(graph_export, None);

        let mut network = Network::new(NetworkConfig::default());

        // Add nodes
        for (id, node) in &graph.nodes {
            network.add_node(id, &node.component, node.metadata.clone())?;
        }

        // Add initial packets
        for iip in &graph.initializers {
            network.add_initial(InitialPacket {
                to: ConnectionPoint {
                    actor: iip.to.node_id.clone(),
                    port: iip.to.port_name.clone(),
                    initial_data: Some(iip.data.clone().into()),
                },
            });
        }

        // Add connections
        for edge in &graph.connections {
            network.add_connection(Connector {
                from: ConnectionPoint {
                    actor: edge.from.node_id.clone(),
                    port: edge.from.port_id.clone(),
                    initial_data: edge.clone().data.map(|d| d.into()),
                },
                to: ConnectionPoint {
                    actor: edge.to.node_id.clone(),
                    port: edge.to.port_id.clone(),
                    initial_data: None,
                },
            });
        }

        // Register all actors from reflow_components (native + API)
        let template_mappings = reflow_components::get_template_mapping();
        for (template_id, actor_name) in &template_mappings {
            if let Some(actor) = reflow_components::get_actor_for_template(template_id) {
                let _ = network.register_actor_arc(template_id, actor);
            }
            if let Some(actor) = reflow_components::get_actor_for_template(template_id) {
                let _ = network.register_actor_arc(actor_name, actor);
            }
        }

        // Register any graph node components not covered by native templates
        // (e.g. api_github_create_issue, api_slack_send_message, etc.)
        for node in graph.nodes.values() {
            if !template_mappings.contains_key(&node.component)
                && let Some(actor) = reflow_components::get_actor_for_template(&node.component)
            {
                let _ = network.register_actor_arc(&node.component, actor);
            }
        }

        // Grab the event receiver BEFORE start() to avoid missing early events
        let net_event_rx = network.get_event_receiver();

        network.start()?;

        Ok((network, net_event_rx, graph))
    }

    /// Update final execution state in the DashMap.
    fn finalize_execution(
        executions: &DashMap<String, ExecutionState>,
        execution_id: &str,
        success: bool,
        results: serde_json::Value,
        errors: Vec<String>,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let result = ExecutionResult {
            success,
            execution_id: execution_id.to_string(),
            start_time: now.clone(),
            end_time: Some(now),
            results,
            errors: if errors.is_empty() {
                None
            } else {
                Some(errors)
            },
            trace_session_id: Some(format!("trace_{}", execution_id)),
        };

        if let Some(mut state) = executions.get_mut(execution_id) {
            state.status = if success {
                ExecutionStatus::Completed
            } else {
                ExecutionStatus::Failed
            };
            state.end_time = Some(Instant::now());
            state.result = Some(result);
        }
    }
}
