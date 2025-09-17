/**
 * @offbit-ai/reflow
 * 
 * Main entry point - users choose their implementation:
 * 
 * For Node.js (native bindings):
 *   const reflow = require('@offbit-ai/reflow/native');
 * 
 * For Browser (WASM):
 *   const reflow = require('@offbit-ai/reflow/wasm');
 *   or
 *   import reflow from '@offbit-ai/reflow/wasm';
 */

throw new Error(
  `Please import a specific implementation:
  
  For Node.js (native bindings):
    const reflow = require('@offbit-ai/reflow/native');
    
  For Browser (WASM):
    import reflow from '@offbit-ai/reflow/wasm';
    
  Or use the auto-detect version:
    const reflow = require('@offbit-ai/reflow/auto');
`
);