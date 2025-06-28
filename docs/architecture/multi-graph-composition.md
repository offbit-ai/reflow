# Multi-Graph Composition

Reflow's multi-graph composition system enables automatic discovery and intelligent composition of multiple graph files into unified, executable workflows. This system transforms complex multi-graph projects into seamlessly integrated workflows through workspace discovery and intelligent stitching.

## Overview

The multi-graph composition architecture provides:

- **Automatic Workspace Discovery**: Recursively finds all `*.graph.json` and `*.graph.yaml` files in your project
- **Folder-Based Namespacing**: Uses directory structure as natural namespaces for organization
- **Smart Auto-Connections**: Automatically detects compatible interfaces between graphs
- **Dependency Resolution**: Resolves inter-graph dependencies and execution ordering
- **One-Command Composition**: Single command transforms entire workspace into executable workflow
- **Interface Analysis**: Analyzes graph interfaces for compatibility and suggests connections

## Architecture Components

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Workspace Discovery System                      │
├─────────────────────────────────────────────────────────────────────┤
│ File Discovery Layer                                               │
│ ┌─────────────────────────────────────────────────────────────────┐ │
│ │ *.graph.json    │ *.graph.yaml    │ Pattern Matching │ Filters  │ │
│ │ (data_flow)     │ (ml_pipeline)   │ (glob patterns)  │ (exclude)│ │
│ │ - 3 processes   │ - 5 processes   │ - depth limits   │ - test/  │ │
│ └─────────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│ Namespace & Analysis Layer                                          │
│ ┌─────────────────────────────────────────────────────────────────┐ │
│ │ Namespace Mgr   │ Interface Analysis │ Dependency Res │ Auto-Connect │
│ │ • Folder-based  │ • Exposed ports    │ • Graph deps   │ • Port match │
│ │ • Conflict res  │ • Required ports   │ • Order deps   │ • Confidence │ │
│ │ • Custom rules  │ • Compatibility   │ • Validation   │ • Heuristics │ │
│ └─────────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────┤
│ Unified Network Instance                                            │
│ ┌─────────────────────────────────────────────────────────────────┐ │
│ │ data/           │ ml/            │ monitoring/    │ shared/      │ │
│ │ ├─ ingestion/   │ ├─ training/   │ ├─ metrics     │ ├─ logging  │ │
│ │ │  └─ collector │ │  └─ trainer   │ ├─ alerts     │ ├─ auth     │ │
│ │ └─ processing/  │ └─ inference/  │ └─ dashboard   │ └─ config   │ │
│ │    └─ transformer│   └─ predictor│                │              │ │
│ └─────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### Core Components

1. **WorkspaceDiscovery**: Discovers and loads all graph files in a workspace
2. **GraphLoader**: Loads and validates individual graph files
3. **GraphComposer**: Orchestrates composition of multiple graphs
4. **NamespaceManager**: Manages namespaces and resolves conflicts
5. **DependencyResolver**: Analyzes and resolves graph dependencies
6. **InterfaceAnalyzer**: Detects compatible interfaces for auto-connections

## Workspace Structure

Multi-graph workspaces organize graph files using directory structure as namespaces:

```
workspace/
├── data/
│   ├── ingestion/
│   │   ├── api_collector.graph.json      → namespace: data/ingestion
│   │   └── file_reader.graph.yaml        → namespace: data/ingestion
│   ├── processing/
│   │   ├── cleaner.graph.json            → namespace: data/processing
│   │   ├── transformer.graph.json        → namespace: data/processing
│   │   └── validator.graph.yaml          → namespace: data/processing
│   └── storage/
│       ├── database_writer.graph.json    → namespace: data/storage
│       └── cache_manager.graph.yaml      → namespace: data/storage
├── ml/
│   ├── training/
│   │   ├── model_trainer.graph.json      → namespace: ml/training
│   │   └── feature_engineer.graph.yaml   → namespace: ml/training
│   ├── inference/
│   │   ├── predictor.graph.json          → namespace: ml/inference
│   │   └── batch_scorer.graph.json       → namespace: ml/inference
│   └── evaluation/
│       └── model_evaluator.graph.yaml    → namespace: ml/evaluation
├── monitoring/
│   ├── metrics.graph.json                → namespace: monitoring
│   ├── alerts.graph.yaml                 → namespace: monitoring
│   └── dashboard.graph.json              → namespace: monitoring
└── shared/
    ├── logging.graph.yaml                 → namespace: shared
    ├── auth.graph.json                    → namespace: shared
    └── config.graph.json                  → namespace: shared
```

## Basic Usage

### Simple Workspace Discovery

