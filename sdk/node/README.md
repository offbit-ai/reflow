# @offbit-ai/reflow — Node.js SDK

Reflow is a **modular workflow runtime built on the actor model**. Graphs are declarative DAGs: each node is an actor with named in/out ports, edges route messages, and a network executor runs the whole thing with bounded backpressure and a tracing stream. It ships a standard library of ~300 actors covering data, media, GPU rendering, animation, I/O, and optional ML / CV — plus the hooks to register your own.

This package is the official **Node.js binding** to that runtime. Use it to:

- **Author graphs programmatically** or load them from `GraphExport` JSON (what visual editors emit).
- **Register JavaScript classes as actors** — subclass `Actor`, override `run(ctx)`, done.
- **Compose subgraphs** from other graph exports, optionally filling unresolved components from the bundled catalog.
- **Stream network events** as async iterables.
- **Move large payloads as streams** via `Message.StreamHandle`.
- **Plug in the standard component catalog** so you can start wiring real pipelines without re-authoring every primitive.

## Install

```sh
npm install @offbit-ai/reflow
```

## Quick start — two-node pipeline

```ts
import { Actor, Network, Message } from "@offbit-ai/reflow";

class Doubler extends Actor {
  static component = "doubler";
  static inports = ["in"];
  static outports = ["out"];

  run(ctx) {
    const n = Number(ctx.inputs?.in?.data ?? 0);
    ctx.done({ out: Message.integer(n * 2) });
  }
}

class Log extends Actor {
  static component = "log";
  static inports = ["in"];
  static outports = [];
  run(ctx) { console.log(ctx.inputs?.in); ctx.done(); }
}

const net = new Network();
net.registerActor("tpl_doubler", new Doubler());
net.registerActor("tpl_log",     new Log());

net.addNode("a", "tpl_doubler");
net.addNode("b", "tpl_log");
net.addConnection("a", "out", "b", "in");
net.addInitial("a", "in", { type: "Integer", data: 21 });

net.start();
// ... later:
net.shutdown();
```

## Authoring actors

Extend `Actor`. Declare ports and await semantics via **static fields**:

```ts
class MyActor extends Actor {
  static component = "my_actor";
  static inports = ["a", "b"];
  static outports = ["sum"];
  static awaitAllInports = true;   // default: false

  run(ctx) {
    const a = Number(ctx.inputs.a.data);
    const b = Number(ctx.inputs.b.data);
    ctx.done({ sum: Message.integer(a + b) });
  }
}
```

Inside `run(ctx)`:

| Member | Purpose |
|--------|---------|
| `ctx.inputs` | `Record<string, Message-JSON>` — one entry per port that received a packet this tick. |
| `ctx.config` | Node-level config passed at graph time. |
| `ctx.done(outputs?)` | Emit outputs keyed by output port. Each value is a `Message` or a JSON-shaped Message. |
| `ctx.fail(reason)` | Abort this tick with an error. |

Exactly one of `done` / `fail` must be called per tick. If `run` returns a Promise and it rejects, the SDK calls `fail` for you.

Instance state is just instance state — the class itself holds it:

```ts
class Counter extends Actor {
  static component = "counter";
  static inports = ["tick"];
  static outports = ["count"];
  count = 0;
  run(ctx) {
    this.count += 1;
    ctx.done({ count: Message.integer(this.count) });
  }
}
```

## Multi-graph composition

Merge N `GraphExport` documents (what visual editors emit, or what
`Graph.toJson()` returns) into a single runnable graph. Namespaces are
resolved automatically; cross-graph connections are wired through the
`connections` array.

```ts
import { composeGraphs, Graph, Network } from "@offbit-ai/reflow";

const composed = composeGraphs({
  graphs: [leftExport, rightExport],
  connections: [
    { from: { process: "gsrc/src",   port: "out" },
      to:   { process: "gsink/sink", port: "in"  } },
  ],
  shared_resources: [],
  properties: { name: "pipeline" },
  case_sensitive: false,
});

const graph = Graph.fromJson(composed);
const net = Network.fromGraph(graph);
```

## Standard component catalog

