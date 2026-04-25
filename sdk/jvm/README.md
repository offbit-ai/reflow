# Reflow — JVM SDK (`ai.offbit.reflow`)

Reflow is a **modular flow-based programming runtime built on the actor model**. Graphs are declarative DAGs: each node is an actor with named in/out ports, edges route messages, and a network executor runs the whole thing with bounded backpressure and a tracing stream. It ships a standard library of ~300 actors covering data, media, GPU rendering, animation, I/O, and optional ML / CV — plus the hooks to register your own.

This module is the **JVM binding** (Java + Kotlin-friendly) to that runtime. It links to a native shared library built from `sdk/jvm/src/native` via JNI (written in Rust) and exposes idiomatic Java classes that mirror the Node / Go / Python SDKs one-for-one. Kotlin users get a trailing-lambda DSL and coroutine-friendly adapters on top.

- Maven coordinates: `ai.offbit:reflow:<version>`
- Java/Kotlin package: `ai.offbit.reflow`

```kotlin
// Gradle
dependencies { implementation("ai.offbit:reflow:0.2.0") }
```

```xml
<!-- Maven -->
<dependency>
    <groupId>ai.offbit</groupId>
    <artifactId>reflow</artifactId>
    <version>0.2.0</version>
</dependency>
```

## Quick start

```java
import ai.offbit.reflow.*;
import java.util.List;

class Doubler extends Actor {
    @Override public String component() { return "doubler"; }
    @Override public List<String> inports()  { return List.of("in"); }
    @Override public List<String> outports() { return List.of("out"); }

    @Override public void run(ActorCallContext ctx) {
        // parse integer from ctx.inputsJson() (use your JSON library of choice)
        long n = /* parse */;
        ctx.emit("out", Message.integer(n * 2));
        ctx.done();
    }
}

try (var net = new Network()) {
    net.registerActor("tpl_doubler", new Doubler());
    net.addNode("a", "tpl_doubler");
    net.addInitial("a", "in", "{\"type\":\"Integer\",\"data\":21}");
    net.start();
    // ...
    net.shutdown();
}
```

## Kotlin DSL

```kotlin
import ai.offbit.reflow.*

val doubler = actor {
    component = "doubler"
    inports = listOf("in")
    outports = listOf("out")
    onRun { ctx ->
        val n = parseIntegerInput(ctx.inputsJson(), "in")
        ctx.emit("out", Message.integer(n * 2))
        ctx.done()
    }
}

network {
    registerActor("tpl_doubler", doubler)
    addNode("a", "tpl_doubler")
    addInitial("a", "in", """{"type":"Integer","data":21}""")
    start()
}
```

With coroutines:

```kotlin
val events = net.events()
events.asFlow(pollMs = 200)
    .filter { "NetworkStarted" in it || "ActorStarted" in it }
    .collect(::println)
```

Destructure stream frames:

```kotlin
val (kind, data, error) = reader.recv(500)
```

## Authoring actors

Subclass `Actor`. Override `component()`, `inports()`, `outports()`,
optionally `awaitAllInports()`, and `run(ActorCallContext)`.

Inside `run(ctx)`:

| Method | Purpose |
|--------|---------|
| `ctx.inputsJson()` | JSON object keyed by port name; values are tagged Messages. |
| `ctx.configJson()` | Per-node config JSON. |
| `ctx.emit(port, Message)` | Queue one output packet (transfers message ownership). |
| `ctx.done()` | Resolve the tick; any emitted packets are flushed. |
| `ctx.fail(reason)` | Abort the tick with an error. |

Exactly one of `done` / `fail` must be called per tick. Exceptions thrown
from `run` are automatically converted to `fail(...)` by the SDK.

## Multi-graph composition

Merge N `GraphExport` documents into a single runnable graph. The input
is a JSON string describing the composition; the result is the composed
`GraphExport` JSON, ready for `Graph.fromJson(...)`.

