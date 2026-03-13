//! ZIP Session — outbound connection from this Reflow node to a Zeal IDE.
//!
//! Uses `zeal-sdk` for all protocol types and API calls. This module:
//! - Connects to Zeal and authenticates with node identity
//! - Registers reflow_components templates on connect
//! - Listens for inbound commands (execution.start, execution.stop, etc.)
//! - Translates [`EngineEvent`]s into ZIP events and emits them to Zeal
//! - Handles distributed orchestration commands (subgraph.assign, peer.connect)

use std::sync::Arc;

use anyhow::Result;
use log::{error, info, warn};
use tokio::sync::Notify;
use zeal_sdk::events::{
    ExecutionError, NodeError, create_execution_completed_event, create_execution_failed_event,
    create_execution_started_event, create_node_completed_event, create_node_failed_event,
};
use zeal_sdk::types::{
    NodeTemplate, Port as ZealPort, PortPosition, PortType, RegisterTemplatesRequest,
    RuntimeRequirements,
};
use zeal_sdk::{ClientConfig, ZealClient};

use crate::engine::{EngineEvent, EngineEventType, ExecutionEngine};

// ============================================================================
// Session Configuration
// ============================================================================

/// Configuration for connecting to a Zeal IDE instance.
#[derive(Debug, Clone)]
pub struct ZipSessionConfig {
    /// Zeal server URL (e.g. "http://localhost:3000")
    pub zeal_url: String,
    /// Namespace for template registration (e.g. "reflow")
    pub namespace: String,
    /// Unique identifier for this Reflow node
    pub node_id: String,
    /// API key for authentication (optional)
    pub api_key: Option<String>,
    /// Capabilities this node advertises
    pub capabilities: Vec<String>,
}

// ============================================================================
// ZIP Session
// ============================================================================

/// Manages the lifecycle of a connection to a Zeal IDE instance.
///
/// The session is the bridge between the local execution engine and
/// the remote Zeal IDE. It translates engine events into ZIP protocol
/// events and dispatches inbound commands to the engine.
pub struct ZipSession {
    config: ZipSessionConfig,
    client: ZealClient,
    #[allow(dead_code)]
    engine: Arc<ExecutionEngine>,
    shutdown: Arc<Notify>,
}

impl ZipSession {
    /// Create a new ZIP session. Does not connect yet — call [`start`] for that.
    pub fn new(config: ZipSessionConfig, engine: Arc<ExecutionEngine>) -> Result<Self> {
        let client_config = ClientConfig {
            base_url: config.zeal_url.clone(),
            ..Default::default()
        };

        let client = ZealClient::new(client_config)?;

        Ok(Self {
            config,
            client,
            engine,
            shutdown: Arc::new(Notify::new()),
        })
    }

    /// Connect to Zeal, register templates, and start the event loop.
    ///
    /// This runs until the session is shut down or the connection drops.
    pub async fn start(&self) -> Result<()> {
        info!(
            "ZIP session connecting to {} as node '{}'",
            self.config.zeal_url, self.config.node_id
        );

        // Step 1: Register all reflow_components templates with Zeal
        self.register_templates().await?;

        info!(
            "ZIP session active — listening for commands from {}",
            self.config.zeal_url
        );

        // Step 2: Event loop — listen for commands from Zeal
        // The zeal-sdk WebSocket connection handles the real-time channel.
        // For now we wait for shutdown signal; the WebSocket listener
        // will be wired in when the SDK's event subscription is integrated.
        self.shutdown.notified().await;

        info!("ZIP session disconnected from {}", self.config.zeal_url);
        Ok(())
    }

    /// Shut down the session gracefully.
    pub fn shutdown(&self) {
        self.shutdown.notify_one();
    }

