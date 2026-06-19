//! `reflow serve` — run the Reflow HTTP server daemon.
//!
//! The `reflow_server` crate currently carries an out-of-tree `zeal-sdk` path
//! dependency, so the CLI doesn't link it. Instead `serve` execs the
//! `reflow_server` binary (built separately) with Zeal disabled — a thin
//! convenience wrapper.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct ServeArgs {
    /// Port to listen on.
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
}

pub async fn run(args: ServeArgs) -> Result<()> {
    let bin = locate_reflow_server();
    tracing::info!("starting {} on port {} (Zeal disabled)", bin.display(), args.port);
    let status = tokio::process::Command::new(&bin)
        .arg("--port")
        .arg(args.port.to_string())
        .status()
        .await
        .with_context(|| {
            format!(
                "could not run `{}`. Build it first (cargo build -p reflow_server) \
                 or put it on PATH.",
                bin.display()
            )
        })?;
    if !status.success() {
        bail!("reflow_server exited unsuccessfully ({status})");
    }
    Ok(())
}

/// Prefer a `reflow_server` next to the current `reflow` executable; otherwise
/// fall back to the name on `PATH`.
fn locate_reflow_server() -> PathBuf {
    let name = if cfg!(windows) {
        "reflow_server.exe"
    } else {
        "reflow_server"
    };
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        let candidate = dir.join(name);
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(name)
}
