//! `reflow serve` — run the Reflow HTTP server in-process.
//!
//! Links `reflow_server` directly (with `default-features = false`, so the
//! out-of-tree Zeal SDK is omitted) and calls `start_server`. No separate
//! binary, no subprocess — one self-contained `reflow` tool.

use anyhow::Result;
use clap::Args;
use reflow_server::{ServerConfig, start_server};

#[derive(Args)]
pub struct ServeArgs {
    /// Port to listen on.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
    /// Address to bind.
    #[arg(long, default_value = "0.0.0.0")]
    pub bind: String,
    /// Optional Redis URL for workflow persistence (survives restarts).
    #[arg(long)]
    pub redis: Option<String>,
}

pub async fn run(args: ServeArgs) -> Result<()> {
    let config = ServerConfig {
        port: args.port,
        bind_address: args.bind,
        redis_url: args.redis,
        // Zeal is omitted in the CLI build; leave the session unconfigured.
        zeal_url: None,
        ..ServerConfig::default()
    };
    tracing::info!(
        "starting reflow server on {}:{} (Zeal disabled){}",
        config.bind_address,
        config.port,
        if config.redis_url.is_some() {
            ", Redis persistence on"
        } else {
            ""
        }
    );
    start_server(Some(config)).await
}
