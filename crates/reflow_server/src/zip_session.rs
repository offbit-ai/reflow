//! ZIP Session — outbound connection from this Reflow node to a Zeal IDE.
//!
//! Uses `zeal-sdk` for all protocol types and API calls. This module:
//! - Connects to Zeal and authenticates with node identity
//! - Registers reflow_components templates on connect
//! - Opens a WebSocket to `/ws/zip` for real-time event emission
//! - Translates [`EngineEvent`]s into ZIP events and pushes them to Zeal
//! - Handles distributed orchestration commands (subgraph.assign, peer.connect)

use std::sync::Arc;

use anyhow::Result;
use futures_util::SinkExt;
use log::{debug, error, info, warn};
use tokio::sync::{Mutex, Notify};
use zeal_sdk::events::{
    ExecutionCompletedOptions, ExecutionError, ExecutionFailedOptions, ExecutionSummary,
    NodeCompletedOptions, NodeError, ZipExecutionEvent, create_execution_completed_event,
    create_execution_failed_event, create_execution_started_event, create_node_completed_event,
    create_node_executing_event, create_node_failed_event,
};
use zeal_sdk::types::{
    CategoryRegistration, NodeTemplate, RegisterCategoriesRequest, RegisterTemplatesRequest,
    RuntimeRequirements, SubcategoryRegistration, WebhookConfig,
};
use zeal_sdk::{ClientConfig, WS_PATH, ZealClient};

use crate::engine::{EngineEvent, EngineEventType, ExecutionEngine};
use crate::template_adapter::to_zeal_template;
use reflow_network::template::TemplateRegistry;

// ============================================================================
// Session Configuration
// ============================================================================

/// Configuration for connecting to a Zeal IDE instance.
#[derive(Debug, Clone)]
pub struct ZipSessionConfig {
    /// Zeal server URL (e.g. "http://localhost:3000")
    pub zeal_url: String,
    /// This Reflow server's public URL (e.g. "http://localhost:8080")
    /// Used for webhook registration so Zeal can reach us.
    pub server_url: String,
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
// WebSocket Sender
// ============================================================================

type WsSink = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    tokio_tungstenite::tungstenite::Message,
>;

type WsStream = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Thread-safe WebSocket sender for pushing ZIP events to Zeal.
struct ZipWebSocket {
    sink: Mutex<Option<WsSink>>,
}

impl ZipWebSocket {
    fn new() -> Self {
        Self {
            sink: Mutex::new(None),
        }
    }

    /// Connect to Zeal's WebSocket endpoint. Returns the read half for incoming messages.
    async fn connect(&self, zeal_url: &str) -> Result<WsStream> {
        let ws_url = Self::http_to_ws(zeal_url);
        info!("[ZipWS] connecting to {}", ws_url);

        let (ws_stream, _response) = tokio_tungstenite::connect_async(&ws_url).await?;
        let (sink, read) = futures_util::StreamExt::split(ws_stream);

        *self.sink.lock().await = Some(sink);
        info!("[ZipWS] connected to {}", ws_url);
        Ok(read)
    }

    /// Send a serializable event as a JSON text frame.
    async fn send<T: serde::Serialize>(&self, event: &T) -> Result<()> {
        let json = serde_json::to_string(event)?;
        let mut guard = self.sink.lock().await;
        if let Some(sink) = guard.as_mut() {
            sink.send(tokio_tungstenite::tungstenite::Message::Text(json))
                .await?;
            Ok(())
        } else {
            anyhow::bail!("WebSocket not connected");
        }
    }

    /// Send a binary frame over the WebSocket.
    ///
    /// Used for streaming binary data (image/audio chunks) to Zeal display
    /// nodes. The frame format is:
    ///
    /// ```text
    /// [1 byte: frame_type] [8 bytes: stream_id (LE)] [payload...]
    ///   0x01 = Begin (payload = JSON metadata)
    ///   0x02 = Data  (payload = raw bytes)
    ///   0x03 = End   (no payload)
    ///   0x04 = Error (payload = UTF-8 error message)
    /// ```
    async fn send_binary(&self, data: Vec<u8>) -> Result<()> {
        let mut guard = self.sink.lock().await;
        if let Some(sink) = guard.as_mut() {
            sink.send(tokio_tungstenite::tungstenite::Message::Binary(data))
                .await?;
            Ok(())
        } else {
            anyhow::bail!("WebSocket not connected");
        }
    }

