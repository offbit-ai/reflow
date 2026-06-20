//! `reflow` — the unified Reflow command-line tool.
//!
//! Run graphs in-process, manage multi-graph workspaces, spawn distributed
//! peers, drive tracing, and serve the HTTP API — one entry point over the
//! existing runtime APIs.

mod commands;
mod runtime;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "reflow", version, about = "Run and orchestrate Reflow graphs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Load a graph file and execute it in-process.
    Run(commands::run::RunArgs),
    /// Inspect or validate a graph file.
    #[command(subcommand)]
    Graph(commands::graph::GraphCmd),
    /// Discover and run multi-graph workspaces.
    #[command(subcommand)]
    Workspace(commands::workspace::WorkspaceCmd),
    /// Spawn and drive distributed network peers.
    #[command(subcommand)]
    Peer(commands::peer::PeerCmd),
    /// Run the peer-discovery registry server.
    #[command(subcommand)]
    Discovery(commands::discovery::DiscoveryCmd),
    /// Run the tracing collector or consume traces.
    #[command(subcommand)]
    Trace(commands::trace::TraceCmd),
    /// Run the Reflow HTTP server daemon.
    Serve(commands::serve::ServeArgs),
}

fn init_logging() {
    use tracing_subscriber::EnvFilter;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_logging();
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => commands::run::run(args).await,
        Command::Graph(cmd) => commands::graph::run(cmd).await,
        Command::Workspace(cmd) => commands::workspace::run(cmd).await,
        Command::Peer(cmd) => commands::peer::run(cmd).await,
        Command::Discovery(cmd) => commands::discovery::run(cmd).await,
        Command::Trace(cmd) => commands::trace::run(cmd).await,
        Command::Serve(args) => commands::serve::run(args).await,
    }
}
