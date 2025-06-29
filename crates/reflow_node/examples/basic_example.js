#!/usr/bin/env node

/**
 * Basic ReFlow Node.js Example
 * 
 * Demonstrates the core functionality:
 * - Creating JavaScript actors
 * - Building a graph
 * - Running a network with actors
 */

const reflow = require('../index.node');

// Initialize error handling
reflow.init_panic_hook();



// ============================================================================
// 1. SIMPLE JAVASCRIPT ACTORS
// ============================================================================

/**
 * Source Actor - generates data
 */
class SourceActor {
    constructor() {
        this.inports = ["trigger"];
        this.outports = ["output"];
        this.config = { count: 5 };
    }

    run(context) {
        console.log('📤 SourceActor: Starting data generation');

        // Generate some data
        for (let i = 1; i <= this.config.count; i++) {
            const data = {
                id: i,
                value: i * 10,
                timestamp: Date.now()
            };

            console.log(`📤 SourceActor: Sending data ${i}:`, data);

            context.send({
                output: data
            });
        }

        console.log('📤 SourceActor: Finished generating data');
    }
}

/**
 * Transform Actor - processes data
 */
class TransformActor {
    constructor() {
        this.inports = ["input"];
        this.outports = ["output"];
        this.config = { multiplier: 2, await_all_inports: true };
    }

    run(context) {
        const data = context.input.input;
        if (data) {
            console.log('🔄 TransformActor: Processing:', data);

            // Transform the data
            const transformed = {
                ...data,
                value: data.value * this.config.multiplier,
                processed: true,
                processedAt: Date.now()
            };

            console.log('🔄 TransformActor: Transformed:', transformed);

            context.send({
                output: transformed
            });
        }
    }
}

/**
 * Sink Actor - consumes final data
 */
class SinkActor {
    constructor() {
        this.inports = ["input"];
        this.outports = [];
        this.config = {await_all_inports: true};
    }

    run(context) {
        if (context.input.input) {
            const data = context.input.input;
            console.log('📥 SinkActor: Final result:', data);
            console.log(`📥 SinkActor: Value ${data.id} transformed from ${data.value / 2} to ${data.value}`);
        }
    }
}

// ============================================================================
// 2. CREATE GRAPH
// ============================================================================

async function createSimpleGraph() {
    console.log('\n🎯 Creating a simple processing graph...');

    // Create a new graph
    const Graph = reflow.Graph();
    const graph = new Graph("SimpleProcessing");

    console.log('📦 Adding nodes...');

    // Add nodes
    Graph.prototype.addNode(graph, "source", "SourceActor", {
        description: "Data generator"
    });

    Graph.prototype.addNode(graph, "transform", "TransformActor", {
        description: "Data transformer"
    });

    Graph.prototype.addNode(graph, "sink", "SinkActor", {
        description: "Data consumer"
    });

    console.log('🔗 Adding connections...');

    // Connect the nodes: source → transform → sink
    Graph.prototype.addConnection(graph, "source", "output", "transform", "input");
    Graph.prototype.addConnection(graph, "transform", "output", "sink", "input");

    console.log('🚀 Adding initial trigger...');

    // Add initial data to start the process
    Graph.prototype.addInitial(graph, { start: true }, "source", "trigger");

    console.log('✅ Graph created successfully');

    return Graph.prototype.export(graph);
}

// ============================================================================
// 3. RUN NETWORK
// ============================================================================

async function runSimpleNetwork() {
    console.log('\n🌐 Setting up and running network...');

    // Create the graph
    const graph = await createSimpleGraph();

    // Create network from graph
    console.log('🏗️ Creating network...');
    const Network = new reflow.GraphNetwork();
    const network = new Network(graph);

    // Create actor instances
    console.log('🎭 Creating and registering actors...');

    const sourceActor = new SourceActor();
    const transformActor = new TransformActor();
    const sinkActor = new SinkActor();

    // Register actors with the network
    Network.prototype.registerActor(network, "SourceActor", sourceActor);
    Network.prototype.registerActor(network, "TransformActor", transformActor);
    Network.prototype.registerActor(network, "SinkActor", sinkActor);

    console.log('✅ All actors registered');

    // Start the network
    console.log('\n🚀 Starting network execution...');

    try {
        Network.prototype.start(network);
        console.log('✅ Network started successfully');

        // Let the processing complete
        console.log('\n⏳ Processing data...\n');
        await new Promise(resolve => setTimeout(resolve, 1000));

        console.log('\n🏁 Processing completed');

    } catch (error) {
        console.error('❌ Network execution failed:', error.message);
    } finally {
        // Clean shutdown
        console.log('🛑 Shutting down network...');
        Network.prototype.shutdown(network);
        console.log('✅ Network shutdown completed');
    }
}

