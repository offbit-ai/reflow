# Distributed Workflow Example

Learn how to build and deploy distributed workflows using Reflow's distributed networking capabilities.

## Overview

This tutorial demonstrates how to create a complete distributed workflow that spans multiple network instances. We'll build a real-world example: a distributed data processing and machine learning pipeline.

## What You'll Build

A distributed system with three network instances:

1. **Data Instance**: Collects and processes raw data
2. **ML Instance**: Trains and evaluates machine learning models  
3. **API Instance**: Serves predictions and provides monitoring

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│  Data Instance  │───▶│  ML Instance    │───▶│  API Instance   │
│                 │    │                 │    │                 │
│ • Data Collector│    │ • Feature Eng.  │    │ • Prediction API│
│ • Data Processor│    │ • Model Trainer │    │ • Monitoring    │
│ • Data Validator│    │ • Model Eval.   │    │ • Dashboard     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
```

## Prerequisites

- Rust development environment
- Basic understanding of Reflow actors and networks
- Familiarity with distributed systems concepts

## Step 1: Project Setup

Create the project structure:

```bash
mkdir distributed_ml_pipeline
cd distributed_ml_pipeline

# Create instance directories
mkdir -p instances/{data,ml,api}
mkdir -p shared/actors
mkdir -p shared/types

# Initialize Cargo workspace
cargo init --name distributed_ml_pipeline
```

### Cargo.toml

```toml
[workspace]
members = [
    "instances/data",
    "instances/ml", 
    "instances/api",
    "shared/actors",
    "shared/types"
]

[workspace.dependencies]
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

## Step 2: Shared Types and Actors

### Shared Types

Create `shared/types/src/lib.rs`:

```rust
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub features: Vec<f64>,
    pub metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedData {
    pub record_id: String,
    pub processed_features: Vec<f64>,
    pub quality_score: f64,
    pub processing_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingData {
    pub features: Vec<Vec<f64>>,
    pub labels: Vec<f64>,
    pub metadata: TrainingMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainingMetadata {
    pub total_samples: usize,
    pub feature_count: usize,
    pub training_timestamp: DateTime<Utc>,
    pub data_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrainedModel {
    pub model_id: String,
    pub model_data: Vec<u8>, // Serialized model
    pub performance_metrics: ModelMetrics,
    pub training_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelMetrics {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionRequest {
    pub request_id: String,
    pub features: Vec<f64>,
    pub model_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionResponse {
    pub request_id: String,
    pub prediction: f64,
    pub confidence: f64,
    pub model_version: String,
    pub processing_time_ms: u64,
}
```

### Shared Actors

Create `shared/actors/src/lib.rs`:

```rust
use reflow_network::{
    actor::{Actor, ActorConfig, ActorContext, ActorLoad, MemoryState, Port},
    message::{Message, EncodableValue},
};
use shared_types::*;
use std::{collections::HashMap, sync::Arc};
use actor_macro::actor;
use anyhow::Error;

/// Logging actor that can be shared across all instances
#[actor(
    DistributedLoggerActor,
    inports::<100>(Input),
    outports::<50>(Output),
    state(MemoryState)
)]
pub async fn distributed_logger_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config();
    
    let instance_name = config.get_string("instance_name").unwrap_or("unknown".to_string());
    let log_level = config.get_string("log_level").unwrap_or("info".to_string());
    
    for (port, message) in payload.iter() {
        let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        
        match message {
            Message::String(s) => {
                println!("[{}] [{}] [{}]: {}", timestamp, instance_name, log_level.to_uppercase(), s);
            },
            Message::Object(obj) => {
                if let Ok(json_str) = serde_json::to_string_pretty(obj) {
                    println!("[{}] [{}] [{}]:\n{}", timestamp, instance_name, log_level.to_uppercase(), json_str);
                }
            },
            _ => {
                println!("[{}] [{}] [{}]: {:?}", timestamp, instance_name, log_level.to_uppercase(), message);
            }
        }
    }
    
    Ok(HashMap::new())
}

/// Metrics collector for monitoring distributed system performance
#[actor(
    MetricsCollectorActor,
    inports::<100>(Input),
    outports::<50>(Output, Alert),
    state(MemoryState)
)]
pub async fn metrics_collector_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let mut output = HashMap::new();
    
    for (port, message) in payload.iter() {
        if let Message::Object(metric_data) = message {
            // Store metrics in state
            {
                let mut state_lock = state.lock();
                if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                    let metrics_key = format!("metrics_{}", chrono::Utc::now().timestamp());
                    state_data.insert(metrics_key, metric_data.as_value().clone());
                    
                    // Keep only last 100 metrics entries
                    let keys: Vec<String> = state_data.data().keys()
                        .filter(|k| k.starts_with("metrics_"))
                        .cloned()
                        .collect();
                    
                    if keys.len() > 100 {
                        let mut sorted_keys = keys;
                        sorted_keys.sort();
                        for key in sorted_keys.into_iter().take(keys.len() - 100) {
                            state_data.data_mut().remove(&key);
                        }
                    }
                }
            }
            
            // Check for alert conditions
            if let Some(error_rate) = metric_data.as_value().get("error_rate").and_then(|v| v.as_f64()) {
                if error_rate > 0.1 { // 10% error rate threshold
                    let alert = Message::object(EncodableValue::from(serde_json::json!({
                        "type": "high_error_rate",
                        "error_rate": error_rate,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "severity": "warning"
                    })));
                    output.insert("Alert".to_string(), alert);
                }
            }
            
            // Forward metrics for further processing
            output.insert("Output".to_string(), message.clone());
        }
    }
    
    Ok(output)
}
```

