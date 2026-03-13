#!/usr/bin/env node
/**
 * Test JavaScript actor for MicroSandbox integration
 * Tests Message serialization, compression, and bidirectional communication
 */

// Main actor processing function
async function process(context) {
    console.log(`Processing in JavaScript actor: ${context.actor_id}`);
    
    // Test getting various input types
    const integerInput = context.getInput("integer_port", 0);
    const stringInput = context.getInput("string_port", "");
    const arrayInput = context.getInput("array_port", []);
    const objectInput = context.getInput("object_port", {});
    const binaryInput = context.getInput("binary_port", []);
    
    console.log(`Received inputs: int=${integerInput}, str=${stringInput}, arr=${arrayInput}`);
    
    // Test async output sending
    await context.sendOutput("echo_int", integerInput * 2);
    await context.sendOutput("echo_str", stringInput ? stringInput.toUpperCase() : "NO_STRING");
    
    // Test large data compression (>1KB)
    const largeData = "x".repeat(2000); // 2KB of data
    await context.sendOutput("compressed", largeData);
    
    // Test array processing
    if (arrayInput && arrayInput.length > 0) {
        const processedArray = arrayInput.map(item => 
            typeof item === 'number' ? item * 2 : item
        );
        await context.sendOutput("processed_array", processedArray);
    }
    
    // Test object manipulation
    if (objectInput && Object.keys(objectInput).length > 0) {
        const resultObject = {
            ...objectInput,
            processed: true,
            timestamp: context.timestamp
        };
        await context.sendOutput("processed_object", resultObject);
    }
    
    // Test binary data handling
    if (binaryInput && binaryInput.length > 0) {
        await context.sendOutput("binary_size", binaryInput.length);
    }
    
    // Test state persistence (if Redis is available)
    try {
        const counter = await context.getState("counter", 0);
        const newCounter = counter + 1;
        await context.setState("counter", newCounter);
        await context.sendOutput("state_counter", newCounter);
    } catch (e) {
        console.log(`State persistence not available: ${e}`);
    }
    
    // Return final outputs
    return {
        status: "success",
        actor_id: context.actor_id,
        input_count: Object.keys(context.payload).length,
        outputs_sent: Object.keys(context._outputs).length
    };
}

// Test streaming/progressive outputs
async function streamProcessor(context) {
    const iterations = context.getInput("iterations", 5);
    const delay = context.getInput("delay", 100); // milliseconds
    
    for (let i = 0; i < iterations; i++) {
        // Send progress updates
        const progress = {
            current: i + 1,
            total: iterations,
            percent: ((i + 1) / iterations) * 100
        };
        await context.sendOutput("progress", progress);
        
        // Process chunk
        const chunkData = `Chunk ${i}: ${"data".repeat(100)}`; // ~400 bytes per chunk
        await context.sendOutput("chunk", chunkData);
        
        // Small delay to simulate processing
        await new Promise(resolve => setTimeout(resolve, delay));
    }
    
    // Send completion
    await context.sendOutput("complete", true);
    
    return {
        total_chunks: iterations,
        status: "completed"
    };
}

// Test error handling
async function errorHandler(context) {
    const shouldError = context.getInput("should_error", false);
    const errorMessage = context.getInput("error_message", "Test error");
    
    if (shouldError) {
        throw new Error(errorMessage);
    }
    
    return {
        status: "no_error",
        message: "Executed successfully"
    };
}

// Mark functions as actors
process.__actor__ = true;
streamProcessor.__actor__ = true;
errorHandler.__actor__ = true;

// Export for module usage
module.exports = {
    process,
    streamProcessor,
    errorHandler,
    __actor_metadata__: {
        component: "javascript_test_actor",
        description: "JavaScript test actor for MicroSandbox integration",
        version: "1.0.0",
        inports: [
            "integer_port",
            "string_port",
            "array_port",
            "object_port",
            "binary_port",
            "iterations",
            "delay",
            "should_error",
            "error_message"
        ],
        outports: [
            "echo_int",
            "echo_str",
            "compressed",
            "processed_array",
            "processed_object",
            "binary_size",
            "state_counter",
            "progress",
            "chunk",
            "complete",
            "error"
        ]
    }
};