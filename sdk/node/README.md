# @offbit-ai/reflow — Node.js SDK

Reflow is a **modular workflow runtime built on the actor model**. Graphs are declarative DAGs: each node is an actor with named in/out ports, edges route messages, and a network executor runs the whole thing with bounded backpressure and a tracing stream. It ships a standard library of ~300 actors covering data, media, GPU rendering, animation, I/O, and optional ML / CV — plus the hooks to register your own.

This package is the official **Node.js binding** to that runtime. Use it to:

- **Author graphs programmatically** or load them from `GraphExport` JSON (what visual editors emit).
- **Register JavaScript functions as actors** — your callback runs on every tick with typed inputs and emits on named output ports.
- **Compose subgraphs** from other graph exports, optionally filling unresolved components from the bundled catalog.
- **Stream network events** (actor starts, failures, trace packets) as async iterables.
- **Plug in the standard component catalog** so you can start wiring real pipelines without re-authoring every primitive.

## Install

```sh
npm install @offbit-ai/reflow
```

## Quick start — run a two-node graph

```ts
import {
  ReflowNetwork,
  ReflowActor,
  ReflowMessage,
  templateActor,
} from "@offbit-ai/reflow";

const double = ReflowActor.fromCallback(
  { component: "double", inports: ["in"], outports: ["out"] },
  (ctx) => {
    const n = Number(ctx.inputs?.in?.data ?? 0);
    ctx.done({ out: ReflowMessage.integer(n * 2).asJson() });
  }
);

const log = ReflowActor.fromCallback(
  { component: "log", inports: ["in"], outports: [] },
  (ctx) => {
    console.log("got:", ctx.inputs?.in);
    ctx.done();
  }
);

const net = new ReflowNetwork();
net.registerActor("tpl_double", double);
net.registerActor("tpl_log", log);

net.addNode("a", "tpl_double");
net.addNode("b", "tpl_log");
net.addConnection("a", "out", "b", "in");
net.addInitial("a", "in", { type: "Integer", data: 21 });

net.start();
// ... events, then at some point:
net.shutdown();
```

## Actor callbacks

Every callback receives an `ActorCallContext`:

```ts
interface ActorCallContext {
  readonly inputs: Record<string, unknown>; // port name → JSON-shaped Message
  readonly config: Record<string, unknown>; // per-node config

  done(outputs?: Record<string, unknown>): void; // emit and return
  fail(reason: string): void;                    // abort this tick
}
```

Outputs are keyed by output port name. Each value is either a `ReflowMessage` (via `.asJson()`) or a plain JSON-shaped Message (`{ type: "Integer", data: 3 }`). **Always** call exactly one of `done` / `fail`.

## Standard component catalog

Register bundled actors by id:

```ts
import { templateActor, templateList } from "@offbit-ai/reflow";

net.registerActor("tpl_http_request", templateActor("tpl_http_request"));
console.log(templateList().filter((id) => id.startsWith("tpl_math_")));
```

The catalog is the same one documented at [`docs/components/standard-library.md`](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md) (274 templates).

## Subgraphs

```ts
import { SubgraphBuilder } from "@offbit-ai/reflow";

const sub = new SubgraphBuilder(graphExportJson);
sub.registerActor("my_custom", myJsActor);
sub.fillFromCatalog();             // resolve bundled components automatically
const sgActor = sub.build();
net.registerActor("tpl_sub", sgActor);
```

## Events

```ts
const stream = net.events();
(async () => {
  let evt;
  while ((evt = await stream.recv())) {
    console.log(evt);
  }
})();
```

Subscribe **before** `net.start()` so no events are missed.

## Versioning

Native addon is built from the same Rust sources as the core runtime
(`reflow_rt`). The first digit of the SDK version tracks the runtime's
major; minor / patch follow the runtime's minor / patch plus SDK-only
fixes. Expect API stability within a major.

## License

MIT OR Apache-2.0.
