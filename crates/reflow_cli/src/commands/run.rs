//! `reflow run <graph.json>` — load a graph and execute it in-process.

use crate::runtime::{self, TraceOpts};
use anyhow::Result;
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct RunArgs {
    /// Path to the graph file (GraphExport JSON).
    pub graph: PathBuf,

    /// Load an actor pack (.rflpack or dylib). Repeatable.
    #[arg(long = "pack", value_name = "PATH")]
    pub packs: Vec<PathBuf>,

    /// Enable tracing for this run.
    #[arg(long)]
    pub trace: bool,

    /// Tracing collector URL (implies --trace). Default ws://localhost:8080.
    #[arg(long = "trace-server", value_name = "WS_URL")]
    pub trace_server: Option<String>,

    /// Stream live trace events to stdout as JSON (implies --trace; uses the
    /// local tap, so no collector is required).
    #[arg(long = "trace-tail")]
    pub trace_tail: bool,
}

pub async fn run(args: RunArgs) -> Result<()> {
    let export = runtime::load_graph_export(&args.graph)?;
    runtime::load_packs(&args.packs)?;
    let label = args.graph.display().to_string();
    runtime::run_graph_export(
        export,
        TraceOpts {
            enabled: args.trace,
            server: args.trace_server,
            tail: args.trace_tail,
        },
        &label,
    )
    .await
}
