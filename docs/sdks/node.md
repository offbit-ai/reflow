# Node.js SDK

`@offbit-ai/reflow` — Node 18+, prebuilt addons for darwin / linux / windows.

## Install

```sh
npm install @offbit-ai/reflow
```

npm resolves the right per-platform `.node` addon via `optionalDependencies`; no compilation step on the user side.

## Hello world

```ts
import {
  Graph, Network, Actor, Message,
} from "@offbit-ai/reflow";

class Doubler extends Actor {
  static component = "doubler";
  static inports = ["in"];
  static outports = ["out"];

  run(ctx) {
    const n = ctx.inputs.in?.data ?? 0;
    ctx.done({ out: Message.integer(Number(n) * 2) });
  }
}

const net = new Network();
net.registerActor("tpl_doubler", new Doubler());
net.addNode("a", "tpl_doubler");
net.addInitial("a", "in", { type: "Integer", data: 21 });
net.start();

for await (const ev of net.events()) {
  console.log(ev);
  if (ev._type === "ActorCompleted") break;
}
```

## Authoring graphs

```ts
const g = new Graph("demo");
g.addNode("a", "tpl_x");
g.addNode("b", "tpl_y");
g.addConnection("a", "out", "b", "in");
g.addGroup("pipe", ["a", "b"], { caption: "pipeline" });
g.renameNode("a", "alpha");
console.log(g.groups());           // [{ id: "pipe", nodes: ["alpha","b"], … }]
console.log(g.connections());      // …
```

The full graph API (T1 mutators + T2 queries — renames, groups, port lifecycle, metadata setters, queries) is mirrored in the SDK; see the [SDK README](https://github.com/offbit-ai/reflow/blob/main/sdk/node/README.md) for the complete surface.

## Bundled component catalog

Every install ships `reflow_components`'s `av-core` slice — ~270 templates covering animation, flow control, math, vector / 2D graphics, asset DB, scene graph, HTTP integration, stream ops, DSP, procedural generation. Heavier palettes (GPU, ML, browser, video, window events, ~6,700 API actors) install as [`.rflpack`](../packs/README.md) bundles.

```ts
import { templateActor, templateList, loadPack } from "@offbit-ai/reflow";

console.log(templateList().filter(id => id.startsWith("tpl_math_")));

// Plug in the ML pack at runtime — adds 12 more template ids.
loadPack("./reflow.pack.ml-0.2.0.rflpack");
const inferenceActor = templateActor("tpl_ml_run_inference");
```

## Subgraphs

```ts
import { SubgraphBuilder } from "@offbit-ai/reflow";
const sub = new SubgraphBuilder(graphExportJson);
sub.registerActor("my_custom", new MyCustom());
sub.fillFromCatalog();   // resolve remaining components from bundled + loaded packs
net.registerActor("tpl_sub", sub.build());
```

## Streams

```ts
const stream = net.createStream({ bufferSize: 64, contentType: "image/jpeg" });
producer.emit({ stream });        // Flow + chunked frames
for await (const frame of stream) console.log(frame.kind, frame.data?.length);
```

## See also

- [Full Node SDK README](https://github.com/offbit-ai/reflow/blob/main/sdk/node/README.md) — complete API + threading notes.
- [Authoring Actors](../api/actors/creating-actors.md) — runtime semantics shared by every SDK.
- [Packs](../packs/README.md) — extending the catalog at runtime.
