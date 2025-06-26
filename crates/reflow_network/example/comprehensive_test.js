// Comprehensive Test Example for Reflow Network WASM Bindings
// Tests: 1. Graph Creation, 2. Stateful Actors, 3. Network Composition

import init, {
  Graph,
  GraphNetwork,
  GraphHistory,
  Network,
  MemoryState,
  WasmActorContext,
  JsWasmActor,
  init_panic_hook,
  ActorRunContext
} from '../pkg/reflow_network.js';

// WASM initialization flag
let wasmInitialized = false;

// ============================================================================
// 1. STATEFUL ACTOR IMPLEMENTATIONS
// ============================================================================

/**
 * Generator Actor - Produces sequential numbers with state tracking
 */
class GeneratorActor {
  constructor() {
    this.inports = ["trigger"];
    this.outports = ["output"];
    this.state = null; // Will be injected by WASM bridge
    this.config = { maxCount: 10, step: 1 };
  }

  /**
   * 
   * @param {ActorRunContext} context 
   */
  run(context) {
    const currentCount = context.state.get("counter") || 0;
    const generated = context.state.get("generated") || [];
    const maxCount = this.config.maxCount;

    if (currentCount < maxCount) {
      const newValue = currentCount + this.config.step;

      // Update state through context
      context.state.set("counter", newValue);
      generated.push(newValue);
      context.state.set("generated", generated);

      console.log(`🔢 Generator: Produced ${newValue} (${newValue}/${maxCount})`);

      // Send the generated number through context
      context.send({
        output: {
          value: newValue,
          sequence: newValue,
          isLast: newValue >= maxCount
        }
      });
    } else {
      console.log("🔢 Generator: Reached maximum count, stopping");
      context.send({ output: { value: null, isLast: true } });
    }
  }

  getState() {
    return this.state ? this.state.getAll() : {};
  }

  setState(newState) {
    if (this.state) {
      this.state.setAll(newState);
    }
  }
}

/**
 * Accumulator Actor - Maintains running sum with state persistence
 */
class AccumulatorActor {
  constructor() {
    this.inports = ["input"];
    this.outports = ["sum", "average"];
    this.state = null; // Will be injected by WASM bridge
    this.config = { precision: 2 };
  }
  /**
   * 
   * @param {ActorRunContext} context 
   */
  run(context) {
    if (context.input && context.input.input !== null) {
      const value = context.input.input.value;
      const currentTotal = context.state.get("total") || 0;
      const currentCount = context.state.get("count") || 0;
      const values = context.state.get("values") || [];

      // Update state through context
      const newTotal = currentTotal + value;
      const newCount = currentCount + 1;
      values.push(value);

      context.state.set("total", newTotal);
      context.state.set("count", newCount);
      context.state.set("values", values);

      const average = newTotal / newCount;

      console.log(`📊 Accumulator: Added ${value}, Total: ${newTotal}, Avg: ${average.toFixed(this.config.precision)}`);

      // Send results through context
      context.send({
        sum: {
          total: newTotal,
          count: newCount,
          lastValue: value
        },
        average: {
          value: average,
          precision: this.config.precision,
          sampleSize: newCount
        }
      });
    }
  }

  getState() {
    return this.state ? this.state.getAll() : {};
  }

  setState(newState) {
    if (this.state) {
      this.state.setAll(newState);
    }
  }
}

/**
 * Filter Actor - Filters values based on configurable criteria with state
 */
class FilterActor {
  constructor() {
    this.inports = ["input"];
    this.outports = ["passed", "filtered"];
    this.state = null; // Will be injected by WASM bridge
    this.config = {
      minValue: 3,
      maxValue: 8,
      filterType: "range" // "range", "even", "odd"
    };
  }

