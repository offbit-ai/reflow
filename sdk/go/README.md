# Reflow — Go SDK

Reflow is a **modular flow-based programming runtime built on the actor model**. Graphs are declarative DAGs: each node is an actor with named in/out ports, edges route messages, and a network executor runs the whole thing with bounded backpressure and a tracing stream. It ships a standard library of ~300 actors covering data, media, GPU rendering, animation, I/O, and optional ML / CV — plus the hooks to register your own.

This package is the **Go binding** to that runtime. It links to `reflow_rt_capi` via cgo and exposes idiomatic Go types that mirror the other language SDKs one-for-one.

```go
import reflow "github.com/offbit-ai/reflow/sdk/go"
```

## Requirements

- Go 1.21+
- The `reflow_rt_capi` shared library built from the parent repository:

  ```sh
  cargo build -p reflow_rt_capi --release
  ```

  During development the SDK links against `target/debug/libreflow_rt_capi.dylib` / `.so` relative to this directory. For production, point `CGO_LDFLAGS` at the release artifact shipped with your release.

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

```go
ids, _ := reflow.TemplateList()
actor, _ := reflow.TemplateActor("tpl_http_request")
net.RegisterActor("tpl_http_request", actor)
```

Catalog reference: [docs/components/standard-library.md](https://github.com/offbit-ai/reflow/blob/main/docs/components/standard-library.md) (~300 templates).

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
