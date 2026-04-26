// Browser entrypoint for @offbit-ai/reflow.
//
// In browsers, the napi `.node` addon can't load — there's no Node
// process and no native module loader. This file is selected via the
// `"browser"` conditional export in package.json so bundlers
// (Vite, webpack, esbuild, …) resolve `import { Graph } from
// "@offbit-ai/reflow"` to the wasm-bindgen build instead.
//
// The runtime surface is intentionally narrower than the Node SDK —
// it covers Graph + GraphNetwork (the subset that compiles to wasm),
// not the full Actor base class with stream helpers. Browser-side
// actors are authored as JS classes and registered via
// `network.registerActorJs(name, klass)`.

import init, * as wasm from "./wasm/reflow_rt_wasm.js";

let _ready;

/**
 * Initialize the wasm module. Call once before constructing Graph
 * or GraphNetwork. Subsequent calls return the cached promise.
 *
 * @param {RequestInfo|URL|BufferSource|WebAssembly.Module} [moduleOrUrl]
 *   Optional override for where to fetch the .wasm. By default
 *   wasm-bindgen resolves relative to this module's URL.
 */
export function ready(moduleOrUrl) {
  if (!_ready) _ready = init(moduleOrUrl);
  return _ready;
}

// Re-export the wasm-bindgen surface under the names used by the
// Node SDK so isomorphic code keeps the same imports. Method-name
// mismatches (snake_case vs camelCase) are documented in
// sdk/wasm/README.md; we don't paper over them here because that
// would mean shadowing every class with a JS proxy and inflating
// the bundle.
export const Graph = wasm.Graph;
export const Network = wasm.GraphNetwork;
export const GraphNetwork = wasm.GraphNetwork;
export const version = wasm.version;
export const bindInputEvents = wasm.bindInputEvents;

export default {
  Graph,
  Network,
  GraphNetwork,
  version,
  bindInputEvents,
  ready,
};
