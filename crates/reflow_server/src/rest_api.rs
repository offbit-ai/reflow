//! REST API — HTTP handlers for direct (non-Zeal) workflow execution.
//!
//! This module provides a thin HTTP layer over the execution engine
//! for headless use cases where no Zeal IDE is connected.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, State, WebSocketUpgrade, ws},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
};
use log::{error, info};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;

use crate::engine::{ExecutionEngine, ExecutionState};
use crate::event_bridge::EventBridge;
use crate::zeal_converter::{ZealWorkflow, convert_zeal_to_graph_export};

// ============================================================================
// Request / Response Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowExecutionRequest {
    pub graph_json: serde_json::Value,
    pub input: serde_json::Value,
    pub metadata: ExecutionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    pub execution_id: String,
    pub workflow_id: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_headers: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_webhook_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_tracing: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub timestamp: u64,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        }
    }
}

// ============================================================================
// App State (shared across handlers)
// ============================================================================

#[derive(Clone)]
pub struct AppState {
    pub engine: Arc<ExecutionEngine>,
    /// When set, execution events are forwarded to TraceCollector + ZipSession.
    pub event_bridge: Option<Arc<EventBridge>>,
}

// ============================================================================
// Handlers
// ============================================================================

async fn health_check() -> impl IntoResponse {
    Json(ApiResponse::success("Server is healthy"))
}

