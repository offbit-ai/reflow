package reflow

import (
	"strings"
	"testing"
	"time"
)

// First-class tracing in the Go SDK: enable via config, consume locally via
// Network.Traces() (no collector required), and observe correlated events with
// content checksums. Reuses newDoubler/newCollect from reflow_test.go.
func TestNetworkTracing(t *testing.T) {
	net, err := NewNetworkWithConfig(map[string]any{
		"tracing": map[string]any{
			"server_url": "ws://127.0.0.1:8080",
			"enabled":    true,
		},
	})
	if err != nil {
		t.Fatalf("NewNetworkWithConfig: %v", err)
	}
	defer net.Close()

	d := newDoubler()
	c := newCollect()
	if err := net.RegisterGoActor("tpl_doubler", d); err != nil {
		t.Fatalf("register doubler: %v", err)
	}
	if err := net.RegisterGoActor("tpl_collect", c); err != nil {
		t.Fatalf("register collect: %v", err)
	}
	if err := net.AddNode("a", "tpl_doubler", nil); err != nil {
		t.Fatalf("AddNode a: %v", err)
	}
	if err := net.AddNode("b", "tpl_collect", nil); err != nil {
		t.Fatalf("AddNode b: %v", err)
	}
	if err := net.AddConnection("a", "out", "b", "in"); err != nil {
		t.Fatalf("AddConnection: %v", err)
	}
	if err := net.AddInitial("a", "in", map[string]any{"type": "Integer", "data": 21}); err != nil {
		t.Fatalf("AddInitial: %v", err)
	}

	traces := net.Traces()
	if traces == nil {
		t.Fatal("Traces() returned nil — tracing not enabled?")
	}
	defer traces.Close()

	if err := net.Start(); err != nil {
		t.Fatalf("Start: %v", err)
	}

	seen := map[string]bool{}
	sawChecksum := false
	deadline := time.Now().Add(3 * time.Second)
	for time.Now().Before(deadline) {
		evt, err := traces.Recv(200 * time.Millisecond)
		if err != nil {
			break
		}
		if evt == nil {
			continue // timeout
		}
		switch et := evt["event_type"].(type) {
		case string:
			seen[et] = true
		case map[string]any:
			for k := range et {
				seen[k] = true
			}
		}
		if data, ok := evt["data"].(map[string]any); ok {
			if msg, ok := data["message"].(map[string]any); ok {
				if cs, ok := msg["checksum"].(string); ok && strings.HasPrefix(cs, "sha256:") {
					sawChecksum = true
				}
			}
		}
		if seen["ActorCreated"] && sawChecksum {
			break
		}
	}
	_ = net.Shutdown()

	if !seen["ActorCreated"] {
		t.Errorf("expected ActorCreated trace event; seen=%v", seen)
	}
	if !sawChecksum {
		t.Error("expected a data-flow snapshot carrying a sha256 checksum")
	}
}
