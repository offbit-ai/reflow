# Graph Analysis and Validation

Reflow's graph system provides extensive analysis capabilities for validation, performance optimization, and structural insights. This guide covers all analysis features available in the graph system.

## Flow Validation

### Comprehensive Validation

The `validate_flow` method performs a complete analysis of graph integrity:

```rust
use reflow_network::graph::{FlowValidation, PortMismatch};

// Perform full validation
let validation = graph.validate_flow()?;

// Check for issues
if !validation.cycles.is_empty() {
    for cycle in validation.cycles {
        println!("Cycle detected: {:?}", cycle);
    }
}

if !validation.orphaned_nodes.is_empty() {
    println!("Orphaned nodes: {:?}", validation.orphaned_nodes);
}

if !validation.port_mismatches.is_empty() {
    for mismatch in validation.port_mismatches {
        println!("Port type mismatch: {}", mismatch);
    }
}
```

### Validation Results Structure

```rust
pub struct FlowValidation {
    pub cycles: Vec<Vec<String>>,           // Detected cycles
    pub orphaned_nodes: Vec<String>,        // Disconnected nodes
    pub port_mismatches: Vec<PortMismatch>, // Type incompatibilities
}

pub struct PortMismatch {
    pub from_node: String,
    pub from_port: String,
    pub from_type: PortType,
    pub to_node: String,
    pub to_port: String,
    pub to_type: PortType,
    pub reason: String,
}
```

## Cycle Detection

### Basic Cycle Detection

```rust
// Detect first cycle found
if let Some(cycle) = graph.detect_cycles() {
    println!("Cycle path: {:?}", cycle);
    // cycle is Vec<String> showing the path of the cycle
}

// Check if specific node is in a cycle
if graph.is_node_in_cycle("suspicious_node") {
    println!("Node is part of a cycle");
}
```

### Comprehensive Cycle Analysis

```rust
use reflow_network::graph::CycleAnalysis;

let cycle_analysis = graph.analyze_cycles();

println!("Total cycles found: {}", cycle_analysis.total_cycles);
println!("Cycle lengths: {:?}", cycle_analysis.cycle_lengths);
println!("Nodes involved in cycles: {:?}", cycle_analysis.nodes_in_cycles);

if let Some(longest) = cycle_analysis.longest_cycle {
    println!("Longest cycle: {:?} (length: {})", longest, longest.len());
}

if let Some(shortest) = cycle_analysis.shortest_cycle {
    println!("Shortest cycle: {:?} (length: {})", shortest, shortest.len());
}
```

### All Cycles Detection

```rust
// Find all cycles in the graph
let all_cycles = graph.detect_all_cycles();

for (i, cycle) in all_cycles.iter().enumerate() {
    println!("Cycle {}: {:?}", i + 1, cycle);
}
```

## Orphaned Node Analysis

### Basic Orphaned Node Detection

```rust
// Find all orphaned nodes
let orphaned = graph.find_orphaned_nodes();

for node in orphaned {
    println!("Orphaned node: {}", node);
}
```

### Detailed Orphaned Analysis

```rust
use reflow_network::graph::OrphanedNodeAnalysis;

let orphan_analysis = graph.analyze_orphaned_nodes();

println!("Total orphaned nodes: {}", orphan_analysis.total_orphaned);

println!("Completely isolated nodes:");
for node in orphan_analysis.completely_isolated {
    println!("  - {}", node);
}

println!("Unreachable nodes (have connections but no path from entry points):");
for node in orphan_analysis.unreachable {
    println!("  - {}", node);
}

println!("Disconnected groups:");
for (i, group) in orphan_analysis.disconnected_groups.iter().enumerate() {
    println!("  Group {}: {:?}", i + 1, group);
}
```

## Port Type Validation

### Port Compatibility Checking

```rust
// Validate all port types in the graph
let port_mismatches = graph.validate_port_types();

for mismatch in port_mismatches {
    println!("Port mismatch: {} -> {}", 
        format!("{}:{}", mismatch.from_node, mismatch.from_port),
        format!("{}:{}", mismatch.to_node, mismatch.to_port)
    );
    println!("  Types: {:?} -> {:?}", mismatch.from_type, mismatch.to_type);
    println!("  Reason: {}", mismatch.reason);
}
```

### Custom Type Compatibility

The graph system includes built-in type compatibility rules:

