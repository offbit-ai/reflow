//! `reflow-peer` — runs a single member of a distributed Reflow network from a
//! TOML config file. A thin shim over [`reflow_distributed::peer::run_peer`];
//! the same logic is reachable via `reflow peer spawn`.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use reflow_distributed::peer::run_peer;
use reflow_distributed::peer_config::PeerConfig;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "Reflow distributed peer")]
struct Args {
    /// Path to a TOML peer config file.
    #[arg(short, long)]
    config: PathBuf,

    /// One-shot test send: `<network_id>:<actor_id>:<port>:<text>`. Fires after
    /// federation is up; the peer keeps running afterward.
    #[arg(long)]
    send: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let config = PeerConfig::from_path(&args.config)?;
    run_peer(config, args.send).await
}