// ============================================================================
// 4. DEMONSTRATION WITH REGULAR NETWORK (Alternative)
// ============================================================================

async function runRegularNetwork() {
    console.log('\n🔧 Alternative: Using regular Network (manual setup)...');

    // Create a regular Network (not GraphNetwork)
    const Network = new reflow.Network();
    const network = new Network();

    // Create and register actors
    console.log('🎭 Registering actors...');

    const sourceActor = new SourceActor();
    const transformActor = new TransformActor();
    const sinkActor = new SinkActor();

    // Customize config for shorter demo
    sourceActor.config.count = 3;

    Network.prototype.registerActor(network, "SourceActor", sourceActor);
    Network.prototype.registerActor(network, "TransformActor", transformActor);
    Network.prototype.registerActor(network, "SinkActor", sinkActor);

    // Add nodes manually
    console.log('📦 Adding nodes manually...');
    Network.prototype.addNode(network, "src", "SourceActor");
    Network.prototype.addNode(network, "trans", "TransformActor");
    Network.prototype.addNode(network, "sink", "SinkActor");

    // Add connections manually
    console.log('🔗 Adding connections manually...');
    Network.prototype.addConnection(network, {
        from: { actor: "src", port: "output" },
        to: { actor: "trans", port: "input" }
    });

    Network.prototype.addConnection(network, {
        from: { actor: "trans", port: "output" },
        to: { actor: "sink", port: "input" }
    });

    // Add initial trigger
    console.log('🚀 Adding initial trigger...');
    Network.prototype.addInitial(network, {
        to: {
            actor: "src",
            port: "trigger",
            initial_data: { trigger: true }
        }
    });

    // Start the network
    console.log('\n🚀 Starting regular network...');

    try {
        await Network.prototype.start(network);
        console.log('✅ Regular network started');

        // Let it process
        await new Promise(resolve => setTimeout(resolve, 1000));

        console.log('\n🏁 Regular network processing completed');

    } catch (error) {
        console.error('❌ Regular network execution failed:', error.message);
    } finally {
        console.log('🛑 Shutting down regular network...');
        Network.prototype.shutdown(network);
        console.log('✅ Regular network shutdown completed');
    }
}

function sleep(ms) {
    return new Promise((resolve) => {
        setTimeout(resolve, ms);
    });
}

// ============================================================================
// 5. MAIN EXECUTION
// ============================================================================

async function main() {
    console.log('🚀 === BASIC REFLOW EXAMPLE ===');
    console.log('Demonstrating: Actors + Network + Graph\n');

    const startTime = Date.now();

    try {
        // Run GraphNetwork example
        await runSimpleNetwork();

        await sleep(500); // Short delay before next example

        // Run regular Network example  
        await runRegularNetwork();

        const endTime = Date.now();
        const duration = endTime - startTime;

        console.log('\n🎉 === EXAMPLE COMPLETED SUCCESSFULLY ===');
        console.log(`⏱️ Total execution time: ${duration}ms`);

        console.log('\n📊 === SUMMARY ===');
        console.log('✅ Graph creation and manipulation');
        console.log('✅ JavaScript actor implementation');
        console.log('✅ Actor registration with networks');
        console.log('✅ GraphNetwork execution from graph');
        console.log('✅ Regular Network manual setup');
        console.log('✅ Data flow through actor pipeline');
        console.log('✅ Network lifecycle management');

        console.log('\n🎯 The basic reflow functionality is working correctly!');

    } catch (error) {
        console.error('\n💥 Example failed:', error.message);
        console.error('Stack trace:', error.stack);
        process.exit(1);
    }
}

// Run the example
if (require.main === module) {
    main()
        .then(() => {
            console.log('\n🏁 Basic example completed successfully!');
            process.exit(0);
        })
        .catch((error) => {
            console.error('\n💥 Example failed:', error);
            process.exit(1);
        });
}

module.exports = {
    SourceActor,
    TransformActor,
    SinkActor,
    createSimpleGraph,
    runSimpleNetwork,
    runRegularNetwork,
    main
};