    /// Register all reflow_components templates with the connected Zeal instance.
    ///
    /// Registers both native actors (HTTP, flow control, media, etc.) and
    /// all pre-generated API service actors with their required env vars,
    /// brand icons, and port declarations.
    async fn register_templates(&self) -> Result<()> {
        let template_mappings = reflow_components::get_template_mapping();
        let mut templates: Vec<NodeTemplate> = Vec::new();

        let version = Some(env!("CARGO_PKG_VERSION").to_string());
        let capabilities = Some(self.config.capabilities.clone());

        // 1. Register native actor templates
        for template_id in template_mappings.keys() {
            templates.push(NodeTemplate {
                id: template_id.clone(),
                type_name: template_id.clone(),
                title: template_id.replace("tpl_", "").replace('_', " "),
                subtitle: None,
                category: "reflow".to_string(),
                subcategory: None,
                description: format!("Reflow actor: {}", template_id),
                icon: "cpu".to_string(),
                variant: None,
                shape: None,
                size: None,
                ports: vec![],
                properties: None,
                property_rules: None,
                runtime: Some(RuntimeRequirements {
                    executor: "reflow".to_string(),
                    version: version.clone(),
                    required_env_vars: None,
                    capabilities: capabilities.clone(),
                }),
            });
        }

        // 2. Register pre-generated API actor templates with full metadata
        let api_infos = reflow_components::get_api_template_infos();
        for info in api_infos {
            let mut ports = Vec::new();

            // Declare inports
            for port_name in info.inports {
                ports.push(ZealPort {
                    id: port_name.to_string(),
                    label: port_name.replace('_', " "),
                    port_type: PortType::Input,
                    position: PortPosition::Left,
                    data_type: Some("any".to_string()),
                    required: None,
                    multiple: None,
                });
            }

            // Declare outports
            for port_name in info.outports {
                ports.push(ZealPort {
                    id: port_name.to_string(),
                    label: port_name.replace('_', " "),
                    port_type: PortType::Output,
                    position: PortPosition::Right,
                    data_type: Some("any".to_string()),
                    required: None,
                    multiple: None,
                });
            }

            templates.push(NodeTemplate {
                id: info.template_id.to_string(),
                type_name: info.template_id.to_string(),
                title: info.title.to_string(),
                subtitle: Some(info.subcategory.to_string()),
                category: info.category.to_string(),
                subcategory: Some(info.subcategory.to_string()),
                description: info.description.to_string(),
                icon: info.icon.to_string(),
                variant: None,
                shape: None,
                size: None,
                ports,
                properties: None,
                property_rules: None,
                runtime: Some(RuntimeRequirements {
                    executor: "reflow".to_string(),
                    version: version.clone(),
                    required_env_vars: Some(vec![info.env_var.to_string()]),
                    capabilities: capabilities.clone(),
                }),
            });
        }

        if templates.is_empty() {
            warn!("No templates to register with Zeal");
            return Ok(());
        }

        let count = templates.len();
        let api_count = api_infos.len();
        let request = RegisterTemplatesRequest {
            namespace: self.config.namespace.clone(),
            templates,
            webhook_url: None,
        };

        match self.client.templates().register(request).await {
            Ok(response) => {
                info!(
                    "Registered {} templates with Zeal ({} native + {} API actors, acknowledged: {})",
                    count,
                    count - api_count,
                    api_count,
                    response.registered
                );
            }
            Err(e) => {
                error!("Failed to register templates with Zeal: {}", e);
                return Err(e.into());
            }
        }

        Ok(())
    }

    /// Forward an engine event to Zeal as a ZIP event.
    ///
    /// Called by the trace collector or event bridge when the engine
    /// emits lifecycle events during execution.
    pub async fn emit_engine_event(&self, event: &EngineEvent) -> Result<()> {
        match &event.event_type {
            EngineEventType::Started => {
                let _zip_event = create_execution_started_event(
                    &event.workflow_id,
                    &event.execution_id,
                    &event.workflow_id, // workflow name — use ID as fallback
                    None,
                );
                // TODO: send via WebSocket when SDK event emitter is wired
                info!(
                    "[ZIP] execution.started workflow={} execution={}",
                    event.workflow_id, event.execution_id
                );
            }
            EngineEventType::ActorCompleted { actor_id } => {
                let _zip_event = create_node_completed_event(
                    &event.workflow_id,
                    actor_id,
                    vec![], // output connections — will be populated from graph metadata
                    None,
                );
                info!(
                    "[ZIP] node.completed workflow={} node={}",
                    event.workflow_id, actor_id
                );
            }
            EngineEventType::ActorFailed { actor_id, error } => {
                let _zip_event = create_node_failed_event(
                    &event.workflow_id,
                    actor_id,
                    vec![],
                    Some(NodeError {
                        message: error.clone(),
                        code: None,
                        stack: None,
                    }),
                    None,
                );
                warn!(
                    "[ZIP] node.failed workflow={} node={} error={}",
                    event.workflow_id, actor_id, error
                );
            }
            EngineEventType::Completed => {
                let _zip_event = create_execution_completed_event(
                    &event.workflow_id,
                    &event.execution_id,
                    0, // duration — will be computed from engine timing
                    0, // nodes executed — will be tracked by engine
                    None,
                );
                info!(
                    "[ZIP] execution.completed workflow={} execution={}",
                    event.workflow_id, event.execution_id
                );
            }
            EngineEventType::Failed { error } => {
                let _zip_event = create_execution_failed_event(
                    &event.workflow_id,
                    &event.execution_id,
                    Some(ExecutionError {
                        message: error.clone(),
                        code: None,
                        node_id: None,
                    }),
                    None,
                );
                error!(
                    "[ZIP] execution.failed workflow={} execution={} error={}",
                    event.workflow_id, event.execution_id, error
                );
            }
            EngineEventType::NetworkIdle => {
                // No direct ZIP mapping — informational only
            }
        }

        Ok(())
    }
}
