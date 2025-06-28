# Browser Deployment Guide

Learn how to deploy Reflow workflows in web browsers using WebAssembly (WASM) bindings.

## Overview

Reflow provides complete WebAssembly bindings that allow you to run actor-based workflows directly in web browsers. This enables:

- **Interactive workflow editors** with real-time visualization
- **Client-side data processing** without server dependencies
- **Hybrid applications** combining browser UI with Rust performance
- **Educational tools** for learning workflow concepts

## Quick Start

### 1. Build WASM Bindings

First, build the WebAssembly bindings using `wasm-pack`:

```bash
# Install wasm-pack if you haven't already
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Navigate to the reflow_network crate
cd crates/reflow_network

# Build the WASM bindings for web
wasm-pack build --target web --out-dir pkg
```

This generates the `pkg/` directory with:
- `reflow_network.js` - JavaScript bindings
- `reflow_network.d.ts` - TypeScript definitions  
- `reflow_network_bg.wasm` - WebAssembly binary

### 2. Create a Basic HTML Page

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Reflow Browser Example</title>
</head>
<body>
    <h1>Reflow in the Browser</h1>
    <button id="runWorkflow">Run Workflow</button>
    <div id="output"></div>

    <script type="module">
        import init, { 
            Network, 
            Graph, 
            MemoryState,
            init_panic_hook 
        } from './pkg/reflow_network.js';

        // Initialize WASM
        await init();
        init_panic_hook();

        // Your workflow code here
        console.log('Reflow WASM loaded successfully!');
    </script>
</body>
</html>
```

### 3. Serve with a Local Server

WASM requires files to be served via HTTP (not `file://`):

```bash
# Using Python 3
python -m http.server 8000

# Using Node.js
npx http-server -p 8000

# Using any other static file server
```

Navigate to `http://localhost:8000` to view your application.

## Core WASM API

### Initialization

```javascript
import init, { 
    Network, 
    Graph, 
    GraphNetwork,
    GraphHistory,
    MemoryState,
    WasmActorContext,
    JsWasmActor,
    ActorRunContext,
    init_panic_hook 
} from './pkg/reflow_network.js';

// Initialize WASM module
await init();

// Set up better error reporting
init_panic_hook();
```

### Creating Actors for Browser

Actors in the browser use a simplified JavaScript interface:

```javascript
class MyActor {
    constructor() {
        this.inports = ["input"];
        this.outports = ["output"];
        this.state = null; // Managed by WASM bridge
        this.config = { /* actor configuration */ };
    }

    /**
     * Main actor execution method
     * @param {ActorRunContext} context - Execution context
     */
    run(context) {
        // Access input data
        const inputData = context.input.input;
        
        // Read/write state
        const currentCount = context.state.get('count') || 0;
        context.state.set('count', currentCount + 1);
        
        // Process data
        const result = {
            processed: inputData,
            count: currentCount + 1,
            timestamp: Date.now()
        };
        
        // Send output
        context.send({ output: result });
    }
}
```

### Graph Creation

Create graphs with visual positioning for browser-based editors:

```javascript
// Create a new graph
const graph = new Graph("MyWorkflow", true, {
    description: "A browser-based workflow",
    version: "1.0.0"
});

// Add nodes with positioning
graph.addNode("generator", "GeneratorActor", {
    x: 100, y: 100,
    description: "Generates data"
});

graph.addNode("processor", "ProcessorActor", {
    x: 300, y: 100,
    description: "Processes data"
});

// Add connections
graph.addConnection("generator", "output", "processor", "input", {
    label: "Data flow",
    color: "#4CAF50"
});

// Add initial data
graph.addInitial({ start: true }, "generator", "trigger");

// Add graph-level ports
graph.addInport("start", "generator", "trigger", { type: "flow" });
graph.addOutport("results", "processor", "output", { type: "object" });
```

### Network Composition

Create and run networks in the browser:

```javascript
// Create network from graph
const network = new GraphNetwork(graph);

// Register actor implementations
network.registerActor("GeneratorActor", new GeneratorActor());
network.registerActor("ProcessorActor", new ProcessorActor());

// Set up event monitoring
let eventCount = 0;
network.next((event) => {
    eventCount++;
    console.log(`Event #${eventCount}:`, {
        type: event._type,
        actor: event.actorId,
        port: event.port,
        hasData: !!event.data
    });
});

// Start the network
await network.start();

// The network will now process data according to your graph
```

### State Management

Share state between JavaScript and Rust:

```javascript
// Create a memory state
const state = new MemoryState();

// Set values
state.set("counter", 42);
state.set("message", "Hello from JavaScript");
state.set("config", { enabled: true, level: "debug" });

// Get values
const counter = state.get("counter");
const message = state.get("message");

// Check existence
if (state.has("config")) {
    console.log("Config exists");
}

// Get all state as an object
const allState = state.getAll();

// Clear state
state.clear();

// Get state size
const size = state.size();
```

## Advanced Features

### Graph History with Undo/Redo

```javascript
// Create graph with history support
const [graph, history] = Graph.withHistoryAndLimit(50);

// Make changes to the graph
graph.addNode("newNode", "MyActor", { x: 200, y: 200 });

// Process events to update history
history.processEvents(graph);

// Check if undo/redo is available
const state = history.getState();
console.log("Can undo:", state.can_undo);
console.log("Can redo:", state.can_redo);

// Perform undo/redo operations
if (state.can_undo) {
    history.undo(graph);
}

