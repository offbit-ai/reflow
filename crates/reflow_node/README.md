# ReFlow Network - Node.js Bindings

[![npm version](https://badge.fury.io/js/%40reflow%2Fnetwork-node.svg)](https://badge.fury.io/js/%40reflow%2Fnetwork-node)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Node.js bindings for Reflow - A powerful, actor-based workflow execution engine built in Rust**

## 🚀 Overview

This package provides Node.js bindings for the Reflow workflow execution engine using [Neon](https://neon-bindings.com/). Reflow uses the **actor model** for concurrent, message-passing computation, enabling you to build complex data processing pipelines, real-time systems, and distributed workflows.

### Key Features

🎭 **Actor-Based Architecture** - Isolated, concurrent actors communicate via message passing  
📊 **Visual Workflows** - Graph-based workflow representation with history and undo  
⚡ **High Performance** - Rust-powered execution with zero-copy optimizations  
🔄 **Real-Time Processing** - Built-in networking, WebSockets, and live data streams  
📦 **Extensible** - Rich component library + custom component creation  
🌐 **Native Node.js Integration** - Full filesystem and networking capabilities

## 🔄 API Compatibility

The Node.js bindings provide the same API surface as the WASM version but with native Node.js capabilities:

| API Class | Description | Enhanced Features |
|-----------|-------------|-------------------|
| `Network` | Core actor runtime and message routing | Native async/await, full networking |
| `GraphNetwork` | Graph-specific network execution | Filesystem persistence, better performance |
| `Graph` | Graph structure manipulation | Direct file I/O, enhanced operations |
| `GraphHistory` | Graph history and undo operations | Persistent history, better memory management |
| `Actor` | Individual workflow actors | Native threading, enhanced execution |
| `Workspace` | Multi-graph workspace management | File system integration, directory watching |
| `MultiGraphNetwork` | Complex multi-graph execution | Advanced networking, distributed execution |
| `NamespaceManager` | Namespace management and conflict resolution | Enhanced resolution strategies |
| `GraphDependencyManager` | Graph dependency resolution | File-based dependency tracking |

## 📦 Installation

### Prerequisites

- **Node.js 16+** (recommended: Node.js 18+)
- **Rust toolchain** (for building from source)
- **Platform**: macOS, Linux, or Windows

### From npm (coming soon)

```bash
npm install @reflow/network-node
```

### Building from Source

```bash
# Clone the repository
git clone https://github.com/your-org/reflow.git
cd reflow/crates/reflow_node

# Build the native module
cargo build --release

# Run the example
npm test
```

## 🎯 Quick Start

```javascript
const reflow = require('@reflow/network-node');

async function main() {
    // Initialize error handling
    reflow.init_panic_hook();
    
    // Create a new graph for your workflow
    const graph = new reflow.Graph();
    
    // Add actors to the graph
    graph.addNode("source", "DataSource", {
        data: [1, 2, 3, 4, 5]
    });
    
    graph.addNode("processor", "MapActor", {
        function: "x => x * 2"
    });
    
    graph.addNode("sink", "Logger", {});
    
    // Connect the actors
    graph.addConnection("source", "output", "processor", "input", {});
    graph.addConnection("processor", "output", "sink", "input", {});
    
    // Create and execute the network
    const network = new reflow.Network();
    await network.loadGraph(graph);
    await network.start();
    
    console.log('✅ Workflow executed successfully!');
}

main().catch(console.error);
```

## 🔍 API Reference

### Core Classes

#### `Network`
Core actor runtime and message routing system.

```javascript
const network = new reflow.Network();
await network.loadGraph(graph);     // Load a graph for execution
await network.start();              // Start the network
await network.stop();               // Stop the network
network.shutdown();                 // Clean shutdown
```

#### `Graph`
Graph structure manipulation and management.

```javascript
const graph = new reflow.Graph();
graph.addNode(id, component, metadata);           // Add a node
graph.addConnection(from, fromPort, to, toPort);  // Connect nodes
graph.removeNode(id);                            // Remove a node
const exported = graph.export();                // Export graph structure
```

#### `Actor`
Individual workflow actors with message processing.

```javascript
const actor = new reflow.Actor(config);
await actor.initialize();          // Initialize the actor
await actor.processMessage(msg);   // Process a message
actor.getState();                  // Get current state
```

### Multi-Graph System

#### `Workspace`
Multi-graph workspace management with file system integration.

```javascript
const workspace = new reflow.Workspace();
await workspace.loadFromDirectory('./graphs');     // Load graphs from directory
await workspace.saveGraph(graph, './output.json'); // Save graph to file
const composition = workspace.composeGraphs();     // Compose multiple graphs
```

#### `NamespaceManager`
Namespace management and conflict resolution.

```javascript
const nsManager = new reflow.NamespaceManager();
const namespace = nsManager.registerGraph(graph);  // Register a graph
const resolved = nsManager.resolveProcessPath(path); // Resolve process path
```

### Graph Operations

#### `GraphLoader`
Load graphs from various sources.

```javascript
const loader = new reflow.GraphLoader();
const graph = await loader.loadFromFile('./workflow.json');  // Load from file
const graph2 = await loader.loadFromUrl('http://example.com/graph'); // Load from URL
const graphs = await loader.loadMultiple(sources); // Load multiple graphs
```

#### `GraphValidator`
Validate graph structure and dependencies.

```javascript
const validator = new reflow.GraphValidator();
validator.validate(graph);  // Validate a graph structure
```

#### `GraphComposer`
Compose multiple graphs into unified workflows.

```javascript
const composer = new reflow.GraphComposer();
const composed = await composer.composeGraphs({
    sources: [graph1, graph2],
    connections: [...],
    shared_resources: [...]
});
```

### Error Types

```javascript
// Comprehensive error handling
try {
    await workspace.composeGraphs();
} catch (error) {
    if (error instanceof reflow.CompositionError) {
        console.log('Composition failed:', error.message);
    } else if (error instanceof reflow.ValidationError) {
        console.log('Validation failed:', error.message);
    }
}
```

## 🚀 Workflow Capabilities

### Data Processing Pipelines
```javascript
// ETL pipeline with error handling
source → validate → transform → load → audit
```

### Real-Time Analytics  
```javascript
// Live data processing
websocket → parse → aggregate → alert → dashboard
```

### IoT Data Processing
```javascript
// Sensor data workflow  
mqtt → decode → filter → analyze → store → notify
```

### Media Processing
```javascript
// Audio/video pipeline
upload → decode → process → encode → publish
```

## 📁 Project Structure

```
crates/reflow_node/
├── src/
│   ├── lib.rs                 # Main Neon bindings entry
│   ├── runtime.rs             # Tokio async runtime
│   └── neon_bindings/         # Individual binding modules
│       ├── mod.rs
│       ├── network.rs         # Network bindings
│       ├── graph.rs           # Graph bindings
│       ├── actor.rs           # Actor bindings
│       ├── multi_graph.rs     # Multi-graph bindings
│       ├── errors.rs          # Error type bindings
│       └── utils.rs           # Utility functions
├── examples/
│   └── basic_example.js       # Usage example
├── Cargo.toml                 # Rust dependencies
├── package.json               # Node.js package info
└── README.md                  # This file
```

## 🛠️ Development

### Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
npm test
```

### Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality  
4. Ensure all tests pass
5. Submit a pull request

## 📋 Requirements

- **Node.js**: 16.0.0 or higher
- **Rust**: 1.70.0 or higher (for building)
- **Platform**: macOS (Intel/ARM), Linux (x64/ARM64), Windows (x64)

## 📄 License

MIT License - see [LICENSE](LICENSE) file for details.

## 🤝 Support

- **Documentation**: [docs.reflow.network](https://docs.reflow.network)
- **Issues**: [GitHub Issues](https://github.com/your-org/reflow/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/reflow/discussions)

---

**ReFlow Network Node.js Bindings - Native workflow execution for Node.js! 🚀**
