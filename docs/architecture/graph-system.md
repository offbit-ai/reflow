# Graph System Architecture

Reflow's graph system provides a comprehensive flow-based programming (FBP) foundation for building visual workflow editors, data processing pipelines, and complex computational graphs. The system supports real-time validation, automatic layout, performance analysis, and both native Rust and WebAssembly implementations.

## Core Concepts

### Graph Structure

A Reflow graph consists of:

- **Nodes**: Processing units that represent actors or components
- **Connections**: Data flow paths between node ports
- **Ports**: Input/output endpoints with typed interfaces
- **Initial Information Packets (IIPs)**: Static data injected into the graph
- **Groups**: Logical collections of related nodes

```rust
use reflow_network::graph::{Graph, GraphNode, GraphConnection, GraphEdge, PortType};
use std::collections::HashMap;

// Create a new graph
let mut graph = Graph::new("MyWorkflow", false, None);

// Add nodes
graph.add_node("source", "DataSource", None);
graph.add_node("processor", "DataProcessor", None);
graph.add_node("sink", "DataSink", None);

// Connect nodes
graph.add_connection("source", "output", "processor", "input", None);
graph.add_connection("processor", "output", "sink", "input", None);
```

### Port Type System

Reflow uses a sophisticated type system to ensure data compatibility between connected nodes:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PortType {
    Any,                              // Accepts any data type
    Flow,                            // Control flow signals
    Event,                           // Event-driven data
    Boolean,                         // Boolean values
    Integer,                         // Integer numbers
    Float,                          // Floating-point numbers
    String,                         // Text data
    Object(String),                 // Structured objects with schema
    Array(Box<PortType>),          // Arrays of typed elements
    Stream,                        // Streaming data
    Encoded,                       // Binary encoded data
    Option(Box<PortType>),         // Optional values
}
```

### Type Compatibility

The system automatically validates type compatibility when connections are made:

```rust
// These connections are valid
graph.add_connection("int_source", "out", "float_sink", "in", None); // Integer → Float
graph.add_connection("any_source", "out", "string_sink", "in", None); // Any → String
graph.add_connection("data", "out", "stream", "in", None);            // Any → Stream

// This would be invalid and rejected
// graph.add_connection("string_source", "out", "int_sink", "in", None); // String ↛ Integer
```

## Graph Operations

### Node Management

```rust
// Add node with metadata
let metadata = HashMap::from([
    ("x".to_string(), json!(100)),
    ("y".to_string(), json!(200)),
    ("description".to_string(), json!("Processes incoming data"))
]);
graph.add_node("processor", "DataProcessor", Some(metadata));

// Update node metadata
graph.set_node_metadata("processor", HashMap::from([
    ("color".to_string(), json!("#ff0000"))
]));

// Remove node (also removes all connections)
graph.remove_node("processor");
```

### Connection Management

```rust
// Add connection with metadata
let conn_metadata = HashMap::from([
    ("weight".to_string(), json!(0.8)),
    ("priority".to_string(), json!("high"))
]);
graph.add_connection("source", "data", "sink", "input", Some(conn_metadata));

// Get connection details
if let Some(connection) = graph.get_connection("source", "data", "sink", "input") {
    println!("Connection: {:?}", connection);
}

// Remove specific connection
graph.remove_connection("source", "data", "sink", "input");
```

### Initial Information Packets (IIPs)

IIPs allow you to inject static data into the graph at startup:

```rust
use serde_json::json;

// Add configuration data
graph.add_initial(
    json!({"database_url": "postgresql://localhost/mydb"}),
    "database_connector",
    "config",
    None
);

// Add initial data with index for array ports
graph.add_initial_index(
    json!("input_file.txt"),
    "file_reader",
    "filenames",
    0,
    None
);
```

### Graph Ports

Expose internal node ports as graph-level interfaces:

```rust
// Add input port to graph
graph.add_inport(
    "data_input",           // External port name
    "processor",            // Internal node
    "input",               // Internal port
    PortType::Any,         // Port type
    None                   // Metadata
);

// Add output port to graph
graph.add_outport(
    "processed_data",      // External port name
    "processor",           // Internal node
    "output",              // Internal port
    PortType::Object("ProcessedData".to_string()),
    None
);
```

## Graph Validation

### Automatic Validation

The graph system performs continuous validation:

```rust
// Validate entire graph
let validation_result = graph.validate_flow()?;

if !validation_result.cycles.is_empty() {
    println!("Cycles detected: {:?}", validation_result.cycles);
}

if !validation_result.orphaned_nodes.is_empty() {
    println!("Orphaned nodes: {:?}", validation_result.orphaned_nodes);
}

for mismatch in validation_result.port_mismatches {
    println!("Port mismatch: {}", mismatch);
}
```

### Cycle Detection

Advanced cycle detection with path tracking:

```rust
// Detect first cycle
if let Some(cycle) = graph.detect_cycles() {
    println!("Cycle found: {:?}", cycle);
}

