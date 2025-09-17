// Example of using the Reflow Network Actor System in a web application

import { Network, ActorLoad, MemoryState, BrowserActorContext,  init_panic_hook, ActorRunContext } from '../pkg/release/reflow_network.js';

// Initialize panic hook for better error messages
init_panic_hook();

// Define a simple actor
class SimpleActor {
  constructor() {
    this.inports = ["input"];
    this.outports = ["output"];
    this.state = {};
    this.config = {};
  }

  /**
   * 
   * @param {ActorRunContext} context 
   */
  run(context) {
    console.log("Actor received data:", JSON.stringify(context.input.input));
    
    // Process the data
    const result = {
      message: `Processed: ${JSON.stringify(context.input.input)}`,
      timestamp: Date.now()
    };
    
    // Send the result to the output port
    context.send({ output: result });
  }
}

// Main function to set up and run the actor system
async function main() {
  try {
    // Create a network
    const network = new Network();
    
    // Create an actor
    const actor = new SimpleActor();
    const jsActor = network.createActor("simpleActor", actor);
    
    // Add a node to the network
    network.addNode("processor", "simpleActor");
    
    // Start the network
    await network.start();
    
    // Send a message to the actor
    network.sendToActor("processor", "input", { 
      text: "Hello, Actor System!",
      count: 42
    });
    
    // Listen for network events
    network.next((event) => {
      console.log("Network event:", event);
    });
    
    // Demonstrate actor state management
    const state = new MemoryState();
    state.set("counter", 1);
    state.set("name", "Example Actor");
    
    console.log("State value:", state.get("counter"));
    console.log("State has 'name':", state.has("name"));
    console.log("State size:", state.size());
    
    // Get all state as an object
    const allState = state.getAll();
    console.log("All state:", allState);
    
    // Create an actor context for more complex operations
    const context = new BrowserActorContext(
      { input: "Sample input data" },
      { timeout: 5000 }
    );
    
    console.log("Context payload:", context.getPayload());
    console.log("Context config:", context.getConfig());
    
    // Send a message through the context
    context.sendToOutport("output", { result: "Success!" });
    
    // Mark the actor as done processing
    context.done();
    
    // Get information about the network
    console.log("Actor names:", network.getActorNames());
    console.log("Active actors:", network.getActiveActors());
    console.log("Actor count:", network.getActorCount());
    
    // Execute an actor directly and get the result
    const result = await network.executeActor("processor", { 
      command: "calculate",
      value: 100
    });
    console.log("Execution result:", result);
    
    // Shutdown the network when done
    setTimeout(() => {
      network.shutdown();
      console.log("Network shutdown complete");
    }, 5000);
    
  } catch (error) {
    console.error("Error in actor system:", error);
  }
}

// Run the example
main().catch(console.error);