```rust
use reflow_network::multi_graph::workspace::{WorkspaceDiscovery, WorkspaceConfig};

// Configure workspace discovery
let config = WorkspaceConfig {
    root_path: PathBuf::from("./my_workspace"),
    graph_patterns: vec![
        "**/*.graph.json".to_string(),
        "**/*.graph.yaml".to_string(),
    ],
    excluded_paths: vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/test/**".to_string(),
    ],
    max_depth: Some(8),
    namespace_strategy: NamespaceStrategy::FolderStructure,
    ..WorkspaceConfig::default()
};

// Discover all graphs in workspace
let discovery = WorkspaceDiscovery::new(config);
let workspace = discovery.discover_workspace().await?;

println!("🎉 Discovered {} graphs across {} namespaces", 
    workspace.graphs.len(), 
    workspace.namespaces.len()
);
```

### Automatic Composition

```rust
use reflow_network::multi_graph::{GraphComposer, GraphComposition};

// Create composer and auto-compose workspace
let mut composer = GraphComposer::new();
let composition = GraphComposition::from_workspace(workspace)?;

// Compose into single executable graph
let unified_graph = composer.compose_graphs(composition).await?;

// The unified graph can now be executed as a single workflow
let mut network = Network::new(NetworkConfig::default());
let graph = Graph::load(unified_graph, None);
// Use the composed graph...
```

## Graph Dependencies and Interfaces

### Declaring Dependencies

Graphs can declare explicit dependencies on other graphs:

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "ml_trainer",
    "namespace": "ml/training",
    "version": "1.0.0"
  },
  "processes": {
    "feature_engineer": {
      "component": "FeatureEngineerActor",
      "metadata": {}
    },
    "model_trainer": {
      "component": "ModelTrainerActor",
      "metadata": {}
    }
  },
  "connections": [
    {
      "from": { "nodeId": "feature_engineer", "portId": "Output" },
      "to": { "nodeId": "model_trainer", "portId": "Input" }
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
  
  "graphDependencies": [
    {
      "graphName": "data_transformer",
      "namespace": "data/processing",
      "versionConstraint": ">=1.0.0",
      "required": true,
      "description": "Requires clean data from transformer"
    }
  ],
  "externalConnections": [
    {
      "connectionId": "transformer_to_trainer",
      "targetGraph": "data_transformer",
      "targetNamespace": "data/processing",
      "fromProcess": "normalizer",
      "fromPort": "Output",
      "toProcess": "feature_engineer",
      "toPort": "Input",
      "description": "Use cleaned data for training"
    }
  ],
  "providedInterfaces": {
    "trained_model_output": {
      "interfaceId": "trained_model_output",
      "processName": "model_trainer",
      "portName": "Output",
      "dataType": "TrainedModel",
      "description": "Trained ML model"
    }
  },
  "requiredInterfaces": {
    "clean_data_input": {
      "interfaceId": "clean_data_input",
      "processName": "feature_engineer",
      "portName": "Input",
      "dataType": "CleanedDataRecord",
      "description": "Clean data from processing pipeline",
      "required": true
    }
  }
}
```

### Interface-Based Connections

Graphs can connect via defined interfaces rather than direct process connections:

```rust
use reflow_network::multi_graph::GraphConnectionBuilder;

// Build connections between discovered graphs
let mut connection_builder = GraphConnectionBuilder::new(workspace);

// Connect using interfaces (recommended)
connection_builder
    .connect_interface(
        "data_transformer",     // Source graph
        "clean_data_output",    // Source interface
        "ml_trainer",           // Target graph
        "clean_data_input"      // Target interface
    )?
    .connect_interface(
        "ml_trainer",
        "trained_model_output",
        "ml_predictor",
        "model_input"
    )?;

let connections = connection_builder.build();
```

## Namespace Management

### Folder-Based Namespacing

By default, directory structure becomes namespace hierarchy:

```rust
// File: data/processing/transformer.graph.json
// Namespace: "data/processing"
// Qualified name: "data/processing/transformer"

// File: ml/training/trainer.graph.json  
// Namespace: "ml/training"
// Qualified name: "ml/training/trainer"
```

### Custom Namespace Strategies

```rust
use reflow_network::multi_graph::NamespaceStrategy;

// Semantic-based namespacing
let config = WorkspaceConfig {
    namespace_strategy: NamespaceStrategy::custom(
        "semantic_based", 
        Some(serde_json::json!({
            "keywords": {
                "ml": ["model", "train", "predict", "feature"],
                "data": ["ingest", "collect", "process", "clean"],
                "monitoring": ["metric", "alert", "dashboard", "log"]
            }
        }))
    )?,
    // ... other config
};
```

### Conflict Resolution

When graphs have conflicting names, the system provides several resolution strategies:

```rust
use reflow_network::multi_graph::NamespaceConflictPolicy;

let namespace_manager = GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve);

