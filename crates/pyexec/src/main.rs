mod error;
mod package_manager;
mod python_vm;
mod rpc;
mod websocket;

use anyhow::Result;
use std::{net::SocketAddr, os};
use tokio::net::TcpListener;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    let server_host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port = std::env::var("SERVER_PORT").unwrap_or_else(|_| "8080".to_string());
    let addr = format!("{}:{}", server_host, server_port).parse::<SocketAddr>()?;
    let listener = TcpListener::bind(&addr).await?;
    info!("PyExec service started on {}", addr);

    // Initialize Python with PyO3
    pyo3::prepare_freethreaded_python();

    // Start accepting WebSocket connections
    websocket::accept_connections(listener).await?;

    Ok(())
}