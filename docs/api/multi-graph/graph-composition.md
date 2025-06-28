# Graph Composition

Learn how to compose multiple discovered graphs into unified workflows.

## Overview

Graph composition allows you to:

- **Combine multiple graphs**: Merge discovered graphs into a single executable network
- **Create cross-graph connections**: Connect processes across different graph namespaces
- **Resolve dependencies**: Handle inter-graph dependencies automatically
- **Share resources**: Create shared processes accessible by multiple graphs
- **Build unified workflows**: Transform modular graphs into cohesive pipelines

## Basic Composition

### Using GraphComposer

The primary tool for composing graphs:

```rust
use reflow_network::multi_graph::{GraphComposer, GraphComposition, GraphSource};

// Create composer
let mut composer = GraphComposer::new();

// Define composition
let composition = GraphComposition {
    sources: vec![
        GraphSource::JsonFile("data/collector.graph.json".to_string()),
        GraphSource::JsonFile("data/processor.graph.json".to_string()),
        GraphSource::JsonFile("ml/trainer.graph.json".to_string()),
    ],
    connections: vec![
        // Cross-graph connections defined here
    ],
    shared_resources: vec![
        // Shared processes defined here
    ],
    properties: HashMap::from([
        ("name".to_string(), serde_json::json!("composed_workflow")),
        ("version".to_string(), serde_json::json!("1.0.0")),
    ]),
    case_sensitive: Some(false),
    metadata: None,
};

// Compose into unified graph
let composed_graph = composer.compose_graphs(composition).await?;
```

### From Workspace Discovery

Compose directly from discovered workspaces:

```rust
use reflow_network::multi_graph::workspace::{WorkspaceDiscovery, WorkspaceConfig};

// Discover workspace
let discovery = WorkspaceDiscovery::new(WorkspaceConfig::default());
let workspace = discovery.discover_workspace().await?;

// Convert to composition sources
let sources: Vec<GraphSource> = workspace.graphs
    .into_iter()
    .map(|g| GraphSource::GraphExport(g.graph))
    .collect();

let composition = GraphComposition {
    sources,
    connections: vec![], // Will be populated
    shared_resources: vec![],
    properties: HashMap::from([
        ("name".to_string(), serde_json::json!("workspace_composition")),
    ]),
    case_sensitive: Some(false),
    metadata: None,
};

let composed_graph = composer.compose_graphs(composition).await?;
```

## Cross-Graph Connections

### Manual Connection Definition

Create explicit connections between graphs:

```rust
use reflow_network::multi_graph::{CompositionConnection, CompositionEndpoint};

let composition = GraphComposition {
    sources: vec![
        GraphSource::JsonFile("data/collector.graph.json".to_string()),
        GraphSource::JsonFile("ml/trainer.graph.json".to_string()),
    ],
    connections: vec![
        CompositionConnection {
            from: CompositionEndpoint {
                process: "data/collector".to_string(),  // Namespaced process name
                port: "Output".to_string(),
                index: None,
            },
            to: CompositionEndpoint {
                process: "ml/feature_engineer".to_string(),
                port: "Input".to_string(),
                index: None,
            },
            metadata: Some(HashMap::from([
                ("description".to_string(), serde_json::json!("Data pipeline to ML training")),
            ])),
        },
    ],
    // ... rest of composition
};
```

### Using Connection Builder

Programmatically build connections:

```rust
use reflow_network::multi_graph::GraphConnectionBuilder;

// First, discover workspace to get graph information
let workspace = discovery.discover_workspace().await?;

// Create connection builder
let mut connection_builder = GraphConnectionBuilder::new(workspace);

// Build connections using fluent API
connection_builder
    .connect(
        "collector",       // from graph
        "data_collector", // from process
        "Output",         // from port
        "processor",      // to graph
        "data_cleaner",   // to process
        "Input"           // to port
    )?
    .connect(
        "processor",
        "data_transformer",
        "Output",
        "trainer",
        "feature_engineer",
        "Input"
    )?;

// Get connections for composition
let connections = connection_builder.build();

let composition = GraphComposition {
    sources: workspace.graphs.into_iter()
        .map(|g| GraphSource::GraphExport(g.graph))
        .collect(),
    connections,
    // ... rest of composition
};
```

