# Multi-Graph Workspace Tutorial

Learn how to build and manage complex multi-graph workflows using Reflow's workspace discovery and composition system.

## Overview

This tutorial demonstrates how to create a multi-graph workspace that automatically discovers and composes multiple graph files into a unified workflow. We'll build a complete example with data processing, machine learning, and monitoring components.

## What You'll Build

A workspace containing multiple interconnected graphs:

```
workspace/
├── data/
│   ├── ingestion/
│   │   └── collector.graph.json      # Data collection pipeline
│   └── processing/
│       └── transformer.graph.json    # Data transformation pipeline
├── ml/
│   └── training/
│       └── trainer.graph.json        # ML training pipeline
├── monitoring/
│   └── system_monitor.graph.json     # System monitoring
└── simple/
    ├── generator.graph.json          # Simple data generator
    └── processor.graph.json          # Simple data processor
```

## Prerequisites

- Basic understanding of Reflow actors and graphs
- Familiarity with JSON graph definitions
- Understanding of dependency management concepts

## Step 1: Project Setup

Create the workspace structure:

```bash
mkdir multi_graph_workspace
cd multi_graph_workspace

# Create the directory structure
mkdir -p data/ingestion data/processing ml/training monitoring simple src

# Initialize Cargo project
cargo init --name multi_graph_workspace
```

### Cargo.toml

```toml
[package]
name = "multi_graph_workspace"
version = "0.1.0"
edition = "2021"

[dependencies]
reflow_network = { path = "../../crates/reflow_network" }
actor_macro = { path = "../../crates/actor_macro" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
```

## Step 2: Create Custom Actors

First, let's create the custom actors we'll use across our graphs in `src/actors.rs`:

```rust
//! Custom actors for the multi-graph workspace example

use std::{collections::HashMap, sync::Arc};
use actor_macro::actor;
use anyhow::Error;
use reflow_network::{
    actor::{Actor, ActorConfig, ActorBehavior, ActorContext, ActorLoad, MemoryState, Port},
    message::Message,
    message::EncodableValue
};

/// Simple timer actor that emits periodic events
#[actor(
    SimpleTimerActor,
    inports::<100>(Start),
    outports::<50>(Output),
    state(MemoryState)
)]
pub async fn simple_timer_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let outport_channels = context.get_outports();

    let interval_secs = context.get_config().get_number("interval").unwrap_or(1000.0);

    // Check if we should start the timer
    if let Some(start_msg) = payload.get("Start") {
        let should_start = match start_msg {
            Message::Boolean(b) => *b,
            Message::Integer(i) => *i != 0,
            Message::String(s) => !s.is_empty(),
            _ => true,
        };

        if should_start {
            // Store timer state
            {
                let mut state_lock = state.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    state_data.insert("running", serde_json::json!(true));
                    state_data.insert("interval", serde_json::json!(interval_secs));
                    state_data.insert("tick_count", serde_json::json!(0));
                }
            }

            // Get max ticks (default to 10 for demos)
            let max_ticks = payload
                .get("MaxTicks")
                .and_then(|m| match m {
                    Message::Integer(i) => Some(*i as u64),
                    Message::Float(f) => Some(*f as u64),
                    _ => None,
                })
                .unwrap_or(10);

            // Spawn timer task with proper load management
            let state_clone = state.clone();
            let outports = outport_channels.clone();
            let load = context.get_load();
            
            // Increase load count for the background task
            load.lock().inc();
            
            tokio::spawn(async move {
                let mut tick_count = 0;
                
                // Ensure we decrease load count when the task finishes
                let _load_guard = scopeguard::guard(load.clone(), |load| {
                    load.lock().dec();
                });
                
                loop {
                    // Check if timer should still be running
                    let should_continue = {
                        let state_lock = state_clone.lock();
                        if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                            let running = state_data
                                .get("running")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let current_ticks = state_data
                                .get("tick_count")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0) as u64;
                            running && current_ticks < max_ticks
                        } else {
                            false
                        }
                    };

                    if !should_continue {
                        break;
                    }

                    // Wait for interval
                    tokio::time::sleep(tokio::time::Duration::from_secs(interval_secs as u64)).await;

                    // Increment tick count
                    tick_count += 1;
                    
                    // Update state
                    {
                        let mut state_lock = state_clone.lock();
                        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                            state_data.insert("tick_count", serde_json::json!(tick_count));
                        }
                    }

                    // Send tick message
                    let tick_message = Message::object(
                        EncodableValue::from(serde_json::json!({
                            "tick": tick_count,
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "source": "SimpleTimerActor",
                            "max_ticks": max_ticks
                        }))
                    );

                    if outports.0.send_async(HashMap::from([
                        ("Output".to_owned(), tick_message)
                    ])).await.is_err() {
                        break;
                    }
                }
            });

            println!("✅ SimpleTimerActor started with interval: {}s", interval_secs);
        }
    }

    Ok(HashMap::new())
}

/// Simple logger actor that logs incoming messages
#[actor(
    SimpleLoggerActor,
    inports::<100>(Input, Prefix),
    outports::<50>(Output),
    state(MemoryState)
)]
pub async fn simple_logger_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();

    if let Some(input_msg) = payload.get("Input") {
        // Get prefix from payload or state
        let prefix = if let Some(Message::String(p)) = payload.get("Prefix") {
            p.clone()
        } else {
            let state_lock = state.lock();
            if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("prefix")
                    .and_then(|v| v.as_str())
                    .unwrap_or("LOG")
                    .to_string().into()
            } else {
                "LOG".to_string().into()
            }
        };

        // Log the message with timestamp
        let timestamp = chrono::Utc::now().format("%H:%M:%S%.3f");
        println!("[{}] {}: {:?}", timestamp, prefix, input_msg);

        // Update log count in state
        {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                let count = state_data
                    .get("log_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) + 1;
                state_data.insert("log_count", serde_json::json!(count));
            }
        }

        // Pass through the input
        Ok(HashMap::from([("Output".to_owned(), input_msg.clone())]))
    } else {
        Ok(HashMap::new())
    }
}

/// Data generator actor that creates sample data
#[actor(
    DataGeneratorActor,
    inports::<100>(Trigger, Type),
    outports::<50>(Output),
    state(MemoryState)
)]
pub async fn data_generator_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();

    if payload.contains_key("Trigger") {
        // Get data type from payload or state
        let data_type = if let Some(Message::String(t)) = payload.get("Type") {
            t.clone()
        } else {
            let state_lock = state.lock();
            if let Some(state_data) = state_lock.as_any().downcast_ref::<MemoryState>() {
                state_data
                    .get("data_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("number")
                    .to_string().into()
            } else {
                "number".to_string().into()
            }
        };

        // Update generation count
        let generation_count = {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                let count = state_data
                    .get("generation_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) + 1;
                state_data.insert("generation_count", serde_json::json!(count));
                count
            } else {
                1
            }
        };

        // Generate data based on type
        let generated_data = match data_type.as_str() {
            "number" => Message::Integer(generation_count),
            "string" => Message::String(format!("generated_data_{}", generation_count).into()),
            "object" => Message::object(
                EncodableValue::from(serde_json::json!({
                    "id": generation_count,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "type": "generated",
                    "value": format!("sample_value_{}", generation_count)
                }))
            ),
            _ => Message::String(format!("unknown_type_data_{}", generation_count).into()),
        };

        Ok(HashMap::from([("Output".to_owned(), generated_data)]))
    } else {
        Ok(HashMap::new())
    }
}
```

## Step 3: Define Graph Files

