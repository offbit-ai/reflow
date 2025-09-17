# @offbit-ai/reflow

**A powerful, actor-based workflow execution engine**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![WebAssembly](https://img.shields.io/badge/WebAssembly-compatible-green.svg)](https://webassembly.org/)
[![Native](https://img.shields.io/badge/Native-Node.js-green.svg)](https://nodejs.org/)

## What is Reflow?

Reflow is a modular, high-performance workflow execution engine that uses the **actor model** for concurrent, message-passing computation. This npm package provides a unified interface that automatically selects the best implementation:

- **Native bindings** for Node.js environments (optimal performance)
- **WebAssembly** for browsers and when native bindings are unavailable (universal compatibility)

### Key Features

🎭 **Actor-Based Architecture** - Isolated, concurrent actors communicate via message passing  
📊 **Visual Workflows** - Graph-based workflow representation with history and undo  
⚡ **High Performance** - Rust-powered execution compiled to WebAssembly  
🌐 **Cross-Platform** - Works in browsers and Node.js  
🔄 **Real-Time Processing** - Built-in event system for live data streams  
📦 **Extensible** - Create custom actors in JavaScript  

## Installation

```bash
npm install @offbit-ai/reflow
```

## Quick Start

### Basic Example

```javascript
import init, { Network, Graph, GraphNetwork } from '@offbit-ai/reflow';

// Initialize the WASM module
await init();

// Create a simple network
const network = new Network();

// Define a custom actor
class SimpleActor {
  constructor() {
    this.inports = ["input"];
    this.outports = ["output"];
  }

  run(context) {
    const data = context.input.input;
    const result = data * 2;
    context.send({ output: result });
  }
}

// Register and use the actor
network.registerActor("SimpleActor", new SimpleActor());
network.addNode("processor", "SimpleActor");

// Start the network
await network.start();

// Send data to the actor
network.sendToActor("processor", "input", 21);
```

### Graph-Based Workflow

```javascript
import init, { Graph, GraphNetwork } from '@offbit-ai/reflow';

await init();

// Create a graph
const graph = new Graph("MyWorkflow", true, {
  description: "A data processing pipeline"
});

// Add nodes (actors) to the graph
graph.addNode("generator", "GeneratorActor", { x: 100, y: 100 });
graph.addNode("processor", "ProcessorActor", { x: 300, y: 100 });
graph.addNode("logger", "LoggerActor", { x: 500, y: 100 });

// Connect the nodes
graph.addConnection("generator", "output", "processor", "input", {});
graph.addConnection("processor", "output", "logger", "input", {});

// Add initial data to trigger the pipeline
graph.addInitial({ value: 10 }, "generator", "trigger", {});

// Create a network from the graph
const network = new GraphNetwork(graph);

// Register actor implementations
network.registerActor("GeneratorActor", generatorActor);
network.registerActor("ProcessorActor", processorActor);
network.registerActor("LoggerActor", loggerActor);

// Start the network
await network.start();
```

## Actor Development

### Creating Custom Actors

Actors are the building blocks of Reflow workflows. Each actor has input ports, output ports, and a processing function.

```javascript
class CustomActor {
  constructor() {
    this.inports = ["input", "config"];
    this.outports = ["output", "error"];
    this.state = null; // Will be injected by the framework
  }

  run(context) {
    try {
      // Access input data
      const inputData = context.input.input;
      const config = context.input.config || {};
      
      // Access and update state
      const counter = context.state.get("counter") || 0;
      context.state.set("counter", counter + 1);
      
      // Process the data
      const result = this.processData(inputData, config);
      
      // Send output
      context.send({ 
        output: { 
          result, 
          processedCount: counter + 1 
        } 
      });
    } catch (error) {
      // Send error to error port
      context.send({ 
        error: { 
          message: error.message,
          timestamp: Date.now()
        } 
      });
    }
  }
  
  processData(data, config) {
    // Your processing logic here
    return data;
  }
}
```

### Stateful Actors

Actors can maintain state across invocations:

```javascript
import { MemoryState } from '@offbit-ai/reflow';

class StatefulActor {
  constructor() {
    this.inports = ["input"];
    this.outports = ["output"];
  }

  run(context) {
    // Access persistent state
    const count = context.state.get("count") || 0;
    const history = context.state.get("history") || [];
    
    // Update state
    context.state.set("count", count + 1);
    history.push(context.input.input);
    context.state.set("history", history);
    
    // Send output with state info
    context.send({
      output: {
        currentCount: count + 1,
        historyLength: history.length,
        lastValue: context.input.input
      }
    });
  }
}
```

## Advanced Features

### Graph History and Undo/Redo

```javascript
import { Graph, GraphHistory } from '@offbit-ai/reflow';

// Create a graph with history support
const [graph, history] = Graph.withHistoryAndLimit(50);

// Make changes to the graph
graph.addNode("node1", "Actor1", {});
graph.addConnection("node1", "out", "node2", "in", {});

// Process events to update history
history.processEvents(graph);

// Undo/Redo operations
if (history.getState().can_undo) {
  history.undo(graph);
}

if (history.getState().can_redo) {
  history.redo(graph);
}
```

### Network Events

```javascript
// Subscribe to network events
network.next((event) => {
  switch(event._type) {
    case "ActorStarted":
      console.log(`Actor ${event.actorId} started`);
      break;
    case "ActorCompleted":
      console.log(`Actor ${event.actorId} completed`);
      break;
    case "MessageSent":
      console.log(`Message sent from ${event.fromActor} to ${event.toActor}`);
      break;
    case "ActorFailed":
      console.error(`Actor ${event.actorId} failed: ${event.error}`);
      break;
  }
});
```

### Direct Actor Execution

```javascript
// Execute an actor directly and get the result
const result = await network.executeActor("processor", {
  command: "process",
  data: [1, 2, 3, 4, 5]
});

console.log("Execution result:", result);
```

## API Reference

### Core Classes

- **`Graph`** - Represents a workflow as a directed graph
- **`GraphNetwork`** - Executes a graph-based workflow
- **`Network`** - Lower-level network for direct actor management
- **`GraphHistory`** - Manages undo/redo operations on graphs
- **`MemoryState`** - State management for actors

### Actor Context API

The `context` object passed to actor's `run` method provides:

- **`context.input`** - Input data from all inports
- **`context.state`** - Persistent state management
- **`context.config`** - Actor configuration
- **`context.send(messages)`** - Send messages to outports

### Graph Operations

- **`addNode(id, component, metadata)`** - Add an actor node
- **`addConnection(from, fromPort, to, toPort, metadata)`** - Connect nodes
- **`addInitial(data, node, port, metadata)`** - Add initial data
- **`removeNode(id)`** - Remove a node
- **`removeConnection(from, fromPort, to, toPort)`** - Remove connection

## Examples

For complete working examples, see:
- [Comprehensive Test](https://github.com/offbit-ai/reflow/blob/main/crates/reflow_network/example/comprehensive_test.js)
- [Actor Example](https://github.com/offbit-ai/reflow/blob/main/crates/reflow_network/example/actor_example.js)

## Performance

Reflow compiled to WebAssembly provides:
- High-performance actor execution
- Efficient message passing
- Low memory overhead
- Near-native performance for compute-intensive tasks

## Browser Compatibility

Works in all modern browsers that support WebAssembly:
- Chrome 57+
- Firefox 52+
- Safari 11+
- Edge 79+

## Node.js Compatibility

Requires Node.js 12.20+ with WebAssembly support.

## License

MIT License - see [LICENSE](https://github.com/offbit-ai/reflow/blob/main/LICENSE) file for details.

## Support

- GitHub Issues: https://github.com/offbit-ai/reflow/issues
- Documentation: https://github.com/offbit-ai/reflow/tree/main/docs

## Contributing

We welcome contributions! Please see our [Contributing Guide](https://github.com/offbit-ai/reflow/blob/main/CONTRIBUTING.md) for details.