// Comprehensive cycle analysis
let cycle_analysis = graph.analyze_cycles();
println!("Total cycles: {}", cycle_analysis.total_cycles);
println!("Nodes in cycles: {:?}", cycle_analysis.nodes_in_cycles);
```

## Performance Analysis

### Parallelism Detection

Identify opportunities for parallel execution:

```rust
let parallelism = graph.analyze_parallelism();

// Parallel branches that can execute simultaneously
for branch in parallelism.parallel_branches {
    println!("Parallel branch: {:?}", branch.nodes);
}

// Pipeline stages for sequential execution
for stage in parallelism.pipeline_stages {
    println!("Stage {}: {:?}", stage.level, stage.nodes);
}
```

### Bottleneck Analysis

Find performance bottlenecks:

```rust
let bottlenecks = graph.detect_bottlenecks();

for bottleneck in bottlenecks {
    match bottleneck {
        Bottleneck::HighDegree(node) => {
            println!("High-degree bottleneck at node: {}", node);
        }
        Bottleneck::SequentialChain(chain) => {
            println!("Sequential chain that could be parallelized: {:?}", chain);
        }
    }
}
```

### Resource Analysis

Estimate execution requirements:

```rust
let analysis = graph.analyze_for_runtime();

println!("Estimated execution time: {:.2}s", analysis.estimated_execution_time);
println!("Resource requirements: {:?}", analysis.resource_requirements);

for suggestion in analysis.optimization_suggestions {
    match suggestion {
        OptimizationSuggestion::ParallelizableChain { nodes } => {
            println!("Consider parallelizing: {:?}", nodes);
        }
        OptimizationSuggestion::RedundantNode { node, reason } => {
            println!("Redundant node {}: {}", node, reason);
        }
        OptimizationSuggestion::ResourceBottleneck { resource, severity } => {
            println!("Resource bottleneck in {}: {:.1}%", resource, severity * 100.0);
        }
        OptimizationSuggestion::DataTypeOptimization { from, to, suggestion } => {
            println!("Optimize {} → {}: {}", from, to, suggestion);
        }
    }
}
```

## Graph Layout

### Automatic Layout

The system provides intelligent automatic layout:

```rust
// Calculate optimal positions
let positions = graph.calculate_layout();

for (node_id, position) in positions {
    println!("Node {}: x={:.1}, y={:.1}", node_id, position.x, position.y);
}

// Apply layout to graph metadata
graph.auto_layout()?;
```

### Manual Positioning

Set custom node positions:

```rust
// Set specific position
graph.set_node_position("processor", 150.0, 100.0)?;

// Set position with custom dimensions and anchor
let metadata = HashMap::from([
    ("position".to_string(), json!({"x": 200, "y": 150})),
    ("dimensions".to_string(), json!({
        "width": 120,
        "height": 80,
        "anchor": {"x": 0.5, "y": 0.5}  // Center anchor
    }))
]);
graph.set_node_metadata("custom_node", metadata);
```

## Event System

### Real-time Updates

Subscribe to graph changes:

```rust
use reflow_network::graph::GraphEvents;

// Graph creates event channel automatically
let (sender, receiver) = graph.event_channel;

// Listen for events
while let Ok(event) = receiver.recv() {
    match event {
        GraphEvents::AddNode(node_data) => {
            println!("Node added: {:?}", node_data);
        }
        GraphEvents::AddConnection(conn_data) => {
            println!("Connection added: {:?}", conn_data);
        }
        GraphEvents::RemoveNode(node_data) => {
            println!("Node removed: {:?}", node_data);
        }
        // ... handle other events
        _ => {}
    }
}
```

### Event Types

Complete list of graph events:

- `AddNode` / `RemoveNode` / `RenameNode` / `ChangeNode`
- `AddConnection` / `RemoveConnection` / `ChangeConnection`
- `AddInitial` / `RemoveInitial`
- `AddGroup` / `RemoveGroup` / `RenameGroup` / `ChangeGroup`
- `AddInport` / `RemoveInport` / `RenameInport` / `ChangeInport`
- `AddOutport` / `RemoveOutport` / `RenameOutport` / `ChangeOutport`
- `ChangeProperties`
- `StartTransaction` / `EndTransaction` / `Transaction`

## Serialization

### Export Format

Graphs can be serialized to JSON for storage and interchange:

```rust
// Export to JSON-compatible format
let export = graph.export();
let json_string = serde_json::to_string_pretty(&export)?;

// Load from JSON
let loaded_graph = Graph::load(export, Some(metadata));
```

### Export Structure

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "MyWorkflow",
    "description": "A sample workflow"
  },
  "processes": {
    "source": {
      "id": "source",
      "component": "DataSource",
      "metadata": {"x": 0, "y": 0}
    }
  },
  "connections": [
    {
      "from": {"nodeId": "source", "portId": "output"},
      "to": {"nodeId": "sink", "portId": "input"},
      "metadata": {}
    }
  ],
  "inports": {},
  "outports": {},
  "groups": []
}
```

