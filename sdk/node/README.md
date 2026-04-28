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
| `ctx.send(outputs)` | Emit outputs keyed by output port. Safe to call multiple times during a tick. |
| `ctx.done(outputs?)` | Resolve the tick. The optional outputs are sent the same way as `ctx.send`. |
| `ctx.fail(reason)` | Abort this tick with an error. |

The runtime treats every `run` as an async tick that completes when
you call `ctx.done()` (or `ctx.fail`). Internally `run` returns a
`Promise` that the runtime awaits via `wasm-bindgen-futures` (browser)
or the `Future` directly (Node). Until the promise resolves, the
dispatcher will not re-fire the actor on a new packet — your tick
has the runtime's full attention.

That means async code inside `run` is first-class:

```ts
class Fetcher extends Actor {
  static component = "fetcher";
  static inports  = ["url"];
  static outports = ["body"];

  async run(ctx) {
    const url = ctx.inputs.url.data;
    const body = await fetch(url).then((r) => r.text());
    ctx.done({ body: Message.string(body) });
  }
}
```

…and so are deferred-completion patterns where the actor parks the
tick on a callback (`requestAnimationFrame`, an event listener, a
DOM observer):

```ts
class FrameClock extends Actor {
  static component = "clock";
  static inports  = ["tick"];
  static outports = ["tick", "dt"];
  last = performance.now();

  run(ctx) {
    const now = performance.now();
    const dt  = (now - this.last) / 1000;
    this.last = now;
    ctx.send({ dt: Message.float(dt) });
    requestAnimationFrame(() => {
      ctx.send({ tick: Message.flow() });   // self-loop: re-fire next frame
      ctx.done();
    });
    // run() returns now; the runtime awaits ctx.done() inside rAF.
  }
}
```

Exactly one of `done` / `fail` must be called per tick. If `run`
returns a rejected promise, the SDK calls `fail` for you.

### Per-port delivery hints

Declare `static portDelivery` on the subclass to tell the runtime how
each inport should be backed:

```ts
class Renderer extends Actor {
  static inports  = ["frame"];
  static outports = [];
  static portDelivery = { frame: "latest" };  // drop stale frames
  run(ctx) { /* … */ }
}
```

`"latest"` keeps only the freshest packet — older ones are dropped if
the actor falls behind. Default is reliable, in-order delivery.

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

