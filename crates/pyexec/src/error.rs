use pyo3::PyErr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Python execution error: {0}")]
    Python(String),

    #[error("Invalid RPC request: {0}")]
    InvalidRpc(String),

    #[error("Internal server error: {0}")]
    Internal(String),
}

impl From<PyErr> for ServiceError {
    fn from(err: PyErr) -> Self {
        ServiceError::Python(err.to_string())
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(err: anyhow::Error) -> Self {
        ServiceError::Internal(err.to_string())
    }
}