## WebAssembly Support

### Browser Integration

The graph system compiles to WebAssembly for browser usage:

```javascript
import { Graph } from 'reflow-network';

// Create graph in browser
const graph = new Graph("WebWorkflow", false, {});

// Add nodes and connections
graph.addNode("input", "InputNode", {x: 0, y: 0});
graph.addNode("output", "OutputNode", {x: 200, y: 0});
graph.addConnection("input", "out", "output", "in", {});

// Subscribe to events
graph.subscribe((event) => {
    console.log("Graph event:", event);
});

// Export for persistence
const exported = graph.toJSON();
localStorage.setItem('workflow', JSON.stringify(exported));
```

### TypeScript Support

Full TypeScript definitions are generated:

```typescript
interface GraphNode {
    id: string;
    component: string;
    metadata?: Map<string, any>;
}

interface GraphConnection {
    from: GraphEdge;
    to: GraphEdge;
    metadata?: Map<string, any>;
    data?: any;
}

type PortType = 
  | { type: "flow" }
  | { type: "event" }
  | { type: "boolean" }
  | { type: "integer" }
  | { type: "float" }
  | { type: "string" }
  | { type: "object", value: string }
  | { type: "array", value: PortType }
  | { type: "stream" }
  | { type: "encoded" }
  | { type: "any" }
  | { type: "option", value: PortType };
```

## Graph History

### Undo/Redo System

Track changes for undo/redo functionality:

```rust
// Create graph with history tracking
let (mut graph, mut history) = Graph::with_history();

// Make changes
graph.add_node("test", "TestNode", None);
graph.add_connection("test", "out", "sink", "in", None);

// Undo last change
if let Some(event) = history.undo() {
    // Apply inverse operation
    history.apply_inverse(&mut graph, event)?;
}

// Redo change
if let Some(event) = history.redo() {
    // Reapply operation
    history.apply_event(&mut graph, event)?;
}
```

## Advanced Features

### Subgraph Analysis

Extract and analyze subgraphs:

```rust
// Get reachable subgraph from a node
if let Some(subgraph) = graph.get_reachable_subgraph("start_node") {
    let analysis = graph.analyze_subgraph(&subgraph);
    
    println!("Subgraph nodes: {}", analysis.node_count);
    println!("Max depth: {}", analysis.max_depth);
    println!("Is cyclic: {}", analysis.is_cyclic);
    println!("Branching factor: {:.2}", analysis.branching_factor);
}
```

### Graph Traversal

Efficient traversal algorithms:

```rust
// Depth-first traversal
graph.traverse_depth_first("start_node", |node| {
    println!("Visiting node: {}", node.id);
})?;

// Breadth-first traversal
graph.traverse_breadth_first("start_node", |node| {
    println!("Processing: {} ({})", node.id, node.component);
})?;
```

### Node Groups

Organize nodes into logical groups:

```rust
// Create group
graph.add_group("data_processing", vec!["filter".to_string(), "transform".to_string()], None);

// Add node to existing group
graph.add_to_group("data_processing", "validator");

// Remove from group
graph.remove_from_group("data_processing", "validator");
```

## Best Practices

### Performance Optimization

1. **Use indexed operations**: The graph uses internal indices for O(1) lookups
2. **Batch modifications**: Group related changes to minimize event overhead
3. **Validate incrementally**: Use targeted validation for better performance
4. **Cache analysis results**: Store expensive analysis results when graph is stable

### Memory Management

1. **Clean up connections**: Always remove connections before removing nodes
2. **Limit history size**: Use `with_history_and_limit()` for bounded memory usage
3. **Dispose of event listeners**: Unsubscribe from events when no longer needed

### Error Handling

1. **Check return values**: Most operations return Result types
2. **Validate before execution**: Use validation methods before running workflows
3. **Handle cycles gracefully**: Implement cycle detection in your workflow runtime
4. **Monitor resource usage**: Track memory and CPU usage for large graphs

## Integration Examples

### Visual Editor Integration

```rust
// In a visual editor, sync UI with graph events
graph.subscribe(|event| {
    match event {
        GraphEvents::AddNode(data) => ui.add_node_widget(data),
        GraphEvents::RemoveNode(data) => ui.remove_node_widget(data.id),
        GraphEvents::AddConnection(data) => ui.draw_connection(data),
        _ => {}
    }
});
```

### Workflow Execution

```rust
// Convert graph to executable network
let network = Network::from_graph(&graph)?;

// Execute with runtime
let runtime = Runtime::new();
runtime.execute(network).await?;
```

## Next Steps

- [Creating Graphs](../api/graph/creating-graphs.md) - Detailed API guide
- [Graph Analysis](../api/graph/analysis.md) - Validation and performance analysis
- [Layout System](../api/graph/layout.md) - Positioning and visualization
- [Advanced Features](../api/graph/advanced.md) - History, subgraphs, and optimization
- [Building Visual Editors](../tutorials/building-visual-editor.md) - Complete tutorial
