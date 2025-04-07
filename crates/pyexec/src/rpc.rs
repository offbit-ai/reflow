use crate::error::ServiceError;
use crate::package_manager;
use crate::python_vm::{self, ExecutionResult};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum RpcRequest {
    #[serde(rename = "execute_script")]
    ExecuteScript {
        code: String,
        #[serde(default)]
        inputs: Option<Vec<InputParamRequest>>,
        #[serde(default)]
        requirements: Option<Vec<String>>,
        #[serde(default)]
        timeout_seconds: Option<u64>,
    },

    #[serde(rename = "install_packages")]
    InstallPackages { packages: Vec<String> },

    #[serde(rename = "interrupt")]
    Interrupt,

    #[serde(rename = "ping")]
    Ping,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InputParamRequest {
    pub name: String,
    pub data: JsonValue,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RpcMessage {
    pub id: Uuid,
    #[serde(flatten)]
    pub request: RpcRequest,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<RpcResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum RpcResult {
    ExecutionResult {
        stdout: String,
        stderr: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<JsonValue>,
        success: bool,
        execution_time_ms: u64,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        packages_installed: Vec<String>,
    },
    PackageInstallResult {
        success: bool,
        message: String,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        packages_installed: Vec<String>,
    },
    Pong {
        message: String,
    },
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

impl RpcResponse {
    pub fn success(id: Uuid, result: RpcResult) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Uuid, code: i32, message: String) -> Self {
        Self {
            id,
            result: None,
            error: Some(RpcError { code, message }),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ClientMessage {
    pub session_id: Uuid,
    pub message: JsonValue,
}

pub async fn handle_rpc_request(
    session_id: Uuid,
    message: RpcMessage,
) -> Result<RpcResponse, ServiceError> {
    match message.request {
        RpcRequest::ExecuteScript {
            code,
            inputs,
            requirements,
            timeout_seconds,
        } => {
           
            let result =
                python_vm::execute_script_with_timeout(&session_id, &code, inputs, requirements, timeout_seconds).await?;
            Ok(RpcResponse::success(
                message.id,
                RpcResult::ExecutionResult {
                    stdout: result.stdout,
                    stderr: result.stderr,
                    result: result.result,
                    success: result.success,
                    execution_time_ms: result.execution_time.as_millis() as u64,
                    packages_installed: result.packages_installed,
                },
            ))
        }
        RpcRequest::InstallPackages { packages } => {
            // Create a channel for package installation progress
            let (progress_sender, _) = mpsc::unbounded_channel::<String>();

            // Install packages
            let result = package_manager::install_packages(
                &session_id,
                &packages,
                Some(Arc::new(Mutex::new(progress_sender))),
            )
            .await;

            match result {
                Ok(_) => Ok(RpcResponse::success(
                    message.id,
                    RpcResult::PackageInstallResult {
                        success: true,
                        message: format!("Successfully installed {} packages", packages.len()),
                        packages_installed: packages,
                    },
                )),
                Err(e) => Ok(RpcResponse::success(
                    message.id,
                    RpcResult::PackageInstallResult {
                        success: false,
                        message: format!("Failed to install packages: {}", e),
                        packages_installed: Vec::new(),
                    },
                )),
            }
        }
        RpcRequest::Ping => Ok(RpcResponse::success(
            message.id,
            RpcResult::Pong {
                message: "pong".to_string(),
            },
        )),
        RpcRequest::Interrupt => {
            // For now, just acknowledge the interrupt request
            // In a full implementation, you would actually interrupt the Python execution
            Ok(RpcResponse::success(
                message.id,
                RpcResult::Pong {
                    message: "execution interrupted".to_string(),
                },
            ))
        }
    }
}

// Create a JSON message from Python script to client
pub fn create_client_message(session_id: &Uuid, message: &str) -> Result<String, ServiceError> {
    let parsed: JsonValue = serde_json::from_str(message).map_err(|e| ServiceError::Json(e))?;

    let client_message = ClientMessage {
        session_id: *session_id,
        message: parsed,
    };

    serde_json::to_string(&client_message).map_err(|e| ServiceError::Json(e))
}
