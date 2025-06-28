//! Simple workspace example that loads and runs discovered graphs
//!
//! This example demonstrates:
//! 1. Discovering graphs in a workspace using simple actors
//! 2. Loading them into a network
//! 3. Actually running the composed network

use multi_graph_workspace::actors::{DataGeneratorActor, SimpleLoggerActor, SimpleTimerActor};
use reflow_network::graph::Graph;
use reflow_network::multi_graph::{
    CompositionConnection, CompositionEndpoint, GraphComposer, GraphComposition, GraphLoader,
    GraphSource,
};
use reflow_network::network::{Network, NetworkConfig};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🚀 Simple Multi-Graph Workspace Example");
    println!("=======================================");

    // Step 1: Load simple graphs using basic actors
    println!("\n🔍 Phase 1: Loading Simple Graphs");
    println!("---------------------------------");

    let loader = GraphLoader::new();

    // Load generator graph (TimerActor)
    let generator_source = GraphSource::JsonFile("simple/generator.graph.json".to_string());
    let generator_graph = loader.load_graph(generator_source.clone()).await?;
    println!(
        "✅ Loaded generator graph with {} processes",
        generator_graph.processes.len()
    );

    // Load processor graph (LoggerActor)
    let processor_source = GraphSource::JsonFile("simple/processor.graph.json".to_string());
    let processor_graph = loader.load_graph(processor_source.clone()).await?;
    println!(
        "✅ Loaded processor graph with {} processes",
        processor_graph.processes.len()
    );

    // Step 2: Create composition with connection
    println!("\n🔧 Phase 2: Creating Graph Composition");
    println!("-------------------------------------");

    let composition = GraphComposition {
        sources: vec![generator_source, processor_source],
        connections: vec![
            // Connect generator output to processor input
            CompositionConnection {
                from: CompositionEndpoint {
                    process: "simple/data_gen".to_string(), // namespace/process_name
                    port: "Output".to_string(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: "processor/transformer".to_string(), // namespace/process_name
                    port: "Input".to_string(),
                    index: None,
                },
                metadata: Some({
                    let mut meta = HashMap::new();
                    meta.insert(
                        "description".to_string(),
                        serde_json::Value::String("Connect timer to logger".to_string()),
                    );
                    meta
                }),
            },
        ],
        shared_resources: vec![],
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                serde_json::Value::String("simple_pipeline".to_string()),
            );
            props.insert(
                "description".to_string(),
                serde_json::Value::String("Simple timer -> logger pipeline".to_string()),
            );
            props
        },
        case_sensitive: Some(false),
        metadata: None,
    };

    // Step 3: Compose graphs
    println!("🔄 Composing graphs...");

    let mut composer = GraphComposer::new();
    let composed_graph = composer.compose_graphs(composition).await?;

    // Step 4: Display composed graph details
    println!("\n📊 Phase 3: Composition Results");
    println!("==============================");

    let export = composed_graph.export();
    println!("✅ Successfully composed workspace graph!");
    println!("📦 Total processes: {}", export.processes.len());
    println!("🔗 Total connections: {}", export.connections.len());

    // Print process details
    println!("\n🔧 Composed Processes:");
    for (process_name, process_def) in &export.processes {
        println!("  📦 {}: {}", process_name, process_def.component);
    }

    // Print connections
    println!("\n🔗 Composed Connections:");
    for connection in &export.connections {
        if connection.data.is_none() {
            println!(
                "  🔄 {} → {}",
                format!("{}.{}", connection.from.node_id, connection.from.port_id),
                format!("{}.{}", connection.to.node_id, connection.to.port_id)
            );
        }
    }

    // Step 5: Load into Network and run
    println!("\n🌐 Phase 4: Loading into Network");
    println!("===============================");

    // Create network config
    let network_config = NetworkConfig::default();
    let mut network = Network::with_graph(network_config, &Graph::load(export, None));

    // Register our custom actors
    println!("🔧 Registering custom actors...");
    if let Ok(network) = network.lock().as_mut() {
        network.register_actor("SimpleTimerActor", SimpleTimerActor::new())?;
        network.register_actor("SimpleLoggerActor", SimpleLoggerActor::new())?;
        // network.register_actor("DataGeneratorActor", DataGeneratorActor::new())?;

        println!("✅ Successfully created network from composed graph!");
        println!("🚀 Network is ready with the multi-graph workspace!");

        // Step 6: Start the network for a brief demo
        println!("\n⚡ Phase 5: Brief Network Demo");
        println!("=============================");

        println!("🎬 Starting network for demonstration...");

        // Start the network
        network.start().await?;

        // Send initial parameters to start the timer
        println!("📤 Sending configuration to timer...");

        // First send MaxTicks to limit the timer to 5 ticks for demo
        network.send_to_actor(
            "simple/data_gen",
            "MaxTicks",
            reflow_network::message::Message::Integer(5),
        )?;

        // Start the timer
        network.send_to_actor(
            "simple/data_gen",
            "Start",
            reflow_network::message::Message::Boolean(true),
        )?;

        println!("⏰ Timer will run for 10 ticks with 1-second intervals...");

        // Let it run for enough time to complete all ticks
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

        // Shutdown the network
        network.shutdown();
        println!("⏹️  Network stopped.");

        println!("\n🎉 Multi-Graph Workspace Example Complete!");
        println!("=========================================");
        println!("✅ Discovered graphs from workspace");
        println!("✅ Composed them into unified network");
        println!("✅ Loaded and ran the network successfully");
        println!("\nThe simple timer -> logger pipeline demonstrates that");
        println!("multi-graph workspaces can be discovered, composed,");
        println!("and executed as unified networks!");
    }

    Ok(())
}
