//! `reflow graph validate|inspect <graph.json>`.

use crate::runtime;
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum GraphCmd {
    /// Parse and validate a graph file; report unresolved components.
    Validate {
        /// Path to the graph file.
        graph: PathBuf,
    },
    /// Print a summary of a graph (nodes, connections, components).
    Inspect {
        /// Path to the graph file.
        graph: PathBuf,
    },
}

fn resolvable(component: &str) -> bool {
    reflow_rt::pack_loader::instantiate(component).is_some()
        || reflow_rt::components::get_actor_for_template(component).is_some()
}

pub async fn run(cmd: GraphCmd) -> Result<()> {
    match cmd {
        GraphCmd::Validate { graph } => {
            let export = runtime::load_graph_export(&graph)?;
            let components = runtime::component_ids(&export);
            println!(
                "✓ {} — {} nodes, {} connections, {} components",
                graph.display(),
                export.processes.len(),
                export.connections.len(),
                components.len()
            );
            let unresolved: Vec<&String> =
                components.iter().filter(|c| !resolvable(c)).collect();
            if !unresolved.is_empty() {
                println!(
                    "⚠ {} unresolved component(s) (load with --pack at run time): {}",
                    unresolved.len(),
                    unresolved
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(())
        }
        GraphCmd::Inspect { graph } => {
            let export = runtime::load_graph_export(&graph)?;
            let name = export
                .properties
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("(unnamed)");
            println!("graph: {name}");
            println!("nodes ({}):", export.processes.len());
            let mut nodes: Vec<_> = export.processes.iter().collect();
            nodes.sort_by(|a, b| a.0.cmp(b.0));
            for (id, node) in nodes {
                let mark = if resolvable(&node.component) { "" } else { "  (unresolved)" };
                println!("  {id}  [{}]{mark}", node.component);
            }
            println!("connections: {}", export.connections.len());
            println!("inports: {}  outports: {}", export.inports.len(), export.outports.len());
            Ok(())
        }
    }
}
