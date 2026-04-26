# Reflow — Go SDK

Reflow is a **modular flow-based programming runtime built on the actor model**. Graphs are declarative DAGs: each node is an actor with named in/out ports, edges route messages, and a network executor runs the whole thing with bounded backpressure and a tracing stream. It ships a standard library of ~300 actors covering data, media, GPU rendering, animation, I/O, and optional ML / CV — plus the hooks to register your own.

This package is the **Go binding** to that runtime. It links to `reflow_rt_capi` via cgo and exposes idiomatic Go types that mirror the other language SDKs one-for-one.

```go
import reflow "github.com/offbit-ai/reflow/sdk/go"
```

## Requirements

- Go 1.21+
- A C toolchain (for cgo)
- `libreflow_rt_capi` for your platform — installed via one of two paths:

### Install (published)

```sh
go get github.com/offbit-ai/reflow/sdk/go@v0.2.2
cd "$(go env GOMODCACHE)/github.com/offbit-ai/reflow/sdk/go@v0.2.2"
./scripts/install_lib.sh v0.2.2
```

`install_lib.sh` downloads the matching tarball from the
[`sdk/go/vX.Y.Z` GitHub Release](https://github.com/offbit-ai/reflow/releases?q=sdk%2Fgo)
and unpacks it into `lib/<goos>_<goarch>/` and `include/` next to the
Go sources, where cgo can find them. The install is per-version, so
upgrading the module re-runs the script.

### Install (repo-local development)

If you've cloned the monorepo and want to test against your local
Rust changes:

```sh
cargo build -p reflow_rt_capi          # or --release
sdk/go/scripts/link_dev_lib.sh debug   # symlinks target/debug into sdk/go/lib/...
go test ./sdk/go/...
```

`link_dev_lib.sh` symlinks `target/<profile>/libreflow_rt_capi.*`
plus `crates/reflow_rt_capi/include/reflow_rt.h` into the same
`sdk/go/{lib,include}/` layout the published install uses, so cgo
sees identical paths in both setups.

## Quick start

```go
package main

import (
    "fmt"
    "time"
    reflow "github.com/offbit-ai/reflow/sdk/go"
)

type Doubler struct{ reflow.BaseActor }

func NewDoubler() *Doubler {
    return &Doubler{BaseActor: reflow.BaseActor{
        ComponentName: "doubler",
        InportsList:   []string{"in"},
        OutportsList:  []string{"out"},
    }}
}

func (d *Doubler) Run(ctx *reflow.ActorContext) error {
    in := ctx.Input("in")
    if in == nil { return nil }
    n, _ := in.AsInteger()
    return ctx.Emit("out", reflow.MessageInteger(n * 2))
}

func main() {
    net := reflow.NewNetwork()
    defer net.Close()

    _ = net.RegisterGoActor("tpl_doubler", NewDoubler())
    _ = net.AddNode("a", "tpl_doubler", nil)
    _ = net.AddInitial("a", "in", map[string]any{"type": "Integer", "data": 21})

    _ = net.Start()
    time.Sleep(200 * time.Millisecond)
    _ = net.Shutdown()
    _ = fmt.Sprintln("done")
}
```

## Authoring actors

Embed `BaseActor` in your struct and implement `Run`. The static port declarations become fields on the embedded `BaseActor`:

```go
type Sum struct {
    reflow.BaseActor
}

func NewSum() *Sum {
    return &Sum{BaseActor: reflow.BaseActor{
        ComponentName: "sum",
        InportsList:   []string{"a", "b"},
        OutportsList:  []string{"sum"},
        AwaitAll:      true,   // buffer both inputs before firing
    }}
}

func (s *Sum) Run(ctx *reflow.ActorContext) error {
    a, _ := ctx.Input("a").AsInteger()
    b, _ := ctx.Input("b").AsInteger()
    return ctx.Emit("sum", reflow.MessageInteger(a + b))
}
```

Inside `Run`:

| Call | Purpose |
|------|---------|
| `ctx.Input(port)` | Take the inbound message on `port` (returns nil if absent). |
| `ctx.HasInput(port)` | Peek without taking. |
| `ctx.ConfigJSON()` / `ctx.Config()` | Read node-level config. |
| `ctx.StateGet(key)` / `ctx.StateSet(key, v)` | Per-actor `MemoryState`. |
| `ctx.Emit(port, msg)` | Emit an output packet (transfers message ownership). |

Return `nil` on success, or an error to fail this tick.

## Multi-graph composition

Merge N `GraphExport` documents into a single runnable graph:

```go
left  := []byte(`{"caseSensitive":false,"processes":{...},...}`)
right := []byte(`{"caseSensitive":false,"processes":{...},...}`)

composed, err := reflow.ComposeGraphs(reflow.ComposeRequest{
    Graphs: []json.RawMessage{left, right},
    Connections: []reflow.ComposeConnection{
        {
            From: reflow.ComposeEndpoint{Process: "gsrc/src",   Port: "out"},
            To:   reflow.ComposeEndpoint{Process: "gsink/sink", Port: "in"},
        },
    },
    Properties: map[string]any{"name": "pipeline"},
})

g, _ := reflow.LoadGraph(composed)
net := reflow.NewNetworkFromGraph(g)
```

## Standard component catalog

The linked shared library ships the pure-Rust + `av-core` slice of
`reflow_components` — roughly 270 templates covering animation, flow
control, math, vector, 2D graphics, asset DB, scene graph, HTTP
integration, stream ops, DSP, and procedural generation. Heavy optional
palettes (GPU, ML, browser automation, video encoding, window events,
~6,700 API-service wrappers) are **not bundled** and install as
[actor packs](#actor-packs).

```go
ids, _ := reflow.TemplateList()
actor, _ := reflow.TemplateActor("tpl_http_request")
net.RegisterActor("tpl_http_request", actor)
```

Full catalog reference: [docs/components/standard-library.md](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md).

## Actor packs

Packs are `.rflpack` bundles that publish additional templates into the
runtime. `TemplateActor(id)` and `TemplateList()` transparently include
pack-supplied templates after load.

```go
// Peek before committing.
m, _ := reflow.InspectPack("./reflow.pack.ml-0.2.0.rflpack")
fmt.Println(m.Name, m.Templates)

// Load (idempotent).
templates, _ := reflow.LoadPack("./reflow.pack.ml-0.2.0.rflpack")
fmt.Println(templates)

actor, _ := reflow.TemplateActor("tpl_ml_run_inference")
_ = net.RegisterActor("tpl_ml_run_inference", actor)

packs, _ := reflow.ListPacks()
fmt.Println(packs, reflow.PackABIVersion())
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
path to `LoadPack()`:

```sh
VER=0.2.0
curl -LO https://github.com/offbit-ai/reflow/releases/download/pack-v$VER/reflow.pack.ml-$VER.rflpack
```

Each `.rflpack` bundles every supported triple in one file — the
loader picks the right dylib at runtime. Catalog + per-pack contents:
[`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md).

Third-party packs are distributed however their author chooses (module
data dir, GitHub Releases, internal registry) — any local file path
works with `LoadPack()`.

**ABI lockstep.** A pack is pinned to the rustc version of the runtime
it was built against. Pick the `pack-v*` release whose version matches
your `libreflow_rt_capi`; rebuild from source
([`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md))
if you need a pack for a different runtime version.

## Subgraphs

```go
export := []byte(`{ "caseSensitive": false, "processes": { ... }, ... }`)
b, _ := reflow.NewSubgraphBuilder(export)
_ = b.RegisterGoActor("my_custom", NewCustom())
_ = b.FillFromCatalog()           // resolve remaining from bundled catalog
sg, _ := b.Build()
_ = net.RegisterActor("tpl_sub", sg)
```

## Streams

```go
s := reflow.NewStream(reflow.StreamOptions{BufferSize: 64, ContentType: "image/jpeg"})
_ = s.SendBytes(frame1)
_ = s.SendBytes(frame2)
_ = s.End()
_ = ctx.Emit("out", s.IntoMessage())
```

Consumer side:

```go
reader, _ := ctx.Input("frames").TakeStream()
defer reader.Close()
for {
    f, _ := reader.Recv(500 * time.Millisecond)
    switch f.Kind {
    case reflow.FrameData:    handle(f.Data)
    case reflow.FrameEnd:     return
    case reflow.FrameClosed, reflow.FrameTimeout: return
    case reflow.FrameError:   return errors.New(f.Error)
    }
}
```

## Events

```go
events := net.Events()
defer events.Close()

for {
    evt, err := events.Recv(200 * time.Millisecond)
    if err != nil { break }
    if evt == nil { continue } // timeout
    fmt.Println(evt["_type"], evt)
}
```

Subscribe **before** `net.Start()` so no events are missed.

## Testing

```sh
cargo build -p reflow_rt_capi --release
cd sdk/go
go test ./...
```

## License

MIT OR Apache-2.0.
