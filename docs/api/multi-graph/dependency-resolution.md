# Dependency Resolution

Learn how to handle complex dependencies between graphs in multi-graph compositions.

## Overview

Dependency resolution in multi-graph systems involves:

- **Automatic dependency detection**: Analyze graph dependencies from metadata
- **Topological ordering**: Ensure graphs are loaded in dependency order
- **Circular dependency detection**: Identify and resolve circular dependencies
- **Version constraints**: Handle version compatibility between dependent graphs
- **Interface matching**: Verify compatible interfaces between graphs
- **Missing dependency handling**: Graceful handling of unresolved dependencies

## Basic Dependency Resolution

### Dependency Resolver

The core component for handling graph dependencies:

```rust
use reflow_network::multi_graph::{DependencyResolver, DependencyError};

let resolver = DependencyResolver::new();

// Load graphs with dependencies
let graphs = vec![
    graph_export_a,  // depends on graph_b
    graph_export_b,  // no dependencies
    graph_export_c,  // depends on graph_a and graph_b
];

// Resolve dependency order
let ordered_graphs = resolver.resolve_dependencies(&graphs)?;

// Graphs are now ordered: [graph_b, graph_a, graph_c]
for graph in &ordered_graphs {
    let name = graph.properties.get("name").and_then(|v| v.as_str()).unwrap_or("unnamed");
    println!("Loading graph: {}", name);
}
```

### Dependency Declaration

Declare dependencies in graph metadata:

```json
{
  "properties": {
    "name": "ml_trainer",
    "version": "1.2.0",
    "dependencies": [
      "data_processor",
      "feature_engineer"
    ]
  },
  "graph_dependencies": [
    {
      "graph_name": "data_processor",
      "namespace": "data/processing",
      "version_constraint": ">=1.0.0",
      "required": true,
      "description": "Requires processed data for training"
    },
    {
      "graph_name": "feature_engineer",
      "namespace": "ml/features",
      "version_constraint": "^2.1.0",
      "required": true,
      "description": "Requires feature engineering pipeline"
    }
  ]
}
```

## Advanced Dependency Resolution

### Version Constraints

Handle version compatibility:

```rust
use reflow_network::multi_graph::{VersionConstraint, VersionResolver};

// Define version constraints
let constraints = vec![
    VersionConstraint {
        graph_name: "data_processor".to_string(),
        constraint: ">=1.0.0".to_string(),
        required: true,
    },
    VersionConstraint {
        graph_name: "ml_core".to_string(),
        constraint: "^2.0.0".to_string(),  // Compatible with 2.x.x
        required: true,
    },
    VersionConstraint {
        graph_name: "analytics".to_string(),
        constraint: "~1.5.0".to_string(),  // Compatible with 1.5.x
        required: false,
    },
];

let version_resolver = VersionResolver::new();
let resolution_result = version_resolver.resolve_versions(&graphs, &constraints)?;

if resolution_result.has_conflicts() {
    println!("❌ Version conflicts detected:");
    for conflict in &resolution_result.conflicts {
        println!("  {} requires {} but {} is available", 
            conflict.dependent, conflict.required_version, conflict.available_version);
    }
} else {
    println!("✅ All version constraints satisfied");
}
```

### Interface Compatibility

Verify interface compatibility between dependent graphs:

```rust
use reflow_network::multi_graph::{InterfaceResolver, InterfaceCompatibility};

let interface_resolver = InterfaceResolver::new();

// Analyze interface compatibility
let compatibility_result = interface_resolver.analyze_compatibility(&ordered_graphs)?;

for incompatibility in &compatibility_result.incompatibilities {
    match incompatibility.severity {
        Severity::Error => {
            println!("❌ Interface incompatibility: {} → {}", 
                incompatibility.provider, incompatibility.consumer);
            println!("   Expected: {}", incompatibility.expected_signature);
            println!("   Actual: {}", incompatibility.actual_signature);
        },
        Severity::Warning => {
            println!("⚠️  Interface warning: {} → {}", 
                incompatibility.provider, incompatibility.consumer);
            println!("   {}", incompatibility.description);
        },
    }
}
```

