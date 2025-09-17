// ES Module entry point for WASM module
// This file will be copied to wasm/index.mjs in the npm package

import init, * as wasm from './reflow_network.js';

let initialized = false;
let initPromise = null;

/**
 * Initialize the WASM module. Must be called before using any other functions.
 * This function is idempotent - multiple calls will return the same promise.
 * 
 * @returns {Promise<void>}
 */
export async function initializeWasm() {
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

// Create proxies for main classes to ensure initialization
const GraphProxy = new Proxy(wasm.Graph || class {}, {
  construct: function(target, args) {
    if (!initialized) {
      throw new Error('WASM not initialized. Call initializeWasm() first.');
    }
    return new target(...args);
  }
});

const NetworkProxy = new Proxy(wasm.Network || class {}, {
  construct: function(target, args) {
    if (!initialized) {
      throw new Error('WASM not initialized. Call initializeWasm() first.');
    }
    return new target(...args);
  }
});

const GraphNetworkProxy = new Proxy(wasm.GraphNetwork || class {}, {
  construct: function(target, args) {
    if (!initialized) {
      throw new Error('WASM not initialized. Call initializeWasm() first.');
    }
    return new target(...args);
  }
});

// Re-export everything from wasm with our proxies
export * from './reflow_network.js';
export { 
  GraphProxy as Graph,
  NetworkProxy as Network,
  GraphNetworkProxy as GraphNetwork
};