# Creating and Managing Graphs

This guide covers the core APIs for creating, modifying, and managing Reflow graphs.

## Graph Creation

### Basic Graph Creation

```rust
use reflow_network::graph::{Graph, PortType};
use std::collections::HashMap;
use serde_json::json;

// Create a new graph
let mut graph = Graph::new("MyWorkflow", false, None);

// Create with case sensitivity enabled
let mut case_sensitive_graph = Graph::new("CaseSensitive", true, None);

// Create with initial properties
let properties = HashMap::from([
    ("description".to_string(), json!("Data processing workflow")),
    ("version".to_string(), json!("1.0.0")),
    ("author".to_string(), json!("John Doe"))
]);
let mut graph_with_props = Graph::new("WorkflowV1", false, Some(properties));
```

### Graph with History Tracking

```rust
// Create graph with unlimited history
let (mut graph, mut history) = Graph::with_history();

// Create graph with limited history (recommended for production)
let (mut graph, mut history) = Graph::with_history_and_limit(100);
```

## Node Management

### Adding Nodes

```rust
// Basic node addition
graph.add_node("data_source", "FileReader", None);

// Node with position metadata
let metadata = HashMap::from([
    ("x".to_string(), json!(100)),
    ("y".to_string(), json!(200))
]);
graph.add_node("processor", "DataProcessor", Some(metadata));

// Node with comprehensive metadata
let rich_metadata = HashMap::from([
    ("x".to_string(), json!(300)),
    ("y".to_string(), json!(150)),
    ("label".to_string(), json!("CSV Parser")),
    ("color".to_string(), json!("#3498db")),
    ("estimated_time".to_string(), json!(2.5)),
    ("resources".to_string(), json!({
        "memory": 128,
        "cpu": 0.5
    })),
    ("configuration".to_string(), json!({
        "delimiter": ",",
        "has_header": true
    }))
]);
graph.add_node("csv_parser", "CSVParser", Some(rich_metadata));
```

### Node Retrieval

```rust
// Get node reference
if let Some(node) = graph.get_node("processor") {
    println!("Node component: {}", node.component);
    if let Some(metadata) = &node.metadata {
        println!("Node metadata: {:?}", metadata);
    }
}

// Get mutable node reference
if let Some(node) = graph.get_node_mut("processor") {
    // Modify node directly (not recommended - use set_node_metadata instead)
}
```

### Updating Node Metadata

```rust
// Update metadata (merges with existing)
let updates = HashMap::from([
    ("color".to_string(), json!("#e74c3c")),
    ("priority".to_string(), json!("high"))
]);
graph.set_node_metadata("processor", updates);

// Clear specific metadata field by setting to null
let clear_color = HashMap::from([
    ("color".to_string(), json!(null))
]);
graph.set_node_metadata("processor", clear_color);
```

### Node Removal

```rust
// Remove node (automatically removes all connections)
graph.remove_node("old_processor");
```

### Node Renaming

```rust
// Rename node (updates all references)
graph.rename_node("old_name", "new_name");
```

## Connection Management

### Creating Connections

```rust
// Basic connection
graph.add_connection("source", "output", "processor", "input", None);

// Connection with metadata
let conn_metadata = HashMap::from([
    ("weight".to_string(), json!(0.8)),
    ("priority".to_string(), json!("high")),
    ("buffer_size".to_string(), json!(1024))
]);
graph.add_connection("processor", "output", "sink", "input", Some(conn_metadata));
```

### Connection Queries

```rust
// Get specific connection
if let Some(connection) = graph.get_connection("source", "output", "processor", "input") {
    println!("Connection metadata: {:?}", connection.metadata);
}

// Get all connections for a node
let incoming = graph.get_incoming_connections("processor");
for (source_node, source_port, connection) in incoming {
    println!("Input from {}:{}", source_node, source_port);
}

let outgoing = graph.get_outgoing_connections("processor");
for (target_node, target_port, connection) in outgoing {
    println!("Output to {}:{}", target_node, target_port);
}

// Get connections for specific port
let port_incoming = graph.get_incoming_connections_for_port("processor", "input");
let port_outgoing = graph.get_outgoing_connections_for_port("processor", "output");
```

