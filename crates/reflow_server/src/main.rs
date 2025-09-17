use reflow_server::{start_server, ServerConfig};
use anyhow::Result;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments for port
    let args: Vec<String> = env::args().collect();
    let mut port = 8080; // Default port
    
    // Simple argument parsing for --port
    for i in 0..args.len() {
        if args[i] == "--port" && i + 1 < args.len() {
            if let Ok(p) = args[i + 1].parse::<u16>() {
                port = p;
            }
        }
    }
    
    println!("🚀 Starting Reflow Server on port {}...", port);
    
    let config = ServerConfig {
        port,
        bind_address: "0.0.0.0".to_string(),
        max_connections: 1000,
        cors_enabled: true,
        rate_limit_requests_per_minute: 100,
    };
    
    start_server(Some(config)).await
}