### Simple Data Generator (`simple/generator.graph.json`)

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "generator",
    "description": "Simple data generator",
    "version": "1.0.0",
    "namespace": "simple"
  },
  "processes": {
    "timer": {
      "component": "SimpleTimerActor",
      "metadata": {
        "description": "Generates periodic triggers"
      }
    },
    "data_generator": {
      "component": "DataGeneratorActor", 
      "metadata": {
        "description": "Generates sample data"
      }
    }
  },
  "connections": [
    {
      "from": { "nodeId": "timer", "portId": "Output" },
      "to": { "nodeId": "data_generator", "portId": "Trigger" },
      "metadata": {}
    }
  ],
  "inports": {
    "start": {
      "nodeId": "timer",
      "portId": "Start"
    }
  },
  "outports": {
    "data": {
      "nodeId": "data_generator",
      "portId": "Output"
    }
  },
  "groups": [],
  "providedInterfaces": {
    "data_output": {
      "interfaceId": "data_output",
      "processName": "data_generator",
      "portName": "Output",
      "dataType": "GeneratedData",
      "description": "Generated sample data",
      "required": false
    }
  },
  "requiredInterfaces": {},
  "graphDependencies": [],
  "externalConnections": []
}
```

### Simple Data Processor (`simple/processor.graph.json`)

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "processor",
    "description": "Simple data processor",
    "version": "1.0.0",
    "namespace": "simple",
    "dependencies": ["generator"]
  },
  "processes": {
    "logger": {
      "component": "SimpleLoggerActor",
      "metadata": {
        "description": "Logs processed data"
      }
    }
  },
  "connections": [],
  "inports": {
    "data": {
      "nodeId": "logger",
      "portId": "Input"
    }
  },
  "outports": {
    "processed": {
      "nodeId": "logger",
      "portId": "Output"
    }
  },
  "groups": [],
  "providedInterfaces": {
    "processed_output": {
      "interfaceId": "processed_output",
      "processName": "logger",
      "portName": "Output",
      "dataType": "ProcessedData",
      "description": "Processed data output",
      "required": false
    }
  },
  "requiredInterfaces": {
    "data_input": {
      "interfaceId": "data_input",
      "processName": "logger",
      "portName": "Input",
      "dataType": "GeneratedData",
      "description": "Input data to process",
      "required": true
    }
  },
  "graphDependencies": [
    {
      "graphName": "generator",
      "namespace": "simple",
      "versionConstraint": ">=1.0.0",
      "required": true,
      "description": "Requires data generator for input"
    }
  ],
  "externalConnections": [
    {
      "connectionId": "generator_to_processor",
      "targetGraph": "generator",
      "targetNamespace": "simple",
      "fromProcess": "data_generator",
      "fromPort": "Output",
      "toProcess": "logger",
      "toPort": "Input",
      "description": "Connect generator output to processor input"
    }
  ]
}
```

