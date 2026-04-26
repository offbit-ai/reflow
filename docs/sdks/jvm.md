# JVM SDK (Java + Kotlin)

`ai.offbit:reflow` — JDK 17+, single fat jar with native libs bundled as classpath resources for darwin / linux / windows.

## Install

```kotlin
// Gradle (Kotlin DSL)
dependencies {
    implementation("ai.offbit:reflow:0.2.2")
}
```

```xml
<!-- Maven -->
<dependency>
    <groupId>ai.offbit</groupId>
    <artifactId>reflow</artifactId>
    <version>0.2.2</version>
</dependency>
```

A JNI loader inside the jar detects the host triple at startup, extracts the matching `libreflow_rt_jvm.{dylib,so,dll}` to `java.io.tmpdir`, and `System.load`s it. No separate native-lib install step.

## Hello world (Kotlin DSL)

```kotlin
import ai.offbit.reflow.*

network {
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
    registerActor("tpl_doubler", doubler)
    addNode("d", "tpl_doubler")
    addInitial("d", "in", """{"type":"Integer","data":21}""")
    start()
}
```

## Hello world (Java)

```java
import ai.offbit.reflow.*;

class Doubler implements Actor {
    @Override public String component() { return "doubler"; }
    @Override public List<String> inports() { return List.of("in"); }
    @Override public List<String> outports() { return List.of("out"); }
    @Override public void run(ActorCallContext ctx) {
        long n = parseInteger(ctx.inputsJson(), "in");
        ctx.emit("out", Message.integer(n * 2));
        ctx.done();
    }
}

try (var net = new Network()) {
    net.registerActor("tpl_doubler", new Doubler());
    net.addNode("d", "tpl_doubler");
    net.addInitial("d", "in", "{\"type\":\"Integer\",\"data\":21}");
    net.start();
}
```

## Authoring graphs

```kotlin
val g = Graph("demo")
g.addNode("a", "tpl_x")
 .addNode("b", "tpl_y")
 .addConnection("a", "out", "b", "in")
 .addGroup("pipe", "[\"a\",\"b\"]", "{\"caption\":\"pipeline\"}")
 .renameNode("a", "alpha")

println(g.groupsJson())
println(g.connectionsJson())
```

The full graph API is mirrored on `Graph` — see [sdk/jvm/README.md](https://github.com/offbit-ai/reflow/blob/main/sdk/jvm/README.md) for the complete method list. Query methods (`*_json`) return `String`; pair with Jackson, Moshi, or `kotlinx.serialization` to parse.

## Bundled component catalog

```java
long httpActor = Templates.templateActor("tpl_http_request");
net.registerActor("tpl_http_request", httpActor);
String allTemplates = Templates.templateListJson();
```

## Packs

```kotlin
import ai.offbit.reflow.Packs

Packs.loadPack("./reflow.pack.ml-0.2.0.rflpack")
val infer = Templates.templateActor("tpl_ml_run_inference")
println(Packs.listPacks())
```

See [Packs](../packs/README.md) for the full pack workflow.

## Subgraphs

```kotlin
SubgraphBuilder(graphExportJson).use { sub ->
    sub.registerActor("my_custom", MyCustom())
    sub.fillFromCatalog()
    val sg = sub.build()
    net.registerActor("tpl_sub", sg)
}
```

## Streams

```kotlin
val stream = net.createStream(bufferSize = 64, contentType = "image/jpeg")
producer.emit(Message.stream(stream))
stream.reader().use { reader ->
    while (true) {
        val frame = reader.recv() ?: break
        println("${frame.kind} ${frame.data.size}")
    }
}
```

## API reference

- [Maven Central artifact page](https://central.sonatype.com/artifact/ai.offbit/reflow)
- [javadoc.io](https://javadoc.io/doc/ai.offbit/reflow/latest/index.html) — Dokka-rendered docs with the SDK README as the landing page.
- [Full JVM SDK README](https://github.com/offbit-ai/reflow/blob/main/sdk/jvm/README.md).

## See also

- [Authoring Actors](../api/actors/creating-actors.md).
- [Packs](../packs/README.md).