## Step 3: Data Instance

Create the data processing instance in `instances/data/src/main.rs`:

```rust
use reflow_network::{
    actor::{Actor, ActorConfig, ActorContext, ActorLoad, MemoryState, Port},
    distributed_network::{DistributedConfig, DistributedNetwork},
    message::{Message, EncodableValue},
    network::NetworkConfig,
};
use shared_actors::{DistributedLoggerActor, MetricsCollectorActor};
use shared_types::*;
use std::{collections::HashMap, sync::Arc, time::Duration};
use actor_macro::actor;
use anyhow::Error;
use tokio::time::sleep;

/// Data collector that simulates collecting raw data
#[actor(
    DataCollectorActor,
    inports::<100>(Trigger),
    outports::<50>(Output, Metrics),
    state(MemoryState)
)]
async fn data_collector_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let mut output = HashMap::new();
    
    if payload.contains_key("Trigger") {
        // Generate sample data
        let record = DataRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: chrono::Utc::now(),
            features: (0..10).map(|_| rand::random::<f64>()).collect(),
            metadata: HashMap::from([
                ("source".to_string(), serde_json::json!("sensor_array")),
                ("quality".to_string(), serde_json::json!("high")),
            ]),
        };
        
        // Update collection count
        let count = {
            let mut state_lock = state.lock();
            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                let count = state_data.get("collection_count")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0) + 1;
                state_data.insert("collection_count".to_string(), serde_json::json!(count));
                count
            } else {
                1
            }
        };
        
        // Send data for processing
        let data_message = Message::object(EncodableValue::from(serde_json::to_value(record)?));
        output.insert("Output".to_string(), data_message);
        
        // Send metrics
        let metrics = Message::object(EncodableValue::from(serde_json::json!({
            "actor": "data_collector",
            "records_collected": count,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "instance": "data"
        })));
        output.insert("Metrics".to_string(), metrics);
    }
    
    Ok(output)
}

/// Data processor that cleans and validates data
#[actor(
    DataProcessorActor,
    inports::<100>(Input),
    outports::<50>(Output, Metrics, Log),
    state(MemoryState)
)]
async fn data_processor_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let mut output = HashMap::new();
    
    for (port, message) in payload.iter() {
        if port == "Input" {
            if let Message::Object(obj) = message {
                if let Ok(record) = serde_json::from_value::<DataRecord>(obj.as_value().clone()) {
                    // Simulate data processing
                    let processed = ProcessedData {
                        record_id: record.id.clone(),
                        processed_features: record.features.iter()
                            .map(|&f| f * 2.0 + 1.0) // Simple transformation
                            .collect(),
                        quality_score: record.features.iter().sum::<f64>() / record.features.len() as f64,
                        processing_timestamp: chrono::Utc::now(),
                    };
                    
                    // Send processed data
                    let processed_message = Message::object(EncodableValue::from(serde_json::to_value(processed)?));
                    output.insert("Output".to_string(), processed_message);
                    
                    // Send log message
                    let log_message = Message::String(
                        format!("Processed data record {} with quality score {:.2}", 
                            record.id, processed.quality_score).into()
                    );
                    output.insert("Log".to_string(), log_message);
                    
                    // Send metrics
                    let metrics = Message::object(EncodableValue::from(serde_json::json!({
                        "actor": "data_processor",
                        "processing_time_ms": 10, // Simulated
                        "quality_score": processed.quality_score,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "instance": "data"
                    })));
                    output.insert("Metrics".to_string(), metrics);
                }
            }
        }
    }
    
    Ok(output)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    println!("🚀 Starting Data Instance");
    
    // Configure distributed network
    let config = DistributedConfig {
        network_id: "data_instance".to_string(),
        instance_id: "data_001".to_string(),
        bind_address: "127.0.0.1".to_string(),
        bind_port: 9001,
        discovery_endpoints: vec![],
        auth_token: Some("data_token".to_string()),
        max_connections: 10,
        heartbeat_interval_ms: 30000,
        local_network_config: NetworkConfig::default(),
    };
    
    // Create distributed network
    let mut network = DistributedNetwork::new(config).await?;
    
    // Register local actors
    network.register_local_actor("data_collector", DataCollectorActor::new(), None)?;
    network.register_local_actor("data_processor", DataProcessorActor::new(), None)?;
    network.register_local_actor("logger", DistributedLoggerActor::new(), Some(HashMap::from([
        ("instance_name".to_string(), serde_json::json!("data")),
    ])))?;
    network.register_local_actor("metrics", MetricsCollectorActor::new(), None)?;
    
    // Start the network
    network.start().await?;
    
    // Get local network for workflow setup
    {
        let local_net = network.get_local_network();
        let mut net = local_net.write();
        
        // Create workflow connections
        net.add_connection(reflow_network::connector::Connector {
            from: reflow_network::connector::ConnectionPoint {
                actor: "data_collector".to_string(),
                port: "Output".to_string(),
                ..Default::default()
            },
            to: reflow_network::connector::ConnectionPoint {
                actor: "data_processor".to_string(),
                port: "Input".to_string(),
                ..Default::default()
            },
        })?;
        
        net.add_connection(reflow_network::connector::Connector {
            from: reflow_network::connector::ConnectionPoint {
                actor: "data_processor".to_string(),
                port: "Log".to_string(),
                ..Default::default()
            },
            to: reflow_network::connector::ConnectionPoint {
                actor: "logger".to_string(),
                port: "Input".to_string(),
                ..Default::default()
            },
        })?;
        
        net.add_connection(reflow_network::connector::Connector {
            from: reflow_network::connector::ConnectionPoint {
                actor: "data_processor".to_string(),
                port: "Metrics".to_string(),
                ..Default::default()
            },
            to: reflow_network::connector::ConnectionPoint {
                actor: "metrics".to_string(),
                port: "Input".to_string(),
                ..Default::default()
            },
        })?;
    }
    
    println!("✅ Data Instance ready on 127.0.0.1:9001");
    
    // Start data collection loop
    tokio::spawn(async move {
        loop {
            sleep(Duration::from_secs(5)).await;
            
            // Trigger data collection
            let trigger_message = Message::Boolean(true);
            if let Ok(local_net) = network.get_local_network().try_read() {
                let _ = local_net.send_to_actor("data_collector", "Trigger", trigger_message);
            }
        }
    });
    
    // Keep running
    loop {
        sleep(Duration::from_secs(1)).await;
    }
}
```

