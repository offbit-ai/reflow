use anyhow::Result;
use reflow_server::{ServerConfig, start_server};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let mut port = 8080u16;
    let mut zeal_url: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                if let Ok(p) = args[i + 1].parse::<u16>() {
                    port = p;
                }
                i += 2;
            }
            "--zeal" if i + 1 < args.len() => {
                zeal_url = Some(args[i + 1].clone());
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    println!("Starting Reflow Server on port {}...", port);
    if let Some(ref url) = zeal_url {
        println!("Connecting to Zeal IDE at {}", url);
    }

    let config = ServerConfig {
        port,
        bind_address: "0.0.0.0".to_string(),
        max_connections: 1000,
        cors_enabled: true,
        rate_limit_requests_per_minute: 100,
        zeal_url,
        ..Default::default()
    };

    start_server(Some(config)).await
}