if (history.getState().can_redo) {
    history.redo(graph);
}
```

### Real-time Event Monitoring

```javascript
// Set up comprehensive event monitoring
network.next((event) => {
    switch (event._type) {
        case "FlowTrace":
            console.log(`Flow: ${event.from.actorId}:${event.from.port} → ${event.to.actorId}:${event.to.port}`);
            break;
        case "ActorStarted":
            console.log(`Actor started: ${event.actorId}`);
            break;
        case "ActorStopped":
            console.log(`Actor stopped: ${event.actorId}`);
            break;
        case "NetworkStarted":
            console.log("Network started");
            break;
        case "NetworkStopped":
            console.log("Network stopped");
            break;
        default:
            console.log("Other event:", event);
    }
});
```

### Direct Actor Execution

Execute actors directly for testing:

```javascript
// Execute an actor and get results
const result = await network.executeActor("myActor", {
    command: "process",
    data: { value: 100 }
});

console.log("Execution result:", result);
```

## Deployment Considerations

### 1. File Serving

- **CORS**: Ensure proper CORS headers if serving from different domains
- **MIME Types**: Configure server to serve `.wasm` files with correct MIME type
- **Compression**: Enable gzip/brotli compression for WASM files

### 2. Bundle Size Optimization

```bash
# Build optimized release version
wasm-pack build --target web --release --out-dir pkg

# Further optimization with wee_alloc (add to Cargo.toml)
[dependencies]
wee_alloc = "0.4"

# In your lib.rs
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
```

### 3. Loading Strategies

```javascript
// Lazy loading for large applications
async function loadReflowWhenNeeded() {
    const { default: init, Network } = await import('./pkg/reflow_network.js');
    await init();
    return { Network };
}

// Progressive loading with loading indicators
function showLoadingIndicator() {
    document.getElementById('loading').style.display = 'block';
}

function hideLoadingIndicator() {
    document.getElementById('loading').style.display = 'none';
}

showLoadingIndicator();
await init();
hideLoadingIndicator();
```

### 4. Error Handling

```javascript
try {
    await init();
    init_panic_hook();
    
    // Your workflow code
    const network = new Network();
    await network.start();
    
} catch (error) {
    console.error("WASM initialization or execution failed:", error);
    
    // Show user-friendly error message
    document.getElementById('error').textContent = 
        "Failed to load workflow engine. Please refresh the page.";
}
```

## Browser Compatibility

### Supported Browsers

- **Chrome/Edge**: 57+ (full WebAssembly support)
- **Firefox**: 52+ (full WebAssembly support)  
- **Safari**: 11+ (full WebAssembly support)
- **Mobile**: iOS 11+, Android Chrome 57+

### Feature Detection

```javascript
function checkWebAssemblySupport() {
    return typeof WebAssembly === 'object' 
        && typeof WebAssembly.instantiate === 'function';
}

if (!checkWebAssemblySupport()) {
    alert('Your browser does not support WebAssembly. Please update to a modern browser.');
}
```

## Performance Tips

### 1. Memory Management

```javascript
// Clean up resources when done
network.shutdown();

// Clear large state objects
state.clear();

// Avoid memory leaks in event listeners
const unsubscribe = network.next(handleEvent);
// Later: unsubscribe();
```

### 2. Batch Operations

```javascript
// Batch multiple graph modifications
graph.addNode("node1", "Actor1", { x: 100, y: 100 });
graph.addNode("node2", "Actor2", { x: 200, y: 100 });
graph.addConnection("node1", "output", "node2", "input");

// Process all changes at once
history.processEvents(graph);
```

### 3. Efficient Data Passing

```javascript
// Prefer structured data over strings
const efficientData = { 
    type: "sensor_reading",
    value: 42.5,
    timestamp: Date.now()
};

// Avoid large JSON strings
const inefficientData = JSON.stringify(largeObject);
```

## Troubleshooting

### Common Issues

1. **WASM Module Not Found**
   - Ensure `pkg/` directory exists and contains generated files
   - Check file paths in import statements
   - Verify files are served from the same origin

2. **CORS Errors**
   - Use a local web server instead of opening files directly
   - Configure proper CORS headers if needed

3. **Import/Export Errors**
   - Use a modern browser with ES6 module support
   - Check that all imports are correctly spelled
   - Ensure `type="module"` in script tags

4. **Network Startup Failures**
   - Verify all actors are properly registered
   - Check browser console for detailed error messages
   - Ensure graph structure is valid before starting network

### Debug Mode

```javascript
// Enable debug logging
console.log("Network actors:", network.getActorNames());
console.log("Active actors:", network.getActiveActors());
console.log("Actor count:", network.getActorCount());

// Export graph for inspection
const graphData = graph.toJSON();
console.log("Graph structure:", JSON.stringify(graphData, null, 2));
```

## Next Steps

- **[WASM API Reference](../api/wasm/getting-started.md)** - Detailed API documentation
- **[Browser Actors Tutorial](../tutorials/wasm-actor-development.md)** - Building actors for browser
- **[Interactive Graph Editor](../tutorials/browser-workflow-editor.md)** - Creating visual workflow editors
- **[State Management Guide](../api/wasm/state-management.md)** - Advanced state handling

The browser deployment of Reflow opens up exciting possibilities for client-side workflow automation, interactive data processing, and educational applications. Start with the examples above and explore the comprehensive API documentation for advanced usage.
