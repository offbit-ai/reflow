/**
 * @offbit-ai/reflow/auto
 * 
 * Auto-detect ESM version that selects implementation based on environment
 */

// Detect the environment
const isNode = typeof process !== 'undefined' && 
               process.versions && 
               process.versions.node;

let impl;

if (isNode) {
  // Node.js - use native bindings
  const { createRequire } = await import('module');
  const require = createRequire(import.meta.url);
  impl = require('./native/index.js');
  console.debug('[Reflow] Auto-detected Node.js - using native implementation');
} else {
  // Browser - use WASM
  impl = await import('./wasm/reflow.js');
  console.debug('[Reflow] Auto-detected Browser - using WASM implementation');
}

// Export the entire implementation without assumptions
export default impl;