### Interface-Based Connections

Connect using declared interfaces:

```rust
// Connect using interface definitions from graphs
connection_builder
    .connect_interface(
        "processor",           // from graph
        "clean_data_output",   // from interface
        "trainer",             // to graph
        "training_data_input"  // to interface
    )?
    .connect_interface(
        "trainer",
        "model_output",
        "predictor",
        "model_input"
    )?;

let connections = connection_builder.build();
```

## Shared Resources

### Defining Shared Processes

Create processes accessible by multiple graphs:

```rust
use reflow_network::multi_graph::SharedResource;

let composition = GraphComposition {
    sources: vec![
        // Multiple graphs that need logging
        GraphSource::JsonFile("data/processor.graph.json".to_string()),
        GraphSource::JsonFile("ml/trainer.graph.json".to_string()),
        GraphSource::JsonFile("api/service.graph.json".to_string()),
    ],
    shared_resources: vec![
        SharedResource {
            name: "shared_logger".to_string(),
            component: "LoggerActor".to_string(),
            metadata: Some(HashMap::from([
                ("log_level".to_string(), serde_json::json!("info")),
                ("output_file".to_string(), serde_json::json!("workflow.log")),
            ])),
        },
        SharedResource {
            name: "config_manager".to_string(),
            component: "ConfigManagerActor".to_string(),
            metadata: Some(HashMap::from([
                ("config_file".to_string(), serde_json::json!("config.yaml")),
            ])),
        },
    ],
    connections: vec![
        // Connect graphs to shared resources
        CompositionConnection {
            from: CompositionEndpoint {
                process: "data/processor".to_string(),
                port: "LogOutput".to_string(),
                index: None,
            },
            to: CompositionEndpoint {
                process: "shared_logger".to_string(),
                port: "Input".to_string(),
                index: None,
            },
            metadata: None,
        },
        // More connections to shared logger...
    ],
    // ... rest of composition
};
```

### Resource Sharing Patterns

Common patterns for shared resources:

```rust
// 1. Centralized Logging
let shared_logging = SharedResource {
    name: "central_logger".to_string(),
    component: "CentralLoggerActor".to_string(),
    metadata: Some(HashMap::from([
        ("aggregation".to_string(), serde_json::json!(true)),
        ("format".to_string(), serde_json::json!("json")),
    ])),
};

// 2. Configuration Management
let config_service = SharedResource {
    name: "config_service".to_string(),
    component: "ConfigServiceActor".to_string(),
    metadata: Some(HashMap::from([
        ("watch_changes".to_string(), serde_json::json!(true)),
    ])),
};

// 3. Metrics Collection
let metrics_collector = SharedResource {
    name: "metrics_collector".to_string(),
    component: "MetricsCollectorActor".to_string(),
    metadata: Some(HashMap::from([
        ("export_interval".to_string(), serde_json::json!(30)),
        ("export_format".to_string(), serde_json::json!("prometheus")),
    ])),
};

// 4. Authentication Service
let auth_service = SharedResource {
    name: "auth_service".to_string(),
    component: "AuthServiceActor".to_string(),
    metadata: Some(HashMap::from([
        ("token_expiry".to_string(), serde_json::json!(3600)),
        ("jwt_secret".to_string(), serde_json::json!("${JWT_SECRET}")),
    ])),
};
```

## Namespace Management

### Automatic Namespacing

Graphs are automatically namespaced during composition:

```rust
// Original process names in individual graphs:
// collector.graph.json: "data_collector"
// processor.graph.json: "data_processor"  
// trainer.graph.json: "model_trainer"

// After composition with namespace prefixes:
// "data/data_collector"     (from collector graph in data/ folder)
// "data/data_processor"     (from processor graph in data/ folder)
// "ml/model_trainer"        (from trainer graph in ml/ folder)

// Access in composed graph:
let composed_export = composed_graph.export();
assert!(composed_export.processes.contains_key("data/data_collector"));
assert!(composed_export.processes.contains_key("ml/model_trainer"));
```

### Custom Namespace Mapping

Control how namespaces are applied:

```rust
use reflow_network::multi_graph::{NamespaceMapping, NamespaceStrategy};

let namespace_mapping = NamespaceMapping {
    graph_mappings: HashMap::from([
        ("collector".to_string(), "ingestion".to_string()),
        ("processor".to_string(), "processing".to_string()),
        ("trainer".to_string(), "machine_learning".to_string()),
    ]),
    strategy: NamespaceStrategy::CustomMapping,
    collision_resolution: CollisionResolution::Prefix,
};

let composer = GraphComposer::with_namespace_mapping(namespace_mapping);
```

## Advanced Composition

### Conditional Composition

Compose graphs based on conditions:

```rust
use reflow_network::multi_graph::ConditionalComposition;

let conditional_composition = ConditionalComposition {
    base_sources: vec![
        GraphSource::JsonFile("core/processor.graph.json".to_string()),
    ],
    conditional_sources: vec![
        ConditionalSource {
            condition: Condition::EnvironmentVariable("ENABLE_ML".to_string()),
            sources: vec![
                GraphSource::JsonFile("ml/trainer.graph.json".to_string()),
                GraphSource::JsonFile("ml/predictor.graph.json".to_string()),
            ],
        },
        ConditionalSource {
            condition: Condition::ConfigValue("features.analytics".to_string()),
            sources: vec![
                GraphSource::JsonFile("analytics/collector.graph.json".to_string()),
            ],
        },
    ],
};

let composed_graph = composer.compose_conditional(conditional_composition).await?;
```

### Templated Composition

Use templates for dynamic composition:

```rust
use reflow_network::multi_graph::CompositionTemplate;

let template = CompositionTemplate {
    template_file: "templates/data_pipeline.yaml".to_string(),
    parameters: HashMap::from([
        ("input_source".to_string(), serde_json::json!("kafka")),
        ("output_destination".to_string(), serde_json::json!("postgres")),
        ("enable_validation".to_string(), serde_json::json!(true)),
    ]),
};

let composition = composer.render_template(template).await?;
let composed_graph = composer.compose_graphs(composition).await?;
```

### Layered Composition

Build compositions in layers:

```rust
// Base layer: Core functionality
let base_composition = GraphComposition {
    sources: vec![
        GraphSource::JsonFile("core/base.graph.json".to_string()),
    ],
    // ... base configuration
};

// Feature layer: Additional features
let feature_layer = GraphComposition {
    sources: vec![
        GraphSource::JsonFile("features/analytics.graph.json".to_string()),
        GraphSource::JsonFile("features/monitoring.graph.json".to_string()),
    ],
    // ... feature connections
};

// Environment layer: Environment-specific configuration
let env_layer = GraphComposition {
    sources: vec![
        GraphSource::JsonFile("env/production.graph.json".to_string()),
    ],
    // ... environment-specific resources
};

// Compose layers
let base_graph = composer.compose_graphs(base_composition).await?;
let feature_graph = composer.compose_layers(base_graph, feature_layer).await?;
let final_graph = composer.compose_layers(feature_graph, env_layer).await?;
```

## Validation and Testing

### Composition Validation

Validate composed graphs before execution:

```rust
use reflow_network::multi_graph::CompositionValidator;

let validator = CompositionValidator::new();

// Validate composition structure
let validation_result = validator.validate_composition(&composition).await?;

if !validation_result.is_valid() {
    println!("❌ Composition validation failed:");
    for error in &validation_result.errors {
        println!("  - {}", error);
    }
    for warning in &validation_result.warnings {
        println!("  ⚠️  {}", warning);
    }
} else {
    println!("✅ Composition validation passed");
}
```