### Data Collection Pipeline (`data/ingestion/collector.graph.json`)

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "collector",
    "description": "Data collection pipeline",
    "version": "1.0.0",
    "namespace": "data/ingestion"
  },
  "processes": {
    "api_collector": {
      "component": "DataGeneratorActor",
      "metadata": {
        "description": "Collects data from API endpoints",
        "config": {
          "data_type": "object",
          "collection_rate": "high"
        }
      }
    },
    "validator": {
      "component": "SimpleLoggerActor",
      "metadata": {
        "description": "Validates collected data"
      }
    }
  },
  "connections": [
    {
      "from": { "nodeId": "api_collector", "portId": "Output" },
      "to": { "nodeId": "validator", "portId": "Input" },
      "metadata": {}
    }
  ],
  "inports": {
    "trigger": {
      "nodeId": "api_collector",
      "portId": "Trigger"
    }
  },
  "outports": {
    "validated_data": {
      "nodeId": "validator",
      "portId": "Output"
    }
  },
  "groups": [],
  "providedInterfaces": {
    "raw_data_output": {
      "interfaceId": "raw_data_output",
      "processName": "validator",
      "portName": "Output",
      "dataType": "ValidatedData",
      "description": "Validated raw data output",
      "required": false
    }
  },
  "requiredInterfaces": {},
  "graphDependencies": [],
  "externalConnections": []
}
```

### Data Transformation Pipeline (`data/processing/transformer.graph.json`)

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "transformer",
    "description": "Data transformation pipeline",
    "version": "1.0.0",
    "namespace": "data/processing",
    "dependencies": ["collector"]
  },
  "processes": {
    "cleaner": {
      "component": "SimpleLoggerActor",
      "metadata": {
        "description": "Cleans and normalizes data"
      }
    },
    "enricher": {
      "component": "DataGeneratorActor",
      "metadata": {
        "description": "Enriches data with additional context"
      }
    }
  },
  "connections": [
    {
      "from": { "nodeId": "cleaner", "portId": "Output" },
      "to": { "nodeId": "enricher", "portId": "Trigger" },
      "metadata": {}
    }
  ],
  "inports": {
    "raw_data": {
      "nodeId": "cleaner",
      "portId": "Input"
    }
  },
  "outports": {
    "clean_data": {
      "nodeId": "enricher",
      "portId": "Output"
    }
  },
  "groups": [],
  "providedInterfaces": {
    "clean_data_output": {
      "interfaceId": "clean_data_output",
      "processName": "enricher",
      "portName": "Output",
      "dataType": "CleanData",
      "description": "Cleaned and enriched data",
      "required": false
    }
  },
  "requiredInterfaces": {
    "raw_data_input": {
      "interfaceId": "raw_data_input",
      "processName": "cleaner",
      "portName": "Input",
      "dataType": "ValidatedData",
      "description": "Raw data input from collector",
      "required": true
    }
  },
  "graphDependencies": [
    {
      "graphName": "collector",
      "namespace": "data/ingestion",
      "versionConstraint": ">=1.0.0",
      "required": true,
      "description": "Requires data collector for input"
    }
  ],
  "externalConnections": [
    {
      "connectionId": "collector_to_transformer",
      "targetGraph": "collector",
      "targetNamespace": "data/ingestion",
      "fromProcess": "validator",
      "fromPort": "Output",
      "toProcess": "cleaner",
      "toPort": "Input",
      "description": "Connect collector output to transformer input"
    }
  ]
}
```

### ML Training Pipeline (`ml/training/trainer.graph.json`)

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "trainer",
    "description": "ML training pipeline",
    "version": "1.0.0",
    "namespace": "ml/training",
    "dependencies": ["transformer"]
  },
  "processes": {
    "feature_engineer": {
      "component": "SimpleLoggerActor",
      "metadata": {
        "description": "Engineers features for ML training"
      }
    },
    "model_trainer": {
      "component": "DataGeneratorActor",
      "metadata": {
        "description": "Trains ML models",
        "config": {
          "data_type": "object"
        }
      }
    }
  },
  "connections": [
    {
      "from": { "nodeId": "feature_engineer", "portId": "Output" },
      "to": { "nodeId": "model_trainer", "portId": "Trigger" },
      "metadata": {}
    }
  ],
  "inports": {
    "training_data": {
      "nodeId": "feature_engineer",
      "portId": "Input"
    }
  },
  "outports": {
    "trained_model": {
      "nodeId": "model_trainer",
      "portId": "Output"
    }
  },
  "groups": [],
  "providedInterfaces": {
    "model_output": {
      "interfaceId": "model_output",
      "processName": "model_trainer",
      "portName": "Output",
      "dataType": "TrainedModel",
      "description": "Trained ML model",
      "required": false
    }
  },
  "requiredInterfaces": {
    "clean_data_input": {
      "interfaceId": "clean_data_input",
      "processName": "feature_engineer",
      "portName": "Input",
      "dataType": "CleanData",
      "description": "Clean data for training",
      "required": true
    }
  },
  "graphDependencies": [
    {
      "graphName": "transformer",
      "namespace": "data/processing",
      "versionConstraint": ">=1.0.0",
      "required": true,
      "description": "Requires clean data from transformer"
    }
  ],
  "externalConnections": [
    {
      "connectionId": "transformer_to_trainer",
      "targetGraph": "transformer",
      "targetNamespace": "data/processing",
      "fromProcess": "enricher",
      "fromPort": "Output",
      "toProcess": "feature_engineer",
      "toPort": "Input",
      "description": "Connect transformer output to trainer input"
    }
  ]
}
```

### System Monitor (`monitoring/system_monitor.graph.json`)

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "system_monitor",
    "description": "System monitoring and metrics collection",
    "version": "1.0.0",
    "namespace": "monitoring",
    "dependencies": ["trainer", "transformer", "collector"]
  },
  "processes": {
    "metrics_collector": {
      "component": "SimpleLoggerActor",
      "metadata": {
        "description": "Collects system metrics"
      }
    },
    "alert_manager": {
      "component": "DataGeneratorActor",
      "metadata": {
        "description": "Manages alerts and notifications"
      }
    }
  },
  "connections": [
    {
      "from": { "nodeId": "metrics_collector", "portId": "Output" },
      "to": { "nodeId": "alert_manager", "portId": "Trigger" },
      "metadata": {}
    }
  ],
  "inports": {
    "metrics": {
      "nodeId": "metrics_collector",
      "portId": "Input"
    }
  },
  "outports": {
    "alerts": {
      "nodeId": "alert_manager",
      "portId": "Output"
    }
  },
  "groups": [],
  "providedInterfaces": {
    "alert_output": {
      "interfaceId": "alert_output",
      "processName": "alert_manager",
      "portName": "Output",
      "dataType": "Alert",
      "description": "System alerts and notifications",
      "required": false
    }
  },
  "requiredInterfaces": {
    "metrics_input": {
      "interfaceId": "metrics_input",
      "processName": "metrics_collector",
      "portName": "Input",
      "dataType": "SystemMetrics",
      "description": "System metrics for monitoring",
      "required": true
    }
  },
  "graphDependencies": [
    {
      "graphName": "trainer",
      "namespace": "ml/training",
      "versionConstraint": ">=1.0.0",
      "required": false,
      "description": "Monitors ML training pipeline"
    },
    {
      "graphName": "transformer",
      "namespace": "data/processing",
      "versionConstraint": ">=1.0.0",
      "required": false,
      "description": "Monitors data processing pipeline"
    },
    {
      "graphName": "collector",
      "namespace": "data/ingestion",
      "versionConstraint": ">=1.0.0",
      "required": false,
      "description": "Monitors data collection pipeline"
    }
  ],
  "externalConnections": []
}
```

