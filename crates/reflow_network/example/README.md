# Reflow Network WASM Examples

This directory contains comprehensive examples and tests for the Reflow Network WASM bindings.

## Files Overview

### Core Examples
- **`comprehensive_test.js`** - Complete test suite demonstrating all major features
- **`comprehensive_test.html`** - Interactive web interface for running tests
- **`actor_example.js`** - Basic actor system example
- **`actor_example.html`** - Simple web demo for actor examples

### Generated WASM Bindings
- **`../pkg/`** - Generated WASM bindings directory
  - `reflow_network.js` - JavaScript bindings
  - `reflow_network.d.ts` - TypeScript definitions
  - `reflow_network_bg.wasm` - WebAssembly binary

## Comprehensive Test Suite

The comprehensive test suite (`comprehensive_test.js`) demonstrates:

### 1. 🏗️ Graph Creation
- Creating graphs with metadata and properties
- Adding nodes with positioning and descriptions
- Establishing connections between nodes
- Setting up graph-level inports and outports
- Graph serialization and export functionality

### 2. 🎭 Stateful Actors
Four different actor types showcasing state management:

#### GeneratorActor
- Produces sequential numbers with configurable limits
- Maintains counter state and generation history
- Demonstrates state persistence across invocations

#### AccumulatorActor
- Calculates running totals and averages
- Tracks all processed values in state
- Shows precision-controlled mathematical operations

#### FilterActor
- Filters values based on configurable criteria
- Maintains separate counts for passed/filtered items
- Supports multiple filter types (range, even/odd)

#### LoggerActor
- Logs all messages with timestamps and metadata
- Maintains message history with configurable limits
- Tracks system uptime and performance metrics

### 3. 🚀 Network Composition
- Actor registration and binding to graph nodes
- Network startup with async/await support
- Real-time event monitoring and handling
- Graceful shutdown procedures

### 4. 📚 Advanced Features
- Graph history with undo/redo capabilities
- State persistence and recovery mechanisms
- Comprehensive error handling and reporting
- Performance monitoring and statistics

## Running the Tests

### Option 1: Web Browser (Recommended)

1. **Build the WASM bindings** (if not already done):
   ```bash
   cd crates/reflow_network
   wasm-pack build --target web --out-dir pkg
   ```

2. **Serve the files** using a local web server:
   ```bash
   # Using Python 3
   python -m http.server 8000
   
   # Using Node.js (if you have http-server installed)
   npx http-server -p 8000
   
   # Using any other static file server
   ```

3. **Open in browser**:
   ```
   http://localhost:8000/example/comprehensive_test.html
   ```

4. **Run the tests** by clicking the "Run Comprehensive Test" button

### Option 2: Node.js Environment

1. **Install dependencies** (if using Node.js modules):
   ```bash
   npm install
   ```

2. **Run the test directly**:
   ```bash
   node --experimental-modules comprehensive_test.js
   ```

### Option 3: Manual Testing

You can also import and run individual components:

```javascript
import { 
  runComprehensiveTest,
  GeneratorActor,
  AccumulatorActor,
  createProcessingGraph 
} from './comprehensive_test.js';

// Run the full test suite
const result = await runComprehensiveTest();

// Or test individual components
const graph = createProcessingGraph();
const actor = new GeneratorActor();
```

## Expected Output

When running successfully, you should see:

### Console Output
```
🧪 Starting Comprehensive Reflow Network Test
============================================================

1️⃣  TESTING GRAPH CREATION
🏗️  Creating Processing Graph...
📦 Adding nodes to graph...
🔗 Adding connections...
✅ Graph created successfully!
   - Nodes: 4
   - Connections: 3

2️⃣  TESTING NETWORK COMPOSITION & EXECUTION
🚀 Creating and Starting Network...
🎭 Registering actors...
🎬 Starting network...
✅ Network started successfully!

⏳ Running pipeline for 5 seconds...
🔢 Generator: Produced 1 (1/10)
📊 Accumulator: Added 1, Total: 1, Avg: 1.00
✅ Filter: PASSED 1 (1 total passed)
📝 Logger: Message #1 after 45ms - {"value":1,"reason":"Meets range criteria","passedCount":1}

[... more processing output ...]

🛑 Stopping network...
📊 Final Statistics:
   - Total network events: 42
   - Pipeline completed successfully

3️⃣  TESTING GRAPH HISTORY & STATE MANAGEMENT
📚 Demonstrating Graph History...
[... history operations ...]

4️⃣  TESTING ACTOR STATE INSPECTION
Note: Actor states would be inspected during network execution
Each actor maintains its own MemoryState with persistent data

============================================================
✅ ALL TESTS COMPLETED SUCCESSFULLY!
🎉 Reflow Network WASM bindings are working correctly!
```

### Web Interface
- Interactive progress bar showing test execution
- Real-time console output with timestamps
- Visual status indicators (success/error/running)
- Detailed results summary with statistics
- Clean, responsive UI with emoji indicators

## Troubleshooting

### Common Issues

1. **WASM Module Not Found**
   - Ensure you've built the WASM bindings with `wasm-pack build`
   - Check that the `pkg/` directory exists and contains the generated files

2. **CORS Errors in Browser**
   - Use a local web server instead of opening HTML files directly
   - Ensure all files are served from the same origin

3. **Import/Export Errors**
   - Make sure you're using a modern browser that supports ES6 modules
   - Check that the file paths in import statements are correct

4. **Network Startup Failures**
   - Verify that all actor classes are properly registered
   - Check the browser console for detailed error messages

### Performance Notes

- The test suite runs for 5 seconds by default to demonstrate the pipeline
- You can modify the timing in the `createAndRunNetwork()` function
- Actor state is maintained in memory and will be lost on page refresh
- For production use, consider implementing persistent state storage

## Extending the Examples

### Adding New Actors

1. Create a new actor class following the pattern:
   ```javascript
   class MyActor {
     constructor() {
       this.inports = ["input"];
       this.outports = ["output"];
       this.state = new MemoryState();
       this.config = { /* your config */ };
     }
     
     run(input, send) {
       // Process input and update state
       // Send output using send()
     }
   }
   ```

2. Register the actor in the network:
   ```javascript
   network.registerActor("MyActor", MyActor);
   ```

3. Add nodes to the graph:
   ```javascript
   graph.addNode("myNode", "MyActor", { x: 100, y: 100 });
   ```

### Modifying the Pipeline

You can easily modify the data processing pipeline by:
- Changing actor configurations
- Adding new connections between actors
- Implementing different filtering or processing logic
- Adding new graph nodes and connections

## API Reference

For detailed API documentation, see:
- `../pkg/reflow_network.d.ts` - TypeScript definitions
- The main Reflow documentation
- Inline comments in the example files

## Contributing

When adding new examples:
1. Follow the existing code style and patterns
2. Include comprehensive error handling
3. Add detailed console logging for debugging
4. Update this README with new features
5. Test in both browser and Node.js environments