## Step 4: ML Instance

Create the ML training instance in `instances/ml/src/main.rs`:

```rust
use reflow_network::{
    actor::{Actor, ActorConfig, ActorContext, ActorLoad, MemoryState, Port},
    distributed_network::{DistributedConfig, DistributedNetwork},
    message::{Message, EncodableValue},
    network::NetworkConfig,
};
use shared_actors::{DistributedLoggerActor, MetricsCollectorActor};
use shared_types::*;
use std::{collections::HashMap, sync::Arc, time::Duration};
use actor_macro::actor;
use anyhow::Error;
use tokio::time::sleep;

/// Feature engineer that prepares data for ML training
#[actor(
    FeatureEngineerActor,
    inports::<100>(Input),
    outports::<50>(Output, Log, Metrics),
    state(MemoryState)
)]
async fn feature_engineer_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let mut output = HashMap::new();
    
    for (port, message) in payload.iter() {
        if port == "Input" {
            if let Message::Object(obj) = message {
                if let Ok(processed) = serde_json::from_value::<ProcessedData>(obj.as_value().clone()) {
                    // Accumulate features for batch training
                    {
                        let mut state_lock = state.lock();
                        if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                            let mut features: Vec<Vec<f64>> = state_data.get("accumulated_features")
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default();
                            
                            let mut labels: Vec<f64> = state_data.get("accumulated_labels")
                                .and_then(|v| serde_json::from_value(v.clone()).ok())
                                .unwrap_or_default();
                            
                            features.push(processed.processed_features.clone());
                            labels.push(processed.quality_score); // Use quality score as label
                            
                            state_data.insert("accumulated_features".to_string(), serde_json::to_value(&features)?);
                            state_data.insert("accumulated_labels".to_string(), serde_json::to_value(&labels)?);
                            
                            // Send training data when we have enough samples
                            if features.len() >= 10 {
                                let training_data = TrainingData {
                                    features: features.clone(),
                                    labels: labels.clone(),
                                    metadata: TrainingMetadata {
                                        total_samples: features.len(),
                                        feature_count: features[0].len(),
                                        training_timestamp: chrono::Utc::now(),
                                        data_source: "data_instance".to_string(),
                                    },
                                };
                                
                                let training_message = Message::object(EncodableValue::from(serde_json::to_value(training_data)?));
                                output.insert("Output".to_string(), training_message);
                                
                                // Reset accumulation
                                state_data.insert("accumulated_features".to_string(), serde_json::json!([]));
                                state_data.insert("accumulated_labels".to_string(), serde_json::json!([]));
                                
                                let log_message = Message::String(
                                    format!("Generated training batch with {} samples", features.len()).into()
                                );
                                output.insert("Log".to_string(), log_message);
                            }
                        }
                    }
                    
                    // Send metrics
                    let metrics = Message::object(EncodableValue::from(serde_json::json!({
                        "actor": "feature_engineer",
                        "features_processed": 1,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "instance": "ml"
                    })));
                    output.insert("Metrics".to_string(), metrics);
                }
            }
        }
    }
    
    Ok(output)
}

/// Model trainer that trains ML models
#[actor(
    ModelTrainerActor,
    inports::<100>(Input),
    outports::<50>(Output, Log, Metrics),
    state(MemoryState)
)]
async fn model_trainer_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let mut output = HashMap::new();
    
    for (port, message) in payload.iter() {
        if port == "Input" {
            if let Message::Object(obj) = message {
                if let Ok(training_data) = serde_json::from_value::<TrainingData>(obj.as_value().clone()) {
                    // Simulate model training
                    sleep(Duration::from_millis(100)).await; // Simulate training time
                    
                    let model = TrainedModel {
                        model_id: uuid::Uuid::new_v4().to_string(),
                        model_data: vec![1, 2, 3, 4, 5], // Dummy model data
                        performance_metrics: ModelMetrics {
                            accuracy: 0.85 + rand::random::<f64>() * 0.1,
                            precision: 0.82 + rand::random::<f64>() * 0.15,
                            recall: 0.78 + rand::random::<f64>() * 0.2,
                            f1_score: 0.80 + rand::random::<f64>() * 0.15,
                        },
                        training_timestamp: chrono::Utc::now(),
                    };
                    
                    // Send trained model
                    let model_message = Message::object(EncodableValue::from(serde_json::to_value(model.clone())?));
                    output.insert("Output".to_string(), model_message);
                    
                    // Send log message
                    let log_message = Message::String(
                        format!("Trained model {} with accuracy {:.3}", 
                            model.model_id, model.performance_metrics.accuracy).into()
                    );
                    output.insert("Log".to_string(), log_message);
                    
                    // Send metrics
                    let metrics = Message::object(EncodableValue::from(serde_json::json!({
                        "actor": "model_trainer",
                        "model_id": model.model_id,
                        "accuracy": model.performance_metrics.accuracy,
                        "training_samples": training_data.metadata.total_samples,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "instance": "ml"
                    })));
                    output.insert("Metrics".to_string(), metrics);
                }
            }
        }
    }
    
    Ok(output)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    
    println!("🚀 Starting ML Instance");
    
    // Configure distributed network
    let config = DistributedConfig {
        network_id: "ml_instance".to_string(),
        instance_id: "ml_001".to_string(),
        bind_address: "127.0.0.1".to_string(),
        bind_port: 9002,
        discovery_endpoints: vec![],
        auth_token: Some("ml_token".to_string()),
        max_connections: 10,
        heartbeat_interval_ms: 30000,
        local_network_config: NetworkConfig::default(),
    };
    
    // Create distributed network
    let mut network = DistributedNetwork::new(config).await?;
    
    // Register local actors
    network.register_local_actor("feature_engineer", FeatureEngineerActor::new(), None)?;
    network.register_local_actor("model_trainer", ModelTrainerActor::new(), None)?;
    network.register_local_actor("logger", DistributedLoggerActor::new(), Some(HashMap::from([
        ("instance_name".to_string(), serde_json::json!("ml")),
    ])))?;
    network.register_local_actor("metrics", MetricsCollectorActor::new(), None)?;
    
    // Start the network
    network.start().await?;
    
    // Get local network for workflow setup
    {
        let local_net = network.get_local_network();
        let mut net = local_net.write();
        
        // Create workflow connections
        net.add_connection(reflow_network::connector::Connector {
            from: reflow_network::connector::ConnectionPoint {
                actor: "feature_engineer".to_string(),
                port: "Output".to_string(),
                ..Default::default()
            },
            to: reflow_network::connector::ConnectionPoint {
                actor: "model_trainer".to_string(),
                port: "Input".to_string(),
                ..Default::default()
            },
        })?;
        
        net.add_connection(reflow_network::connector::Connector {
            from: reflow_network::connector::ConnectionPoint {
                actor: "feature_engineer".to_string(),
                port: "Log".to_string(),
                ..Default::default()
            },
            to: reflow_network::connector::ConnectionPoint {
                actor: "logger".to_string(),
                port: "Input".to_string(),
                ..Default::default()
            },
        })?;
        
        net.add_connection(reflow_network::connector::Connector {
            from: reflow_network::connector::ConnectionPoint {
                actor: "model_trainer".to_string(),
                port: "Log".to_string(),
                ..Default::default()
            },
            to: reflow_network::connector::ConnectionPoint {
                actor: "logger".to_string(),
                port: "Input".to_string(),
                ..Default::default()
            },
        })?;
    }
    
    println!("✅ ML Instance ready on 127.0.0.1:9002");
    
    // Connect to data instance
    println!("🔌 Connecting to data instance...");
    network.connect_to_network("127.0.0.1:9001").await?;
    
    // Register remote actors from data instance
    network.register_remote_actor("data_processor", "data_instance").await?;
    
    // Connect data processor to feature engineer
    {
        let local_net = network.get_local_network();
        let net = local_net.read();
        // Note: This would connect via the proxy actor created for data_processor
    }
    
    println!("✅ Connected to data instance");
    
    // Keep running
    loop {
        sleep(Duration::from_secs(1)).await;
    }
}
```

