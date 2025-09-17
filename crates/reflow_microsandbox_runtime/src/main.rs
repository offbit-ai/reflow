use anyhow::Result;
use clap::Parser;
use std::sync::Arc;
use tracing::{info, error};
use reflow_microsandbox_runtime::MicroSandboxRuntime;

/// MicroSandbox Runtime Server for Reflow
/// 
/// This server provides a WebSocket RPC interface for executing
/// actor scripts in secure MicroSandbox environments.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Host to bind the server to
    #[arg(short = 'H', long, default_value = "0.0.0.0")]
    host: String,
    
    /// Port to listen on
    #[arg(short, long, default_value_t = 5556)]
    port: u16,
    
    /// Actor scripts to preload
    #[arg(short, long)]
    actors: Vec<String>,
    
    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize tracing with debug for our module
    let filter = if args.debug {
        "debug"
    } else {
        "info,reflow_microsandbox_runtime=debug"
    };
    
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();
    
    info!(
        "Starting MicroSandbox Runtime Server on {}:{}",
        args.host, args.port
    );
    
    // Create runtime instance
    let runtime = Arc::new(
        MicroSandboxRuntime::new(args.host.clone(), args.port).await?
    );
    
    // Preload actors if specified
    for actor_path in &args.actors {
        info!("Preloading actor: {}", actor_path);
        match runtime.load_actor(actor_path).await {
            Ok(_) => info!("Successfully loaded actor: {}", actor_path),
            Err(e) => error!("Failed to load actor {}: {}", actor_path, e),
        }
    }
    
    // Start the server
    info!("Server ready and listening on ws://{}:{}", args.host, args.port);
    info!("Press Ctrl+C to stop the server");
    
    // Handle shutdown signal
    let runtime_clone = runtime.clone();
    tokio::spawn(async move {
        match tokio::signal::ctrl_c().await {
            Ok(()) => {
                info!("Received shutdown signal, cleaning up...");
                if let Err(e) = runtime_clone.shutdown().await {
                    error!("Error during shutdown: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to listen for shutdown signal: {}", e);
            }
        }
    });
    
    // Start the server (blocks until error or shutdown)
    runtime.start().await?;
    
    info!("Server stopped");
    Ok(())
}