## Step 4: Workspace Discovery Example

Create the main application in `src/main.rs`:

```rust
use reflow_network::{
    multi_graph::{
        workspace::{WorkspaceDiscovery, WorkspaceConfig},
        GraphComposer, GraphComposition, GraphSource,
    },
};
use std::{collections::HashMap, path::PathBuf};

mod actors;
pub use actors::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🚀 Multi-Graph Workspace Example");
    println!("===============================");

    // Configure workspace discovery
    let workspace_config = WorkspaceConfig {
        root_path: PathBuf::from("."),
        graph_patterns: vec![
            "**/*.graph.json".to_string(),
            "**/*.graph.yaml".to_string(),
        ],
        excluded_paths: vec![
            "**/target/**".to_string(),
            "**/.git/**".to_string(),
        ],
        max_depth: Some(5),
        namespace_strategy: reflow_network::multi_graph::NamespaceStrategy::FolderStructure,
    };

    // Discover workspace
    let discovery = WorkspaceDiscovery::new(workspace_config);
    let workspace = discovery.discover_workspace().await?;

    println!("📊 Workspace Discovery Results:");
    println!("  Discovered {} graphs across {} namespaces",
        workspace.graphs.len(),
        workspace.namespaces.len()
    );

    // Print discovered graphs by namespace
    for (namespace, info) in &workspace.namespaces {
        println!("\n📁 Namespace: {}", namespace);
        println!("  Path: {}", info.path.display());
        println!("  Graphs:");
        for graph_name in &info.graphs {
            let graph_meta = workspace.graphs.iter()
                .find(|g| g.graph.properties.get("name").and_then(|v| v.as_str()).unwrap_or("") == graph_name)
                .unwrap();
            println!("    📈 {} ({})", graph_name, graph_meta.file_info.path.file_name().unwrap().to_string_lossy());
            
            // Show dependencies
            if let Some(deps) = graph_meta.graph.properties.get("dependencies").and_then(|v| v.as_array()) {
                if !deps.is_empty() {
                    print!("      Dependencies: ");
                    for (i, dep) in deps.iter().enumerate() {
                        if i > 0 { print!(", "); }
                        print!("{}", dep.as_str().unwrap_or("unknown"));
                    }
                    println!();
                }
            }
        }
    }

    // Analyze dependencies
    println!("\n🔍 Dependency Analysis:");
    if !workspace.analysis.dependencies.is_empty() {
        for dep in &workspace.analysis.dependencies {
            println!("  📦 {} depends on {} ({})",
                dep.dependent_graph,
                dep.dependency_graph,
                if dep.required { "required" } else { "optional" }
            );
        }
    } else {
        println!("  No dependencies declared");
    }

    // Show provided and required interfaces
    println!("\n🔌 Interface Analysis:");
    
    if !workspace.analysis.provided_interfaces.is_empty() {
        println!("  Provided Interfaces:");
        for interface in &workspace.analysis.provided_interfaces {
            println!("    📤 {}: {} provides {}",
                interface.namespace,
                interface.graph_name,
                interface.interface_name
            );
        }
    }

    if !workspace.analysis.required_interfaces.is_empty() {
        println!("  Required Interfaces:");
        for interface in &workspace.analysis.required_interfaces {
            println!("    📥 {}: {} requires {}",
                interface.namespace,
                interface.graph_name,
                interface.interface_name
            );
        }
    }

    // Create graph composition
    println!("\n🔧 Creating Graph Composition...");
    
    let sources: Vec<GraphSource> = workspace.graphs.iter()
        .map(|g| GraphSource::GraphExport(g.graph.clone()))
        .collect();

    let composition = GraphComposition {
        sources,
        connections: vec![], // Inter-graph connections would go here
        shared_resources: vec![],
        properties: HashMap::from([
            ("name".to_string(), serde_json::json!("multi_graph_workspace")),
            ("description".to_string(), serde_json::json!("Composed multi-graph workspace")),
        ]),
        case_sensitive: Some(false),
        metadata: None,
    };

    // Compose the graphs
    let mut composer = GraphComposer::new();
    let composed_graph = composer.compose_graphs(composition).await?;

    println!("✅ Successfully composed workspace into unified graph!");
    println!("  Total processes: {}", composed_graph.export().processes.len());
    println!("  Total connections: {}", composed_graph.export().connections.len());

    // Show composed processes by namespace
    println!("\n📋 Composed Graph Structure:");
    let mut namespaced_processes: HashMap<String, Vec<String>> = HashMap::new();
    
    for (process_name, _) in &composed_graph.export().processes {
        if let Some(namespace_sep) = process_name.find('/') {
            let namespace = &process_name[..namespace_sep];
            let process = &process_name[namespace_sep + 1..];
            namespaced_processes
                .entry(namespace.to_string())
                .or_insert_with(Vec::new)
                .push(process.to_string());
        } else {
            namespaced_processes
                .entry("root".to_string())
                .or_insert_with(Vec::new)
                .push(process_name.clone());
        }
    }
    
    for (namespace, processes) in &namespaced_processes {
        println!("  📁 {}: {} processes", namespace, processes.len());
        for process in processes {
            println!("    📈 {}", process);
        }
    }

    println!("\n🎯 Workspace composition complete!");
    
    Ok(())
}
```

