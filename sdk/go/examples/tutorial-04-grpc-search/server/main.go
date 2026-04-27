// Tutorial 04: gRPC Crawler service backed by a per-request Reflow
// worker pool.
//
// In vanilla Go this is the canonical "fan-out fetcher" pattern —
// a producer goroutine pushing URLs onto a buffered channel, N
// worker goroutines reading from it, a collector goroutine reading
// their results, plus a sync.WaitGroup tying it all together. The
// Reflow version expresses the same shape as a graph:
//
//   Dispatcher ─┬─► Fetcher_0 ─┐
//               ├─► Fetcher_1 ─┤
//               ├─► Fetcher_2 ─┤─► Sink ─► gRPC stream
//               └─► Fetcher_N ─┘
//
// Each per-request `Crawl` RPC builds a fresh network. The
// Dispatcher round-robins URLs onto N outports; each Fetcher node
// instantiates the same template, runs on its own actor task, and
// fetches concurrently. The Sink holds the gRPC stream handle and
// `Send`s a `Page` for every result it receives.
//
// What the runtime gives you:
//
//   • Bounded channels between every actor — backpressure is
//     automatic; a slow Sink throttles the Fetchers without any
//     manual rate-limiting code.
//   • Lifecycle in one place. `defer net.Close()` tears down all
//     workers; no goroutine leaks possible.
//   • Cancellation across the whole pool. When the gRPC client
//     hangs up, `stream.Context()` fires; we close the network and
//     every worker stops.
//
// Build the capi library first:
//
//   cargo build -p reflow_rt_capi --release
//   cd sdk/go/examples/tutorial-04-grpc-search/server && go run .

package main

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"regexp"
	"strings"
	"time"

	reflow "github.com/offbit-ai/reflow/sdk/go"
	pb "github.com/offbit-ai/reflow/sdk/go/examples/tutorial-04-grpc-search/proto"

	"google.golang.org/grpc"
)

const defaultWorkers = 4

// ─── Custom actors ─────────────────────────────────────────────────────────

// Dispatcher reads the URL list from its `urls` inport and round-robins
// each URL onto one of N outports (`worker_0`, `worker_1`, …).
//
// Round-robin is one line of code. In vanilla Go you'd push to a
// shared channel and let the worker goroutines compete; in Reflow
// each worker has its own inport, and the dispatcher routes
// explicitly. That makes the load policy visible — change
// round-robin to hash-by-host or "send to whoever's idle" by
// editing one method.
type Dispatcher struct {
	reflow.BaseActor
	workers int
}

func NewDispatcher(workers int) *Dispatcher {
	out := make([]string, workers)
	for i := range out {
		out[i] = fmt.Sprintf("worker_%d", i)
	}
	return &Dispatcher{
		BaseActor: reflow.BaseActor{
			ComponentName: "dispatcher",
			InportsList:   []string{"urls"},
			OutportsList:  out,
		},
		workers: workers,
	}
}

func (d *Dispatcher) Run(ctx *reflow.ActorContext) error {
	raw, ok := ctx.Input("urls").Data()
	if !ok {
		return fmt.Errorf("dispatch: missing urls payload")
	}
	var urls []string
	if err := json.Unmarshal(raw, &urls); err != nil {
		return fmt.Errorf("dispatch: %w", err)
	}
	for i, u := range urls {
		port := fmt.Sprintf("worker_%d", i%d.workers)
		if err := ctx.Emit(port, reflow.MessageString(u)); err != nil {
			return err
		}
	}
	return nil
}

// Fetcher pulls a URL from its `url` inport, fetches it, and emits a
// `page` packet with the response metadata. One Fetcher template,
// many node instances — each instance is its own goroutine inside
// the runtime.
type Fetcher struct {
	reflow.BaseActor
	client *http.Client
}

func NewFetcher() *Fetcher {
	return &Fetcher{
		BaseActor: reflow.BaseActor{
			ComponentName: "fetcher",
			InportsList:   []string{"url"},
			OutportsList:  []string{"page"},
		},
		client: &http.Client{Timeout: 10 * time.Second},
	}
}

var titleRe = regexp.MustCompile(`(?is)<title[^>]*>(.*?)</title>`)

