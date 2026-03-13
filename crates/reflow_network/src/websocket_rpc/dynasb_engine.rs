/// DynASB integration — deployment client and actor factory
///
/// Manages the lifecycle of script actors running on dynASB microVMs.
/// Deployment happens via REST, then reflow communicates via the existing
/// WebSocketScriptActor / WebSocketRpcClient JSON-RPC 2.0 protocol.
///
/// Flow:
///   1. DynASBClient::deploy() → POST /api/v1/functions (deploy code to dynASB)
///   2. DynASBClient::create_actor() → returns WebSocketScriptActor pointing at dynASB WS
///   3. Actor runs normally via JSON-RPC "process" calls
///   4. DynASBClient::undeploy() → POST /api/v1/functions/:id/undeploy

use super::client::WebSocketRpcClient;
use super::script_actor::WebSocketScriptActor;
use crate::script_discovery::types::DiscoveredScriptActor;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Configuration for connecting to a dynASB instance
#[derive(Debug, Clone)]
pub struct DynASBConfig {
    /// Base URL for dynASB REST API (e.g. "http://localhost:8080")
    pub api_url: String,
    /// WebSocket URL for JSON-RPC (e.g. "ws://localhost:8080/ws")
    pub ws_url: String,
    /// Redis URL for actor state persistence
    pub redis_url: String,
}

/// Response from dynASB deploy endpoint
#[derive(Debug, Deserialize)]
struct DeployResponse {
    success: bool,
    function_id: Option<String>,
    #[allow(dead_code)]
    vm_id: Option<String>,
    #[allow(dead_code)]
    endpoint: Option<String>,
    deployment_time_ms: Option<u64>,
    error: Option<String>,
}

/// Deployed function handle with status tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DynASBFunction {
    pub function_id: String,
    pub name: String,
    pub runtime: String,
    pub status: DeploymentStatus,
    pub deployment_time_ms: u64,
    pub vm_id: Option<String>,
}

/// Deployment lifecycle status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DeploymentStatus {
    /// Function deployed, VM booting
    Deploying,
    /// VM ready, health check passed
    Ready,
    /// Health check failed or function errored
    Unhealthy,
    /// Function is being undeployed
    Stopping,
    /// Function has been removed
    Stopped,
}

/// Response from dynASB GET /api/v1/functions/:id
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FunctionStatusResponse {
    id: String,
    #[allow(dead_code)]
    name: String,
    #[allow(dead_code)]
    runtime: String,
    state: String,
    vm_id: String,
    #[allow(dead_code)]
    invocation_count: u64,
}

/// Client for managing script actor deployments on dynASB
pub struct DynASBClient {
    config: DynASBConfig,
    http: reqwest::Client,
    deployed: HashMap<String, DynASBFunction>,
}

