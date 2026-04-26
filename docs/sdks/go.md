# Go SDK

`github.com/offbit-ai/reflow/sdk/go` — Go 1.21+, links the runtime via cgo.

Unlike the Node / Python / JVM SDKs, the Go module **doesn't bundle the native runtime**. You install the Go module from source, then drop the matching `libreflow_rt_capi.{so,dylib,dll}` next to it.

## Install

```sh
go get github.com/offbit-ai/reflow/sdk/go@v0.2.1

# After go get, run the bundled installer to fetch the matching native lib:
cd "$(go env GOMODCACHE)/github.com/offbit-ai/reflow/sdk/go@v0.2.1"
./scripts/install_lib.sh v0.2.1
```

`install_lib.sh` downloads the per-triple tarball from the [`sdk/go/v*` GitHub Release](https://github.com/offbit-ai/reflow/releases?q=sdk%2Fgo) and unpacks it into `lib/<goos>_<goarch>/` and `include/`, where cgo finds it at compile time.

For repo-local development (you've cloned the monorepo and want to test against your local Rust changes), use `scripts/link_dev_lib.sh` instead — it symlinks `target/<profile>/libreflow_rt_capi.*` into the same `sdk/go/{lib,include}/` layout.

## Hello world

```go
package main

import (
    "fmt"
    "time"
    reflow "github.com/offbit-ai/reflow/sdk/go"
)

type Doubler struct{ reflow.BaseActor }

func newDoubler() *Doubler {
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
    return ctx.Emit("out", reflow.MessageInteger(n*2))
}

func main() {
    net := reflow.NewNetwork()
    defer net.Close()
    _ = net.RegisterActor("tpl_doubler", newDoubler())
    _ = net.AddNode("a", "tpl_doubler", nil)
    _ = net.AddInitial("a", "in", map[string]any{"type": "Integer", "data": 21}, nil)
    _ = net.Start()
    time.Sleep(200 * time.Millisecond)

    fmt.Println("done")
}
```

## Authoring graphs

```go
g := reflow.NewGraph("demo", false)
defer g.Close()
_ = g.AddNode("a", "tpl_x", nil)
_ = g.AddNode("b", "tpl_y", nil)
_ = g.AddConnection("a", "out", "b", "in", nil)
_ = g.AddGroup("pipe", []string{"a", "b"}, map[string]any{"caption": "pipeline"})
_ = g.RenameNode("a", "alpha")

groupsJSON, _ := g.GroupsJSON()  // []byte; parse with encoding/json
```

The full graph API is mirrored — see [sdk/go/README.md](https://github.com/offbit-ai/reflow/blob/main/sdk/go/README.md) for the complete method list. Read-side methods all return `[]byte` (JSON) so callers pick their own decoder.

## Bundled component catalog

```go
ids, _ := reflow.TemplateList()
actor, _ := reflow.TemplateActor("tpl_http_request")
_ = net.RegisterActor("tpl_http_request", actor)
```

## Packs

```go
templates, _ := reflow.LoadPack("./reflow.pack.ml-0.2.0.rflpack")
fmt.Println("loaded:", templates)
infer, _ := reflow.TemplateActor("tpl_ml_run_inference")
```

See [Packs](../packs/README.md) for the full pack workflow.

## Subgraphs

```go
b, _ := reflow.NewSubgraphBuilder(exportJSON)
_ = b.RegisterGoActor("my_custom", NewCustom())
_ = b.FillFromCatalog()
sg, _ := b.Build()
_ = net.RegisterActor("tpl_sub", sg)
```

## Streams

```go
s := reflow.NewStream(reflow.StreamOptions{BufferSize: 64, ContentType: "image/jpeg"})
producer.Emit("stream", reflow.MessageStream(s))
reader := s.Reader()
for {
    frame, err := reader.Recv()
    if err != nil { break }
    fmt.Println(frame.Kind, len(frame.Data))
}
```

## See also

- [Full Go SDK README](https://github.com/offbit-ai/reflow/blob/main/sdk/go/README.md) — install troubleshooting, cgo notes.
- [Authoring Actors](../api/actors/creating-actors.md).
- [Packs](../packs/README.md).