### Conditional Dependencies

Handle dependencies that are only required under certain conditions:

```rust
use reflow_network::multi_graph::{ConditionalDependency, DependencyCondition};

// Define conditional dependencies in graph metadata
let conditional_deps = vec![
    ConditionalDependency {
        graph_name: "ml_trainer".to_string(),
        condition: DependencyCondition::EnvironmentVariable("ENABLE_ML".to_string()),
        version_constraint: Some(">=2.0.0".to_string()),
        required: true,
    },
    ConditionalDependency {
        graph_name: "analytics_dashboard".to_string(),
        condition: DependencyCondition::ConfigValue("features.analytics".to_string()),
        version_constraint: None,
        required: false,
    },
];

// Resolve conditional dependencies
let resolution_context = ResolutionContext {
    environment_variables: HashMap::from([
        ("ENABLE_ML".to_string(), "true".to_string()),
    ]),
    config_values: HashMap::from([
        ("features.analytics".to_string(), serde_json::json!(true)),
    ]),
};

let resolved_deps = resolver.resolve_conditional_dependencies(
    &conditional_deps, 
    &resolution_context
)?;
```

## Circular Dependency Detection

### Identifying Cycles

Detect and report circular dependencies:

```rust
use reflow_network::multi_graph::CircularDependencyDetector;

let cycle_detector = CircularDependencyDetector::new();
let cycle_result = cycle_detector.detect_cycles(&graphs)?;

if cycle_result.has_cycles() {
    println!("❌ Circular dependencies detected:");
    for cycle in &cycle_result.cycles {
        println!("  🔄 {}", cycle.join(" → "));
        
        // Suggest resolution strategies
        let suggestions = cycle_detector.suggest_resolutions(&cycle)?;
        for suggestion in suggestions {
            println!("    💡 {}", suggestion);
        }
    }
} else {
    println!("✅ No circular dependencies found");
}
```

### Cycle Resolution Strategies

Automatic strategies for resolving circular dependencies:

```rust
use reflow_network::multi_graph::{CycleResolutionStrategy, DependencyBreaker};

let cycle_breaker = DependencyBreaker::new();

// Strategy 1: Optional dependency promotion
let resolution1 = cycle_breaker.resolve_by_optional_promotion(&cycle)?;

// Strategy 2: Interface extraction
let resolution2 = cycle_breaker.resolve_by_interface_extraction(&cycle)?;

// Strategy 3: Dependency inversion
let resolution3 = cycle_breaker.resolve_by_dependency_inversion(&cycle)?;

// Apply the best resolution strategy
let best_resolution = cycle_breaker.select_best_resolution(vec![
    resolution1, resolution2, resolution3
])?;

println!("🔧 Applying resolution: {}", best_resolution.description);
let resolved_graphs = cycle_breaker.apply_resolution(&graphs, &best_resolution)?;
```

## Missing Dependency Handling

### Graceful Degradation

Handle missing dependencies gracefully:

```rust
use reflow_network::multi_graph::{MissingDependencyHandler, DegradationStrategy};

let missing_handler = MissingDependencyHandler::new();

// Configure degradation strategies
let strategies = HashMap::from([
    ("optional_dependencies".to_string(), DegradationStrategy::Skip),
    ("required_dependencies".to_string(), DegradationStrategy::Fail),
    ("soft_dependencies".to_string(), DegradationStrategy::Substitute),
]);

missing_handler.configure_strategies(strategies);

// Handle missing dependencies
let resolution_result = missing_handler.handle_missing_dependencies(
    &graphs,
    &missing_deps
)?;

for action in &resolution_result.actions {
    match action {
        DegradationAction::Skipped(graph_name) => {
            println!("⏭️  Skipped optional dependency: {}", graph_name);
        },
        DegradationAction::Substituted(original, substitute) => {
            println!("🔄 Substituted {} with {}", original, substitute);
        },
        DegradationAction::Failed(graph_name, reason) => {
            println!("❌ Failed to resolve required dependency: {} ({})", graph_name, reason);
        },
    }
}
```

### Dependency Substitution

Provide alternatives for missing dependencies:

```rust
use reflow_network::multi_graph::DependencySubstitution;

let substitutions = vec![
    DependencySubstitution {
        original: "premium_ml_engine".to_string(),
        substitute: "basic_ml_engine".to_string(),
        compatibility_level: CompatibilityLevel::Partial,
        feature_differences: vec![
            "Advanced model optimization not available".to_string(),
            "Reduced prediction accuracy".to_string(),
        ],
    },
    DependencySubstitution {
        original: "enterprise_analytics".to_string(),
        substitute: "community_analytics".to_string(),
        compatibility_level: CompatibilityLevel::Full,
        feature_differences: vec![],
    },
];

missing_handler.register_substitutions(substitutions);

// Apply substitutions during resolution
let result = missing_handler.resolve_with_substitutions(&graphs)?;
```

## Dependency Analysis and Reporting

### Dependency Graph Visualization

Generate dependency graphs for analysis:

```rust
use reflow_network::multi_graph::{DependencyAnalyzer, DependencyGraph};

let analyzer = DependencyAnalyzer::new();

// Generate dependency graph
let dep_graph = analyzer.build_dependency_graph(&graphs)?;

// Export to various formats
dep_graph.export_to_dot("dependencies.dot")?;         // Graphviz DOT
dep_graph.export_to_json("dependencies.json")?;       // JSON format
dep_graph.export_to_mermaid("dependencies.md")?;      // Mermaid diagram

// Analyze graph properties
let analysis = analyzer.analyze_dependency_structure(&dep_graph)?;

println!("📊 Dependency Analysis:");
println!("  Graphs: {}", analysis.total_graphs);
println!("  Dependencies: {}", analysis.total_dependencies);
println!("  Max depth: {}", analysis.max_dependency_depth);
println!("  Strongly connected components: {}", analysis.scc_count);
```

### Impact Analysis

Analyze the impact of dependency changes:

```rust
use reflow_network::multi_graph::ImpactAnalyzer;

let impact_analyzer = ImpactAnalyzer::new();

// Analyze impact of changing a graph
let impact = impact_analyzer.analyze_change_impact(
    &dep_graph,
    "data_processor",  // Graph being changed
    "2.0.0"            // New version
)?;

println!("🎯 Impact Analysis for data_processor v2.0.0:");
println!("  Directly affected graphs: {}", impact.direct_dependents.len());
println!("  Transitively affected graphs: {}", impact.transitive_dependents.len());
println!("  Breaking changes detected: {}", impact.breaking_changes.len());

for change in &impact.breaking_changes {
    println!("  ⚠️  {}: {}", change.affected_graph, change.description);
}
```

## Real-World Examples

### Data Processing Pipeline Dependencies

```rust
// Example: Complex data processing pipeline with dependencies
async fn resolve_data_pipeline_dependencies() -> Result<Vec<GraphExport>, DependencyError> {
    let graphs = vec![
        // Base data collector (no dependencies)
        load_graph("data/ingestion/api_collector.graph.json").await?,
        
        // Data processor (depends on collector)
        load_graph("data/processing/cleaner.graph.json").await?,
        
        // Feature engineer (depends on processor)
        load_graph("ml/features/engineer.graph.json").await?,
        
        // ML trainer (depends on feature engineer)
        load_graph("ml/training/trainer.graph.json").await?,
        
        // Model validator (depends on trainer)
        load_graph("ml/validation/validator.graph.json").await?,
        
        // Inference service (depends on trainer, but not validator)
        load_graph("ml/inference/predictor.graph.json").await?,
        
        // Analytics dashboard (depends on multiple components)
        load_graph("analytics/dashboard.graph.json").await?,
    ];
    
    let resolver = DependencyResolver::new();
    let ordered_graphs = resolver.resolve_dependencies(&graphs)?;
    
    // Result order: collector → cleaner → engineer → trainer → [validator, predictor] → dashboard
    
    Ok(ordered_graphs)
}
```

### ML Pipeline with Version Constraints