    /// Convert http(s) URL to ws(s) URL with the ZIP path.
    fn http_to_ws(url: &str) -> String {
        let base = url
            .replace("https://", "wss://")
            .replace("http://", "ws://");
        format!("{}{}", base.trim_end_matches('/'), WS_PATH)
    }
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
    ws: ZipWebSocket,
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
            ws: ZipWebSocket::new(),
            engine,
            shutdown: Arc::new(Notify::new()),
        })
    }

    /// Connect to Zeal, register templates, open WebSocket, and start the event loop.
    ///
    /// This runs until the session is shut down or the connection drops.
    pub async fn start(&self) -> Result<()> {
        info!(
            "ZIP session connecting to {} as node '{}'",
            self.config.zeal_url, self.config.node_id
        );

        // Step 1a: Register categories so Zeal's palette shows proper grouping
        self.register_categories().await?;

        // Step 1b: Register all reflow_components templates with Zeal
        self.register_templates().await?;

        // Step 1c: Register webhook so Zeal routes execution commands to us
        self.register_webhook().await?;

        // Step 2: Open the real-time WebSocket channel
        let ws_reader = match self.ws.connect(&self.config.zeal_url).await {
            Ok(read) => {
                info!("ZIP WebSocket connected to {}", self.config.zeal_url);
                Some(read)
            }
            Err(e) => {
                warn!(
                    "ZIP WebSocket connection failed (traces will use HTTP only): {}",
                    e
                );
                None
            }
        };

        info!(
            "ZIP session active — listening for commands from {}",
            self.config.zeal_url
        );

        // Step 3: Listen for inbound commands from Zeal
        if let Some(mut reader) = ws_reader {
            let engine = self.engine.clone();
            let shutdown = self.shutdown.clone();

            tokio::select! {
                _ = shutdown.notified() => {
                    info!("ZIP session shutdown requested");
                }
                _ = Self::handle_inbound_messages(&mut reader, &engine) => {
                    warn!("ZIP WebSocket read stream ended");
                }
            }
        } else {
            // No WebSocket — just wait for shutdown
            self.shutdown.notified().await;
        }

        info!("ZIP session disconnected from {}", self.config.zeal_url);
        Ok(())
    }

    /// Handle inbound WebSocket messages from Zeal.
    ///
    /// Zeal sends JSON commands for workflow execution, publish, and control.
    /// Binary frames are currently not expected inbound.
    async fn handle_inbound_messages(reader: &mut WsStream, engine: &Arc<ExecutionEngine>) {
        use futures_util::StreamExt;

        while let Some(msg) = reader.next().await {
            match msg {
                Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                        Self::dispatch_command(json, engine).await;
                    } else {
                        warn!(
                            "[ZipWS] Received non-JSON text: {}...",
                            &text[..text.len().min(100)]
                        );
                    }
                }
                Ok(tokio_tungstenite::tungstenite::Message::Binary(data)) => {
                    debug!("[ZipWS] Received binary frame ({} bytes)", data.len());
                }
                Ok(tokio_tungstenite::tungstenite::Message::Ping(_)) => {
                    // tungstenite handles pong automatically
                }
                Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => {
                    info!("[ZipWS] Zeal closed the connection");
                    break;
                }
                Err(e) => {
                    error!("[ZipWS] Read error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    }

    /// Dispatch a JSON command from Zeal to the appropriate handler.
    async fn dispatch_command(cmd: serde_json::Value, engine: &Arc<ExecutionEngine>) {
        let cmd_type = cmd.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match cmd_type {
            // Zeal requests workflow execution
            "execution.start" | "workflow.execute" => {
                let workflow_id = cmd
                    .get("workflowId")
                    .or(cmd.get("workflow_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let input = cmd
                    .get("input")
                    .or(cmd.get("data"))
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                info!("[ZipWS] Execution command for workflow '{}'", workflow_id);

                match engine.start_webhook_execution(workflow_id, input).await {
                    Ok((execution_id, _rx)) => {
                        info!("[ZipWS] Started execution: {}", execution_id);
                    }
                    Err(e) => {
                        error!("[ZipWS] Execution failed for '{}': {}", workflow_id, e);
                    }
                }
            }

            // Zeal publishes a workflow graph
            "workflow.publish" | "workflow.deploy" => {
                let workflow_id = cmd
                    .get("workflowId")
                    .or(cmd.get("workflow_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                if let Some(graph) = cmd.get("graph").or(cmd.get("workflow")) {
                    // Try to convert from Zeal format if it has graphs[]
                    let graph_json = if graph.get("graphs").is_some() {
                        match serde_json::from_value::<crate::zeal_converter::ZealWorkflow>(
                            graph.clone(),
                        ) {
                            Ok(zeal_wf) => {
                                match crate::zeal_converter::convert_zeal_to_graph_export(&zeal_wf)
                                {
                                    Ok(export) => serde_json::to_value(export).unwrap_or_default(),
                                    Err(e) => {
                                        error!("[ZipWS] Graph conversion failed: {}", e);
                                        return;
                                    }
                                }
                            }
                            Err(_) => graph.clone(),
                        }
                    } else {
                        graph.clone()
                    };

                    engine.store_workflow(workflow_id, graph_json);
                    info!(
                        "[ZipWS] Published workflow '{}' — webhook: /webhook/{}",
                        workflow_id, workflow_id
                    );
                }
            }

            // Zeal unpublishes a workflow — remove from store
            "workflow.unpublish" | "workflow.undeploy" | "workflow.delete" => {
                let workflow_id = cmd
                    .get("workflowId")
                    .or(cmd.get("workflow_id"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                engine.remove_workflow(workflow_id);
                info!(
                    "[ZipWS] Unpublished workflow '{}' — webhook removed",
                    workflow_id
                );
            }

            // Ping/pong for keepalive
            "ping" => {
                debug!("[ZipWS] Received ping");
            }

            _ => {
                debug!("[ZipWS] Unknown command type: '{}'", cmd_type);
            }
        }
    }

    /// Shut down the session gracefully.
    pub fn shutdown(&self) {
        self.shutdown.notify_one();
    }

    /// Register webhook so Zeal routes execution commands to us.
    ///
    /// Zeal calls POST /api/workflows/{id}/execute and sends the
    /// execution command over the ZIP WebSocket. The webhook tells
    /// Zeal that this Reflow instance handles executions for the
    /// registered namespace.
    async fn register_webhook(&self) -> Result<()> {
        // Build our webhook URL — Zeal will POST execution events here
        // In Zeal-mode, commands come via WebSocket, not HTTP.
        // The webhook registration tells Zeal we're an active executor.
        let webhook_url = format!("{}/webhook", self.config.server_url.trim_end_matches('/'));

        let config = WebhookConfig {
            namespace: self.config.namespace.clone(),
            url: webhook_url,
            events: Some(vec![
                "execution.started".to_string(),
                "execution.completed".to_string(),
                "execution.failed".to_string(),
            ]),
            headers: None,
            metadata: Some({
                let mut m = std::collections::HashMap::new();
                m.insert(
                    "node_id".to_string(),
                    serde_json::json!(self.config.node_id),
                );
                m.insert("executor".to_string(), serde_json::json!("reflow"));
                m
            }),
        };

        match self.client.webhooks().register(config).await {
            Ok(response) => {
                info!(
                    "Registered webhook with Zeal: {} ({})",
                    response.webhook_id, response.url
                );
            }
            Err(e) => {
                warn!("Failed to register webhook with Zeal: {} (non-fatal)", e);
            }
        }

        Ok(())
    }

    /// Register categories and subcategories with Zeal.
    ///
    /// Extends existing Zeal categories (like "media") with new subcategories
    /// for stream processing actors, and adds new top-level categories
    /// where needed.
    async fn register_categories(&self) -> Result<()> {
        let sub = |name: &str, display: &str| SubcategoryRegistration {
            name: name.to_string(),
            display_name: display.to_string(),
            description: None,
        };

        let categories = vec![
            // New top-level category for stream infrastructure
            CategoryRegistration {
                name: "stream".to_string(),
                display_name: "Stream".to_string(),
                description: Some("Stream plumbing and conversion actors".to_string()),
                icon: Some("activity".to_string()),
                subcategories: Some(vec![sub("plumbing", "Plumbing")]),
            },
            // "media" already exists with display/images/audio/video subcategories.
            // We add our new subcategories — the API merges them.
            CategoryRegistration {
                name: "media".to_string(),
                display_name: "Media".to_string(),
                description: Some("Audio, video, and image processing".to_string()),
                icon: Some("film".to_string()),
                subcategories: Some(vec![
                    sub("filters", "Filters"),
                    sub("dynamics", "Dynamics"),
                    sub("analysis", "Analysis"),
                    sub("procedural", "Procedural"),
                    sub("spectral", "Spectral"),
                ]),
            },
            // New top-level: 3D geometry
            CategoryRegistration {
                name: "3d".to_string(),
                display_name: "3D".to_string(),
                description: Some("3D geometry, SDF, mesh operations".to_string()),
                icon: Some("box".to_string()),
                subcategories: Some(vec![
                    sub("sdf", "SDF Primitives"),
                    sub("operations", "Operations"),
                    sub("transforms", "Transforms"),
                    sub("export", "Export"),
                ]),
            },
        ];

        let request = RegisterCategoriesRequest { categories };

        match self.client.templates().register_categories(request).await {
            Ok(response) => {
                info!(
                    "Registered {} categories with Zeal ({} updated)",
                    response.registered, response.updated
                );
            }
            Err(e) => {
                warn!("Failed to register categories with Zeal: {}", e);
                // Non-fatal — templates will still register, just may not
                // appear in the right palette groups
            }
        }

        Ok(())
    }

    /// Register all native actor templates and pre-generated API actors with Zeal.
    ///
    /// Registers both native actors (HTTP, flow control, media, etc.) and
    /// all pre-generated API service actors with their required env vars,
    /// brand icons, and port declarations.
    async fn register_templates(&self) -> Result<()> {
        let template_mappings = reflow_components::get_template_mapping();
        let mut templates: Vec<NodeTemplate> = Vec::new();

        let version = Some(env!("CARGO_PKG_VERSION").to_string());
        let capabilities = Some(self.config.capabilities.clone());

        // 1. Register native actor templates with rich metadata. The catalog is
        // owned by reflow_components; this server only adapts it to Zeal.
        let mut template_registry = TemplateRegistry::new();
        template_registry
            .register_catalog(reflow_components::template_catalog(&version, &capabilities));
        let stream_templates = template_registry.into_catalog().templates;

        // Track which templates have rich metadata
        let rich_ids: std::collections::HashSet<String> =
            stream_templates.iter().map(|t| t.id.clone()).collect();
        templates.extend(stream_templates.into_iter().map(to_zeal_template));

        // For any template_mappings not covered by rich metadata, add basic entries
        for template_id in template_mappings.keys() {
            if rich_ids.contains(template_id) {
                continue;
            }
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
                display: None,
            });
        }

        // 2. Register pre-generated API actor templates with full metadata
        #[cfg(feature = "api_services")]
        #[cfg(not(clippy))]
        {
            use crate::zeal_converter::ZealPort;
            use zeal_sdk::{PortPosition, PortType};

            let api_infos = reflow_components::get_api_template_infos();
            for info in api_infos {
                let mut ports = Vec::new();

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
                    display: None,
                });
            }
        }

        if templates.is_empty() {
            warn!("No templates to register with Zeal");
            return Ok(());
        }

        let count = templates.len();
        let api_count = count - template_mappings.len();
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

    /// Encode and send a stream frame as a binary WebSocket message.
    ///
    /// Frame wire format: `[type:1][stream_id:8 LE][payload...]`
    pub async fn send_stream_frame(
        &self,
        stream_id: u64,
        frame: &reflow_network::stream::StreamFrame,
    ) -> Result<()> {
        use reflow_network::stream::StreamFrame;

        let mut buf = Vec::with_capacity(9 + 1024);
        let id_bytes = stream_id.to_le_bytes();

        match frame {
            StreamFrame::Begin {
                content_type,
                size_hint,
                metadata,
            } => {
                buf.push(0x01);
                buf.extend_from_slice(&id_bytes);
                let meta_json = serde_json::json!({
                    "content_type": content_type,
                    "size_hint": size_hint,
                    "metadata": metadata,
                });
                buf.extend_from_slice(serde_json::to_vec(&meta_json)?.as_slice());
            }
            StreamFrame::Data(data) => {
                buf.push(0x02);
                buf.extend_from_slice(&id_bytes);
                buf.extend_from_slice(data);
            }
            StreamFrame::End => {
                buf.push(0x03);
                buf.extend_from_slice(&id_bytes);
            }
            StreamFrame::Error(msg) => {
                buf.push(0x04);
                buf.extend_from_slice(&id_bytes);
                buf.extend_from_slice(msg.as_bytes());
            }
        }

        self.ws.send_binary(buf).await
    }

    /// Send a raw JSON value as a text frame (used by EventBridge for
    /// stream lifecycle events that bypass ZipExecutionEvent).
    pub async fn ws_send_raw(&self, value: &serde_json::Value) -> Result<()> {
        self.ws.send(value).await
    }

    /// Push actor output metadata to Zeal as a node property update.
    ///
    /// Called by the EventBridge when an actor completes with output metadata
    /// (e.g. thumbnail, dimensions). Uses the orchestrator REST API directly
    /// so Zeal updates the node in its store — display components then read
    /// the values via `zeal.getProperties()`.
    pub async fn update_node_properties(
        &self,
        workflow_id: &str,
        node_id: &str,
        metadata: serde_json::Value,
    ) -> Result<()> {
        let properties = match metadata {
            serde_json::Value::Object(map) => Some(
                map.into_iter()
                    .collect::<std::collections::HashMap<String, serde_json::Value>>(),
            ),
            _ => None,
        };

        let request = zeal_sdk::orchestrator::UpdateNodeRequest {
            workflow_id: workflow_id.to_string(),
            graph_id: None,
            properties,
            position: None,
        };

        self.client
            .orchestrator()
            .update_node(node_id, request)
            .await
            .map_err(|e| anyhow::anyhow!("orchestrator.update_node: {}", e))?;

        Ok(())
    }

    /// Forward an engine event to Zeal as a ZIP event over WebSocket.
    ///
    /// Called by the event bridge for each execution event.
    /// Falls back to logging if the WebSocket is not connected.
    pub async fn emit_engine_event(&self, event: &EngineEvent) -> Result<()> {
        // Stream lifecycle events bypass ZipExecutionEvent — send as raw JSON
        match &event.event_type {
            EngineEventType::StreamOpened {
                stream_id,
                node_id,
                port,
                content_type,
                size_hint,
            } => {
                debug!(
                    "[ZIP] stream.opened workflow={} node={} stream_id={} port={}",
                    event.workflow_id, node_id, stream_id, port
                );
                let raw = serde_json::json!({
                    "type": "stream.opened",
                    "workflowId": event.workflow_id,
                    "executionId": event.execution_id,
                    "nodeId": node_id,
                    "port": port,
                    "streamId": stream_id,
                    "contentType": content_type,
                    "sizeHint": size_hint,
                });
                let _ = self.ws.send(&raw).await;
                return Ok(());
            }
            EngineEventType::StreamClosed {
                stream_id,
                node_id,
                total_bytes,
            } => {
                debug!(
                    "[ZIP] stream.closed workflow={} node={} stream_id={}",
                    event.workflow_id, node_id, stream_id
                );
                let raw = serde_json::json!({
                    "type": "stream.closed",
                    "workflowId": event.workflow_id,
                    "executionId": event.execution_id,
                    "nodeId": node_id,
                    "streamId": stream_id,
                    "totalBytes": total_bytes,
                });
                let _ = self.ws.send(&raw).await;
                return Ok(());
            }
            EngineEventType::StreamError {
                stream_id,
                node_id,
                error,
            } => {
                warn!(
                    "[ZIP] stream.error workflow={} node={} stream_id={}",
                    event.workflow_id, node_id, stream_id
                );
                let raw = serde_json::json!({
                    "type": "stream.error",
                    "workflowId": event.workflow_id,
                    "executionId": event.execution_id,
                    "nodeId": node_id,
                    "streamId": stream_id,
                    "error": error,
                });
                let _ = self.ws.send(&raw).await;
                return Ok(());
            }
            _ => {}
        }

        // Standard ZIP protocol events
        let zip_event = self.translate_event(event);

        if let Some(zip_event) = zip_event {
            match self.ws.send(&zip_event).await {
                Ok(()) => {
                    debug!(
                        "[ZIP] sent {} for execution={}",
                        zip_event.event_type(),
                        event.execution_id
                    );
                }
                Err(e) => {
                    debug!(
                        "[ZIP] WebSocket send failed (event={}): {}",
                        zip_event.event_type(),
                        e
                    );
                }
            }
        }

        Ok(())
    }

    /// Translate an EngineEvent into a ZipExecutionEvent.
    fn translate_event(&self, event: &EngineEvent) -> Option<ZipExecutionEvent> {
        match &event.event_type {
            EngineEventType::Started => {
                let zip_event = create_execution_started_event(
                    &event.workflow_id,
                    &event.execution_id,
                    &event.workflow_id,
                    None,
                );
                info!(
                    "[ZIP] execution.started workflow={} execution={}",
                    event.workflow_id, event.execution_id
                );
                Some(ZipExecutionEvent::ExecutionStarted(zip_event))
            }

            EngineEventType::NodeExecuting {
                node_id,
                component: _,
            } => {
                let zip_event = create_node_executing_event(
                    &event.workflow_id,
                    node_id,
                    vec![], // input connections
                    None,
                );
                debug!(
                    "[ZIP] node.executing workflow={} node={}",
                    event.workflow_id, node_id
                );
                Some(ZipExecutionEvent::NodeExecuting(zip_event))
            }

            EngineEventType::ActorCompleted {
                actor_id,
                duration_ms,
                output_size,
                output_connections,
                ..
            } => {
                let zip_event = create_node_completed_event(
                    &event.workflow_id,
                    actor_id,
                    output_connections.clone(),
                    Some(NodeCompletedOptions {
                        duration: *duration_ms,
                        output_size: *output_size,
                        ..Default::default()
                    }),
                );
                debug!(
                    "[ZIP] node.completed workflow={} node={} duration={:?}ms",
                    event.workflow_id, actor_id, duration_ms
                );
                Some(ZipExecutionEvent::NodeCompleted(zip_event))
            }

            EngineEventType::ActorFailed {
                actor_id,
                error,
                output_connections,
                ..
            } => {
                let zip_event = create_node_failed_event(
                    &event.workflow_id,
                    actor_id,
                    output_connections.clone(),
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
                Some(ZipExecutionEvent::NodeFailed(zip_event))
            }

            EngineEventType::Completed {
                duration_ms,
                nodes_executed,
                nodes_failed,
            } => {
                let zip_event = create_execution_completed_event(
                    &event.workflow_id,
                    &event.execution_id,
                    *duration_ms,
                    *nodes_executed,
                    Some(ExecutionCompletedOptions {
                        summary: Some(ExecutionSummary {
                            success_count: nodes_executed - nodes_failed,
                            error_count: *nodes_failed,
                            warning_count: 0,
                        }),
                        ..Default::default()
                    }),
                );
                info!(
                    "[ZIP] execution.completed workflow={} execution={} duration={}ms nodes={} failed={}",
                    event.workflow_id,
                    event.execution_id,
                    duration_ms,
                    nodes_executed,
                    nodes_failed
                );
                Some(ZipExecutionEvent::ExecutionCompleted(zip_event))
            }

            EngineEventType::Failed { error, duration_ms } => {
                let zip_event = create_execution_failed_event(
                    &event.workflow_id,
                    &event.execution_id,
                    Some(ExecutionError {
                        message: error.clone(),
                        code: None,
                        node_id: None,
                    }),
                    Some(ExecutionFailedOptions {
                        duration: *duration_ms,
                        ..Default::default()
                    }),
                );
                error!(
                    "[ZIP] execution.failed workflow={} execution={} error={}",
                    event.workflow_id, event.execution_id, error
                );
                Some(ZipExecutionEvent::ExecutionFailed(zip_event))
            }

            // Stream lifecycle + MessageSent + NetworkIdle — handled in emit_engine_event
            _ => None,
        }
    }
}
