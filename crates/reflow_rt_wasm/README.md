# `reflow_rt_wasm`

Browser-WASM distribution of the Reflow runtime. Wraps `reflow_graph`,
`reflow_actor`, and `reflow_network` for `wasm32-unknown-unknown` via
`wasm-bindgen`. Building this crate produces a `.wasm` + JS-glue
bundle that ships to npm as
[`@offbit-ai/reflow-wasm`](https://www.npmjs.com/package/@offbit-ai/reflow-wasm).

## Building

```sh
# from the repo root
wasm-pack build crates/reflow_rt_wasm \
  --target web \
  --out-dir ../../sdk/wasm/pkg \
  --release
```

`--target web` produces an ES module + `.wasm` ready for native
`<script type="module">` use; switch to `--target bundler` if you
prefer a Vite/webpack pipeline.

The repo's `.cargo/config.toml` sets the
`getrandom_backend="wasm_js"` rustflag automatically.

## Quickstart (JS)

```js
import init, { Graph, GraphNetwork, version } from "@offbit-ai/reflow-wasm";

await init();              // loads + instantiates the .wasm
console.log(version());    // "0.2.0"

const g = new Graph("demo");
g.add_node("a", "tpl_doubler");
g.add_node("b", "tpl_collector");
g.add_connection("a", "out", "b", "in");

const net = GraphNetwork.from_graph(g);
net.register_actor_js("tpl_doubler", { /* JS class with run(ctx) */ });
net.start();
```

## Scope

Currently exposes:

- `Graph` and the full Tier-1 + Tier-2 graph API (mutators + queries)
- `GraphNetwork` for runtime execution of JS actors registered via
  `register_actor_js(name, class)`
- `bindInputEvents(network, target)` for routing browser DOM events
  (keyboard, mouse, touch, wheel, resize) into input actors

The bundled component catalog (`reflow_components`) is **not** part
of this crate yet. Two unrelated reasons:

1. **`rquickjs` C library** — embedded QuickJS doesn't compile to
   `wasm32-unknown-unknown`. Gated to native; the wasm side uses a
   `js_sys::Function` shim that calls into the host browser engine
   instead.
2. **Actor framework `Send` bounds** — the `#[actor(...)]` macro
   wraps each actor's future as `Pin<Box<dyn Future + Send>>`.
   Browser-only types are typically `!Send`: `wgpu::WebQueue`,
   `reqwest`'s `AbortGuard`, and `web_sys::WebSocket` all hold raw
   JS handles (`*mut u8`). On wasm the runtime is single-threaded
   and `Send` is moot, but the macro doesn't yet drop the bound on
   `target_arch = "wasm32"`. Once that lands, wgpu (which **does**
   have a WebGPU backend), browser fetch via reqwest, and
   websockets all become available to wasm-side actors.

Author actors as JS classes and load them via `register_actor_js`
in the meantime, or load WASM-compiled actor packs via the browser
pack loader (see [`docs/pack-format.md`](../../docs/pack-format.md)).

## Versioning

Tracks the workspace minor version. Patch bumps are tagged
`wasm-vX.Y.Z` on the main branch and cut by the
`publish-wasm.yml` workflow.