```rust
// Example: ML pipeline with strict version requirements
async fn resolve_ml_pipeline_with_versions() -> Result<Vec<GraphExport>, DependencyError> {
    let graphs = load_ml_graphs().await?;
    
    let version_constraints = vec![
        VersionConstraint {
            graph_name: "tensorflow_runtime".to_string(),
            constraint: ">=2.8.0".to_string(),
            required: true,
        },
        VersionConstraint {
            graph_name: "data_validator".to_string(),
            constraint: "^1.5.0".to_string(),
            required: true,
        },
        VersionConstraint {
            graph_name: "model_optimizer".to_string(),
            constraint: "~2.1.0".to_string(),
            required: false,
        },
    ];
    
    let resolver = DependencyResolver::with_version_constraints(version_constraints);
    
    // Resolve dependencies with version checking
    let resolution_result = resolver.resolve_with_versions(&graphs)?;
    
    if resolution_result.has_conflicts() {
        // Handle version conflicts
        for conflict in &resolution_result.conflicts {
            eprintln!("Version conflict: {} requires {} but {} is available",
                conflict.dependent, conflict.required_version, conflict.available_version);
        }
        return Err(DependencyError::VersionConflict(resolution_result.conflicts));
    }
    
    Ok(resolution_result.ordered_graphs)
}
```

### Handling Optional Dependencies

```rust
// Example: System with optional features and dependencies
async fn resolve_with_optional_features() -> Result<Vec<GraphExport>, DependencyError> {
    let base_graphs = load_core_graphs().await?;
    let optional_graphs = load_optional_graphs().await?;
    
    let resolver = DependencyResolver::new();
    
    // Configure optional dependency handling
    let config = DependencyResolutionConfig {
        allow_missing_optional: true,
        substitute_missing: true,
        fail_on_missing_required: true,
    };
    
    resolver.configure(config);
    
    // Define substitutions for missing optional dependencies
    let substitutions = vec![
        DependencySubstitution {
            original: "premium_feature_a".to_string(),
            substitute: "basic_feature_a".to_string(),
            compatibility_level: CompatibilityLevel::Partial,
            feature_differences: vec![
                "Advanced analytics not available".to_string(),
            ],
        },
    ];
    
    resolver.register_substitutions(substitutions);
    
    // Resolve with graceful handling of missing optional dependencies
    let all_graphs = [base_graphs, optional_graphs].concat();
    let resolution_result = resolver.resolve_with_graceful_degradation(&all_graphs)?;
    
    // Report what was included/excluded
    for action in &resolution_result.degradation_actions {
        match action {
            DegradationAction::Skipped(graph) => {
                println!("⏭️  Skipped optional feature: {}", graph);
            },
            DegradationAction::Substituted(original, substitute) => {
                println!("🔄 Using {} instead of {}", substitute, original);
            },
        }
    }
    
    Ok(resolution_result.ordered_graphs)
}
```

## Testing Dependency Resolution

### Unit Testing Dependencies

