//! `reflow peer …` — spawn and drive distributed network peers.

use anyhow::Result;
use clap::Subcommand;
use reflow_distributed::peer::run_peer;
use reflow_distributed::peer_config::PeerConfig;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum PeerCmd {
    /// Spawn a peer from a TOML config; join its network and stay running.
    Spawn {
        /// Path to the peer TOML config.
        config: PathBuf,
        /// One-shot message to send on startup: `network:actor:port:text`.
        #[arg(long)]
        send: Option<String>,
    },
}

pub async fn run(cmd: PeerCmd) -> Result<()> {
    let PeerCmd::Spawn { config, send } = cmd;
    let peer_config = PeerConfig::from_path(&config)?;
    run_peer(peer_config, send).await
}
