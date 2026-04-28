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
| `ctx.emit(port, message)` | Queue an output packet. Per-tick drain on `done` — multiple emits to the same port collapse to the last write. |
| `ctx.send({port: message, ...})` | Mid-tick flush — push straight to the outport channel. Use for streaming actors that emit many packets per tick. |
| `ctx.done(outputs=None)` | Emit outputs keyed by output port. Values are `Message` instances or JSON-shaped Messages. |
| `ctx.fail(message)` | Abort this tick with an error. |
| `ctx.pool_upsert(name, id, value)` | Per-actor `{id: value}` map that persists across ticks. The right tool for variable fan-in: N upstreams write under stable ids, the consumer reads the whole map. |
| `ctx.pool_remove(name, id)` / `ctx.pool(name)` / `ctx.pool_count(name)` / `ctx.pool_clear(name)` | Drop / read (returns dict) / size / wipe a pool. |

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

The wheel ships the pure-Rust + `av-core` slice of `reflow_components`
— roughly 270 templates covering animation, flow control, math, vector,
2D graphics, asset DB, scene graph, HTTP integration, stream ops, DSP,
and procedural generation. Heavy optional palettes (GPU, ML, browser
automation, video encoding, window events, ~6,700 API-service wrappers)
are **not bundled** and install as [actor packs](#actor-packs).

```python
from offbit_reflow import template_actor, template_list

net.register_actor("tpl_http_request", template_actor("tpl_http_request"))
print([tid for tid in template_list() if tid.startswith("tpl_math_")])
```

Full catalog reference: [docs/components/standard-library.md](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md).

## Actor packs

Packs are `.rflpack` bundles that publish additional templates into
this SDK at runtime. `template_actor(id)` and `template_list()`
transparently include pack-supplied templates after load.

```python
import offbit_reflow as reflow

# Peek before committing.
print(reflow.inspect_pack("./reflow.pack.ml-0.2.0.rflpack"))

# Load (idempotent).
reflow.load_pack("./reflow.pack.ml-0.2.0.rflpack")

# Pack-owned templates now resolve normally.
net.register_actor("tpl_ml_run_inference",
                   reflow.template_actor("tpl_ml_run_inference"))

print(reflow.list_packs())
print(reflow.pack_abi_version())
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
[`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md).

`load_pack()` accepts either flavour identically — it picks the
binary that matches the runtime triple at load time.

To slim a downloaded full bundle yourself, install the
`reflow_pack_cli` crate and run:

```sh
reflow-pack strip reflow.pack.ml-0.2.0.rflpack
# → reflow.pack.ml-0.2.0-<host-triple>.rflpack
```

Third-party packs are distributed however their author chooses (PyPI
data files, GitHub Releases, internal registry) — any local file path
works with `load_pack()`.

**ABI lockstep.** A pack is pinned to the SDK release it was built
against. Pick the `pack-v*` release whose version matches your
`offbit-reflow`; rebuild from source
([`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md))
if you need a pack for a different SDK version.

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

## Releasing

Releases are built and published by CI — see
`.github/workflows/publish-python.yml`. Tag a commit with
`python-v<version>` (e.g. `python-v0.2.0`) and the workflow builds
wheels for every supported triple (linux x86_64/aarch64, macOS
x86_64/aarch64, windows x64), plus an sdist, verifies metadata,
smoke-tests the wheel on each host, and uploads everything to PyPI.

Publishing currently uses an API token stored as the `PYPI_API_TOKEN`
repository secret. Migration to PyPI trusted publishing (OIDC) is a
one-line swap once the first release is live.

## License

MIT OR Apache-2.0.