## Step 5: Running the Example

Create a simple workspace example in `simple_workspace_example.rs`:

```rust
use reflow_network::{
    multi_graph::workspace::{WorkspaceDiscovery, WorkspaceConfig},
    network::{Network, NetworkConfig},
};
use std::path::PathBuf;

mod actors;
use actors::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    println!("🚀 Simple Multi-Graph Workspace Example");

    // Simple workspace discovery
    let workspace_config = WorkspaceConfig {
        root_path: PathBuf::from("simple"),
        graph_patterns: vec!["*.graph.json".to_string()],
        excluded_paths: vec![],
        max_depth: Some(2),
        namespace_strategy: reflow_network::multi_graph::NamespaceStrategy::FolderStructure,
    };

    let discovery = WorkspaceDiscovery::new(workspace_config);
    let workspace = discovery.discover_workspace().await?;

    println!("Found {} graphs:", workspace.graphs.len());
    for graph_meta in &workspace.graphs {
        println!("  - {} ({})", 
            graph_meta.graph.properties.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed"),
            graph_meta.discovered_namespace
        );
    }

    // Create a simple network to test one of the graphs
    let mut network = Network::new(NetworkConfig::default());

    // Register our actors
    network.register_actor("timer", SimpleTimerActor::new())?;
    network.register_actor("generator", DataGeneratorActor::new())?;
    network.register_actor("logger", SimpleLoggerActor::new())?;

    // Create simple workflow nodes
    network.add_node("timer_node", "timer", None)?;
    network.add_node("generator_node", "generator", None)?;
    network.add_node("logger_node", "logger", None)?;

    // Connect them
    network.add_connection(reflow_network::connector::Connector {
        from: reflow_network::connector::ConnectionPoint {
            actor: "timer_node".to_string(),
            port: "Output".to_string(),
            ..Default::default()
        },
        to: reflow_network::connector::ConnectionPoint {
            actor: "generator_node".to_string(),
            port: "Trigger".to_string(),
            ..Default::default()
        },
    })?;

    network.add_connection(reflow_network::connector::Connector {
        from: reflow_network::connector::ConnectionPoint {
            actor: "generator_node".to_string(),
            port: "Output".to_string(),
            ..Default::default()
        },
        to: reflow_network::connector::ConnectionPoint {
            actor: "logger_node".to_string(),
            port: "Input".to_string(),
            ..Default::default()
        },
    })?;

    // Start the network
    network.start().await?;

    println!("✅ Network started. Starting timer...");

    // Start the timer
    network.send_to_actor("timer_node", "Start", reflow_network::message::Message::Boolean(true))?;

    // Let it run for a bit
    tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;

    network.shutdown();
    println!("🎯 Simple example complete!");

    Ok(())
}
```