```rust
// Built-in compatibility rules:
// Any ↔ Any type (always compatible)
// Integer → Float (automatic promotion)
// T → Stream (streaming any type)
// T → Option<T> (wrapping in option)
// Array<T> → Array<U> (if T → U)

// Example of compatible connections:
graph.add_connection("int_source", "out", "float_processor", "in", None);     // Integer → Float ✓
graph.add_connection("data_source", "out", "stream_processor", "in", None);   // Any → Stream ✓
graph.add_connection("value", "out", "optional_sink", "in", None);            // T → Option<T> ✓
```

## Performance Analysis

### Parallelism Analysis

```rust
use reflow_network::graph::{ParallelismAnalysis, PipelineStage};

let parallelism = graph.analyze_parallelism();

println!("Maximum parallelism: {}", parallelism.max_parallelism);

// Parallel branches that can execute simultaneously
println!("Parallel branches:");
for (i, branch) in parallelism.parallel_branches.iter().enumerate() {
    println!("  Branch {}: {:?}", i + 1, branch.nodes);
    println!("    Entry points: {:?}", branch.entry_points);
    println!("    Exit points: {:?}", branch.exit_points);
}

// Pipeline stages for sequential execution
println!("Pipeline stages:");
for stage in parallelism.pipeline_stages {
    println!("  Stage {}: {:?}", stage.level, stage.nodes);
}
```

### Bottleneck Detection

```rust
use reflow_network::graph::Bottleneck;

let bottlenecks = graph.detect_bottlenecks();

for bottleneck in bottlenecks {
    match bottleneck {
        Bottleneck::HighDegree(node) => {
            let (in_deg, out_deg) = graph.get_connection_degree(&node);
            println!("High-degree bottleneck: {} ({} in, {} out)", node, in_deg, out_deg);
        }
        Bottleneck::SequentialChain(chain) => {
            println!("Sequential chain (could be parallelized): {:?}", chain);
        }
    }
}
```

### High-Degree Node Analysis

```rust
// Find nodes with unusually high connection counts
let high_degree_nodes = graph.find_high_degree_nodes();

for node in high_degree_nodes {
    let (incoming, outgoing) = graph.get_connection_degree(&node);
    let total_degree = incoming + outgoing;
    
    println!("High-degree node: {} (total degree: {})", node, total_degree);
    println!("  Incoming: {}, Outgoing: {}", incoming, outgoing);
    
    // Analyze connected nodes
    let connected = graph.get_connected_nodes(&node);
    println!("  Connected to {} other nodes", connected.len());
}
```

### Sequential Chain Analysis

```rust
// Find chains that could potentially be parallelized
let sequential_chains = graph.find_sequential_chains();

for (i, chain) in sequential_chains.iter().enumerate() {
    println!("Sequential chain {}: {:?}", i + 1, chain);
    println!("  Length: {} nodes", chain.len());
    
    // Analyze chain characteristics
    if chain.len() >= 5 {
        println!("  ⚠️  Long chain - consider breaking into parallel segments");
    }
}
```

## Data Flow Analysis

### Flow Path Tracing

```rust
use reflow_network::graph::{DataFlowPath, DataTransform};

// Trace data flow from a starting node
let flow_paths = graph.trace_data_flow("input_node")?;

for (i, path) in flow_paths.iter().enumerate() {
    println!("Flow path {}:", i + 1);
    println!("  Nodes: {:?}", path.nodes);
    
    println!("  Transformations:");
    for transform in &path.transforms {
        println!("    {} -> {} ({}: {} -> {})", 
            transform.node, 
            transform.operation,
            transform.node,
            transform.input_type, 
            transform.output_type
        );
    }
}
```

### Execution Path Analysis

```rust
use reflow_network::graph::ExecutionPath;

// Find all possible execution paths
let execution_paths = graph.find_execution_paths();

for (i, path) in execution_paths.iter().enumerate() {
    println!("Execution path {}:", i + 1);
    println!("  Nodes: {:?}", path.nodes);
    println!("  Estimated time: {:.2}s", path.estimated_time);
    println!("  Resource requirements: {:?}", path.resource_requirements);
    
    // Check for parallel execution markers
    if path.resource_requirements.contains_key("parallel_branches") {
        let branches = path.resource_requirements["parallel_branches"];
        println!("  ⚡ Contains {} parallel branches", branches);
    }
    
    if path.resource_requirements.contains_key("contains_cycle") {
        println!("  ⚠️  Path contains cycles");
    }
}
```