Test dependency resolution logic:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_simple_dependency_resolution() {
        let graph_a = create_test_graph("graph_a", vec![]);
        let graph_b = create_test_graph("graph_b", vec!["graph_a"]);
        let graph_c = create_test_graph("graph_c", vec!["graph_b"]);
        
        let graphs = vec![graph_c, graph_a, graph_b]; // Intentionally unordered
        
        let resolver = DependencyResolver::new();
        let ordered = resolver.resolve_dependencies(&graphs).unwrap();
        
        assert_eq!(get_graph_name(&ordered[0]), "graph_a");
        assert_eq!(get_graph_name(&ordered[1]), "graph_b");
        assert_eq!(get_graph_name(&ordered[2]), "graph_c");
    }
    
    #[tokio::test]
    async fn test_circular_dependency_detection() {
        let graph_a = create_test_graph("graph_a", vec!["graph_b"]);
        let graph_b = create_test_graph("graph_b", vec!["graph_c"]);
        let graph_c = create_test_graph("graph_c", vec!["graph_a"]);
        
        let graphs = vec![graph_a, graph_b, graph_c];
        
        let resolver = DependencyResolver::new();
        let result = resolver.resolve_dependencies(&graphs);
        
        assert!(matches!(result, Err(DependencyError::CircularDependency(_))));
    }
    
    #[tokio::test]
    async fn test_version_constraint_validation() {
        let graph_a = create_test_graph_with_version("graph_a", "1.0.0", vec![]);
        let graph_b = create_test_graph_with_version("graph_b", "2.0.0", vec![
            ("graph_a", ">=1.5.0")
        ]);
        
        let graphs = vec![graph_a, graph_b];
        
        let resolver = DependencyResolver::new();
        let result = resolver.resolve_dependencies(&graphs);
        
        assert!(matches!(result, Err(DependencyError::VersionConflict(_))));
    }
}
```

### Integration Testing

Test complete dependency resolution workflows:

```rust
#[tokio::test]
async fn test_complete_workspace_dependency_resolution() {
    let workspace_path = "test_workspace";
    setup_test_workspace(workspace_path).await;
    
    let discovery = WorkspaceDiscovery::new(WorkspaceConfig {
        root_path: PathBuf::from(workspace_path),
        ..Default::default()
    });
    
    let workspace = discovery.discover_workspace().await.unwrap();
    
    let resolver = DependencyResolver::new();
    let ordered_graphs = resolver.resolve_dependencies(&workspace.graphs).await.unwrap();
    
    // Verify correct ordering
    verify_dependency_order(&ordered_graphs);
    
    // Verify all required dependencies are satisfied
    verify_all_dependencies_satisfied(&ordered_graphs);
    
    cleanup_test_workspace(workspace_path).await;
}
```

## Best Practices

### 1. Explicit Dependency Declaration

Always declare dependencies explicitly in graph metadata:

```json
{
  "properties": {
    "name": "my_graph",
    "dependencies": ["required_graph_1", "required_graph_2"]
  },
  "graph_dependencies": [
    {
      "graph_name": "required_graph_1",
      "version_constraint": ">=1.0.0",
      "required": true,
      "description": "Provides core data processing functionality"
    }
  ]
}
```

### 2. Use Semantic Versioning

Follow semantic versioning for graph versions:

```json
{
  "properties": {
    "version": "2.1.3"  // MAJOR.MINOR.PATCH
  },
  "graph_dependencies": [
    {
      "graph_name": "data_processor",
      "version_constraint": "^2.0.0"  // Compatible with 2.x.x
    }
  ]
}
```

### 3. Design for Loose Coupling

Minimize dependencies between graphs:

```rust
// Good: Minimal, well-defined dependencies
let graph_deps = vec![
    GraphDependency {
        graph_name: "core_processor".to_string(),
        required: true,
        // Only depends on stable core functionality
    },
];

// Avoid: Tight coupling with many dependencies
let graph_deps = vec![
    // Too many dependencies make the graph fragile
    GraphDependency { graph_name: "helper1".to_string(), required: true },
    GraphDependency { graph_name: "helper2".to_string(), required: true },
    GraphDependency { graph_name: "helper3".to_string(), required: true },
    GraphDependency { graph_name: "helper4".to_string(), required: true },
];
```

### 4. Test Dependency Changes

Always test the impact of dependency changes:

```rust
// Before making changes, analyze impact
let impact = analyzer.analyze_change_impact(&dep_graph, "my_graph", "2.0.0")?;

if impact.has_breaking_changes() {
    println!("⚠️  Breaking changes detected - review carefully");
    for change in &impact.breaking_changes {
        println!("  - {}", change.description);
    }
}
```

### 5. Document Dependencies

Document why dependencies exist and what they provide:

```json
{
  "graph_dependencies": [
    {
      "graph_name": "ml_core",
      "version_constraint": ">=2.0.0",
      "required": true,
      "description": "Provides tensor operations and model training infrastructure required for neural network training"
    }
  ]
}
```

## Next Steps

- [Workspace Discovery](workspace-discovery.md) - Discover graphs for dependency analysis
- [Graph Composition](graph-composition.md) - Compose resolved graphs
- [Tutorial: Multi-Graph Workspace](../../tutorials/multi-graph-workspace.md)

## Related Documentation

- [Architecture: Multi-Graph Composition](../../architecture/multi-graph-composition.md)
- [Graph System Overview](../../architecture/graph-system.md)
