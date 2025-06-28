# Workspace Discovery

Learn how to automatically discover and load graph files in multi-graph workspaces.

## Overview

Workspace discovery enables:

- **Automatic graph discovery**: Find all `*.graph.json` and `*.graph.yaml` files recursively
- **Folder-based namespacing**: Use directory structure as natural namespaces
- **Clean instantiation**: Load discovered graphs into memory with proper isolation
- **Rich metadata**: Inject discovery information and workspace context
- **Flexible configuration**: Control discovery patterns and exclusions

## Basic Discovery

### Simple Workspace Discovery

Discover all graphs in a workspace directory:

```rust
use reflow_network::multi_graph::workspace::{WorkspaceDiscovery, WorkspaceConfig};

// Basic workspace discovery
let config = WorkspaceConfig::default();
let discovery = WorkspaceDiscovery::new(config);

// Discover all graphs in current directory
let workspace = discovery.discover_workspace().await?;

println!("🎉 Discovered {} graphs across {} namespaces", 
    workspace.graphs.len(), 
    workspace.namespaces.len()
);

// Print discovered graphs
for graph_meta in &workspace.graphs {
    let graph_name = graph_meta.graph.properties
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unnamed");
    
    println!("📈 Graph: {} (namespace: {})", 
        graph_name,
        graph_meta.discovered_namespace
    );
}
```

### Custom Discovery Configuration

Configure discovery behavior for your needs:

```rust
use std::path::PathBuf;

let workspace_config = WorkspaceConfig {
    root_path: PathBuf::from("./my_workspace"),
    graph_patterns: vec![
        "**/*.graph.json".to_string(),
        "**/*.graph.yaml".to_string(),
        "**/*.graph.yml".to_string(),
    ],
    excluded_paths: vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/.git/**".to_string(),
        "**/test/**".to_string(),
        "**/.*/**".to_string(),
    ],
    max_depth: Some(8),
    namespace_strategy: NamespaceStrategy::FolderStructure,
};

let discovery = WorkspaceDiscovery::new(workspace_config);
let workspace = discovery.discover_workspace().await?;
```

## Namespace Strategies

### 1. Folder Structure (Default)

Use directory structure as hierarchical namespaces:

```rust
let config = WorkspaceConfig {
    namespace_strategy: NamespaceStrategy::FolderStructure,
    ..Default::default()
};

// Example structure:
// data/ingestion/collector.graph.json    → namespace: "data/ingestion"
// data/processing/transformer.graph.json → namespace: "data/processing"  
// ml/training/trainer.graph.json         → namespace: "ml/training"
// ml/inference/predictor.graph.json      → namespace: "ml/inference"
```

### 2. Flattened Namespace

Put all graphs in the root namespace:

```rust
let config = WorkspaceConfig {
    namespace_strategy: NamespaceStrategy::Flatten,
    ..Default::default()
};

// All graphs get namespace: "" (root)
```

### 3. File-Based Prefixes

Use filename prefixes as namespaces:

```rust
let config = WorkspaceConfig {
    namespace_strategy: NamespaceStrategy::FileBasedPrefix,
    ..Default::default()
};

// Examples:
// ml_trainer.graph.json     → namespace: "ml"
// data_processor.graph.json → namespace: "data"
// auth_service.graph.json   → namespace: "auth"
```

### 4. Custom Namespace Functions

Define custom namespacing logic:

```rust
use reflow_network::multi_graph::workspace::NamespaceStrategy;

// Semantic-based namespacing
let config = WorkspaceConfig {
    namespace_strategy: NamespaceStrategy::custom(
        "semantic_based",
        Some(serde_json::json!({
            "rules": {
                "ml": ["model", "train", "predict"],
                "data": ["ingest", "process", "transform"],
                "api": ["service", "endpoint", "rest"]
            }
        }))
    )?,
    ..Default::default()
};

// Graphs are organized by semantic content
```

## Discovery Results

### Workspace Collection Structure

The discovery process returns a comprehensive workspace collection:

```rust
#[derive(Debug)]
pub struct WorkspaceCollection {
    pub graphs: Vec<GraphWithMetadata>,
    pub namespaces: HashMap<String, NamespaceInfo>,
    pub dependency_analysis: DependencyAnalysis,
    pub workspace_root: PathBuf,
}

// Access discovered information
let workspace = discovery.discover_workspace().await?;

// Individual graphs with metadata
for graph_meta in &workspace.graphs {
    println!("Graph: {}", graph_meta.file_info.graph_name);
    println!("  Path: {}", graph_meta.file_info.path.display());
    println!("  Namespace: {}", graph_meta.discovered_namespace);
    println!("  Size: {} bytes", graph_meta.file_info.size_bytes);
    println!("  Modified: {:?}", graph_meta.file_info.modified);
}

// Namespace organization
for (namespace, info) in &workspace.namespaces {
    println!("📁 Namespace: {} ({} graphs)", namespace, info.graph_count);
    for graph_name in &info.graphs {
        println!("  📈 {}", graph_name);
    }
}
```

### Graph Metadata Enhancement

Discovery automatically enhances graphs with workspace metadata:

```rust
// Original graph properties are preserved and enhanced
let enhanced_graph = &workspace.graphs[0].graph;

// Injected workspace metadata
let workspace_namespace = enhanced_graph.properties
    .get("workspace_namespace")
    .and_then(|v| v.as_str());

let workspace_path = enhanced_graph.properties
    .get("workspace_path")
    .and_then(|v| v.as_str());

let discovery_timestamp = enhanced_graph.properties
    .get("discovery_timestamp")
    .and_then(|v| v.as_str());

println!("Discovered at: {}", discovery_timestamp.unwrap_or("unknown"));
```

## Advanced Discovery

### Filtered Discovery

Discover specific types of graphs:

```rust
use reflow_network::multi_graph::workspace::DiscoveryFilter;

let filter = DiscoveryFilter {
    name_patterns: vec!["*processor*".to_string(), "*trainer*".to_string()],
    capability_requirements: vec!["ml_training".to_string(), "data_processing".to_string()],
    min_file_size: Some(1024), // At least 1KB
    max_file_age_days: Some(30), // Modified within 30 days
};

let filtered_workspace = discovery.discover_workspace_with_filter(filter).await?;
```

### Incremental Discovery

Update workspace with only changed files:

```rust
// Initial discovery
let workspace = discovery.discover_workspace().await?;

// Later, discover only changes
let changes = discovery.discover_changes_since(&workspace).await?;

println!("📊 Changes since last discovery:");
println!("  Added: {} graphs", changes.added.len());
println!("  Modified: {} graphs", changes.modified.len());
println!("  Removed: {} graphs", changes.removed.len());

// Apply changes to workspace
let updated_workspace = discovery.apply_changes(workspace, changes).await?;
```

### Parallel Discovery

Speed up discovery with parallel processing:

```rust
let config = WorkspaceConfig {
    parallel_discovery: true,
    max_concurrent_loads: 8,
    ..Default::default()
};

let discovery = WorkspaceDiscovery::new(config);

// Discovery happens in parallel across multiple threads
let workspace = discovery.discover_workspace().await?;
```

## Dependency Analysis

### Automatic Dependency Detection

Discovery analyzes dependencies between graphs:

```rust
let workspace = discovery.discover_workspace().await?;
let analysis = &workspace.dependency_analysis;

// View dependency relationships
for dep in &analysis.dependencies {
    println!("🔗 {} depends on {} ({})", 
        dep.dependent_graph,
        dep.dependency_graph,
        if dep.required { "required" } else { "optional" }
    );
}

// Check for circular dependencies
if analysis.has_circular_dependencies() {
    println!("⚠️  Circular dependencies detected!");
    for cycle in analysis.get_circular_dependencies() {
        println!("  🔄 {}", cycle.join(" → "));
    }
}
```

### Interface Analysis

Analyze provided and required interfaces:

```rust
// Graphs that provide interfaces
for interface in &analysis.provided_interfaces {
    println!("📤 {} provides interface: {} ({})", 
        interface.graph_name,
        interface.interface_name,
        interface.interface_definition.description.as_ref().unwrap_or(&"No description".to_string())
    );
}

// Graphs that require interfaces
for interface in &analysis.required_interfaces {
    println!("📥 {} requires interface: {} ({})", 
        interface.graph_name,
        interface.interface_name,
        interface.interface_definition.description.as_ref().unwrap_or(&"No description".to_string())
    );
}

// Find interface compatibility
let compatibility_report = analysis.analyze_interface_compatibility();
for incompatibility in compatibility_report.mismatches {
    println!("❌ Interface mismatch: {} → {}", 
        incompatibility.provider,
        incompatibility.consumer
    );
}
```

## Error Handling

### Discovery Errors

Handle common discovery issues:

```rust
match discovery.discover_workspace().await {
    Ok(workspace) => {
        println!("✅ Discovery successful: {} graphs", workspace.graphs.len());
    },
    Err(e) => {
        match e {
            DiscoveryError::GlobError(pattern_err) => {
                eprintln!("❌ Invalid glob pattern: {}", pattern_err);
            },
            DiscoveryError::LoadError(path, reason) => {
                eprintln!("❌ Failed to load {}: {}", path.display(), reason);
            },
            DiscoveryError::UnsupportedFormat(path) => {
                eprintln!("❌ Unsupported file format: {}", path.display());
            },
            DiscoveryError::IoError(io_err) => {
                eprintln!("❌ IO error during discovery: {}", io_err);
            },
            _ => {
                eprintln!("❌ Discovery failed: {}", e);
            }
        }
    }
}
```

### Resilient Discovery

Continue discovery even when some files fail to load:

```rust
let config = WorkspaceConfig {
    continue_on_load_error: true,
    max_load_errors: 5,
    ..Default::default()
};

let discovery = WorkspaceDiscovery::new(config);
let result = discovery.discover_workspace().await?;

// Check for partial failures
if !result.load_errors.is_empty() {
    println!("⚠️  {} files failed to load:", result.load_errors.len());
    for error in &result.load_errors {
        println!("  ❌ {}: {}", error.path.display(), error.reason);
    }
}
```

## Performance Optimization