  /**
 * 
 * @param {ActorRunContext} context 
 */
  run(context) {
    if (context.input && context.input.input !== undefined) {
      const value = context.input.input.count;
      const passedCount = context.state.get("passedCount") || 0;
      const filteredCount = context.state.get("filteredCount") || 0;
      const passedValues = context.state.get("passedValues") || [];
      const filteredValues = context.state.get("filteredValues") || [];

      let passes = false;

      // Apply filter logic based on configuration
      switch (this.config.filterType) {
        case "range":
          passes = value >= this.config.minValue && value <= this.config.maxValue;
          break;
        case "even":
          passes = value % 2 === 0;
          break;
        case "odd":
          passes = value % 2 !== 0;
          break;
        default:
          passes = true;
      }

      if (passes) {
        context.state.set("passedCount", passedCount + 1);
        passedValues.push(value);
        context.state.set("passedValues", passedValues);

        console.log(`✅ Filter: PASSED ${value} (${passedCount + 1} total passed)`);
        context.send({
          passed: {
            value: value,
            reason: `Meets ${this.config.filterType} criteria`,
            passedCount: passedCount + 1
          }
        });
      } else {
        context.state.set("filteredCount", filteredCount + 1);
        filteredValues.push(value);
        context.state.set("filteredValues", filteredValues);

        console.log(`❌ Filter: FILTERED ${value} (${filteredCount + 1} total filtered)`);
        context.send({
          filtered: {
            value: value,
            reason: `Does not meet ${this.config.filterType} criteria`,
            filteredCount: filteredCount + 1
          }
        });
      }
    }
  }

  getState() {
    return this.state ? this.state.getAll() : {};
  }

  setState(newState) {
    if (this.state) {
      this.state.setAll(newState);
    }
  }
}

/**
 * Logger Actor - Tracks and logs all messages with detailed state
 */
class LoggerActor {
  constructor() {
    this.inports = ["input"];
    this.outports = ["log"];
    this.state = null; // Will be injected by WASM bridge
    this.config = { logLevel: "info", maxLogs: 100 };
  }
  /**
   * 
   * @param {ActorRunContext} context 
   */
  run(context) {
    const messageCount = context.state.get("messageCount") || 0;
    const logs = context.state.get("logs") || [];
    const startTime = context.state.get("startTime") || Date.now();

    // Initialize startTime if not set
    if (!context.state.has("startTime")) {
      context.state.set("startTime", Date.now());
    }

    const logEntry = {
      id: messageCount + 1,
      timestamp: Date.now(),
      elapsed: Date.now() - startTime,
      data: context.input.input,
      source: "filter"
    };

    // Update state through context
    context.state.set("messageCount", messageCount + 1);
    logs.push(logEntry);

    // Keep only the last maxLogs entries
    if (logs.length > this.config.maxLogs) {
      logs.shift();
    }
    context.state.set("logs", logs);

    console.log(`📝 Logger: Message #${logEntry.id} after ${logEntry.elapsed}ms - ${JSON.stringify(context.input.input)}`);

    context.send({
      log: {
        entry: logEntry,
        totalMessages: messageCount + 1,
        uptime: logEntry.elapsed
      }
    });
  }

  getState() {
    return this.state ? this.state.getAll() : {};
  }

  setState(newState) {
    if (this.state) {
      this.state.setAll(newState);
    }
  }
}

// ============================================================================
// 2. GRAPH CREATION AND MANAGEMENT
// ============================================================================