async fn start_workflow(
    State(state): State<AppState>,
    Json(request): Json<WorkflowExecutionRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let workflow_id = request.metadata.workflow_id.clone();
    let execution_id = request.metadata.execution_id.clone();

    match state
        .engine
        .start_execution(
            request.graph_json,
            request.input,
            execution_id.clone(),
            workflow_id.clone(),
        )
        .await
    {
        Ok((execution_id, event_rx)) => {
            // Attach the event bridge to forward events to Zeal
            if let Some(bridge) = &state.event_bridge {
                bridge.attach(workflow_id, execution_id.clone(), event_rx);
            }

            Ok(Json(ApiResponse::success(serde_json::json!({
                "execution_id": execution_id,
                "status": "started",
                "message": "Workflow started in background"
            }))))
        }
        Err(e) => {
            error!("Workflow start error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_execution_status(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
) -> Result<Json<ApiResponse<ExecutionState>>, StatusCode> {
    match state.engine.get_execution(&execution_id) {
        Some(execution) => Ok(Json(ApiResponse::success(execution))),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn cancel_workflow(
    State(state): State<AppState>,
    Path(execution_id): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    match state.engine.cancel_execution(&execution_id).await {
        Ok(()) => Ok(Json(ApiResponse::success(serde_json::json!({
            "execution_id": execution_id,
            "status": "cancelled"
        })))),
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

async fn execute_zeal_workflow(
    State(state): State<AppState>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    let zeal_workflow: ZealWorkflow = serde_json::from_value(
        request
            .get("workflow")
            .ok_or(StatusCode::BAD_REQUEST)?
            .clone(),
    )
    .map_err(|_| StatusCode::BAD_REQUEST)?;

    let input = request
        .get("input")
        .unwrap_or(&serde_json::Value::Null)
        .clone();

    let workflow_id = zeal_workflow.id.clone();

    match state
        .engine
        .start_zeal_execution(zeal_workflow, input)
        .await
    {
        Ok((execution_id, event_rx)) => {
            // Attach the event bridge to forward events to Zeal
            if let Some(bridge) = &state.event_bridge {
                bridge.attach(workflow_id, execution_id.clone(), event_rx);
            }

            Ok(Json(ApiResponse::success(serde_json::json!({
                "execution_id": execution_id,
                "status": "started",
                "message": "Zeal workflow converted and started"
            }))))
        }
        Err(e) => {
            error!("Zeal workflow execution error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn convert_zeal_workflow(
    Json(zeal_workflow): Json<ZealWorkflow>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    match convert_zeal_to_graph_export(&zeal_workflow) {
        Ok(graph_export) => {
            let required_actors: Vec<String> = zeal_workflow
                .graphs
                .iter()
                .flat_map(|g| &g.nodes)
                .filter_map(|node| node.template_id.as_ref())
                .cloned()
                .collect();

            Ok(Json(ApiResponse::success(serde_json::json!({
                "reflow_graph": graph_export,
                "required_actors": required_actors,
                "conversion_metadata": {
                    "source_workflow_id": zeal_workflow.id,
                    "source_workflow_name": zeal_workflow.name,
                    "node_count": zeal_workflow.graphs.iter().map(|g| g.nodes.len()).sum::<usize>(),
                    "connection_count": zeal_workflow.graphs.iter().map(|g| g.connections.len()).sum::<usize>(),
                }
            }))))
        }
        Err(e) => {
            error!("Zeal workflow conversion error: {}", e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

/// Publish a workflow graph to Reflow.
///
/// Zeal pushes a workflow graph here. Reflow stores it and returns
/// a webhook URL that can trigger execution of this workflow.
///
/// POST /workflows/{workflow_id}/publish
/// Body: { "workflow": ZealWorkflow } or { "graph": GraphExport }
async fn publish_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(request): Json<serde_json::Value>,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    // Accept either a Zeal workflow or a raw graph
    let graph_json = if let Some(workflow) = request.get("workflow") {
        let zeal_workflow: ZealWorkflow =
            serde_json::from_value(workflow.clone()).map_err(|_| StatusCode::BAD_REQUEST)?;
        let graph_export = convert_zeal_to_graph_export(&zeal_workflow)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
        serde_json::to_value(graph_export).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else if let Some(graph) = request.get("graph") {
        graph.clone()
    } else {
        return Err(StatusCode::BAD_REQUEST);
    };

    // Store the graph for webhook triggering
    state.engine.store_workflow(&workflow_id, graph_json);

    // The webhook URL for this workflow
    let webhook_path = format!("/webhook/{}", workflow_id);

    info!("Workflow '{}' published — webhook: {}", workflow_id, webhook_path);

    Ok(Json(ApiResponse::success(serde_json::json!({
        "workflow_id": workflow_id,
        "status": "published",
        "webhook": webhook_path,
    }))))
}

/// Webhook trigger endpoint.
///
/// When a POST hits /webhook/{workflow_id}, the server:
/// 1. Looks up the workflow graph (stored by a prior /zeal/workflows call)
/// 2. Finds all ServerRequestActor nodes in the graph
/// 3. Injects the HTTP request data (body, headers, method, path) into their config
/// 4. Starts the workflow execution
///
/// The ServerRequestActor reads the request from its config and outputs
/// structured fields (body, headers, params, method, url) to downstream actors.
async fn handle_webhook(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, StatusCode> {
    // Parse body as JSON (fall back to string)
    let body_json = serde_json::from_slice::<serde_json::Value>(&body)
        .unwrap_or_else(|_| serde_json::json!(String::from_utf8_lossy(&body).to_string()));

    // Extract headers as JSON object
    let headers_json: serde_json::Map<String, serde_json::Value> = headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str().ok().map(|val| (k.to_string(), serde_json::json!(val)))
        })
        .collect();

    // Build the request data that ServerRequestActor will read from config
    let request_data = serde_json::json!({
        "body": body_json,
        "headers": headers_json,
        "method": "POST",
        "path": format!("/webhook/{}", workflow_id),
    });

    // Execute the workflow with the request data as input.
    // The engine's run_execution injects this into ServerRequestActor nodes' config.
    match state
        .engine
        .start_webhook_execution(&workflow_id, request_data.clone())
        .await
    {
        Ok((execution_id, event_rx)) => {
            if let Some(bridge) = &state.event_bridge {
                bridge.attach(workflow_id.clone(), execution_id.clone(), event_rx);
            }

            Ok(Json(ApiResponse::success(serde_json::json!({
                "execution_id": execution_id,
                "workflow_id": workflow_id,
                "status": "triggered",
            }))))
        }
        Err(e) => {
            error!("Webhook execution error for {}: {}", workflow_id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_websocket(socket, state))
}

async fn handle_websocket(mut socket: ws::WebSocket, state: AppState) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);

    loop {
        tokio::select! {
            Some(msg) = socket.recv() => {
                match msg {
                    Ok(ws::Message::Text(text)) => {
                        let tx_clone = tx.clone();
                        let state_clone = state.clone();
                        tokio::spawn(async move {
                            handle_ws_message(&text, &state_clone, &tx_clone).await;
                        });
                    }
                    Ok(ws::Message::Close(_)) | Err(_) => break,
                    _ => {}
                }
            }
            Some(msg) = rx.recv() => {
                if socket.send(ws::Message::Text(msg)).await.is_err() {
                    break;
                }
            }
            else => break,
        }
    }
}

async fn handle_ws_message(
    message: &str,
    state: &AppState,
    tx: &tokio::sync::mpsc::Sender<String>,
) {
    let json: serde_json::Value = match serde_json::from_str(message) {
        Ok(v) => v,
        Err(e) => {
            let _ = send_json(
                tx,
                serde_json::json!({"type": "error", "error": format!("Invalid JSON: {}", e)}),
            )
            .await;
            return;
        }
    };

    let msg_type = match json.get("type").and_then(|t| t.as_str()) {
        Some(t) => t,
        None => {
            let _ = send_json(
                tx,
                serde_json::json!({"type": "error", "error": "Missing message type"}),
            )
            .await;
            return;
        }
    };

    match msg_type {
        "start_workflow" => {
            if let Some(data) = json.get("data") {
                match serde_json::from_value::<WorkflowExecutionRequest>(data.clone()) {
                    Ok(request) => {
                        let workflow_id = request.metadata.workflow_id.clone();
                        match state
                            .engine
                            .start_execution(
                                request.graph_json,
                                request.input,
                                request.metadata.execution_id.clone(),
                                workflow_id.clone(),
                            )
                            .await
                        {
                            Ok((execution_id, event_rx)) => {
                                // Attach event bridge
                                if let Some(bridge) = &state.event_bridge {
                                    bridge.attach(workflow_id, execution_id.clone(), event_rx);
                                }

                                let _ = send_json(
                                    tx,
                                    serde_json::json!({
                                        "type": "workflow_started",
                                        "success": true,
                                        "execution_id": execution_id,
                                    }),
                                )
                                .await;
                            }
                            Err(e) => {
                                let _ = send_json(
                                    tx,
                                    serde_json::json!({
                                        "type": "workflow_error",
                                        "error": e.to_string(),
                                    }),
                                )
                                .await;
                            }
                        }
                    }
                    Err(e) => {
                        let _ = send_json(
                            tx,
                            serde_json::json!({
                                "type": "error",
                                "error": format!("Invalid request: {}", e),
                            }),
                        )
                        .await;
                    }
                }
            }
        }
        "subscribe_workflow" => {
            if let Some(execution_id) = json.get("execution_id").and_then(|id| id.as_str()) {
                if let Some(event_rx) = state.engine.get_network_receiver(execution_id).await {
                    let tx_clone = tx.clone();
                    let eid = execution_id.to_string();
                    tokio::spawn(async move {
                        while let Ok(event) = event_rx.recv_async().await {
                            let msg = serde_json::json!({
                                "type": "network_event",
                                "execution_id": &eid,
                                "event": event,
                                "timestamp": chrono::Utc::now().timestamp_millis(),
                            });
                            if send_json(&tx_clone, msg).await.is_err() {
                                break;
                            }
                        }
                    });
                    let _ = send_json(
                        tx,
                        serde_json::json!({
                            "type": "subscription_ack",
                            "success": true,
                            "execution_id": execution_id,
                        }),
                    )
                    .await;
                } else {
                    let _ = send_json(
                        tx,
                        serde_json::json!({
                            "type": "error",
                            "error": format!("Execution {} not found or not ready", execution_id),
                        }),
                    )
                    .await;
                }
            }
        }
        "cancel_workflow" => {
            if let Some(execution_id) = json.get("execution_id").and_then(|id| id.as_str()) {
                match state.engine.cancel_execution(execution_id).await {
                    Ok(()) => {
                        let _ = send_json(
                            tx,
                            serde_json::json!({
                                "type": "workflow_cancelled",
                                "success": true,
                                "execution_id": execution_id,
                            }),
                        )
                        .await;
                    }
                    Err(e) => {
                        let _ = send_json(
                            tx,
                            serde_json::json!({
                                "type": "error",
                                "error": e.to_string(),
                            }),
                        )
                        .await;
                    }
                }
            }
        }
        _ => {
            let _ = send_json(
                tx,
                serde_json::json!({
                    "type": "error",
                    "error": format!("Unknown message type: {}", msg_type),
                }),
            )
            .await;
        }
    }
}

async fn send_json(
    tx: &tokio::sync::mpsc::Sender<String>,
    value: serde_json::Value,
) -> Result<(), ()> {
    if let Ok(msg) = serde_json::to_string(&value) {
        tx.send(msg).await.map_err(|_| ())
    } else {
        Err(())
    }
}

// ============================================================================
// Router Setup
// ============================================================================

/// Build the REST API router with all handlers.
pub fn build_router(
    engine: Arc<ExecutionEngine>,
    event_bridge: Option<Arc<EventBridge>>,
) -> Router {
    let state = AppState {
        engine,
        event_bridge,
    };

    Router::new()
        .route("/health", get(health_check))
        .route("/workflows", post(start_workflow))
        .route("/workflows/{execution_id}", get(get_execution_status))
        .route("/workflows/{execution_id}/cancel", post(cancel_workflow))
        .route("/zeal/workflows", post(execute_zeal_workflow))
        .route("/zeal/convert", post(convert_zeal_workflow))
        .route("/workflows/{workflow_id}/publish", post(publish_workflow))
        .route("/webhook/{workflow_id}", post(handle_webhook))
        .route("/webhook/{workflow_id}/{path:.*}", post(handle_webhook))
        .route("/ws", get(websocket_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
