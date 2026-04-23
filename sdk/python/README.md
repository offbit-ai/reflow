# offbit-reflow — Python SDK for Reflow

Reflow is a **modular flow-based programming runtime built on the actor model**. Graphs are declarative DAGs: each node is an actor with named in/out ports, edges route messages, and a network executor runs the whole thing with bounded backpressure and a tracing stream. It ships a standard library of ~300 actors covering data, media, GPU rendering, animation, I/O, and optional ML / CV — plus the hooks to register your own.

This package is the official **Python SDK**. It wraps the runtime via `pyo3` and exposes idiomatic Python classes that mirror the Node / Go SDKs one-for-one.

```sh
pip install offbit-reflow
```

```python
from offbit_reflow import Actor, Network, Message
```

## Quick start

```python
from offbit_reflow import Actor, Network, Message

class Doubler(Actor):
    component = "doubler"
    inports = ["in"]
    outports = ["out"]

    def run(self, ctx):
        n = ctx.inputs["in"]["data"]
        ctx.done({"out": Message.integer(n * 2)})

class Log(Actor):
    component = "log"
    inports = ["in"]
    outports = []

    def run(self, ctx):
        print("got:", ctx.inputs["in"])
        ctx.done()

net = Network()
net.register_actor("tpl_doubler", Doubler())
net.register_actor("tpl_log", Log())

net.add_node("a", "tpl_doubler")
net.add_node("b", "tpl_log")
net.add_connection("a", "out", "b", "in")
net.add_initial("a", "in", {"type": "Integer", "data": 21})

net.start()
# ... later:
net.shutdown()
```

## Authoring actors

Subclass `Actor`. Class-level attributes declare ports and await semantics; the instance `run(ctx)` method is the per-tick body:

```python
class Sum(Actor):
    component = "sum"
    inports = ["a", "b"]
    outports = ["sum"]
    await_all_inports = True

    def run(self, ctx):
        a = ctx.inputs["a"]["data"]
        b = ctx.inputs["b"]["data"]
        ctx.done({"sum": Message.integer(a + b)})
```

Inside `run(ctx)`:

| Member | Purpose |
|--------|---------|
| `ctx.inputs` | `dict` keyed by port — each entry is a JSON-shaped Message. |
| `ctx.config` | Per-node config passed at graph time. |
| `ctx.done(outputs=None)` | Emit outputs keyed by output port. Values are `Message` instances or JSON-shaped Messages. |
| `ctx.fail(message)` | Abort this tick with an error. |

Exactly one of `done` / `fail` must be called per tick. If `run` raises, the SDK calls `fail` with the exception's message.

## Multi-graph composition

Merge N `GraphExport` dicts into a single runnable graph:

```python
from offbit_reflow import compose_graphs, Graph, Network

composed = compose_graphs({
    "graphs": [left_export, right_export],   # dicts
    "connections": [
        {"from": {"process": "gsrc/src",   "port": "out"},
         "to":   {"process": "gsink/sink", "port": "in"}},
    ],
    "shared_resources": [],
    "properties": {"name": "pipeline"},
    "case_sensitive": False,
})

g = Graph.from_json(composed)
net = Network.from_graph(g)
```

## Standard component catalog

```python
from offbit_reflow import template_actor, template_list

net.register_actor("tpl_http_request", template_actor("tpl_http_request"))
print([tid for tid in template_list() if tid.startswith("tpl_math_")])
```

The catalog is documented at [docs/components/standard-library.md](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md) (~300 templates).

## Subgraphs

```python
from offbit_reflow import SubgraphBuilder

sub = SubgraphBuilder(graph_export_json)   # dict or parsed object
sub.register_actor("my_custom", MyCustom())
sub.fill_from_catalog()                    # resolve bundled components
sg = sub.build()
net.register_actor("tpl_sub", sg)
```

## Streams

Producer side:

```python
from offbit_reflow import Stream

s = Stream.create(buffer_size=64, content_type="image/jpeg")
s.send_bytes(frame1)
s.send_bytes(frame2)
s.end()
ctx.done({"out": s.into_message()})
```

Consumer side:

```python
rdr = ctx.inputs["frames"].take_stream()
while True:
    f = rdr.recv(500)
    if f["kind"] == "data":
        handle(f["data"])
    elif f["kind"] == "end":
        break
    elif f["kind"] in ("closed", "timeout"):
        break
    elif f["kind"] == "error":
        raise RuntimeError(f["error"])
```

## Events

```python
events = net.events()
while True:
    evt = events.recv(timeout_ms=200)
    if evt is None:
        continue
    print(evt.get("_type"), evt)
```

Subscribe **before** `net.start()` so no events are missed.

## Building locally

```sh
cd sdk/python
python -m venv .venv && source .venv/bin/activate
pip install maturin pytest
maturin develop
pytest -q
```

## License

MIT OR Apache-2.0.