## Usage

### Running the Full Workspace Example

```bash
# In your workspace directory
cargo run --bin multi_graph_workspace
```

Expected output:
```
🚀 Multi-Graph Workspace Example
📊 Workspace Discovery Results:
  Discovered 6 graphs across 4 namespaces

📁 Namespace: simple
  Path: ./simple
  Graphs:
    📈 generator (generator.graph.json)
    📈 processor (processor.graph.json)
      Dependencies: generator

📁 Namespace: data/ingestion
  Path: ./data/ingestion
  Graphs:
    📈 collector (collector.graph.json)

📁 Namespace: data/processing
  Path: ./data/processing
  Graphs:
    📈 transformer (transformer.graph.json)
      Dependencies: collector

📁 Namespace: ml/training
  Path: ./ml/training
  Graphs:
    📈 trainer (trainer.graph.json)
      Dependencies: transformer

📁 Namespace: monitoring
  Path: ./monitoring
  Graphs:
    📈 system_monitor (system_monitor.graph.json)
      Dependencies: trainer, transformer, collector

🔍 Dependency Analysis:
  📦 processor depends on generator (required)
  📦 transformer depends on collector (required)
  📦 trainer depends on transformer (required)
  📦 system_monitor depends on trainer (optional)
  📦 system_monitor depends on transformer (optional)
  📦 system_monitor depends on collector (optional)

🔌 Interface Analysis:
  Provided Interfaces:
    📤 simple: generator provides data_output
    📤 simple: processor provides processed_output
    📤 data/ingestion: collector provides raw_data_output
    📤 data/processing: transformer provides clean_data_output
    📤 ml/training: trainer provides model_output
    📤 monitoring: system_monitor provides alert_output
  Required Interfaces:
    📥 simple: processor requires data_input
    📥 data/processing: transformer requires raw_data_input
    📥 ml/training: trainer requires clean_data_input
    📥 monitoring: system_monitor requires metrics_input

🔧 Creating Graph Composition...
✅ Successfully composed workspace into unified graph!
  Total processes: 12
  Total connections: 8

📋 Composed Graph Structure:
  📁 simple: 3 processes
    📈 timer
    📈 data_generator
    📈 logger
  📁 data: 3 processes
    📈 api_collector
    📈 validator
    📈 cleaner
    📈 enricher
  📁 ml: 2 processes
    📈 feature_engineer
    📈 model_trainer
  📁 monitoring: 2 processes
    📈 metrics_collector
    📈 alert_manager

🎯 Workspace composition complete!
```

