//! Example demonstrating the workspace discovery system
//! 
//! This example shows how to:
//! 1. Discover all graph files in a workspace
//! 2. Load them with proper validation
//! 3. Analyze dependencies
//! 4. Compose them into a unified network

use std::path::PathBuf;
use reflow_network::multi_graph::{
    GraphLoader, GraphSource, GraphComposer, GraphComposition, 
    CompositionConnection, CompositionEndpoint
};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Starting Multi-Graph Workspace Example");
    println!("=========================================");
    
    // Step 1: Load individual graphs
    println!("\n🔍 Phase 1: Loading Individual Graphs");
    println!("-------------------------------------");
    
    let loader = GraphLoader::new();
    
    // Load collector graph
    let collector_source = GraphSource::JsonFile(
        "data/ingestion/collector.graph.json".to_string()
    );
    let collector_graph = loader.load_graph(collector_source.clone()).await?;
    println!("✅ Loaded collector graph with {} processes", collector_graph.processes.len());
    
    // Load transformer graph
    let transformer_source = GraphSource::JsonFile(
        "data/processing/transformer.graph.json".to_string()
    );
    let transformer_graph = loader.load_graph(transformer_source.clone()).await?;
    println!("✅ Loaded transformer graph with {} processes", transformer_graph.processes.len());
    
    // Load trainer graph
    let trainer_source = GraphSource::JsonFile(
        "ml/training/trainer.graph.json".to_string()
    );
    let trainer_graph = loader.load_graph(trainer_source.clone()).await?;
    println!("✅ Loaded trainer graph with {} processes", trainer_graph.processes.len());
    
    // Step 2: Analyze graphs
    println!("\n📊 Phase 2: Graph Analysis");
    println!("-------------------------");
    
    analyze_graph("collector", &collector_graph);
    analyze_graph("transformer", &transformer_graph);
    analyze_graph("trainer", &trainer_graph);
    
    // Step 3: Create composition
    println!("\n🔧 Phase 3: Graph Composition");
    println!("-----------------------------");
    
    let composition = GraphComposition {
        sources: vec![
            collector_source,
            transformer_source,
            trainer_source,
        ],
        connections: vec![
            // Connect collector to transformer
            CompositionConnection {
                from: CompositionEndpoint {
                    process: "data/ingestion/data_validator".to_string(),
                    port: "Output".to_string(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: "data/processing/cleaner".to_string(),
                    port: "Input".to_string(),
                    index: None,
                },
                metadata: Some({
                    let mut meta = HashMap::new();
                    meta.insert("description".to_string(), 
                        serde_json::Value::String("Connect data collector to transformer".to_string()));
                    meta
                }),
            },
            // Connect transformer to trainer
            CompositionConnection {
                from: CompositionEndpoint {
                    process: "data/processing/normalizer".to_string(),
                    port: "Output".to_string(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: "ml/training/feature_engineer".to_string(),
                    port: "Input".to_string(),
                    index: None,
                },
                metadata: Some({
                    let mut meta = HashMap::new();
                    meta.insert("description".to_string(), 
                        serde_json::Value::String("Connect transformer to trainer".to_string()));
                    meta
                }),
            },
        ],
        shared_resources: vec![],
        properties: {
            let mut props = HashMap::new();
            props.insert("name".to_string(), 
                serde_json::Value::String("workspace_pipeline".to_string()));
            props.insert("description".to_string(), 
                serde_json::Value::String("Complete data processing and ML training pipeline".to_string()));
            props
        },
        case_sensitive: Some(false),
        metadata: None,
    };
    
    // Step 4: Compose graphs
    println!("🔄 Composing graphs into unified network...");
    
    let mut composer = GraphComposer::new();
    let composed_graph = composer.compose_graphs(composition).await?;
    
    // Step 5: Display results
    println!("\n🎉 Phase 4: Composition Results");
    println!("==============================");
    
    let export = composed_graph.export();
    println!("✅ Successfully composed workspace graph!");
    println!("📊 Total processes: {}", export.processes.len());
    println!("🔗 Total connections: {}", export.connections.len());
    println!("📥 Total inports: {}", export.inports.len());
    println!("📤 Total outports: {}", export.outports.len());
    println!("👥 Total groups: {}", export.groups.len());
    
    // Print process details
    if !export.processes.is_empty() {
        println!("\n🔧 Composed Processes:");
        for (process_name, process_def) in &export.processes {
            println!("  📦 {}: {}", process_name, process_def.component);
        }
    }
    
    // Print connections
    if !export.connections.is_empty() {
        println!("\n🔗 Composed Connections:");
        for connection in &export.connections {
            if connection.data.is_none() {
                println!("  🔄 {} → {}", 
                    format!("{}.{}", connection.from.node_id, connection.from.port_id),
                    format!("{}.{}", connection.to.node_id, connection.to.port_id)
                );
            } else {
                println!("  📋 IIP → {}.{}", connection.to.node_id, connection.to.port_id);
            }
        }
    }
    
    println!("\n✅ Multi-graph workspace composition completed successfully!");
    println!("The composed graph is ready for execution.");
    
    Ok(())
}

fn analyze_graph(name: &str, graph: &reflow_network::graph::types::GraphExport) {
    println!("\n📈 Graph: {}", name);
    println!("   Processes: {}", graph.processes.len());
    println!("   Connections: {}", graph.connections.len());
    println!("   Inports: {}", graph.inports.len());
    println!("   Outports: {}", graph.outports.len());
    
    // Check for dependencies
    if let Some(deps) = graph.properties.get("dependencies") {
        if let Some(deps_array) = deps.as_array() {
            if !deps_array.is_empty() {
                println!("   Dependencies: {:?}", 
                    deps_array.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                );
            }
        }
    }
    
    // Check for namespace
    if let Some(namespace) = graph.properties.get("namespace") {
        if let Some(ns_str) = namespace.as_str() {
            println!("   Namespace: {}", ns_str);
        }
    }
}