The SDK ships the lightweight slice of the standard component
catalog — roughly 270 templates covering animation, flow control,
math, vector, 2D graphics, asset DB, scene graph, HTTP integration,
stream ops, DSP, and procedural generation. Heavy optional palettes
(GPU, ML, browser automation, video encoding, window events, ~6,700
API-service wrappers) are **not bundled** and install as
[actor packs](#actor-packs).

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
whose tag starts with `pack-v`. **Pack and SDK builds must come
from the same release wave** (matching `REFLOW_PACK_ABI_VERSION`)
— see [the pack ↔ SDK compatibility matrix](https://github.com/offbit-ai/reflow/blob/main/docs/pack-compatibility.md)
for the supported pairings. Each release ships two flavours of
every pack:

| Flavour | Filename | When to use |
|---|---|---|
| Full multi-triple | `<name>-<version>.rflpack` (~22 MiB) | Distributing to mixed-platform consumers |
| Per-triple slim | `<name>-<version>-<triple>.rflpack` (~3 MiB) | Shipping to a known platform — much smaller download |

```sh
VER=0.2.0
# Slim variant for the host you're running on (Apple Silicon shown).
curl -LO https://github.com/offbit-ai/reflow/releases/download/pack-v$VER/reflow.pack.ml-$VER-aarch64-apple-darwin.rflpack

# Or the full bundle if you don't know the deployment target ahead of time.
curl -LO https://github.com/offbit-ai/reflow/releases/download/pack-v$VER/reflow.pack.ml-$VER.rflpack
```

Triples published per pack are listed in
[`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md);
every pack except `browser` ships a `wasm32-unknown-unknown`
slim that's the smallest of all.

`loadPack()` accepts either flavour identically — it picks the
binary that matches the runtime triple at load time.

If you've already downloaded a full bundle and want to ship a
slim copy with your own application, the bundled `reflow-pack`
CLI strips it in one shot:

```sh
reflow-pack strip reflow.pack.ml-0.2.0.rflpack
# → reflow.pack.ml-0.2.0-<host-triple>.rflpack
```

Third-party packs are distributed however their author chooses (npm
tarball, GitHub Releases, internal registry) — any local file path
works with `loadPack()`.

**ABI lockstep.** A pack is pinned to the SDK release it was built
against. Pick the `pack-v*` release whose version matches your
`@offbit-ai/reflow`; if you need a pack for a different SDK version,
rebuild from source — see
[`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md).

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
- `import { ... } from "@offbit-ai/reflow/native"` → raw native bindings (`ReflowActor`, `ReflowNetwork`, ...) if you want to skip the class layer. Escape hatch — most code wants the default import.
- `import { ... } from "@offbit-ai/reflow/browser"` → explicit browser-WASM build (rarely needed; bundlers route the default import here automatically).

## Browser target

The same package ships a WebAssembly build of the runtime under
[`wasm/`](./wasm) for browser use. The `"browser"` conditional
export in `package.json` makes Vite, webpack, and esbuild resolve
`import { Graph, Network } from "@offbit-ai/reflow"` to the wasm
bundle when bundling for the browser; Node continues to load the
native addon.

The browser surface uses the same class names, method names, and
argument order as Node, so isomorphic code reads the same in both
targets:

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
- `initGpuContext(canvasSelector)` — initialize the shared GPU
  context against an HTML canvas (see GPU section below)

Native-only stacks (file I/O, video encode, headless browser
automation, ML/CV taskpacks) are not in the wasm bundle — those
remain in the Node component catalog. Browser code that calls a
native-only template will fail at registration time with a clear
"template not found" error.

### GPU on wasm

Reflow's GPU actors target **WebGPU** in the browser. SDF
rendering, scene rasterization, marching cubes, mesh ops — all
of it runs once you initialize the GPU context against a target
canvas:

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
bytes rather than displayed.

The canvas argument is required because Chromium's WebGPU
implementation refuses to hand out a presentation-capable adapter
without a target surface. Subsequent calls are no-ops; on Node
the argument is ignored and the GPU context is initialized lazily
on first actor use.

### Loading actor packs in the browser

`.rflpack` bundles ship a browser build alongside the native
binaries (see [pack format](../packs/README.md)). Browser code
loads them straight from a URL — `loadPack(url)` fetches the
bundle, validates the manifest, and compiles the browser build
into a usable WebAssembly module.

```js
import { ready, loadPack, Network } from "@offbit-ai/reflow";

await ready();

const network = new Network();
const pack = await loadPack(
  "https://github.com/offbit-ai/reflow/releases/download/" +
  "pack-v0.2/reflow.pack.gpu-0.2.0.rflpack",
  { network },
);

console.log(pack.name);       // "reflow.pack.gpu"
console.log(pack.version);    // "0.2.0"
console.log(pack.templates);  // ["tpl_sdf_render", ...]
console.log(pack.registered); // [{ name: "tpl_sdf_render", factoryId: 0 }, ...]
console.log(network.getActorNames()); // includes "tpl_sdf_render"
```

Pass `{ network }` to wire the pack into a `Network` immediately,
or call `pack.attachTo(network)` later. Either way, every template
the pack publishes is registered on the network and visible to
`network.getActorNames()` before the call resolves.

Returned object: `{ manifest, name, version, templates,
registered, attachTo, ... }`. The `registered` array carries
`{ name, factoryId, inports, outports }` for each template the
pack wired up.

Each registered template appears in `network.getActorNames()`
once `attachTo()` resolves; running the network ticks them as
normal. Synchronous pack actors (compute, transforms, sync GPU
work) run end-to-end today. Asynchronous pack actors — anything
that awaits `fetch` or other browser Promises — are not yet
runnable in the browser; they'll be enabled in a follow-up
release.

**ABI handshake.** `loadPack` rejects packs that weren't built
against this runtime's release. A mismatch surfaces immediately
as a load-time error rather than a silent runtime corruption.

**CORS.** GitHub release assets serve permissive CORS headers, so
cross-origin browser fetches "just work". For other hosts you
may need to proxy or set headers explicitly.

**Pack target coverage.** Not every first-party pack ships a wasm32
build today — see the [pack catalog](../packs/README.md) for the
matrix. Loading a pack that lacks a wasm32 entry fails fast with a
"no wasm32 build" error instead of trying a native fallback.

> Note: the registration handshake (instantiating the compiled
> module against the runtime's actor registry) is on a follow-up
> milestone. `loadPack` today gives you the verified, ABI-checked
> module — wiring it into a running `Network` is TBD.

## License

MIT OR Apache-2.0.