### Caching Discovery Results

Cache discovery results to speed up subsequent runs:

```rust
use reflow_network::multi_graph::workspace::DiscoveryCache;

let cache = DiscoveryCache::new("./workspace_cache");
let discovery = WorkspaceDiscovery::with_cache(config, cache);

// First run: Full discovery and cache
let workspace = discovery.discover_workspace().await?;

// Subsequent runs: Load from cache if nothing changed
let cached_workspace = discovery.discover_workspace().await?; // Much faster!
```

### Memory Management

Configure memory usage for large workspaces:

```rust
let config = WorkspaceConfig {
    lazy_load_graphs: true,        // Load graph content on demand
    max_memory_usage_mb: 512,      // Limit memory usage
    graph_content_cache_size: 100, // Cache up to 100 graph contents
    ..Default::default()
};
```

### Progress Monitoring

Monitor discovery progress for large workspaces:

```rust
use reflow_network::multi_graph::workspace::DiscoveryProgress;

let (discovery, mut progress_rx) = WorkspaceDiscovery::with_progress(config);

// Start discovery in background
let workspace_future = discovery.discover_workspace();

// Monitor progress
tokio::spawn(async move {
    while let Some(progress) = progress_rx.recv().await {
        match progress {
            DiscoveryProgress::FilesFound(count) => {
                println!("📁 Found {} graph files", count);
            },
            DiscoveryProgress::LoadingFile(path) => {
                println!("📈 Loading {}", path.display());
            },
            DiscoveryProgress::NamespaceCreated(namespace, graph_count) => {
                println!("📂 Namespace '{}' with {} graphs", namespace, graph_count);
            },
            DiscoveryProgress::Complete(total_graphs) => {
                println!("✅ Discovery complete: {} graphs", total_graphs);
                break;
            }
        }
    }
});

// Wait for completion
let workspace = workspace_future.await?;
```

## Integration Examples

### Example Workspace Structure

```
my_workspace/
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

### Complete Discovery Example

```rust
use reflow_network::multi_graph::workspace::*;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure discovery
    let config = WorkspaceConfig {
        root_path: PathBuf::from("./my_workspace"),
        graph_patterns: vec![
            "**/*.graph.json".to_string(),
            "**/*.graph.yaml".to_string(),
        ],
        excluded_paths: vec![
            "**/test/**".to_string(),
            "**/.*/**".to_string(),
        ],
        max_depth: Some(6),
        namespace_strategy: NamespaceStrategy::FolderStructure,
    };
    
    // Perform discovery
    println!("🔍 Starting workspace discovery...");
    let discovery = WorkspaceDiscovery::new(config);
    let workspace = discovery.discover_workspace().await?;
    
    // Print results
    println!("\n📊 Discovery Results");
    println!("================");
    println!("📁 Workspace root: {}", workspace.workspace_root.display());
    println!("🎯 Total graphs: {}", workspace.graphs.len());
    println!("📂 Namespaces: {}", workspace.namespaces.len());
    
    // Show namespace breakdown
    println!("\n📂 Namespace Organization:");
    for (namespace, info) in &workspace.namespaces {
        println!("  📁 {} ({} graphs)", namespace, info.graphs.len());
        for graph_name in &info.graphs {
            println!("    📈 {}", graph_name);
        }
    }
    
    // Show dependencies
    println!("\n🔗 Dependencies:");
    for dep in &workspace.dependency_analysis.dependencies {
        println!("  {} → {} ({})", 
            dep.dependent_graph,
            dep.dependency_graph,
            if dep.required { "required" } else { "optional" }
        );
    }
    
    println!("\n✅ Workspace discovery completed successfully!");
    
    Ok(())
}
```

## Best Practices

### 1. Organize by Function

```rust
// Good: Functional organization
data/
  ingestion/     # Data collection graphs
  processing/    # Data transformation graphs  
  storage/       # Data persistence graphs
ml/
  training/      # ML training graphs
  inference/     # ML prediction graphs
  evaluation/    # ML validation graphs
```

### 2. Consistent Naming

```rust
// Good: Descriptive, consistent names
api_data_collector.graph.json
stream_data_processor.graph.json
ml_model_trainer.graph.json
postgres_storage_writer.graph.json

// Avoid: Generic names
collector.graph.json
processor.graph.json
trainer.graph.json
writer.graph.json
```

### 3. Graph Documentation

Include metadata in graph files for better discovery:

```json
{
  "properties": {
    "name": "data_processor",
    "description": "Processes incoming data streams with validation and transformation",
    "version": "1.2.0",
    "tags": ["data", "processing", "validation"],
    "capabilities": ["stream_processing", "data_validation"],
    "dependencies": ["data_collector"]
  }
}
```

## Next Steps

- [Graph Composition](graph-composition.md) - Combine discovered graphs
- [Dependency Resolution](dependency-resolution.md) - Handle graph dependencies
- [Tutorial: Multi-Graph Workspace](../../tutorials/multi-graph-workspace.md)

## Related Documentation

- [Architecture: Multi-Graph Composition](../../architecture/multi-graph-composition.md)
- [Graph System Overview](../../architecture/graph-system.md)