impl DynASBClient {
    pub fn new(config: DynASBConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            deployed: HashMap::new(),
        }
    }

    /// Deploy a script actor to dynASB
    ///
    /// Returns the function handle with the ID assigned by dynASB.
    pub async fn deploy(
        &mut self,
        name: &str,
        runtime: &str,
        code: &str,
        handler: &str,
        dependencies: Option<HashMap<String, String>>,
        timeout_seconds: Option<u32>,
    ) -> Result<DynASBFunction> {
        let body = json!({
            "name": name,
            "runtime": runtime,
            "code": code,
            "handler": handler,
            "timeout_seconds": timeout_seconds.unwrap_or(300),
            "dependencies": dependencies,
        });

        let url = format!("{}/api/v1/functions", self.config.api_url);
        info!("Deploying function '{}' to dynASB", name);

        let response: DeployResponse = self.http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Failed to reach dynASB API")?
            .json()
            .await
            .context("Failed to parse deploy response")?;

        if !response.success {
            return Err(anyhow::anyhow!(
                "Deployment failed: {}",
                response.error.unwrap_or_else(|| "unknown error".to_string())
            ));
        }

        let function_id = response.function_id
            .ok_or_else(|| anyhow::anyhow!("No function_id in deploy response"))?;

        info!(
            "Function deployed: id={}, time={}ms",
            function_id,
            response.deployment_time_ms.unwrap_or(0)
        );

        let func = DynASBFunction {
            function_id: function_id.clone(),
            name: name.to_string(),
            runtime: runtime.to_string(),
            status: DeploymentStatus::Deploying,
            deployment_time_ms: response.deployment_time_ms.unwrap_or(0),
            vm_id: response.vm_id,
        };

        self.deployed.insert(function_id.clone(), func.clone());
        Ok(func)
    }

    /// Create a WebSocketScriptActor connected to dynASB for a deployed function.
    ///
    /// The returned actor speaks the same JSON-RPC 2.0 protocol and integrates
    /// seamlessly into reflow networks.
    pub async fn create_actor(
        &self,
        func: &DynASBFunction,
        metadata: DiscoveredScriptActor,
    ) -> Result<WebSocketScriptActor> {
        // WebSocket URL includes function_id for routing
        let ws_url = format!("{}/{}", self.config.ws_url, func.function_id);
        let rpc_client = Arc::new(WebSocketRpcClient::new(ws_url));

        let actor = WebSocketScriptActor::new(
            metadata,
            rpc_client,
            self.config.redis_url.clone(),
        ).await;

        Ok(actor)
    }

    /// Undeploy a function from dynASB
    pub async fn undeploy(&mut self, function_id: &str) -> Result<()> {
        let url = format!(
            "{}/api/v1/functions/{}/undeploy",
            self.config.api_url, function_id
        );

        self.http.post(&url)
            .send()
            .await
            .context("Failed to undeploy function")?;

        self.deployed.remove(function_id);
        info!("Function undeployed: {}", function_id);
        Ok(())
    }

    /// Undeploy all functions
    pub async fn undeploy_all(&mut self) -> Result<()> {
        let ids: Vec<String> = self.deployed.keys().cloned().collect();
        for id in ids {
            let _ = self.undeploy(&id).await;
        }
        Ok(())
    }

    /// Check the health/status of a deployed function by querying dynASB.
    ///
    /// Maps the remote state string to a `DeploymentStatus` and updates
    /// the local tracking record.
    pub async fn health_check(&mut self, function_id: &str) -> Result<DeploymentStatus> {
        let url = format!(
            "{}/api/v1/functions/{}",
            self.config.api_url, function_id
        );

        let resp: FunctionStatusResponse = self.http
            .get(&url)
            .send()
            .await
            .context("Failed to reach dynASB health endpoint")?
            .json()
            .await
            .context("Failed to parse function status response")?;

        let status = match resp.state.as_str() {
            "ready" | "running" => DeploymentStatus::Ready,
            "deploying" | "starting" | "booting" => DeploymentStatus::Deploying,
            "stopping" => DeploymentStatus::Stopping,
            "stopped" | "removed" => DeploymentStatus::Stopped,
            _ => DeploymentStatus::Unhealthy,
        };

        // Update local tracking
        if let Some(func) = self.deployed.get_mut(function_id) {
            func.status = status.clone();
            if func.vm_id.is_none() && !resp.vm_id.is_empty() {
                func.vm_id = Some(resp.vm_id);
            }
        }

        Ok(status)
    }

    /// Poll until a deployed function reports `Ready`, or timeout.
    ///
    /// Checks every `poll_interval` until the function is ready or the
    /// deadline expires.
    pub async fn wait_until_ready(
        &mut self,
        function_id: &str,
        timeout: Duration,
        poll_interval: Duration,
    ) -> Result<DeploymentStatus> {
        let deadline = tokio::time::Instant::now() + timeout;

        loop {
            let status = self.health_check(function_id).await?;

            match status {
                DeploymentStatus::Ready => {
                    info!("Function {} is ready", function_id);
                    return Ok(status);
                }
                DeploymentStatus::Unhealthy | DeploymentStatus::Stopped => {
                    warn!("Function {} entered terminal state: {:?}", function_id, status);
                    return Err(anyhow::anyhow!(
                        "Function {} is {:?}, cannot become ready",
                        function_id, status
                    ));
                }
                DeploymentStatus::Deploying | DeploymentStatus::Stopping => {
                    if tokio::time::Instant::now() >= deadline {
                        return Err(anyhow::anyhow!(
                            "Timed out waiting for function {} to become ready (last status: {:?})",
                            function_id, status
                        ));
                    }
                    debug!("Function {} still {:?}, polling again in {:?}", function_id, status, poll_interval);
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    }

    /// Produce deployment metadata as a flat `HashMap<String, Value>` suitable
    /// for injection into `GraphNode.metadata`.
    ///
    /// Keys are prefixed with `dynasb.` to avoid collisions.
    pub fn deployment_metadata(&self, function_id: &str) -> HashMap<String, Value> {
        let mut meta = HashMap::new();

        if let Some(func) = self.deployed.get(function_id) {
            meta.insert("dynasb.function_id".into(), json!(func.function_id));
            meta.insert("dynasb.status".into(), json!(func.status));
            meta.insert("dynasb.runtime".into(), json!(func.runtime));
            meta.insert("dynasb.deployment_time_ms".into(), json!(func.deployment_time_ms));
            if let Some(vm_id) = &func.vm_id {
                meta.insert("dynasb.vm_id".into(), json!(vm_id));
            }
            meta.insert("dynasb.api_url".into(), json!(self.config.api_url));
            meta.insert("dynasb.ws_url".into(), json!(format!(
                "{}/{}", self.config.ws_url, func.function_id
            )));
        }

        meta
    }

    /// List deployed functions
    pub fn deployed_functions(&self) -> &HashMap<String, DynASBFunction> {
        &self.deployed
    }
}

impl Drop for DynASBClient {
    fn drop(&mut self) {
        if !self.deployed.is_empty() {
            let ids: Vec<String> = self.deployed.keys().cloned().collect();
            let api_url = self.config.api_url.clone();
            let http = self.http.clone();
            tokio::spawn(async move {
                for id in ids {
                    let url = format!("{}/api/v1/functions/{}/undeploy", api_url, id);
                    let _ = http.post(&url).send().await;
                }
            });
        }
    }
}
