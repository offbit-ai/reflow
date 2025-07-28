# WebAssembly API - Getting Started

Complete guide to using Reflow's WebAssembly bindings for browser-based workflow automation.

## Overview

Reflow's WebAssembly (WASM) bindings provide a complete JavaScript interface for running actor-based workflows in web browsers. The API maintains the same conceptual model as the native Rust implementation while offering browser-friendly interfaces.

## Core Architecture

```
┌─────────────────────────────────────────────────────┐
│                 Browser Application                 │
├─────────────────────────────────────────────────────┤
│ JavaScript Actor Classes                            │
│ ├─ MyActor.run(context)                            │
│ ├─ AnotherActor.run(context)                       │
│ └─ CustomActor.run(context)                        │
├─────────────────────────────────────────────────────┤
│ Browser JavaScript Bindings                           │
│ ├─ Graph, GraphNetwork, GraphHistory               │
│ ├─ Network, MemoryState, ActorRunContext           │
│ └─ BrowserActorContext, JsBrowserActor                   │
├─────────────────────────────────────────────────────┤
│ WebAssembly Runtime                                 │
│ ├─ Rust Actor System (compiled to WASM)           │
│ ├─ Graph Management & Validation                   │
│ └─ Network Execution Engine                        │
└─────────────────────────────────────────────────────┘
```

## Quick Start

### 1. Installation & Setup

```bash
# Clone the repository
git clone https://github.com/reflow-project/reflow
cd reflow

# Build WASM bindings
cd crates/reflow_network
wasm-pack build --target web --out-dir pkg
```

### 2. Basic HTML Setup

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <title>Reflow WASM Example</title>
</head>
<body>
    <h1>Reflow WebAssembly Example</h1>
    <button id="runWorkflow">Run Workflow</button>
    <pre id="output"></pre>

    <script type="module">
        import init, { 
            Graph,
            GraphNetwork,
            MemoryState,
            init_panic_hook 
        } from './pkg/reflow_network.js';

        // Initialize WASM
        await init();
        init_panic_hook();

        console.log('✅ Reflow WASM initialized successfully!');
    </script>
</body>
</html>
```

### 3. Your First Actor

```javascript
class HelloWorldActor {
    constructor() {
        this.inports = ["input"];
        this.outports = ["output"];
        this.state = null; // Managed by WASM
        this.config = { greeting: "Hello" };
    }

    /**
     * Actor execution method
     * @param {ActorRunContext} context - Execution context
     */
    run(context) {
        // Get input data
        const input = context.input.input;
        
        // Access state
        const count = context.state.get('count') || 0;
        context.state.set('count', count + 1);
        
        // Process and send output
        const greeting = `${this.config.greeting}, ${input}! (execution #${count + 1})`;
        context.send({ output: greeting });
        
        console.log(`HelloWorldActor: ${greeting}`);
    }
}
```

### 4. Create and Run a Graph

```javascript
async function createAndRunWorkflow() {
    // Create a graph
    const graph = new Graph("HelloWorkflow", true, {
        description: "A simple greeting workflow",
        version: "1.0.0"
    });

    // Add nodes
    graph.addNode("greeter", "HelloWorldActor", {
        x: 100, y: 100,
        description: "Greets the input"
    });

    // Add initial data
    graph.addInitial("World", "greeter", "input", {
        description: "Initial greeting target"
    });

    // Create network
    const network = new GraphNetwork(graph);
    
    // Register actor
    network.registerActor("HelloWorldActor", new HelloWorldActor());
    
    // Start and run
    await network.start();
    
    console.log("🚀 Workflow started!");
}

// Run the workflow
document.getElementById('runWorkflow').addEventListener('click', createAndRunWorkflow);
```

## Core API Classes

### Graph

The `Graph` class represents a workflow definition with nodes, connections, and metadata.

```javascript
// Create a new graph
const graph = new Graph(name, caseSensitive, properties);

// Basic operations
graph.addNode(nodeId, actorType, metadata);
graph.removeNode(nodeId);
graph.addConnection(fromNode, fromPort, toNode, toPort, metadata);
graph.removeConnection(fromNode, fromPort, toNode, toPort);

// Graph-level ports
graph.addInport(publicName, nodeId, portId, metadata);
graph.addOutport(publicName, nodeId, portId, metadata);

// Initial data
graph.addInitial(data, nodeId, portId, metadata);

// Export/Import
const graphData = graph.toJSON();
const loadedGraph = Graph.load(graphData, metadata);
```

### GraphNetwork

The `GraphNetwork` class executes graphs with registered actors.

```javascript
// Create from graph
const network = new GraphNetwork(graph);

// Register actors
network.registerActor("ActorType", new ActorImplementation());

// Network lifecycle
await network.start();
network.shutdown();

// Monitoring
network.next((event) => {
    console.log("Network event:", event);
});

// Direct execution
const result = await network.executeActor("nodeId", inputData);
```

### MemoryState

The `MemoryState` class provides persistent state management across actor executions.

```javascript
// Create state
const state = new MemoryState();

