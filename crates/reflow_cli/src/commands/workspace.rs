//! `reflow workspace …` — discover, analyze, compose, and run multi-graph workspaces.

use crate::runtime::{self, TraceOpts};
use anyhow::{anyhow, Result};
use clap::Subcommand;
use reflow_rt::network::multi_graph::GraphComposer;
use reflow_rt::network::multi_graph::workspace::{
    WorkspaceComposition, WorkspaceConfig, WorkspaceDiscovery,
};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum WorkspaceCmd {
    /// Discover graphs in a directory and print graphs + namespaces + analysis.
    Discover { dir: PathBuf },
    /// List the graphs discovered in a workspace.
    List { dir: PathBuf },
    /// List the namespaces discovered in a workspace.
    Namespaces { dir: PathBuf },
    /// Analyze cross-graph dependencies and interfaces.
    Analyze { dir: PathBuf },
    /// Compose all discovered graphs into one graph.
    Compose {
        dir: PathBuf,
        /// Write the composed graph JSON here instead of stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Compose the workspace and run it in-process.
    Run {
        dir: PathBuf,
        /// Enable tracing for the run.
        #[arg(long)]
        trace: bool,
    },
}

async fn discover(dir: &PathBuf) -> Result<WorkspaceComposition> {
    let config = WorkspaceConfig {
        root_path: dir.clone(),
        ..Default::default()
    };
    WorkspaceDiscovery::new(config)
        .discover_workspace()
        .await
        .map_err(|e| anyhow!("workspace discovery failed: {e:?}"))
}

fn graph_name(g: &reflow_rt::graph::types::GraphExport) -> String {
    g.properties
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("(unnamed)")
        .to_string()
}

async fn compose(dir: &PathBuf) -> Result<reflow_rt::graph::types::GraphExport> {
    let wc = discover(dir).await?;
    let mut composer = GraphComposer::new();
    let graph = composer
        .compose_graphs(wc.composition)
        .await
        .map_err(|e| anyhow!("composition failed: {e:?}"))?;
    Ok(graph.export())
}

pub async fn run(cmd: WorkspaceCmd) -> Result<()> {
    match cmd {
        WorkspaceCmd::Discover { dir } => {
            let wc = discover(&dir).await?;
            println!("workspace: {}", wc.workspace_root.display());
            println!(
                "{} graph(s), {} namespace(s)",
                wc.discovered_graphs.len(),
                wc.discovered_namespaces.len()
            );
            let mut nss: Vec<_> = wc.discovered_namespaces.values().collect();
            nss.sort_by(|a, b| a.namespace.cmp(&b.namespace));
            for info in nss {
                println!("  {} — {} graph(s)", info.namespace, info.graph_count);
            }
            println!(
                "dependencies: {}, provided interfaces: {}, required interfaces: {}, auto-connections: {}",
                wc.analysis.dependencies.len(),
                wc.analysis.exposed_interfaces.len(),
                wc.analysis.required_interfaces.len(),
                wc.analysis.auto_connections.len()
            );
            Ok(())
        }
        WorkspaceCmd::List { dir } => {
            let wc = discover(&dir).await?;
            let mut graphs: Vec<_> = wc.discovered_graphs.iter().collect();
            graphs.sort_by_key(|g| graph_name(&g.graph));
            for g in graphs {
                println!(
                    "{}\t[{}]",
                    graph_name(&g.graph),
                    g.workspace_metadata.discovered_namespace
                );
            }
            Ok(())
        }
        WorkspaceCmd::Namespaces { dir } => {
            let wc = discover(&dir).await?;
            let mut nss: Vec<_> = wc.discovered_namespaces.values().collect();
            nss.sort_by(|a, b| a.namespace.cmp(&b.namespace));
            for info in nss {
                println!("{}\t{} graph(s)\t{}", info.namespace, info.graph_count, info.path.display());
            }
            Ok(())
        }
        WorkspaceCmd::Analyze { dir } => {
            let wc = discover(&dir).await?;
            let a = &wc.analysis;
            println!("dependencies ({}):", a.dependencies.len());
            for d in &a.dependencies {
                println!("  {d:?}");
            }
            println!("provided interfaces ({}):", a.exposed_interfaces.len());
            println!("required interfaces ({}):", a.required_interfaces.len());
            println!("auto-connections ({}):", a.auto_connections.len());
            Ok(())
        }
        WorkspaceCmd::Compose { dir, out } => {
            let export = compose(&dir).await?;
            let json = serde_json::to_string_pretty(&export)?;
            match out {
                Some(path) => {
                    std::fs::write(&path, json)?;
                    eprintln!("reflow: wrote composed graph to {}", path.display());
                }
                None => println!("{json}"),
            }
            Ok(())
        }
        WorkspaceCmd::Run { dir, trace } => {
            let export = compose(&dir).await?;
            let label = format!("workspace {}", dir.display());
            runtime::run_graph_export(
                export,
                TraceOpts {
                    enabled: trace,
                    ..Default::default()
                },
                &label,
            )
            .await
        }
    }
}
