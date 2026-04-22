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

## Standard component catalog

Register bundled actors by id:

```ts
import { templateActor, templateList } from "@offbit-ai/reflow";

net.registerActor("tpl_http_request", templateActor("tpl_http_request"));
console.log(templateList().filter((id) => id.startsWith("tpl_math_")));
```

Catalog reference: [docs/components/standard-library.md](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md) (274 templates).

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

## License

MIT OR Apache-2.0.
