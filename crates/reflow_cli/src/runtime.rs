//! Shared helpers: load a graph file, resolve its actors against the bundled
//! catalog + loaded packs, and wait for a shutdown signal.

use anyhow::{bail, Context, Result};
use std::collections::BTreeSet;
use std::path::Path;

use reflow_rt::graph::types::GraphExport;
use reflow_rt::graph::Graph;
use reflow_rt::network::network::{Network, NetworkConfig};

/// Tracing options for a run.
#[derive(Default)]
pub struct TraceOpts {
    pub enabled: bool,
    pub server: Option<String>,
    pub tail: bool,
}

impl TraceOpts {
    fn any(&self) -> bool {
        self.enabled || self.tail || self.server.is_some()
    }
}

/// Read and parse a graph JSON file into a `GraphExport`. Ensures a `name`
/// property exists (defaulted from the file name) since `Graph::load` requires
/// it.
pub fn load_graph_export(path: &Path) -> Result<GraphExport> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading graph file {}", path.display()))?;
    let mut export: GraphExport = serde_json::from_str(&text)
        .with_context(|| format!("parsing {} as a Reflow graph (GraphExport JSON)", path.display()))?;
    if !export.properties.contains_key("name") {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("graph")
            .to_string();
        export
            .properties
            .insert("name".into(), serde_json::Value::String(name));
    }
    Ok(export)
}

/// Distinct component (template) ids referenced by a graph's nodes.
pub fn component_ids(export: &GraphExport) -> Vec<String> {
    export
        .processes
        .values()
        .map(|n| n.component.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Load actor packs from the given paths into the process-wide registry.
pub fn load_packs(packs: &[std::path::PathBuf]) -> Result<()> {
    for path in packs {
        let ids = reflow_rt::pack_loader::load_pack(path)
            .with_context(|| format!("loading actor pack {}", path.display()))?;
        tracing::info!("loaded pack {} ({} templates)", path.display(), ids.len());
    }
    Ok(())
}

/// Resolve every component the graph references and register it into the
/// network: loaded packs first, then the bundled `reflow_components` catalog.
/// Returns an error naming any component that couldn't be resolved.
pub fn resolve_and_register(net: &mut Network, export: &GraphExport) -> Result<()> {
    let mut unresolved = Vec::new();
    for comp in component_ids(export) {
        let actor = reflow_rt::pack_loader::instantiate(&comp)
            .or_else(|| reflow_rt::components::get_actor_for_template(&comp));
        match actor {
            // `register_actor_arc` errors if a template is already registered
            // (e.g. by a pack); that's fine — ignore the duplicate.
            Some(actor) => {
                let _ = net.register_actor_arc(&comp, actor);
            }
            None => unresolved.push(comp),
        }
    }
    if !unresolved.is_empty() {
        let available = reflow_rt::components::get_template_mapping();
        let mut sample: Vec<&String> = available.keys().take(20).collect();
        sample.sort();
        bail!(
            "could not resolve {} component(s): {}\n\
             Load the actor pack that provides them with --pack <path>, or check the ids.\n\
             {} bundled templates are available (e.g. {}).",
            unresolved.len(),
            unresolved.join(", "),
            available.len(),
            sample
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    Ok(())
}

/// Block until Ctrl-C.
pub async fn wait_for_ctrl_c() {
    let _ = tokio::signal::ctrl_c().await;
}

/// Build a network from a graph export, resolve its actors, start it, and run
/// until Ctrl-C. Shared by `reflow run` and `reflow workspace run`.
pub async fn run_graph_export(export: GraphExport, trace: TraceOpts, label: &str) -> Result<()> {
    let mut config = NetworkConfig::default();
    config.tracing.enabled = trace.any();
    if let Some(url) = &trace.server {
        config.tracing.server_url = url.clone();
    }

    let graph = Graph::load(export.clone(), None);
    let network = Network::with_graph(config, &graph);

    if trace.tail {
        let rx = network.lock().unwrap().get_trace_receiver();
        tokio::spawn(async move {
            while let Ok(evt) = rx.recv_async().await {
                if let Ok(json) = serde_json::to_string(&evt) {
                    println!("{json}");
                }
            }
        });
    }

    {
        let mut net = network.lock().unwrap();
        resolve_and_register(&mut net, &export)?;
        net.start()?;
    }
    tracing::info!(
        "started {label} — {} nodes, {} connections",
        export.processes.len(),
        export.connections.len()
    );
    eprintln!("reflow: running — press Ctrl-C to stop");

    wait_for_ctrl_c().await;
    eprintln!("reflow: shutting down…");
    network.lock().unwrap().shutdown();
    Ok(())
}
