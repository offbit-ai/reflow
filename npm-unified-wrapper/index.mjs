/**
 * @offbit-ai/reflow
 * 
 * Main entry point - users choose their implementation:
 * 
 * For Node.js (native bindings):
 *   import reflow from '@offbit-ai/reflow/native';
 * 
 * For Browser (WASM):
 *   import reflow from '@offbit-ai/reflow/wasm';
 * 
 * For auto-detection:
 *   import reflow from '@offbit-ai/reflow/auto';
 */

throw new Error(
  `Please import a specific implementation:
  
  For Node.js (native bindings):
    import reflow from '@offbit-ai/reflow/native';
    
  For Browser (WASM):
    import reflow from '@offbit-ai/reflow/wasm';
    
  Or use the auto-detect version:
    import reflow from '@offbit-ai/reflow/auto';
`
);