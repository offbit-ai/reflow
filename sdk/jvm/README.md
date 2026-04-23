# Reflow — JVM SDK (`ai.offbit.reflow`)

Reflow is a **modular flow-based programming runtime built on the actor model**. Graphs are declarative DAGs: each node is an actor with named in/out ports, edges route messages, and a network executor runs the whole thing with bounded backpressure and a tracing stream. It ships a standard library of ~300 actors covering data, media, GPU rendering, animation, I/O, and optional ML / CV — plus the hooks to register your own.

This module is the **JVM binding** (Java + Kotlin-friendly) to that runtime. It links to a native shared library built from `sdk/jvm/src/native` via JNI (written in Rust) and exposes idiomatic Java classes that mirror the Node / Go / Python SDKs one-for-one. Kotlin users get a trailing-lambda DSL and coroutine-friendly adapters on top.

- Group: `ai.offbit`
- Package: `ai.offbit.reflow`

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

```java
long httpActor = Templates.templateActor("tpl_http_request");
net.registerActor("tpl_http_request", httpActor);
String ids = Templates.templateListJson();   // JSON array
```

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