### Connection Analysis

```rust
// Check if nodes are connected
if graph.are_nodes_connected("source", "processor") {
    println!("Nodes are connected");
}

// Check specific port connections
if graph.are_ports_connected("source", "output", "processor", "input") {
    println!("Ports are connected");
}

// Get connection degrees
let (in_degree, out_degree) = graph.get_connection_degree("processor");
println!("Node has {} inputs and {} outputs", in_degree, out_degree);

// Get port-specific degrees
let (port_in, port_out) = graph.get_port_connection_degree("processor", "data");
```

### Connection Updates

```rust
// Update connection metadata
let new_metadata = HashMap::from([
    ("bandwidth".to_string(), json!("high")),
    ("encrypted".to_string(), json!(true))
]);
graph.set_connection_metadata("source", "output", "processor", "input", new_metadata);
```

### Connection Removal

```rust
// Remove specific connection
graph.remove_connection("source", "output", "processor", "input");

// Remove all connections for a node (called automatically when removing node)
graph.remove_node_connections("isolated_node");
```

## Graph Ports (Inports/Outports)

Graph ports expose internal node ports as external interfaces, making subgraphs reusable.

### Adding Input Ports

```rust
// Basic inport
graph.add_inport("data_input", "processor", "input", PortType::Any, None);

// Inport with metadata
let port_metadata = HashMap::from([
    ("description".to_string(), json!("Main data input stream")),
    ("required".to_string(), json!(true)),
    ("default_value".to_string(), json!(null))
]);
graph.add_inport("config", "processor", "config", PortType::Object("Config".to_string()), Some(port_metadata));
```

### Adding Output Ports

```rust
// Basic outport
graph.add_outport("processed_data", "processor", "output", PortType::Object("ProcessedData".to_string()), None);

// Outport with metadata
let out_metadata = HashMap::from([
    ("description".to_string(), json!("Processed data stream")),
    ("format".to_string(), json!("json"))
]);
graph.add_outport("results", "processor", "result", PortType::Array(Box::new(PortType::Object("Result".to_string()))), Some(out_metadata));
```

### Port Management

```rust
// Update port metadata
let port_updates = HashMap::from([
    ("required".to_string(), json!(false)),
    ("deprecated".to_string(), json!(true))
]);
graph.set_inport_metadata("data_input", port_updates);
graph.set_outport_metadata("results", port_updates);

// Rename ports
graph.rename_inport("old_input", "new_input");
graph.rename_outport("old_output", "new_output");

// Remove ports
graph.remove_inport("unused_input");
graph.remove_outport("unused_output");
```

## Initial Information Packets (IIPs)

IIPs provide static data to nodes at startup.

### Adding IIPs

```rust
// Basic IIP
graph.add_initial(
    json!("config.yaml"),
    "file_reader",
    "filename",
    None
);

// IIP with metadata
let iip_metadata = HashMap::from([
    ("source".to_string(), json!("configuration")),
    ("priority".to_string(), json!("high"))
]);
graph.add_initial(
    json!({"host": "localhost", "port": 8080}),
    "server",
    "config",
    Some(iip_metadata)
);

// IIP with array index
graph.add_initial_index(
    json!("file1.txt"),
    "multi_reader",
    "files",
    0,
    None
);
```

### Graph-level IIPs

When using graph ports, you can add IIPs at the graph level:

```rust
// Add IIP to graph inport
graph.add_graph_initial(
    json!({"mode": "production"}),
    "config_input",  // Graph inport name
    None
);

// Add indexed IIP to graph inport
graph.add_graph_initial_index(
    json!("primary.db"),
    "database_files",  // Graph inport name
    0,
    None
);
```

### Removing IIPs

```rust
// Remove node-level IIP
graph.remove_initial("file_reader", "filename");

// Remove graph-level IIP
graph.remove_graph_initial("config_input");
```

## Node Groups

Groups provide logical organization of related nodes.

### Creating Groups