// Basic operations
state.set(key, value);
const value = state.get(key);
const exists = state.has(key);
state.remove(key);
state.clear();

// Bulk operations
const allData = state.getAll();
state.setAll(dataObject);

// Utilities
const size = state.size();
const keys = state.keys();
const values = state.values();
```

### ActorRunContext

The `ActorRunContext` provides actors with access to inputs, state, and output channels.

```javascript
class MyActor {
    run(context) {
        // Access inputs
        const inputData = context.input.portName;
        
        // State management
        context.state.set('key', 'value');
        const value = context.state.get('key');
        
        // Send outputs
        context.send({
            outputPort: resultData
        });
        
        // Access configuration
        const config = this.config;
    }
}
```

## Event System

### Network Events

Monitor network execution with the event system:

```javascript
network.next((event) => {
    switch (event._type) {
        case "FlowTrace":
            console.log(`Data flow: ${event.from.actorId}:${event.from.port} → ${event.to.actorId}:${event.to.port}`);
            console.log("Data:", event.from.data);
            break;
            
        case "ActorStarted":
            console.log(`Actor started: ${event.actorId}`);
            break;
            
        case "ActorStopped":
            console.log(`Actor stopped: ${event.actorId}`);
            break;
            
        case "NetworkStarted":
            console.log("Network execution started");
            break;
            
        case "NetworkStopped":
            console.log("Network execution stopped");
            break;
            
        case "ProcessError":
            console.error(`Error in ${event.actorId}:`, event.error);
            break;
            
        default:
            console.log("Other event:", event);
    }
});
```

### Graph Events

Monitor graph modifications:

```javascript
graph.subscribe((event) => {
    switch (event.type) {
        case "nodeAdded":
            console.log(`Node added: ${event.nodeId}`);
            break;
            
        case "nodeRemoved":
            console.log(`Node removed: ${event.nodeId}`);
            break;
            
        case "connectionAdded":
            console.log(`Connection: ${event.from} → ${event.to}`);
            break;
            
        case "connectionRemoved":
            console.log(`Connection removed: ${event.from} → ${event.to}`);
            break;
    }
});
```

## Advanced Features

### Graph History with Undo/Redo

```javascript
// Create graph with history support
const [graph, history] = Graph.withHistoryAndLimit(50);

// Make changes
graph.addNode("processor", "ProcessorActor", { x: 200, y: 100 });
graph.addConnection("input", "output", "processor", "input");

// Update history
history.processEvents(graph);

// Check state
const state = history.getState();
console.log("Can undo:", state.can_undo);
console.log("Can redo:", state.can_redo);
console.log("Undo stack size:", state.undo_size);

// Perform operations
if (state.can_undo) {
    history.undo(graph);
}

if (history.getState().can_redo) {
    history.redo(graph);
}

// Clear history
history.clear();
```

### Direct Actor Execution

Test actors individually without full network setup:

```javascript
// Execute actor directly
const actor = new MyActor();
const result = await network.executeActor("nodeId", {
    input: "test data",
    config: { mode: "debug" }
});

console.log("Direct execution result:", result);
```

### Batch Graph Operations

Efficiently modify graphs with multiple operations:

```javascript
// Batch multiple operations
const operations = [
    () => graph.addNode("node1", "Actor1", { x: 100, y: 100 }),
    () => graph.addNode("node2", "Actor2", { x: 200, y: 100 }),
    () => graph.addConnection("node1", "output", "node2", "input"),
    () => graph.addInitial("start", "node1", "trigger")
];

// Execute all operations
operations.forEach(op => op());

// Process all changes at once
history.processEvents(graph);
```

## Data Types and Serialization

### Supported Data Types

The WASM bridge supports these JavaScript types:

```javascript
// Primitive types
const stringData = "Hello World";
const numberData = 42;
const booleanData = true;
const nullData = null;

// Objects and arrays
const objectData = {
    id: 123,
    name: "Example",
    tags: ["tag1", "tag2"],
    metadata: {
        created: new Date().toISOString(),
        version: "1.0"
    }
};

const arrayData = [1, 2, 3, "mixed", { nested: true }];

// Send through actor context
context.send({
    output: {
        primitive: numberData,
        object: objectData,
        array: arrayData
    }
});
```

### Serialization Best Practices

```javascript
// ✅ Good: Structured data
const goodData = {
    type: "sensor_reading",
    value: 23.5,
    timestamp: Date.now(),
    metadata: {
        sensor_id: "temp_01",
        location: "warehouse_a"
    }
};

// ❌ Avoid: Large JSON strings
const badData = JSON.stringify(largeObject);

// ✅ Good: Split large data
const chunkedData = {
    chunk_id: 1,
    total_chunks: 5,
    data: partialData
};
```

## Error Handling

### Comprehensive Error Handling

```javascript
try {
    // Initialize WASM
    await init();
    init_panic_hook();
    
    // Create and start network
    const network = new GraphNetwork(graph);
    network.registerActor("MyActor", new MyActor());
    await network.start();
    
} catch (error) {
    console.error("Error during initialization:", error);
    
    // Handle specific error types
    if (error.message.includes("WASM")) {
        alert("Failed to load WebAssembly. Please check browser compatibility.");
    } else if (error.message.includes("Actor")) {
        alert("Actor registration failed. Please check actor implementation.");
    } else {
        alert("Unexpected error. Please refresh the page.");
    }
}

