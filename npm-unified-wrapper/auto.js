/**
 * @offbit-ai/reflow/auto
 * 
 * Auto-detect version that selects implementation based on environment
 */

// Detect the environment
const isNode = typeof process !== 'undefined' && 
               process.versions && 
               process.versions.node;

// Export the appropriate implementation
if (isNode) {
  // Node.js - use native bindings
  module.exports = require('./native/index.js');
  console.debug('[Reflow] Auto-detected Node.js - using native implementation');
} else {
  // Browser - use WASM
  module.exports = require('./wasm/reflow.js');
  console.debug('[Reflow] Auto-detected Browser - using WASM implementation');
}