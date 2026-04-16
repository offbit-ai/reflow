//! Reflow Server — a single node in the Reflow distributed workflow network.
//!
//! # Architecture
//!
//! ```text
//! ┌────────────────────────────────────────────────────────┐
//! │                   reflow_server                        │
//! │                                                        │
//! │  zip_session ──► Outbound connection to Zeal IDE       │
//! │  rest_api    ──► Inbound HTTP API for direct callers   │
//! │  engine      ──► Core execution lifecycle              │
//! │  event_bridge ► EngineEvent → TraceCollector + ZIP WS  │
//! │  trace_collector ► EngineEvent → ZIP trace sessions    │
//! │  peer_mesh   ──► Distributed peer-to-peer mesh         │
//! │  zeal_converter ► Zeal workflow → Reflow graph         │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! All ZIP protocol types come from the `zeal-sdk` crate.
#![allow(
    dead_code,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::too_many_arguments,
    clippy::while_let_loop
)]

pub mod engine;
pub mod event_bridge;
pub mod peer_mesh;
pub mod rest_api;
pub mod template_metadata;
pub mod trace_collector;
pub mod workflow_store;
pub mod zeal_converter;
pub mod zip_session;

use std::sync::Arc;

use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};

// Re-export key types for external consumers and main.rs
pub use engine::{EngineEvent, ExecutionEngine, ExecutionState, ExecutionStatus};
pub use event_bridge::EventBridge;
pub use rest_api::{ApiResponse, AppState, ExecutionMetadata, WorkflowExecutionRequest};

// ============================================================================
// Server Configuration
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub port: u16,
    pub bind_address: String,
    pub max_connections: usize,
    pub cors_enabled: bool,
    pub rate_limit_requests_per_minute: usize,
    /// Optional Zeal IDE URL to connect to via ZIP.
    /// When set, the server establishes an outbound ZIP session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zeal_url: Option<String>,
    /// Namespace for ZIP template registration (default: "reflow").
    #[serde(default = "default_namespace")]
    pub namespace: String,
    /// Unique node ID in the distributed mesh.
    #[serde(default = "default_node_id")]
    pub node_id: String,
    /// Optional Redis URL for workflow persistence.
    /// When set, published workflows survive server restarts.
    /// Falls back to in-memory when not set or Redis is unreachable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url: Option<String>,
}

fn default_namespace() -> String {
    "reflow".to_string()
}

fn default_node_id() -> String {
    format!("reflow-{}", &uuid::Uuid::new_v4().to_string()[..8])
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 8080,
            bind_address: "0.0.0.0".to_string(),
            max_connections: 1000,
            cors_enabled: true,
            rate_limit_requests_per_minute: 100,
            zeal_url: None,
            namespace: default_namespace(),
            node_id: default_node_id(),
            redis_url: None,
        }
    }
}

// ============================================================================
// Server Startup
// ============================================================================

/// Start the Reflow server with all subsystems.
pub async fn start_server(config: Option<ServerConfig>) -> Result<()> {
    let config = config.unwrap_or_default();

    // 1. Create the shared execution engine (with Redis persistence if configured)
    let engine = Arc::new(ExecutionEngine::new_with_redis(config.redis_url.clone()));

    // 2. Optionally create the observability pipeline (TraceCollector + ZipSession + EventBridge)
    let event_bridge = if let Some(zeal_url) = &config.zeal_url {
        // Trace collector submits per-node data via HTTP
        let trace_collector = Arc::new(trace_collector::TraceCollector::new(zeal_url));

        // ZIP session handles template registration + WebSocket event stream
        let server_url = format!("http://{}:{}", config.bind_address, config.port);

        let zip_config = zip_session::ZipSessionConfig {
            zeal_url: zeal_url.clone(),
            server_url,
            namespace: config.namespace.clone(),
            node_id: config.node_id.clone(),
            api_key: None,
            capabilities: vec!["distributed".into(), "streaming".into()],
        };

        let zip_session = Arc::new(zip_session::ZipSession::new(zip_config, engine.clone())?);

        // Start the ZIP session (template registration + WebSocket + command loop)
        let session_clone = zip_session.clone();
        tokio::spawn(async move {
            if let Err(e) = session_clone.start().await {
                log::error!("ZIP session error: {}", e);
            }
        });

        info!("ZIP session started → {}", zeal_url);

        // Create the event bridge that wires execution events to both consumers
        let bridge = Arc::new(EventBridge::new(Some(trace_collector), Some(zip_session)));

        Some(bridge)
    } else {
        None
    };

    // 3. Build the REST API router with the event bridge
    let app = rest_api::build_router(engine.clone(), event_bridge);

    // 4. Start the HTTP server
    let addr = format!("{}:{}", config.bind_address, config.port);
    info!("Reflow Server starting on: {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_engine_creation() {
        let engine = ExecutionEngine::new();
        assert!(engine.get_execution("nonexistent").is_none());
    }

    #[tokio::test]
    async fn test_start_execution() {
        let engine = ExecutionEngine::new();

        let result = engine
            .start_execution(
                json!({
                    "processes": {},
                    "connections": [],
                    "inports": {},
                    "outports": {},
                    "groups": [],
                    "properties": {}
                }),
                json!({}),
                "test_exec_001".to_string(),
                "test_workflow".to_string(),
            )
            .await;

        assert!(result.is_ok());
        let (eid, _rx) = result.unwrap();
        assert_eq!(eid, "test_exec_001");
        assert!(engine.get_execution("test_exec_001").is_some());
    }

    #[tokio::test]
    async fn test_cancel_nonexistent() {
        let engine = ExecutionEngine::new();
        let result = engine.cancel_execution("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_config_defaults() {
        let config = ServerConfig::default();
        assert_eq!(config.port, 8080);
        assert!(config.zeal_url.is_none());
        assert_eq!(config.namespace, "reflow");
        assert!(config.node_id.starts_with("reflow-"));
    }

    #[tokio::test]
    async fn test_rest_api_router_builds() {
        let engine = Arc::new(ExecutionEngine::new());
        let _router = rest_api::build_router(engine, None);
    }
}
