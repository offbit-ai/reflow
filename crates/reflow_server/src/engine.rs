//! Execution engine — owns Network instances and drives workflow lifecycle.
//!
//! This is the core of a Reflow node. It:
//! - Creates an isolated `Network` per workflow execution
//! - Registers actors from `reflow_components`
//! - Drives execution and emits [`EngineEvent`]s
//! - Manages execution state (queued → running → completed/failed/cancelled)

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use log::{error, info};
use reflow_network::connector::{ConnectionPoint, Connector, InitialPacket};
use reflow_network::graph::Graph;
use reflow_network::graph::types::GraphExport;
use reflow_network::network::{Network, NetworkConfig};
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
    ActorCompleted { actor_id: String },
    ActorFailed { actor_id: String, error: String },
    NetworkIdle,
    Completed,
    Failed { error: String },
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
    ) -> Option<flume::Receiver<reflow_network::network::NetworkEvent>> {
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
    async fn run_execution(
        execution_id: String,
        workflow_id: String,
        graph_json: serde_json::Value,
        _input: serde_json::Value,
        executions: Arc<DashMap<String, ExecutionState>>,
        event_tx: flume::Sender<EngineEvent>,
    ) {
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

        let mut success = false;
        let mut final_result = serde_json::Value::Null;
        let mut errors: Vec<String> = Vec::new();

        match Self::create_and_start_network(&graph_json).await {
            Ok(network_handle) => {
                // Store the network handle
                if let Some(mut state) = executions.get_mut(&execution_id) {
                    let network = Arc::try_unwrap(network_handle.clone())
                        .map(|mutex| mutex.into_inner().unwrap())
                        .unwrap_or_else(|arc| (*arc).lock().unwrap().clone());
                    state.network_handle = Some(Arc::new(tokio::sync::Mutex::new(network)));
                }

                success = true;
                final_result = serde_json::json!({
                    "status": "running",
                    "execution_id": execution_id,
                    "workflow_id": workflow_id,
                });
                info!("Network started for execution: {}", execution_id);
            }
            Err(e) => {
                let error_msg = format!("Network startup failed: {}", e);
                errors.push(error_msg.clone());
                error!("{}", error_msg);

                let _ = event_tx.send(EngineEvent {
                    workflow_id: workflow_id.clone(),
                    execution_id: execution_id.clone(),
                    event_type: EngineEventType::Failed { error: error_msg },
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    data: serde_json::Value::Null,
                });
            }
        }

        // Emit completion/failure
        if success {
            let _ = event_tx.send(EngineEvent {
                workflow_id: workflow_id.clone(),
                execution_id: execution_id.clone(),
                event_type: EngineEventType::Completed,
                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                data: final_result.clone(),
            });
        }

        // Update final state
        let now = chrono::Utc::now().to_rfc3339();
        let result = ExecutionResult {
            success,
            execution_id: execution_id.clone(),
            start_time: now.clone(),
            end_time: Some(now),
            results: final_result,
            errors: if errors.is_empty() {
                None
            } else {
                Some(errors)
            },
            trace_session_id: Some(format!("trace_{}", execution_id)),
        };

        if let Some(mut state) = executions.get_mut(&execution_id) {
            state.status = if success {
                ExecutionStatus::Completed
            } else {
                ExecutionStatus::Failed
            };
            state.end_time = Some(Instant::now());
            state.result = Some(result);
        }
    }

    /// Create a Network from graph JSON, register all actors, and start it.
    async fn create_and_start_network(
        graph_json: &serde_json::Value,
    ) -> Result<Arc<std::sync::Mutex<Network>>> {
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

        // Register all actors from reflow_components
        let template_mappings = reflow_components::get_template_mapping();
        for (template_id, actor_name) in template_mappings {
            if let Some(actor) = reflow_components::get_actor_for_template(&template_id) {
                let _ = network.register_actor_arc(&template_id, actor);
            }
            if let Some(actor) = reflow_components::get_actor_for_template(&template_id) {
                let _ = network.register_actor_arc(&actor_name, actor);
            }
        }

        network.start()?;

        Ok(Arc::new(std::sync::Mutex::new(network)))
    }
}
