//! `reflow-discovery` — bare-bones discovery server.
//!
//! Run it on every cluster member that needs to host the discovery
//! contract. Peers configured with this URL in their
//! `discovery_endpoints` will register on startup, re-register on
//! each refresh tick, and read the current network roster from it.

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use reflow_distributed::{ServerConfig, serve};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "Reflow distributed network discovery server")]
struct Args {
    /// Address to bind on. Use `0.0.0.0:<port>` to accept from all
    /// interfaces; `127.0.0.1:<port>` to keep it local.
    #[arg(short, long, default_value = "0.0.0.0:9000")]
    bind: SocketAddr,

    /// Drop entries that haven't re-registered in this many
    /// seconds. Set higher than your peers' refresh cadence.
    #[arg(long, default_value_t = 60)]
    entry_ttl_secs: u64,

    /// How often the pruner sweeps the registry. Read-side
    /// filtering happens regardless, so this mostly affects map
    /// memory.
    #[arg(long, default_value_t = 15)]
    prune_interval_secs: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    serve(ServerConfig {
        bind: args.bind,
        entry_ttl: Duration::from_secs(args.entry_ttl_secs),
        prune_interval: Duration::from_secs(args.prune_interval_secs),
    })
    .await
}
