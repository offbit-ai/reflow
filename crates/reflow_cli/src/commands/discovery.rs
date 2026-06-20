//! `reflow discovery serve` — the peer-discovery registry server.

use anyhow::{Context, Result};
use clap::Subcommand;
use std::net::SocketAddr;
use std::time::Duration;

#[derive(Subcommand)]
pub enum DiscoveryCmd {
    /// Run the discovery registry HTTP server.
    Serve {
        /// Address to bind, e.g. 0.0.0.0:9000.
        #[arg(long, default_value = "0.0.0.0:9000")]
        bind: String,
        /// Seconds before an unrefreshed entry expires.
        #[arg(long, default_value_t = 60)]
        entry_ttl_secs: u64,
        /// Seconds between prune sweeps.
        #[arg(long, default_value_t = 15)]
        prune_interval_secs: u64,
    },
}

pub async fn run(cmd: DiscoveryCmd) -> Result<()> {
    let DiscoveryCmd::Serve {
        bind,
        entry_ttl_secs,
        prune_interval_secs,
    } = cmd;
    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid --bind address {bind:?}"))?;
    reflow_distributed::serve(reflow_distributed::ServerConfig {
        bind: addr,
        entry_ttl: Duration::from_secs(entry_ttl_secs),
        prune_interval: Duration::from_secs(prune_interval_secs),
    })
    .await
}