// Automatic resolution generates unique names:
// "data_processor" -> "data_processor" (first)
// "data_processor" -> "data_processor_1" (second)
// "data_processor" -> "data_processor_2" (third)
```

## Advanced Features

### Workspace Configuration

```yaml
# workspace.config.yaml
workspace:
  root_path: "./my_project"
  
  graph_patterns:
    - "**/*.graph.json"
    - "**/*.graph.yaml"
  
  excluded_paths:
    - "**/node_modules/**"
    - "**/target/**" 
    - "**/.git/**"
    - "**/test/**"
  
  max_depth: 8
  
  namespace_strategy:
    type: "folder_structure"
  
  auto_connect: true
  dependency_resolution: "automatic"

composer:
  enable_auto_connections: true
  connection_confidence_threshold: 0.75
  validate_before_compose: true
  output_path: "./workspace.composed.graph.json"
```

### Auto-Connection Discovery

The system can automatically suggest connections between compatible graph interfaces:

```rust
use reflow_network::multi_graph::InterfaceAnalyzer;

let analyzer = InterfaceAnalyzer::new();
let suggestions = analyzer.analyze_workspace(&workspace).await?;

for suggestion in suggestions.auto_connections {
    println!("🔗 Suggested connection: {} -> {}",
        suggestion.from_interface, suggestion.to_interface);
    println!("   Confidence: {:.2}", suggestion.confidence);
    println!("   Reason: {}", suggestion.reasoning);
}
```

### Dependency Resolution

```rust
use reflow_network::multi_graph::DependencyResolver;

let resolver = DependencyResolver::new();
let ordered_graphs = resolver.resolve_dependencies(&workspace.graphs)?;

println!("📊 Dependency Resolution Order:");
for (i, graph) in ordered_graphs.iter().enumerate() {
    println!("  {}. {}", i + 1, graph.get_name());
}
```

## Programmatic API

### Workspace Discovery API

```rust
// Programmatic workspace discovery
let mut discovery = WorkspaceDiscovery::new(config);

// Custom filtering
discovery.add_filter(|path: &Path| -> bool {
    // Only include graphs with "production" in the name
    path.to_string_lossy().contains("production")
});

// Custom namespace generation
discovery.set_namespace_generator(|path: &Path| -> String {
    // Custom logic for namespace generation
    if path.to_string_lossy().contains("critical") {
        format!("critical/{}", path.parent().unwrap().file_name().unwrap().to_string_lossy())
    } else {
        path.parent().unwrap().to_string_lossy().to_string()
    }
});

let workspace = discovery.discover_workspace().await?;
```

### Dynamic Graph Loading

```rust
use reflow_network::multi_graph::GraphSource;

// Load graphs from different sources
let sources = vec![
    GraphSource::JsonFile("./graphs/processor.graph.json".to_string()),
    GraphSource::NetworkApi("http://config-server/graphs/ml_model".to_string()),
    GraphSource::JsonContent(json_string),
];

let loader = GraphLoader::new();
let graphs = loader.load_multiple_graphs(sources).await?;
```

### Custom Graph Composition

```rust
// Custom composition logic
let composition = GraphComposition {
    sources: workspace.graph_sources(),
    connections: vec![
        CompositionConnection {
            from: CompositionEndpoint {
                process: "data/processing/cleaner".to_string(),
                port: "Output".to_string(),
                index: None,
            },
            to: CompositionEndpoint {
                process: "ml/training/trainer".to_string(),
                port: "Input".to_string(),
                index: None,
            },
            metadata: Some(HashMap::from([
                ("priority".to_string(), serde_json::Value::String("high".to_string())),
            ])),
        }
    ],
    shared_resources: vec![
        SharedResource {
            name: "logger".to_string(),
            component: "LoggerActor".to_string(),
            metadata: Some(HashMap::from([
                ("level".to_string(), serde_json::Value::String("info".to_string())),
            ])),
        }
    ],
    properties: HashMap::from([
        ("name".to_string(), serde_json::Value::String("workspace_composition".to_string())),
        ("version".to_string(), serde_json::Value::String("1.0.0".to_string())),
    ]),
    case_sensitive: Some(false),
    metadata: None,
};
```

## Command Line Interface

### Discovery Commands

```bash
# Discover all graphs in workspace
reflow workspace discover --path ./my_project

# Output discovery results
reflow workspace discover --path ./my_project --output workspace.json