### Resource Requirements Analysis

```rust
// Analyze resource requirements for the entire graph
let resource_analysis = graph.analyze_resource_requirements();

println!("Graph resource requirements:");
for (resource, requirement) in resource_analysis {
    match resource.as_str() {
        "memory" => println!("  Memory: {:.1} MB", requirement),
        "cpu" => println!("  CPU cores: {:.1}", requirement),
        "disk" => println!("  Disk space: {:.1} GB", requirement),
        "network" => println!("  Network bandwidth: {:.1} Mbps", requirement),
        _ => println!("  {}: {:.2}", resource, requirement),
    }
}
```

## Runtime Analysis

### Comprehensive Runtime Analysis

```rust
use reflow_network::graph::{EnhancedGraphAnalysis, OptimizationSuggestion};

let runtime_analysis = graph.analyze_for_runtime();

println!("=== Runtime Analysis ===");
println!("Estimated execution time: {:.2}s", runtime_analysis.estimated_execution_time);
println!("Resource requirements: {:?}", runtime_analysis.resource_requirements);

// Parallelism opportunities
println!("\nParallelism analysis:");
println!("  Max parallelism: {}", runtime_analysis.parallelism.max_parallelism);
println!("  Parallel branches: {}", runtime_analysis.parallelism.parallel_branches.len());
println!("  Pipeline stages: {}", runtime_analysis.parallelism.pipeline_stages.len());

// Optimization suggestions
println!("\nOptimization suggestions:");
for suggestion in runtime_analysis.optimization_suggestions {
    match suggestion {
        OptimizationSuggestion::ParallelizableChain { nodes } => {
            println!("  ⚡ Parallelize chain: {:?}", nodes);
        }
        OptimizationSuggestion::RedundantNode { node, reason } => {
            println!("  🗑️  Remove redundant node '{}': {}", node, reason);
        }
        OptimizationSuggestion::ResourceBottleneck { resource, severity } => {
            println!("  ⚠️  Resource bottleneck in '{}': {:.1}% usage", resource, severity * 100.0);
        }
        OptimizationSuggestion::DataTypeOptimization { from, to, suggestion } => {
            println!("  🔧 Optimize types {} → {}: {}", from, to, suggestion);
        }
    }
}

// Performance bottlenecks
println!("\nPerformance bottlenecks:");
for bottleneck in runtime_analysis.performance_bottlenecks {
    match bottleneck {
        Bottleneck::HighDegree(node) => {
            println!("  🔥 High-degree node: {}", node);
        }
        Bottleneck::SequentialChain(chain) => {
            println!("  🐌 Sequential bottleneck: {:?}", chain);
        }
    }
}
```

## Subgraph Analysis

### Extracting Subgraphs

```rust
use reflow_network::graph::{Subgraph, SubgraphAnalysis};

// Get reachable subgraph from a node
if let Some(subgraph) = graph.get_reachable_subgraph("start_node") {
    println!("Subgraph from 'start_node':");
    println!("  Nodes: {:?}", subgraph.nodes);
    println!("  Entry points: {:?}", subgraph.entry_points);
    println!("  Exit points: {:?}", subgraph.exit_points);
    println!("  Internal connections: {}", subgraph.internal_connections.len());
    
    // Analyze subgraph characteristics
    let analysis = graph.analyze_subgraph(&subgraph);
    println!("  Analysis:");
    println!("    Node count: {}", analysis.node_count);
    println!("    Connection count: {}", analysis.connection_count);
    println!("    Max depth: {}", analysis.max_depth);
    println!("    Is cyclic: {}", analysis.is_cyclic);
    println!("    Branching factor: {:.2}", analysis.branching_factor);
}
```

### Independent Subgraph Detection

```rust
// Find all independent subgraphs
let subgraphs = graph.find_subgraphs();

println!("Found {} independent subgraphs:", subgraphs.len());
for (i, subgraph) in subgraphs.iter().enumerate() {
    println!("  Subgraph {}: {} nodes", i + 1, subgraph.nodes.len());
    
    let analysis = graph.analyze_subgraph(subgraph);
    if analysis.is_cyclic {
        println!("    ⚠️  Contains cycles");
    }
    
    if subgraph.entry_points.len() > 1 {
        println!("    ⚡ Multiple entry points - potential for parallel input");
    }
    
    if subgraph.exit_points.len() > 1 {
        println!("    📊 Multiple exit points - produces multiple outputs");
    }
}
```