### Testing Composed Graphs

Test the composed graph before deployment:

```rust
use reflow_network::multi_graph::CompositionTester;

let tester = CompositionTester::new();

// Create test scenarios
let test_scenarios = vec![
    TestScenario {
        name: "data_flow_test".to_string(),
        inputs: HashMap::from([
            ("data/collector".to_string(), vec![
                Message::String("test_data".to_string())
            ]),
        ]),
        expected_outputs: HashMap::from([
            ("ml/predictor".to_string(), vec![
                Message::Object(serde_json::json!({"prediction": 0.95}))
            ]),
        ]),
        timeout_ms: 5000,
    },
];

// Run tests
let test_results = tester.run_tests(&composed_graph, test_scenarios).await?;

for result in &test_results {
    if result.passed {
        println!("✅ Test '{}' passed", result.scenario_name);
    } else {
        println!("❌ Test '{}' failed: {}", result.scenario_name, result.error.as_ref().unwrap());
    }
}
```

## Performance Optimization

### Lazy Loading

Only load necessary graphs:

```rust
let config = CompositionConfig {
    lazy_loading: true,
    load_on_demand: true,
    cache_loaded_graphs: true,
    max_concurrent_loads: 4,
};

let composer = GraphComposer::with_config(config);
```

### Parallel Composition

Compose large numbers of graphs in parallel:

```rust
let config = CompositionConfig {
    parallel_composition: true,
    max_parallel_graphs: 8,
    composition_timeout_ms: 30000,
};

let composer = GraphComposer::with_config(config);
```

### Memory Management

Control memory usage during composition:

```rust
let config = CompositionConfig {
    max_memory_usage_mb: 1024,
    cleanup_intermediate_results: true,
    stream_large_graphs: true,
};

let composer = GraphComposer::with_config(config);
```

## Real-World Examples

### Data Processing Pipeline

```rust
// Compose a complete data processing pipeline
async fn create_data_pipeline() -> Result<Graph, CompositionError> {
    let mut composer = GraphComposer::new();
    
    let composition = GraphComposition {
        sources: vec![
            GraphSource::JsonFile("ingestion/api_collector.graph.json".to_string()),
            GraphSource::JsonFile("processing/data_cleaner.graph.json".to_string()),
            GraphSource::JsonFile("processing/transformer.graph.json".to_string()),
            GraphSource::JsonFile("storage/database_writer.graph.json".to_string()),
        ],
        connections: vec![
            CompositionConnection {
                from: CompositionEndpoint {
                    process: "ingestion/api_collector".to_string(),
                    port: "RawData".to_string(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: "processing/data_cleaner".to_string(),
                    port: "Input".to_string(),
                    index: None,
                },
                metadata: None,
            },
            CompositionConnection {
                from: CompositionEndpoint {
                    process: "processing/data_cleaner".to_string(),
                    port: "CleanedData".to_string(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: "processing/transformer".to_string(),
                    port: "Input".to_string(),
                    index: None,
                },
                metadata: None,
            },
            CompositionConnection {
                from: CompositionEndpoint {
                    process: "processing/transformer".to_string(),
                    port: "TransformedData".to_string(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: "storage/database_writer".to_string(),
                    port: "Input".to_string(),
                    index: None,
                },
                metadata: None,
            },
        ],
        shared_resources: vec![
            SharedResource {
                name: "logger".to_string(),
                component: "LoggerActor".to_string(),
                metadata: Some(HashMap::from([
                    ("level".to_string(), serde_json::json!("info")),
                ])),
            },
        ],
        properties: HashMap::from([
            ("name".to_string(), serde_json::json!("data_processing_pipeline")),
            ("version".to_string(), serde_json::json!("1.0.0")),
        ]),
        case_sensitive: Some(false),
        metadata: None,
    };
    
    composer.compose_graphs(composition).await
}
```

### ML Training Pipeline