// Network-level error handling
network.next((event) => {
    if (event._type === "ProcessError") {
        console.error(`Actor ${event.actorId} failed:`, event.error);
        
        // Implement recovery logic
        handleActorError(event.actorId, event.error);
    }
});

function handleActorError(actorId, error) {
    // Log error details
    console.error(`Processing error in ${actorId}:`, error);
    
    // Attempt recovery
    if (error.includes("timeout")) {
        // Restart actor or increase timeout
    } else if (error.includes("validation")) {
        // Fix input data and retry
    }
}
```

### Actor Error Handling

```javascript
class RobustActor {
    run(context) {
        try {
            // Main processing logic
            const input = context.input.input;
            const result = this.processData(input);
            context.send({ output: result });
            
        } catch (error) {
            console.error(`Error in ${this.constructor.name}:`, error);
            
            // Send error information
            context.send({
                error: {
                    message: error.message,
                    timestamp: Date.now(),
                    input: context.input
                }
            });
        }
    }
    
    processData(input) {
        // Validate input
        if (!input || typeof input !== 'object') {
            throw new Error("Invalid input: expected object");
        }
        
        // Process with error handling
        return {
            processed: true,
            data: input,
            timestamp: Date.now()
        };
    }
}
```

## Performance Optimization

### Memory Management

```javascript
// Clean up resources properly
function cleanup() {
    // Shutdown network
    if (network) {
        network.shutdown();
    }
    
    // Clear state
    if (state) {
        state.clear();
    }
    
    // Remove event listeners
    if (unsubscribe) {
        unsubscribe();
    }
}

// Set up cleanup on page unload
window.addEventListener('beforeunload', cleanup);
```

### Efficient State Usage

```javascript
class OptimizedActor {
    run(context) {
        // Read state once
        const state = context.state.getAll();
        
        // Modify locally
        state.counter = (state.counter || 0) + 1;
        state.lastUpdate = Date.now();
        
        // Write back once
        context.state.setAll(state);
        
        // Process and send output
        context.send({ output: state.counter });
    }
}
```

### Batch Processing

```javascript
class BatchProcessor {
    constructor() {
        this.inports = ["input"];
        this.outports = ["output"];
        this.config = { batchSize: 10 };
    }
    
    run(context) {
        // Accumulate inputs
        const batch = context.state.get('batch') || [];
        batch.push(context.input.input);
        
        if (batch.length >= this.config.batchSize) {
            // Process entire batch
            const results = batch.map(item => this.processItem(item));
            
            // Send batch results
            context.send({ output: results });
            
            // Clear batch
            context.state.set('batch', []);
        } else {
            // Store for next execution
            context.state.set('batch', batch);
        }
    }
    
    processItem(item) {
        return { processed: item, timestamp: Date.now() };
    }
}
```

## Development Tools

### Debug Mode

```javascript
// Enable debug logging
function enableDebugMode(network) {
    let eventCount = 0;
    
    network.next((event) => {
        eventCount++;
        console.group(`Event #${eventCount}: ${event._type}`);
        console.log("Full event:", event);
        
        if (event._type === "FlowTrace") {
            console.log(`From: ${event.from.actorId}:${event.from.port}`);
            console.log(`To: ${event.to.actorId}:${event.to.port}`);
            console.log("Data:", event.from.data);
        }
        
        console.groupEnd();
    });
    
    // Network information
    console.log("Registered actors:", network.getActorNames());
    console.log("Active actors:", network.getActiveActors());
    console.log("Total actor count:", network.getActorCount());
}
```

### Graph Inspection

```javascript
function inspectGraph(graph) {
    const data = graph.toJSON();
    
    console.group("Graph Inspection");
    console.log("Graph name:", data.properties?.name || "Unnamed");
    console.log("Case sensitive:", data.caseSensitive);
    console.log("Processes:", Object.keys(data.processes || {}));
    console.log("Connections:", data.connections?.length || 0);
    console.log("Inports:", Object.keys(data.inports || {}));
    console.log("Outports:", Object.keys(data.outports || {}));
    console.log("Initial data:", data.initializers?.length || 0);
    console.log("Full structure:", data);
    console.groupEnd();
}
```

## Next Steps

- **[Browser Actors](actors-in-browser.md)** - Creating actors for web environments
- **[Graph Management](graphs-and-networks.md)** - Advanced graph operations
- **[State Management](state-management.md)** - Persistent state handling
- **[Events & Monitoring](events-and-monitoring.md)** - Real-time event handling
- **[Browser Workflow Editor Tutorial](../../tutorials/browser-workflow-editor.md)** - Building visual editors

The WebAssembly API provides a powerful foundation for building browser-based workflow applications. Start with the examples above and explore the detailed API documentation for advanced usage patterns.
