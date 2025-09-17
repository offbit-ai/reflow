// Entry point for WASM module
// This file will be copied to wasm/index.js in the npm package

const init = require('./reflow_network.js');
const wasm = require('./reflow_network.js');

let initialized = false;
let initPromise = null;

/**
 * Initialize the WASM module. Must be called before using any other functions.
 * This function is idempotent - multiple calls will return the same promise.
 * 
 * @returns {Promise<void>}
 */
async function initializeWasm() {
  if (initialized) {
    return Promise.resolve();
  }
  
  if (initPromise) {
    return initPromise;
  }
  
  initPromise = (async () => {
    try {
      // Initialize the WASM module using reflow_network's default init function
      await init();
      
      // Initialize panic hook for better error messages
      if (wasm.init_panic_hook) {
        wasm.init_panic_hook();
      }
      
      initialized = true;
      console.log('Reflow WASM module initialized successfully');
    } catch (error) {
      console.error('Failed to initialize Reflow WASM module:', error);
      throw error;
    }
  })();
  
  return initPromise;
}

// Export all the WASM exports
module.exports = {
  ...wasm,
  initializeWasm,
  // Ensure users know they need to initialize
  Graph: new Proxy(wasm.Graph || {}, {
    construct: function(target, args) {
      if (!initialized) {
        throw new Error('WASM not initialized. Call initializeWasm() first.');
      }
      return new target(...args);
    }
  }),
  Network: new Proxy(wasm.Network || {}, {
    construct: function(target, args) {
      if (!initialized) {
        throw new Error('WASM not initialized. Call initializeWasm() first.');
      }
      return new target(...args);
    }
  }),
  GraphNetwork: new Proxy(wasm.GraphNetwork || {}, {
    construct: function(target, args) {
      if (!initialized) {
        throw new Error('WASM not initialized. Call initializeWasm() first.');
      }
      return new target(...args);
    }
  })
};