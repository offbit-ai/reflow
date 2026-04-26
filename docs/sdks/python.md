# Python SDK

`offbit-reflow` — Python 3.9+, abi3 wheels for darwin / linux / windows.

## Install

```sh
pip install offbit-reflow
```

PyPI ships pre-built wheels (one per platform, abi3-py39 so 3.9–3.13+ work from the same wheel). No build dependencies on the user side.

## Hello world

```python
import offbit_reflow as reflow

class Doubler(reflow.Actor):
    component = "doubler"
    inports = ["in"]
    outports = ["out"]

    async def run(self, ctx):
        n = ctx.inputs.get("in", {}).get("data", 0)
        ctx.emit("out", reflow.Message.integer(int(n) * 2))

net = reflow.Network()
net.register_actor("tpl_doubler", Doubler())
net.add_node("a", "tpl_doubler")
net.add_initial("a", "in", {"type": "Integer", "data": 21})
net.start()

for ev in net.events():
    print(ev)
    if ev["_type"] == "ActorCompleted":
        break
```

## Authoring graphs

```python
g = reflow.Graph("demo")
g.add_node("a", "tpl_x")
g.add_node("b", "tpl_y")
g.add_connection("a", "out", "b", "in")
g.add_group("pipe", ["a", "b"], {"caption": "pipeline"})
g.rename_node("a", "alpha")
print(g.groups())
print(g.connections())
```

Full graph API: rename / port lifecycle / metadata setters / group CRUD / queries — same surface across every SDK. See [sdk/python/README.md](https://github.com/offbit-ai/reflow/blob/main/sdk/python/README.md) for the complete signature list.

## Bundled component catalog

The wheel ships ~270 pure-Rust + `av-core` templates. Heavier palettes (GPU, ML, browser, video, window events, ~6,700 API actors) install as [`.rflpack`](../packs/README.md) bundles.

```python
print([t for t in reflow.template_list() if t.startswith("tpl_math_")])

reflow.load_pack("./reflow.pack.ml-0.2.0.rflpack")
infer = reflow.template_actor("tpl_ml_run_inference")
```

## Subgraphs

```python
sub = reflow.SubgraphBuilder(graph_export_json)
sub.register_actor("my_custom", MyCustom())
sub.fill_from_catalog()
net.register_actor("tpl_sub", sub.build())
```

## Streams

```python
stream = net.create_stream(buffer_size=64, content_type="image/jpeg")
producer.emit({"stream": stream})
for frame in stream:
    print(frame["kind"], len(frame.get("data", b"")))
```

## See also

- [Full Python SDK README](https://github.com/offbit-ai/reflow/blob/main/sdk/python/README.md) — complete API + GIL notes.
- [Authoring Actors](../api/actors/creating-actors.md).
- [Packs](../packs/README.md).