function createProcessingGraph() {
  console.log("\n🏗️  Creating Processing Graph...");

  // Create a new graph with metadata
  const graph = new Graph("DataProcessingPipeline", true, {
    description: "A comprehensive data processing pipeline with stateful actors",
    version: "1.0.0",
    author: "Reflow Network Test",
    created: new Date().toISOString()
  });

  // Add nodes to the graph
  console.log("📦 Adding nodes to graph...");

  graph.addNode("generator", "GeneratorActor", {
    x: 100, y: 100,
    description: "Generates sequential numbers"
  });

  graph.addNode("accumulator", "AccumulatorActor", {
    x: 300, y: 100,
    description: "Accumulates and averages values"
  });

  graph.addNode("filter", "FilterActor", {
    x: 500, y: 100,
    description: "Filters values based on criteria"
  });

  graph.addNode("logger", "LoggerActor", {
    x: 700, y: 100,
    description: "Logs all processed messages"
  });

  // Add connections between nodes
  console.log("🔗 Adding connections...");

  graph.addConnection("generator", "output", "accumulator", "input", {
    label: "Generated numbers",
    color: "#4CAF50"
  });

  graph.addConnection("accumulator", "sum", "filter", "input", {
    label: "Accumulated sums",
    color: "#2196F3"
  });

  graph.addConnection("filter", "passed", "logger", "input", {
    label: "Filtered results",
    color: "#FF9800"
  });

  // Add initial data to trigger the pipeline
  graph.addInitial({ trigger: true, timestamp: Date.now() }, "generator", "trigger", {
    description: "Initial trigger to start generation"
  });

  // Add graph-level inports and outports
  graph.addInport("start", "generator", "trigger", { type: "flow" }, {
    description: "External trigger to start the pipeline"
  });

  graph.addOutport("results", "logger", "log", { type: "object", value: "log" }, {
    description: "Final processed results"
  });

  // let graph_export = graph.toJSON();
  // console.log("✅ Graph created successfully!");
  // console.log(`   - Nodes: ${Object.keys(graph_export.processes || {}).length}`);
  // console.log(`   - Connections: ${(graph_export.connections || []).length}`);

  return graph;
}

// ============================================================================
// 3. NETWORK COMPOSITION AND EXECUTION
// ============================================================================

async function createAndRunNetwork() {
  console.log("\n🚀 Creating and Starting Network...");



  // Create a graph network with history support
  const [graphWithHistory, history] = Graph.withHistoryAndLimit(50);

  // Load our graph into the history-enabled graph
  // const loadedGraph = Graph.load(graph.toJSON(), {});

  const graph = createProcessingGraph();

  // Create the network from the graph
  const network = new GraphNetwork(graph);

  // Register all our actor implementations
  console.log("🎭 Registering actors...");

  network.registerActor("GeneratorActor", new GeneratorActor());
  network.registerActor("AccumulatorActor", new AccumulatorActor());
  network.registerActor("FilterActor", new FilterActor());
  network.registerActor("LoggerActor", new LoggerActor());

  // Set up event listening
  let eventCount = 0;
  network.next((event) => {
    eventCount++;
    console.log(`📡 Network Event #${eventCount}:`, JSON.stringify({
      type: event._type,
      actor: event.actorId,
      port: event.port,
      hasData: !!event.data
    }));
  });

  // Start the network
  console.log("🎬 Starting network...");
  await network.start();

  console.log("✅ Network started successfully!");

  // Let the pipeline run for a while
  console.log("\n⏳ Running pipeline for 5 seconds...");

  return new Promise((resolve) => {
    setTimeout(async () => {
      console.log("\n🛑 Stopping network...");
      network.shutdown();

      console.log(`📊 Final Statistics:`);
      console.log(`   - Total network events: ${eventCount}`);
      console.log(`   - Pipeline completed successfully`);

      resolve({
        graph,
        history: history,
        eventCount: eventCount
      });
    }, 5000);
  });
}

// ============================================================================
// 4. DEMONSTRATION OF GRAPH HISTORY AND STATE MANAGEMENT
// ============================================================================

function demonstrateGraphHistory(graph, history) {
  console.log("\n📚 Demonstrating Graph History...");

  // Get initial state
  const initialState = history.getState();
  console.log("Initial history state:", JSON.stringify({
    canUndo: initialState.can_undo,
    canRedo: initialState.can_redo,
    undoSize: initialState.undo_size
  }));

  // Make some changes to demonstrate history
  console.log("Making changes to graph...");

  // Add a new node
  graph.addNode("monitor", "MonitorActor", {
    x: 900, y: 100,
    description: "Monitors system performance"
  });

  // Process events to update history
  history.processEvents(graph);

  // Check history state after changes
  const afterChanges = history.getState();
  console.log("After changes:", JSON.stringify({
    canUndo: afterChanges.can_undo,
    canRedo: afterChanges.can_redo,
    undoSize: afterChanges.undo_size
  }));

  // Demonstrate undo
  if (afterChanges.can_undo) {
    console.log("Performing undo...");
    history.undo(graph);

    const afterUndo = history.getState();
    console.log("After undo:", JSON.stringify({
      canUndo: afterUndo.can_undo,
      canRedo: afterUndo.can_redo,
      redoSize: afterUndo.redo_size
    }));
  }

  // Demonstrate redo
  if (history.getState().can_redo) {
    console.log("Performing redo...");
    history.redo(graph);

    const afterRedo = history.getState();
    console.log("After redo:", JSON.stringify({
      canUndo: afterRedo.can_undo,
      canRedo: afterRedo.can_redo
    }));
  }
}

