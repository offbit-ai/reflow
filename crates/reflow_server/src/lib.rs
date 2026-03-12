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
//! │  trace_collector ► EngineEvent → ZIP trace sessions    │
//! │  peer_mesh   ──► Distributed peer-to-peer mesh         │
//! │  zeal_converter ► Zeal workflow → Reflow graph         │
//! └────────────────────────────────────────────────────────┘
//! ```
//!
//! All ZIP protocol types come from the `zeal-sdk` crate.

pub mod engine;
pub mod peer_mesh;
pub mod rest_api;
pub mod trace_collector;
pub mod zeal_converter;
pub mod zip_session;

use std::sync::Arc;

use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};

// Re-export key types for external consumers and main.rs
pub use engine::{EngineEvent, ExecutionEngine, ExecutionState, ExecutionStatus};
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
        }
    }
}

// ============================================================================
// Server Startup
// ============================================================================

/// Start the Reflow server with all subsystems.
pub async fn start_server(config: Option<ServerConfig>) -> Result<()> {
    let config = config.unwrap_or_default();

    // 1. Create the shared execution engine
    let engine = Arc::new(ExecutionEngine::new());

    // 2. Build the REST API router
    let app = rest_api::build_router(engine.clone());

    // 3. Optionally start a ZIP session to Zeal
    if let Some(zeal_url) = &config.zeal_url {
        let zip_config = zip_session::ZipSessionConfig {
            zeal_url: zeal_url.clone(),
            namespace: config.namespace.clone(),
            node_id: config.node_id.clone(),
            api_key: None,
            capabilities: vec!["distributed".into(), "streaming".into()],
        };

        let session = zip_session::ZipSession::new(zip_config, engine.clone())?;

        // Run the ZIP session in the background
        tokio::spawn(async move {
            if let Err(e) = session.start().await {
                log::error!("ZIP session error: {}", e);
            }
        });

        info!("ZIP session started → {}", zeal_url);
    }

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
        let _router = rest_api::build_router(engine);
    }
}