## Graph Traversal Analysis

### Traversal with Analysis

```rust
use std::collections::HashSet;

// Depth-first traversal with custom analysis
let mut visited_order = Vec::new();
let mut max_depth = 0;
let mut current_depth = 0;

graph.traverse_depth_first("start_node", |node| {
    visited_order.push(node.id.clone());
    current_depth += 1;
    max_depth = max_depth.max(current_depth);
    
    println!("Visiting {} at depth {}", node.id, current_depth);
    
    // Analyze node characteristics
    if let Some(metadata) = &node.metadata {
        if let Some(estimated_time) = metadata.get("estimated_time") {
            println!("  Estimated processing time: {:?}", estimated_time);
        }
    }
})?;

println!("Traversal completed:");
println!("  Visit order: {:?}", visited_order);
println!("  Maximum depth: {}", max_depth);
```

### Breadth-First Layer Analysis

```rust
// Breadth-first traversal to analyze layers
let mut layers: HashMap<usize, Vec<String>> = HashMap::new();
let mut current_layer = 0;

graph.traverse_breadth_first("start_node", |node| {
    // In a real implementation, you'd track depth
    layers.entry(current_layer)
        .or_insert_with(Vec::new)
        .push(node.id.clone());
    
    println!("Layer {}: {}", current_layer, node.id);
})?;

// Analyze layer characteristics
for (layer, nodes) in layers {
    println!("Layer {} has {} nodes: {:?}", layer, nodes.len(), nodes);
    
    if nodes.len() > 1 {
        println!("  ⚡ Layer {} can be parallelized", layer);
    }
}
```

## Custom Analysis Functions

### Building Custom Analyzers

```rust
// Custom analyzer for finding critical paths
fn find_critical_path(graph: &Graph, start: &str, end: &str) -> Option<Vec<String>> {
    let mut longest_path = Vec::new();
    let mut max_weight = 0.0;
    
    // Use path tracing to find all paths
    if let Ok(paths) = graph.trace_data_flow(start) {
        for path in paths {
            if path.nodes.last() == Some(&end.to_string()) {
                // Calculate path weight based on estimated times
                let mut path_weight = 0.0;
                
                for node_id in &path.nodes {
                    if let Some(node) = graph.get_node(node_id) {
                        if let Some(metadata) = &node.metadata {
                            if let Some(time) = metadata.get("estimated_time") {
                                if let Some(t) = time.as_f64() {
                                    path_weight += t;
                                }
                            }
                        }
                    }
                }
                
                if path_weight > max_weight {
                    max_weight = path_weight;
                    longest_path = path.nodes;
                }
            }
        }
    }
    
    if longest_path.is_empty() {
        None
    } else {
        Some(longest_path)
    }
}

// Usage
if let Some(critical_path) = find_critical_path(&graph, "input", "output") {
    println!("Critical path: {:?}", critical_path);
}
```

### Performance Metrics Collection

```rust
use std::time::Instant;

// Benchmark graph operations
fn benchmark_graph_operations(graph: &Graph) {
    let start = Instant::now();
    
    // Benchmark cycle detection
    let cycle_start = Instant::now();
    let _cycles = graph.detect_all_cycles();
    let cycle_time = cycle_start.elapsed();
    
    // Benchmark validation
    let validation_start = Instant::now();
    let _validation = graph.validate_flow();
    let validation_time = validation_start.elapsed();
    
    // Benchmark parallelism analysis
    let parallelism_start = Instant::now();
    let _parallelism = graph.analyze_parallelism();
    let parallelism_time = parallelism_start.elapsed();
    
    let total_time = start.elapsed();
    
    println!("=== Performance Metrics ===");
    println!("Graph size: {} nodes, {} connections", 
        graph.nodes.len(), 
        graph.connections.len()
    );
    println!("Cycle detection: {:?}", cycle_time);
    println!("Flow validation: {:?}", validation_time);
    println!("Parallelism analysis: {:?}", parallelism_time);
    println!("Total analysis time: {:?}", total_time);
}
```

## Analysis Best Practices

### Incremental Analysis

For large graphs, perform incremental analysis:

```rust
// Instead of full validation on every change
let full_validation = graph.validate_flow()?; // Expensive

// Use targeted analysis
if let Some(cycle) = graph.detect_cycles() {
    // Handle cycles specifically
}

// Check only specific node connections
let node_issues = graph.find_orphaned_nodes()
    .into_iter()
    .filter(|n| recently_modified_nodes.contains(n))
    .collect::<Vec<_>>();
```

### Caching Analysis Results

```rust
use std::cell::RefCell;

struct CachedAnalyzer {
    graph: Graph,
    cached_validation: RefCell<Option<FlowValidation>>,
    validation_dirty: RefCell<bool>,
}

impl CachedAnalyzer {
    fn get_validation(&self) -> Result<FlowValidation, GraphError> {
        if *self.validation_dirty.borrow() {
            let validation = self.graph.validate_flow()?;
            *self.cached_validation.borrow_mut() = Some(validation.clone());
            *self.validation_dirty.borrow_mut() = false;
            Ok(validation)
        } else {
            Ok(self.cached_validation.borrow().clone().unwrap())
        }
    }
    
    fn invalidate_cache(&self) {
        *self.validation_dirty.borrow_mut() = true;
    }
}
```

### Parallel Analysis

For very large graphs, consider parallel analysis:

```rust
use std::thread;

// Analyze different aspects in parallel
let graph_clone = graph.clone();
let cycle_handle = thread::spawn(move || {
    graph_clone.detect_all_cycles()
});

let graph_clone2 = graph.clone();
let orphan_handle = thread::spawn(move || {
    graph_clone2.analyze_orphaned_nodes()
});

let graph_clone3 = graph.clone();
let parallelism_handle = thread::spawn(move || {
    graph_clone3.analyze_parallelism()
});

// Collect results
let cycles = cycle_handle.join().unwrap();
let orphan_analysis = orphan_handle.join().unwrap();
let parallelism_analysis = parallelism_handle.join().unwrap();

println!("Parallel analysis completed:");
println!("  Cycles: {}", cycles.len());
println!("  Orphaned: {}", orphan_analysis.total_orphaned);
println!("  Max parallelism: {}", parallelism_analysis.max_parallelism);
```

## Analysis Error Handling

### Robust Error Handling

```rust
use reflow_network::graph::GraphError;

fn safe_analysis(graph: &Graph) -> Result<(), Box<dyn std::error::Error>> {
    // Validate graph structure first
    match graph.validate_flow() {
        Ok(validation) => {
            if !validation.cycles.is_empty() {
                println!("⚠️  Cycles detected - some analyses may not be reliable");
            }
        }
        Err(e) => {
            eprintln!("Validation failed: {}", e);
            return Err(Box::new(e));
        }
    }
    
    // Perform safe traversal
    match graph.traverse_depth_first("start", |node| {
        println!("Processing: {}", node.id);
    }) {
        Ok(_) => println!("Traversal completed successfully"),
        Err(GraphError::NodeNotFound(node)) => {
            eprintln!("Start node '{}' not found", node);
        }
        Err(e) => {
            eprintln!("Traversal error: {}", e);
            return Err(Box::new(e));
        }
    }
    
    Ok(())
}
```

## Integration with Visual Editors

### Real-time Analysis Updates

```rust
// Update UI based on analysis results
fn update_editor_with_analysis(graph: &Graph, ui: &mut GraphEditor) {
    // Highlight cycles
    if let Some(cycle) = graph.detect_cycles() {
        for node in cycle {
            ui.highlight_node(&node, "error");
        }
    }
    
    // Show bottlenecks
    let bottlenecks = graph.detect_bottlenecks();
    for bottleneck in bottlenecks {
        match bottleneck {
            Bottleneck::HighDegree(node) => {
                ui.highlight_node(&node, "bottleneck");
            }
            Bottleneck::SequentialChain(chain) => {
                ui.highlight_chain(&chain, "optimization-opportunity");
            }
        }
    }
    
    // Show parallel opportunities
    let parallelism = graph.analyze_parallelism();
    for branch in parallelism.parallel_branches {
        ui.group_nodes(&branch.nodes, "parallel-group");
    }
}
```

## Next Steps

- [Layout System](layout.md) - Positioning and visualization
- [Advanced Features](advanced.md) - History, subgraphs, and optimization
- [Creating Graphs](creating-graphs.md) - Basic graph operations
- [Building Visual Editors](../../tutorials/building-visual-editor.md) - Complete tutorial