func (f *Fetcher) Run(ctx *reflow.ActorContext) error {
	url, _ := ctx.Input("url").AsString()
	t0 := time.Now()
	page := map[string]any{
		"url":     url,
		"status":  uint32(0),
		"title":   "",
		"bytes":   uint64(0),
		"took_ms": uint64(0),
	}
	defer func() {
		page["took_ms"] = uint64(time.Since(t0).Milliseconds())
		msg, _ := reflow.MessageObject(page)
		_ = ctx.Emit("page", msg)
	}()
	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		page["title"] = err.Error()
		return nil
	}
	req.Header.Set("User-Agent", "reflow-tutorial-04/0.2 (https://github.com/offbit-ai/reflow)")
	resp, err := f.client.Do(req)
	if err != nil {
		page["title"] = err.Error()
		return nil
	}
	defer resp.Body.Close()
	page["status"] = uint32(resp.StatusCode)
	body, err := io.ReadAll(io.LimitReader(resp.Body, 256*1024))
	if err != nil {
		page["title"] = err.Error()
		return nil
	}
	page["bytes"] = uint64(len(body))
	if m := titleRe.FindSubmatch(body); m != nil {
		page["title"] = strings.TrimSpace(string(m[1]))
	}
	return nil
}

// Sink writes each page packet to the gRPC stream. The handler waits
// on `done` to know when the network has finished or the client has
// disconnected.
type Sink struct {
	reflow.BaseActor
	stream pb.Crawler_CrawlServer
	want   int
	got    int
	done   chan error
}

func NewSink(stream pb.Crawler_CrawlServer, want int) *Sink {
	return &Sink{
		BaseActor: reflow.BaseActor{
			ComponentName: "sink",
			InportsList:   []string{"page"},
		},
		stream: stream,
		want:   want,
		done:   make(chan error, 1),
	}
}

func (s *Sink) Run(ctx *reflow.ActorContext) error {
	raw, ok := ctx.Input("page").Data()
	if !ok {
		return nil
	}
	var p struct {
		URL    string `json:"url"`
		Status uint32 `json:"status"`
		Title  string `json:"title"`
		Bytes  uint64 `json:"bytes"`
		TookMs uint64 `json:"took_ms"`
	}
	if err := json.Unmarshal(raw, &p); err != nil {
		return err
	}
	if err := s.stream.Send(&pb.Page{
		Url:    p.URL,
		Status: p.Status,
		Title:  p.Title,
		Bytes:  p.Bytes,
		TookMs: p.TookMs,
	}); err != nil {
		// Client disconnected. Surface up so the handler can stop.
		select {
		case s.done <- err:
		default:
		}
		return nil
	}
	s.got++
	if s.got >= s.want {
		select {
		case s.done <- nil:
		default:
		}
	}
	return nil
}

// ─── gRPC handler ──────────────────────────────────────────────────────────

type server struct {
	pb.UnimplementedCrawlerServer
}

func (s *server) Crawl(req *pb.CrawlRequest, stream pb.Crawler_CrawlServer) error {
	if len(req.Urls) == 0 {
		return nil
	}
	workers := int(req.Workers)
	if workers == 0 {
		workers = defaultWorkers
	}

	rnet := reflow.NewNetwork()
	defer rnet.Close()

	dispatcher := NewDispatcher(workers)
	sink := NewSink(stream, len(req.Urls))

	if err := rnet.RegisterGoActor("tpl_dispatcher", dispatcher); err != nil {
		return err
	}
	if err := rnet.RegisterGoActor("tpl_fetcher", NewFetcher()); err != nil {
		return err
	}
	if err := rnet.RegisterGoActor("tpl_sink", sink); err != nil {
		return err
	}

	if err := rnet.AddNode("dispatch", "tpl_dispatcher", nil); err != nil {
		return err
	}
	if err := rnet.AddNode("collect", "tpl_sink", nil); err != nil {
		return err
	}
	for i := 0; i < workers; i++ {
		id := fmt.Sprintf("fetcher_%d", i)
		if err := rnet.AddNode(id, "tpl_fetcher", nil); err != nil {
			return err
		}
		if err := rnet.AddConnection("dispatch", fmt.Sprintf("worker_%d", i), id, "url"); err != nil {
			return err
		}
		if err := rnet.AddConnection(id, "page", "collect", "page"); err != nil {
			return err
		}
	}

	// Seed the dispatcher with the URL list.
	if err := rnet.AddInitial("dispatch", "urls", map[string]any{
		"type": "Array",
		"data": req.Urls,
	}); err != nil {
		return err
	}

	if err := rnet.Start(); err != nil {
		return err
	}

	select {
	case err := <-sink.done:
		return err
	case <-stream.Context().Done():
		return stream.Context().Err()
	}
}

func main() {
	lis, err := net.Listen("tcp", ":50404")
	if err != nil {
		log.Fatalf("listen: %v", err)
	}
	s := grpc.NewServer()
	pb.RegisterCrawlerServer(s, &server{})
	log.Println("listening on :50404")
	if err := s.Serve(lis); err != nil {
		log.Fatalf("serve: %v", err)
	}
}