```rust
// Compose ML training and inference pipeline
async fn create_ml_pipeline() -> Result<Graph, CompositionError> {
    let workspace = WorkspaceDiscovery::new(WorkspaceConfig {
        root_path: PathBuf::from("./ml_workspace"),
        ..Default::default()
    }).discover_workspace().await?;
    
    let mut connection_builder = GraphConnectionBuilder::new(workspace);
    
    // Build ML pipeline connections
    connection_builder
        .connect_interface(
            "data_preprocessor",
            "processed_data_output",
            "feature_engineer",
            "raw_data_input"
        )?
        .connect_interface(
            "feature_engineer",
            "features_output",
            "model_trainer",
            "training_data_input"
        )?
        .connect_interface(
            "model_trainer",
            "trained_model_output",
            "model_evaluator",
            "model_input"
        )?
        .connect_interface(
            "model_trainer",
            "trained_model_output",
            "inference_service",
            "model_input"
        )?;
    
    let connections = connection_builder.build();
    
    let composition = GraphComposition {
        sources: workspace.graphs.into_iter()
            .map(|g| GraphSource::GraphExport(g.graph))
            .collect(),
        connections,
        shared_resources: vec![
            SharedResource {
                name: "model_registry".to_string(),
                component: "ModelRegistryActor".to_string(),
                metadata: Some(HashMap::from([
                    ("storage_backend".to_string(), serde_json::json!("s3")),
                ])),
            },
            SharedResource {
                name: "metrics_tracker".to_string(),
                component: "MetricsTrackerActor".to_string(),
                metadata: Some(HashMap::from([
                    ("export_interval".to_string(), serde_json::json!(60)),
                ])),
            },
        ],
        properties: HashMap::from([
            ("name".to_string(), serde_json::json!("ml_training_pipeline")),
            ("description".to_string(), serde_json::json!("Complete ML training and inference pipeline")),
        ]),
        case_sensitive: Some(false),
        metadata: None,
    };
    
    let mut composer = GraphComposer::new();
    composer.compose_graphs(composition).await
}
```

## Best Practices

### 1. Plan Your Composition

- Design graph boundaries thoughtfully
- Keep related functionality together
- Plan for reusability across compositions

### 2. Use Clear Naming

```rust
// Good: Descriptive endpoint names
CompositionEndpoint {
    process: "data_ingestion/api_collector".to_string(),
    port: "ValidatedApiData".to_string(),
    index: None,
}

// Avoid: Generic names
CompositionEndpoint {
    process: "graph1/proc1".to_string(),
    port: "Output".to_string(),
    index: None,
}
```

### 3. Document Connections

```rust
CompositionConnection {
    from: CompositionEndpoint { /* ... */ },
    to: CompositionEndpoint { /* ... */ },
    metadata: Some(HashMap::from([
        ("description".to_string(), serde_json::json!("Cleaned data flows to ML feature engineering")),
        ("data_type".to_string(), serde_json::json!("CleanedDataRecord")),
        ("expected_rate".to_string(), serde_json::json!("1000 records/minute")),
    ])),
}
```

### 4. Validate Early and Often

```rust
// Validate before composing
let validation_result = validator.validate_composition(&composition).await?;
assert!(validation_result.is_valid());

// Test after composing
let test_results = tester.run_tests(&composed_graph, test_scenarios).await?;
assert!(test_results.iter().all(|r| r.passed));
```

### 5. Use Shared Resources Wisely

- Share stateless services (logging, config)
- Be cautious with stateful shared resources
- Consider resource contention and bottlenecks

## Next Steps

- [Dependency Resolution](dependency-resolution.md) - Handle complex dependencies
- [Workspace Discovery](workspace-discovery.md) - Discover graphs to compose
- [Tutorial: Multi-Graph Workspace](../../tutorials/multi-graph-workspace.md)

## Related Documentation

- [Architecture: Multi-Graph Composition](../../architecture/multi-graph-composition.md)
- [Graph System Overview](../../architecture/graph-system.md)