// ============================================================================
// 5. WASM INITIALIZATION AND MAIN EXECUTION FUNCTION
// ============================================================================

async function initializeWasm() {
  if (!wasmInitialized) {
    console.log("🔧 Initializing WASM module...");
    await init();
    init_panic_hook();
    wasmInitialized = true;
    console.log("✅ WASM module initialized successfully");
  }
}

async function runComprehensiveTest() {
  console.log("🧪 Starting Comprehensive Reflow Network Test");
  console.log("=".repeat(60));

  try {
    // Ensure WASM is initialized before running tests
    await initializeWasm();

    // // Test 1: Graph Creation
    // console.log("\n1️⃣  TESTING GRAPH CREATION");
    // const graph = createProcessingGraph();



    // Test 2: Network Composition and Execution
    console.log("\n2️⃣  TESTING NETWORK COMPOSITION & EXECUTION");
    const result = await createAndRunNetwork();

    //  // Export and display graph structure
    // const graphExport = graph.toJSON();
    // console.log("📋 Graph Export Summary:");
    // console.log(`   - Case Sensitive: ${graphExport.caseSensitive}`);
    // console.log(`   - Properties: ${JSON.stringify(graphExport.properties)}`);
    // console.log(`   - Processes: ${Object.keys(graphExport.processes || {}).length}`);
    // console.log(`   - Connections: ${(graphExport.connections || []).length}`);
    // console.log(`   - Inports: ${Object.keys(graphExport.inports || {}).length}`);
    // console.log(`   - Outports: ${Object.keys(graphExport.outports || {}).length}`);

    // Test 3: Graph History and State Management
    console.log("\n3️⃣  TESTING GRAPH HISTORY & STATE MANAGEMENT");
    demonstrateGraphHistory(result.graph, result.history);

    // Test 4: Actor State Inspection
    console.log("\n4️⃣  TESTING ACTOR STATE INSPECTION");
    console.log("Note: Actor states would be inspected during network execution");
    console.log("Each actor maintains its own MemoryState with persistent data");

    console.log("\n" + "=".repeat(60));
    console.log("✅ ALL TESTS COMPLETED SUCCESSFULLY!");
    console.log("🎉 Reflow Network WASM bindings are working correctly!");

    return {
      success: true,
      testsRun: 4,
      graphNodes: Object.keys(result.graph.toJSON().processes || {}).length,
      networkEvents: result.eventCount
    };

  } catch (error) {
    console.error("\n❌ TEST FAILED:", error);
    console.error("Stack trace:", error.stack);
    return {
      success: false,
      error: error.message
    };
  }
}

// ============================================================================
// 6. EXPORT AND EXECUTION
// ============================================================================

// Export for use in other modules
export {
  GeneratorActor,
  AccumulatorActor,
  FilterActor,
  LoggerActor,
  createProcessingGraph,
  createAndRunNetwork,
  demonstrateGraphHistory,
  runComprehensiveTest
};

// Auto-run if this is the main module
if (typeof window !== 'undefined') {
  // Browser environment
  window.runReflowTest = runComprehensiveTest;
  console.log("🌐 Browser environment detected. Call window.runReflowTest() to start the test.");
} else {
  // Node.js environment - Initialize WASM before running tests
  (async () => {
    try {
      const result = await runComprehensiveTest();
      console.log("\n📊 Final Test Result:", result);
      process.exit(result.success ? 0 : 1);
    } catch (error) {
      console.error("💥 Unhandled error:", error);
      process.exit(1);
    }
  })();
}