```java
String request = """
{
  "graphs": [<GraphExport>, <GraphExport>],
  "connections": [{
    "from": { "process": "gsrc/src",   "port": "out" },
    "to":   { "process": "gsink/sink", "port": "in"  }
  }],
  "properties": { "name": "pipeline" },
  "case_sensitive": false
}
""";

String composedJson = MultiGraph.compose(request);
Graph composed = Graph.fromJson(composedJson);
Network net = Network.fromGraph(composed);
```

## Standard component catalog

The JNI native library ships the pure-Rust + `av-core` slice of
`reflow_components` — roughly 270 templates covering animation, flow
control, math, vector, 2D graphics, asset DB, scene graph, HTTP
integration, stream ops, DSP, and procedural generation. Heavy optional
palettes (GPU, ML, browser automation, video encoding, window events,
~6,700 API-service wrappers) are **not bundled** and install as
[actor packs](#actor-packs).

```java
long httpActor = Templates.templateActor("tpl_http_request");
net.registerActor("tpl_http_request", httpActor);
String ids = Templates.templateListJson();   // JSON array
```

Full catalog reference: [docs/components/standard-library.md](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md).

## Actor packs

Packs are `.rflpack` bundles that publish additional templates into the
runtime. `Templates.templateActor(id)` and `Templates.templateListJson()`
transparently include pack-supplied templates after load.

```kotlin
import ai.offbit.reflow.Packs
import ai.offbit.reflow.Templates

// Peek before committing.
println(Packs.inspectPack("./reflow.pack.ml-0.2.0.rflpack"))

// Load (idempotent).
Packs.loadPack("./reflow.pack.ml-0.2.0.rflpack")

val actor = Templates.templateActor("tpl_ml_run_inference")
net.registerActor("tpl_ml_run_inference", actor)

println(Packs.listPacks())
println(Packs.packAbiVersion())
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
path to `Packs.loadPack()`:

```sh
VER=0.2.0
curl -LO https://github.com/offbit-ai/reflow/releases/download/pack-v$VER/reflow.pack.ml-$VER.rflpack
```

Each `.rflpack` bundles every supported triple in one file — the
loader picks the right dylib at runtime. Catalog + per-pack contents:
[`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md).

Third-party packs are distributed however their author chooses (Maven
classified artifact, GitHub Releases, internal registry) — any local
file path works with `Packs.loadPack()`.

**ABI lockstep.** A pack is pinned to the rustc version of the JNI
library it was built against. Pick the `pack-v*` release whose version
matches your `libreflow_rt_jvm`; rebuild from source
([`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md))
if you need a pack for a different JNI version.

## Subgraphs

```java
try (var sub = new SubgraphBuilder(graphExportJson)) {
    sub.registerActor("my_custom", new MyCustom());
    sub.fillFromCatalog();
    long sg = sub.build();
    net.registerActor("tpl_sub", sg);
}
```

## Streams

Producer:

```java
Stream s = Stream.create(64, "a", "out", "image/jpeg");
s.sendBytes(frame1);
s.sendBytes(frame2);
s.end();
ctx.emit("out", s.intoMessage());
```

Consumer — given a `Message` whose kind is `StreamHandle`:

```java
try (var reader = msg.takeStream()) {
    for (;;) {
        var f = reader.recv(500);
        switch (f.kind) {
            case DATA     -> handle(f.data);
            case END, CLOSED, TIMEOUT -> { return; }
            case ERROR    -> throw new RuntimeException(f.error);
            case BEGIN    -> { /* metadata */ }
        }
    }
}
```

## Events

```java
try (var events = net.events()) {
    String json;
    while ((json = events.recv(200)) != null) {
        System.out.println(json);
    }
}
```

Subscribe **before** `net.start()` so no events are missed.

## Building locally

```sh
# Builds the JNI native library (target/release/libreflow_rt_jvm.dylib)
# and runs JUnit tests against it.
cd sdk/jvm
gradle test
```

The Gradle build depends on `cargo` and an installed JDK 17+.

## License

MIT OR Apache-2.0.
