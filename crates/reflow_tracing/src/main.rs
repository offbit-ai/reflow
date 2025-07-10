//! # Reflow Tracing Server
//!
//! A standalone tracing server for the Reflow actor system.

use anyhow::Result;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{info, warn};
use tracing_subscriber;

mod config;
mod server;
mod protocol;
mod storage;

use config::Config;
use server::TraceServer;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    info!("Starting Reflow Tracing Server...");

    // Load configuration
    let config = Config::load()
        .unwrap_or_else(|_| {
            warn!("Could not load config file, using defaults");
            Config::default()
        });

    // Create the server
    let server = TraceServer::new(config.clone()).await?;

    // Start the server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port).parse()?;
    let listener = TcpListener::bind(addr).await?;
    
    info!("Tracing server listening on {}", addr);

    // Run the server
    server.run(listener).await?;

    Ok(())
}