# Analyze workspace structure and dependencies
reflow workspace analyze --path ./my_project --output analysis.json

# List discovered graphs and namespaces
reflow workspace list --path ./my_project --format table
```

### Composition Commands

```bash
# Auto-compose with high confidence connections
reflow workspace compose \
    --path ./my_project \
    --auto-connect \
    --confidence-threshold 0.8 \
    --validate \
    --output workspace.composed.graph.json

# Use configuration file
reflow workspace compose --config workspace.config.yaml

# Validate workspace before composition
reflow workspace validate --path ./my_project
```

### Export Commands

```bash
# Export enhanced graph schemas
reflow workspace export --path ./my_project --enhanced --output enhanced_graphs/

# Generate workspace documentation
reflow workspace docs --path ./my_project --output docs/
```

## Best Practices

### Graph Organization

1. **Use Descriptive Namespaces**: Organize graphs logically by function, not just technology
2. **Define Clear Interfaces**: Use provided/required interfaces for loose coupling
3. **Minimize Dependencies**: Reduce inter-graph dependencies for flexibility
4. **Version Your Graphs**: Include version information for dependency management

### Directory Structure

```
my_project/
├── core/                    # Core business logic graphs
│   ├── user_management/
│   ├── order_processing/
│   └── payment_handling/
├── integrations/            # External system integrations
│   ├── crm_sync/
│   ├── analytics_export/
│   └── notification_service/
├── pipelines/              # Data processing pipelines
│   ├── etl/
│   ├── ml_training/
│   └── reporting/
└── utilities/              # Shared utility graphs
    ├── logging/
    ├── monitoring/
    └── configuration/
```

### Interface Design

```json
{
  "providedInterfaces": {
    "user_data": {
      "interfaceId": "user_data",
      "processName": "user_processor",
      "portName": "Output",
      "dataType": "UserRecord",
      "description": "Processed user data with validation",
      "required": false,
      "metadata": {
        "schema_version": "1.2.0",
        "format": "json",
        "compression": "none"
      }
    }
  },
  "requiredInterfaces": {
    "raw_user_input": {
      "interfaceId": "raw_user_input",
      "processName": "user_processor", 
      "portName": "Input",
      "dataType": "RawUserData",
      "description": "Raw user data for processing",
      "required": true,
      "metadata": {
        "max_size": "10MB",
        "format": "json"
      }
    }
  }
}
```

## Error Handling

### Discovery Errors

```rust
use reflow_network::multi_graph::DiscoveryError;

match discovery.discover_workspace().await {
    Ok(workspace) => {
        // Process workspace
    },
    Err(DiscoveryError::GlobError(e)) => {
        eprintln!("Pattern matching error: {}", e);
    },
    Err(DiscoveryError::LoadError(path, e)) => {
        eprintln!("Failed to load {}: {}", path.display(), e);
    },
    Err(DiscoveryError::ValidationError(e)) => {
        eprintln!("Graph validation failed: {}", e);
    },
    Err(e) => eprintln!("Discovery error: {}", e),
}
```

### Composition Errors

```rust
use reflow_network::multi_graph::CompositionError;

match composer.compose_graphs(composition).await {
    Ok(graph) => {
        // Use composed graph
    },
    Err(CompositionError::DependencyError(e)) => {
        eprintln!("Dependency resolution failed: {}", e);
    },
    Err(CompositionError::NamespaceError(e)) => {
        eprintln!("Namespace conflict: {}", e);
    },
    Err(e) => eprintln!("Composition error: {}", e),
}
```

## Performance Considerations

### Large Workspaces

```rust
// Optimize for large workspaces
let config = WorkspaceConfig {
    max_depth: Some(6),  // Limit directory traversal depth
    excluded_paths: vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/.git/**".to_string(),
        "**/build/**".to_string(),
        "**/dist/**".to_string(),
    ],
    // ... other config
};
```

### Parallel Loading

```rust
// Discovery automatically parallelizes graph loading
let workspace = discovery.discover_workspace().await?;
// Graphs are loaded concurrently for better performance
```

### Caching

```rust
// Enable caching for repeated discoveries
let config = WorkspaceConfig {
    cache_discoveries: true,
    cache_ttl_seconds: Some(300), // 5 minutes
    // ... other config
};
```

## Next Steps

- [Workspace Discovery API](../api/graph/workspace-discovery.md) - Detailed API documentation
- [Graph Stitching API](../api/graph/graph-stitching.md) - Advanced composition features  
- [Multi-Graph Workspace Tutorial](../tutorials/multi-graph-workspace.md) - Step-by-step guide
- [ActorConfig System](../api/actors/actor-config.md) - Actor configuration system