The SDK ships the pure-Rust + `av-core` slice of `reflow_components` —
roughly 270 templates covering animation, flow control, math, vector,
2D graphics, asset DB, scene graph, HTTP integration, stream ops, DSP,
and procedural generation. Heavy optional palettes (GPU, ML, browser
automation, video encoding, window events, ~6,700 API-service wrappers)
are **not bundled** and install as [actor packs](#actor-packs).

```ts
import { templateActor, templateList } from "@offbit-ai/reflow";

net.registerActor("tpl_http_request", templateActor("tpl_http_request"));
console.log(templateList().filter((id) => id.startsWith("tpl_math_")));
```

Full catalog reference: [docs/components/standard-library.md](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md).

## Actor packs

Packs are `.rflpack` bundles that publish additional templates into
this SDK at runtime — the GPU renderer palette, the ML stack, browser
automation, etc. `templateActor(id)` and `templateList()` transparently
include pack-supplied templates after load.

```ts
import { loadPack, inspectPack, listPacks, packAbiVersion, templateActor } from "@offbit-ai/reflow";

// Peek before committing.
console.log(inspectPack("./reflow.pack.ml-0.2.0.rflpack"));

// Load (idempotent; safe to call repeatedly).
loadPack("./reflow.pack.ml-0.2.0.rflpack");

// Pack-owned templates now resolve normally.
net.registerActor("tpl_ml_run_inference", templateActor("tpl_ml_run_inference"));

console.log(listPacks());
console.log(packAbiVersion());   // ABI the SDK expects from a .rflpack
```

First-party packs live under [`sdk/packs/`](https://github.com/offbit-ai/reflow/tree/main/sdk/packs):

| Pack                | Templates | Pulls in                                    |
|---------------------|:---------:|---------------------------------------------|
| `reflow.pack.browser`      | 1    | chromiumoxide                              |
| `reflow.pack.video_encode` | 1    | openh264                                   |
| `reflow.pack.ml`           | 12   | CV ops, LiteRT inference                   |
| `reflow.pack.gpu`          | 6    | wgpu SDF / scene / 2D renderers            |
| `reflow.pack.window_events`| 5    | Keyboard / mouse / gamepad / touch / window|
| `reflow.pack.api_services` | ~6700| Generated Slack / Stripe / Jira / Notion / …|

### Where to get `.rflpack` files

First-party bundles ship as assets on every [GitHub Release](https://github.com/offbit-ai/reflow/releases)
whose tag starts with `pack-v`. Grab the one you want and hand its
path to `loadPack()`:

```sh
VER=0.2.0
curl -LO https://github.com/offbit-ai/reflow/releases/download/pack-v$VER/reflow.pack.ml-$VER.rflpack
```

Each `.rflpack` bundles every supported triple in one file — the
loader picks the right dylib at runtime. Catalog + per-pack contents:
[`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md).

Third-party packs are distributed however their author chooses (npm
tarball, GitHub Releases, internal registry) — any local file path
works with `loadPack()`.

**ABI lockstep.** A pack is pinned to the rustc version of the SDK it
was built against. Pick the `pack-v*` release whose version matches
your `@offbit-ai/reflow`; rebuild from source
([`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md))
if you need a pack for a different SDK version.

## Subgraphs

```ts
import { SubgraphBuilder } from "@offbit-ai/reflow";

const sub = new SubgraphBuilder(graphExportJson);
sub.registerActor("my_custom", new MyCustom());
sub.fillFromCatalog();                   // resolve bundled components
const sgActor = sub.build();
net.registerActor("tpl_sub", sgActor);
```

## Streams

Producer side:

```ts
import { Stream } from "@offbit-ai/reflow";

const s = Stream.create({ bufferSize: 64, contentType: "image/jpeg" });
s.sendBytes(Buffer.from(frame1));
s.sendBytes(Buffer.from(frame2));
s.end();

ctx.done({ out: s.intoMessage() });
```

Consumer side:

```ts
const reader = ctx.inputs.frames.takeStream();   // on a StreamHandle message
while (true) {
  const f = await reader.recv(500);             // timeout in ms
  if (f.kind === "data")     handle(Buffer.from(f.data));
  else if (f.kind === "end") break;
  else if (f.kind === "closed" || f.kind === "timeout") break;
  else if (f.kind === "error") throw new Error(f.error);
}
```

## Events

```ts
const events = net.events();
(async () => {
  let evt;
  while ((evt = await events.recv())) {
    console.log(evt._type, evt);
  }
})();
```

Subscribe **before** `net.start()` so no events are missed.

## Building locally

```sh
npm install
npm run build:debug        # produces reflow-runtime.<triple>.node
npm test                   # runs test/*.mjs against the built addon
```

## Package entry points

- `import { ... } from "@offbit-ai/reflow"` → high-level API with the `Actor` class.
- `import { ... } from "@offbit-ai/reflow/native"` → raw napi-rs bindings (`ReflowActor`, `ReflowNetwork`, ...) if you want to skip the class layer.
- `import { ... } from "@offbit-ai/reflow/browser"` → explicit browser-WASM build (rarely needed; bundlers route the default import here automatically).

## Browser target

The same package ships a wasm-bindgen build of the runtime under
[`wasm/`](./wasm) for browser use. The `"browser"` conditional
export in `package.json` makes Vite/webpack/esbuild resolve
`import { Graph, Network } from "@offbit-ai/reflow"` to the wasm
bundle when bundling for the browser; Node continues to load the
napi `.node` addon.

The browser surface uses the same class names, method names, and
argument order as Node — the entry file (`reflow.browser.mjs`)
wraps the wasm-bindgen build behind a thin shim. Isomorphic code
reads the same in both targets:

```js
import { Graph, Network, Actor, Message, ready } from "@offbit-ai/reflow";

// Browser only — call once before constructing anything.
// (No-op shape on Node; the `ready` helper resolves immediately.)
await ready();

class Doubler extends Actor {
  static component = "doubler";
  static inports = ["in"];
  static outports = ["out"];
  run(ctx) {
    ctx.send({ out: Message.integer(2 * ctx.input.in.data) });
    ctx.done();
  }
}

const net = new Network();
net.addNode("a", "tpl_doubler");
net.addNode("b", "tpl_collector");
net.addConnection("a", "out", "b", "in");
net.addInitial("a", "in", Message.integer(21));
net.registerActor("tpl_doubler", new Doubler());
await net.start();

const events = net.events();
console.log(await events.recv()); // { type: "NetworkStarted", ... }
```

Browser-side scope:

- `Graph` — full Tier-1 + Tier-2 mutator and query API
- `Network` — same constructor + imperative API as Node
  (`new Network()`, `addNode`, `addConnection`, `addInitial`,
  `registerActor`, `start`, `shutdown`, `events`)
- `Actor` — same authoring base class as Node
- `Message` — same payload constructors as Node
- `EventStream` — Promise-based `.recv()` over network events
- `bindInputEvents(network, target)` — routes DOM events to input actors
- `version()` — runtime version string
- `initGpuContext(canvasSelector)` — initialize the shared wgpu
  context against an HTML canvas (see GPU section below)

Native-only stacks (file I/O, video encode, headless browser
automation, ML/CV taskpacks) are not in the wasm bundle — those
remain in the Node-side reflow_components catalog. Browser code
that calls a native-only template will fail at registration time
with a clear "template not found" error.

### GPU on wasm

Reflow's GPU actors are built on **wgpu**, which compiles to the
WebGPU backend on `wasm32-unknown-unknown`. SDF rendering, scene
rasterization, marching cubes, mesh ops — all of it runs in the
browser as long as you initialize the GPU context against a target
canvas first:

```js
import { ready, initGpuContext, Network, Graph } from "@offbit-ai/reflow";

await ready();
await initGpuContext("#viewport");   // CSS selector for the <canvas>

const g = new Graph("scene");
g.addNode("renderer", "tpl_sdf_live_render");
// ...wire and run
```

Pass `null` instead of a selector for off-screen workloads (mesh
operations, SDF readback) where the result is consumed as raw
bytes rather than displayed via the wgpu pipeline.

The init step is required because Chromium's WebGPU implementation
refuses to hand out a presentation-capable adapter without a target
surface. Subsequent calls are no-ops; on native runtimes the
argument is ignored and the GPU context is initialized lazily on
first actor use.

## License

MIT OR Apache-2.0.