## Step 5: API Instance

Create the API serving instance in `instances/api/src/main.rs`:

```rust
use reflow_network::{
    actor::{Actor, ActorConfig, ActorContext, ActorLoad, MemoryState, Port},
    distributed_network::{DistributedConfig, DistributedNetwork},
    message::{Message, EncodableValue},
    network::NetworkConfig,
};
use shared_actors::{DistributedLoggerActor, MetricsCollectorActor};
use shared_types::*;
use std::{collections::HashMap, sync::Arc, time::Duration};
use actor_macro::actor;
use anyhow::Error;
use tokio::time::sleep;

/// Prediction service that serves ML predictions
#[actor(
    PredictionServiceActor,
    inports::<100>(ModelUpdate, PredictionRequest),
    outports::<50>(PredictionResponse, Log, Metrics),
    state(MemoryState)
)]
async fn prediction_service_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let mut output = HashMap::new();
    
    for (port, message) in payload.iter() {
        match port.as_str() {
            "ModelUpdate" => {
                if let Message::Object(obj) = message {
                    if let Ok(model) = serde_json::from_value::<TrainedModel>(obj.as_value().clone()) {
                        // Store the latest model
                        {
                            let mut state_lock = state.lock();
                            if let Some(state_data) = state_lock.as_mut_any().downcast_mut::<MemoryState>() {
                                state_data.insert("current_model".to_string(), serde_json::to_value(model.clone())?);
                                state_data.insert("model_version".to_string(), serde_json::json!(model.model_id));
                            }
                        }
                        
                        let log_message = Message::String(
                            format!("Updated prediction model to {} (accuracy: {:.3})",
