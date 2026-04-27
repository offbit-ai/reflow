# Tutorial 04 — Concurrent worker pool over gRPC

Runnable code for [Real-world Reflow 04: A concurrent worker pool
over gRPC (Go)](../../../../docs/tutorials/real-world/04-grpc-service.md).

The server stands up a `Crawler.Crawl` server-streaming RPC. Each
call spins up a fresh Reflow network shaped like a fan-out worker
pool:

```
Dispatcher ─┬─► Fetcher_0 ─┐
            ├─► Fetcher_1 ─┤
            ├─► Fetcher_2 ─┤─► Sink ─► gRPC stream
            └─► Fetcher_N ─┘
```

The `Dispatcher` round-robins URLs onto N outports; each `Fetcher`
node is its own goroutine inside the runtime; the `Sink` writes a
`Page` to the gRPC stream for every result. Same shape as the
canonical `goroutines + channels` fan-out pattern, expressed as a
graph.

## Get the runtime library

```sh
go get github.com/offbit-ai/reflow/sdk/go@v0.2.3
cd "$(go env GOMODCACHE)/github.com/offbit-ai/reflow/sdk/go@v0.2.3"
./scripts/install_lib.sh v0.2.3
```

This pulls a prebuilt `libreflow_rt_capi` from the matching
GitHub Release and unpacks it into `sdk/go/lib/<platform>/` and
`sdk/go/include/`, where cgo will pick it up.

If you're working from a clone of the monorepo and want to test
against local Rust changes:

```sh
cargo build -p reflow_rt_capi
sdk/go/scripts/link_dev_lib.sh debug
```

## Run

```sh
# server
cd sdk/go/examples/tutorial-04-grpc-search/server
go run .

# in another terminal
cd sdk/go/examples/tutorial-04-grpc-search/client
go run . \
  https://en.wikipedia.org/wiki/Flow-based_programming \
  https://en.wikipedia.org/wiki/Actor_model \
  https://en.wikipedia.org/wiki/Dataflow_programming
```

Pass `-workers N` to the client to size the pool (default 4).
With no URL args, the client falls back to a 5-URL Wikipedia
sample.

## Regenerate the protobuf stubs

The repo ships pre-generated `.pb.go` files. To regenerate after
editing `proto/search.proto`:

```sh
go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
protoc --go_out=. --go_opt=paths=source_relative \
       --go-grpc_out=. --go-grpc_opt=paths=source_relative \
       proto/search.proto
```