```rust
// Create basic group
graph.add_group("data_processing", vec!["parser".to_string(), "validator".to_string(), "transformer".to_string()], None);

// Group with metadata
let group_metadata = HashMap::from([
    ("color".to_string(), json!("#2ecc71")),
    ("description".to_string(), json!("Data processing pipeline")),
    ("collapsed".to_string(), json!(false))
]);
graph.add_group("preprocessing", vec!["cleaner".to_string(), "normalizer".to_string()], Some(group_metadata));
```

### Managing Group Membership

```rust
// Add node to existing group
graph.add_to_group("data_processing", "formatter");

// Remove node from group
graph.remove_from_group("data_processing", "formatter");
```

### Group Metadata

```rust
// Update group metadata
let group_updates = HashMap::from([
    ("collapsed".to_string(), json!(true)),
    ("priority".to_string(), json!("high"))
]);
graph.set_group_metadata("data_processing", group_updates);
```

### Removing Groups

```rust
// Remove entire group (nodes remain, just ungrouped)
graph.remove_group("old_group");
```

## Graph Properties

### Setting Properties

```rust
// Set multiple properties
let properties = HashMap::from([
    ("name".to_string(), json!("Updated Workflow")),
    ("version".to_string(), json!("2.0.0")),
    ("description".to_string(), json!("Enhanced data processing")),
    ("tags".to_string(), json!(["data", "processing", "etl"]))
]);
graph.set_properties(properties);
```

### Getting Properties

```rust
// Properties are accessible via graph.properties field
if let Some(name) = graph.properties.get("name") {
    println!("Graph name: {}", name);
}
```

## Event Handling

### Subscribing to Events

```rust
use reflow_network::graph::GraphEvents;

// Get event receiver
let event_receiver = graph.event_channel.1.clone();

// Handle events in a loop
std::thread::spawn(move || {
    while let Ok(event) = event_receiver.recv() {
        match event {
            GraphEvents::AddNode(data) => {
                println!("Node added: {:?}", data);
            }
            GraphEvents::RemoveNode(data) => {
                println!("Node removed: {:?}", data);
            }
            GraphEvents::AddConnection(data) => {
                println!("Connection added: {:?}", data);
            }
            GraphEvents::RemoveConnection(data) => {
                println!("Connection removed: {:?}", data);
            }
            GraphEvents::ChangeNode(data) => {
                println!("Node changed: {:?}", data);
            }
            // ... handle other events
            _ => {}
        }
    }
});
```

### Event Types Reference

| Event | Triggered When | Data |
|-------|----------------|------|
| `AddNode` | Node is added | Node data |
| `RemoveNode` | Node is removed | Node data |
| `RenameNode` | Node is renamed | `{old, new}` |
| `ChangeNode` | Node metadata changes | `{node, old_metadata, new_metadata}` |
| `AddConnection` | Connection is added | Connection data |
| `RemoveConnection` | Connection is removed | Connection data |
| `ChangeConnection` | Connection metadata changes | `{connection, old_metadata, new_metadata}` |
| `AddInitial` | IIP is added | IIP data |
| `RemoveInitial` | IIP is removed | IIP data |
| `AddGroup` | Group is created | Group data |
| `RemoveGroup` | Group is removed | Group data |
| `RenameGroup` | Group is renamed | `{old, new}` |
| `ChangeGroup` | Group metadata changes | `{group, old_metadata, new_metadata}` |
| `AddInport` | Inport is added | `{id, port}` |
| `RemoveInport` | Inport is removed | `{id, port}` |
| `RenameInport` | Inport is renamed | `{old, new}` |
| `ChangeInport` | Inport metadata changes | `{name, port, old_metadata, new_metadata}` |
| `AddOutport` | Outport is added | `{id, port}` |
| `RemoveOutport` | Outport is removed | `{id, port}` |
| `RenameOutport` | Outport is renamed | `{old, new}` |
| `ChangeOutport` | Outport metadata changes | `{name, port, old_metadata, new_metadata}` |
| `ChangeProperties` | Graph properties change | `{new, before}` |

## Serialization and Loading

### Exporting Graphs

```rust
// Export to GraphExport format
let export = graph.export();

// Serialize to JSON
let json_string = serde_json::to_string_pretty(&export)?;
std::fs::write("workflow.json", json_string)?;
```

### Loading Graphs