### Running the Simple Example

```bash
cargo run --bin simple_workspace_example
```

Expected output:
```
🚀 Simple Multi-Graph Workspace Example
Found 2 graphs:
  - generator (simple)
  - processor (simple)
✅ Network started. Starting timer...
[12:34:56.123] LOG: {"tick":1,"timestamp":"2023-12-01T12:34:56.123Z","source":"SimpleTimerActor","max_ticks":10}
[12:34:57.124] LOG: {"tick":2,"timestamp":"2023-12-01T12:34:57.124Z","source":"SimpleTimerActor","max_ticks":10}
...
🎯 Simple example complete!
```

## Key Concepts Demonstrated

### 1. **Automatic Discovery**
- Workspace automatically finds all `.graph.json` files
- Uses folder structure as natural namespaces
- Handles dependency analysis

### 2. **Namespace Organization**
- `simple/` → `simple` namespace
- `data/ingestion/` → `data/ingestion` namespace
- `ml/training/` → `ml/training` namespace

### 3. **Dependency Management**
- Graphs declare dependencies on other graphs
- System validates and orders graphs by dependencies
- Supports optional and required dependencies

### 4. **Interface Definitions**
- Graphs declare provided and required interfaces
- System analyzes interface compatibility
- Enables automatic connection suggestions

### 5. **Graph Composition**
- Multiple graphs composed into unified workflow
- Namespace prefixes prevent name conflicts
- Maintains original graph structure and relationships

## Advanced Features

### Custom Namespace Strategies

```rust
use reflow_network::multi_graph::NamespaceStrategy;

// Custom semantic-based namespacing
let custom_strategy = NamespaceStrategy::custom("semantic_based", None)?;

let workspace_config = WorkspaceConfig {
    namespace_strategy: custom_strategy,
    ..Default::default()
};
```

### Selective Graph Loading

```rust
// Only load graphs matching specific patterns
let workspace_config = WorkspaceConfig {
    graph_patterns: vec![
        "data/**/*.graph.json".to_string(),  // Only data pipelines
        "ml/**/*.graph.json".to_string(),    // Only ML pipelines
    ],
    ..Default::default()
};
```

### Interface-Based Connections

```rust
use reflow_network::multi_graph::GraphConnectionBuilder;

// Connect graphs using interface definitions
let mut connection_builder = GraphConnectionBuilder::new(workspace);

connection_builder
    .connect_interface(
        "generator",     // Source graph
        "data_output",   // Source interface
        "processor",     // Target graph
        "data_input"     // Target interface
    )?;

let connections = connection_builder.build();
```

## Best Practices

### 1. **Graph Organization**
- Use descriptive folder structures
- Group related graphs in same namespace
- Keep dependencies minimal and explicit

### 2. **Interface Design**
- Define clear input/output interfaces
- Use descriptive interface names
- Document expected data types

### 3. **Dependency Management**
- Declare all dependencies explicitly
- Use version constraints for stability
- Minimize circular dependencies

### 4. **Testing**
- Test individual graphs before composition
- Validate interfaces between graphs
- Test composed workflows end-to-end

## Next Steps

1. **Try the distributed networks tutorial** to learn about cross-network communication
2. **Explore the graph composition API** for advanced composition scenarios
3. **Build your own multi-graph workspace** with domain-specific actors and workflows

The multi-graph workspace system enables you to build complex, modular workflows that scale naturally with your project's complexity while maintaining clean separation of concerns.