```rust
// Load from JSON
let json_content = std::fs::read_to_string("workflow.json")?;
let export: GraphExport = serde_json::from_str(&json_content)?;

// Create graph from export
let metadata = HashMap::from([
    ("loaded_at".to_string(), json!(chrono::Utc::now().to_rfc3339()))
]);
let loaded_graph = Graph::load(export, Some(metadata));
```

## WebAssembly API

When using the graph system in a browser via WebAssembly:

### JavaScript/TypeScript Usage

```javascript
import { Graph, PortType } from 'reflow-network';

// Create graph
const graph = new Graph("WebWorkflow", false, {
    description: "Browser-based workflow"
});

// Add nodes
graph.addNode("input", "InputNode", { x: 0, y: 0 });
graph.addNode("output", "OutputNode", { x: 200, y: 0 });

// Add connections
graph.addConnection("input", "out", "output", "in", {});

// Subscribe to events
graph.subscribe((event) => {
    console.log("Graph event:", event);
    // Update UI based on event
    updateUI(event);
});

// Export for persistence
const exported = graph.toJSON();
localStorage.setItem('workflow', JSON.stringify(exported));

// Load saved workflow
const saved = JSON.parse(localStorage.getItem('workflow'));
const restoredGraph = Graph.load(saved, {});
```

## Error Handling

### Common Error Scenarios

```rust
use reflow_network::graph::GraphError;

// Handle node operations
match graph.add_node("duplicate", "TestNode", None) {
    Ok(_) => println!("Node added successfully"),
    Err(GraphError::DuplicateNode(id)) => println!("Node {} already exists", id),
    Err(e) => println!("Error: {}", e),
}

// Handle traversal errors
match graph.traverse_depth_first("nonexistent", |node| {
    println!("Visiting: {}", node.id);
}) {
    Ok(_) => println!("Traversal completed"),
    Err(GraphError::NodeNotFound(id)) => println!("Start node {} not found", id),
    Err(e) => println!("Traversal error: {}", e),
}
```

### Error Types

- `NodeNotFound(String)` - Referenced node doesn't exist
- `DuplicateNode(String)` - Node with same ID already exists
- `InvalidConnection { from: String, to: String }` - Connection cannot be created
- `CycleDetected` - Operation would create a cycle (if validation enabled)
- `InvalidOperation(String)` - Generic operation error

## Best Practices

### Performance Tips

1. **Batch Operations**: Group related changes to minimize events
   ```rust
   // Instead of individual operations, batch them
   graph.add_node("n1", "Node1", None);
   graph.add_node("n2", "Node2", None);
   graph.add_connection("n1", "out", "n2", "in", None);
   ```

2. **Use Appropriate Data Structures**: Store frequently accessed metadata efficiently
   ```rust
   // Good: structured metadata
   let metadata = HashMap::from([
       ("config".to_string(), json!({
           "retries": 3,
           "timeout": 30
       }))
   ]);
   
   // Avoid: flat key-value for complex data
   ```

3. **Validate Incrementally**: Use targeted validation instead of full graph validation
   ```rust
   // Check specific aspects instead of full validation
   if let Some(cycle) = graph.detect_cycles() {
       // Handle cycle
   }
   ```

### Memory Management

1. **Limit History**: Use bounded history for production systems
   ```rust
   let (graph, history) = Graph::with_history_and_limit(50);
   ```

2. **Clean Up Events**: Ensure event listeners are properly disposed
   ```rust
   // Store receiver handle to drop when done
   let receiver = graph.event_channel.1.clone();
   // ... use receiver
   drop(receiver); // Clean up
   ```

3. **Efficient Metadata**: Avoid storing large objects in metadata
   ```rust
   // Good: reference to external data
   let metadata = HashMap::from([
       ("data_ref".to_string(), json!("storage://large-dataset-id"))
   ]);
   
   // Avoid: embedding large data
   ```

## Next Steps

- [Graph Analysis](analysis.md) - Validation and performance analysis
- [Layout System](layout.md) - Positioning and visualization
- [Advanced Features](advanced.md) - History, subgraphs, and optimization
- [Building Visual Editors](../../tutorials/building-visual-editor.md) - Complete tutorial
