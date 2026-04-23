package reflow

import (
	"encoding/json"
	"strings"
	"testing"
)

func TestComposeGraphsMergesTwoExports(t *testing.T) {
	leftProcs := map[string]any{
		"a": map[string]any{"id": "a", "component": "tpl_passthrough", "metadata": nil},
	}
	rightProcs := map[string]any{
		"b": map[string]any{"id": "b", "component": "tpl_passthrough", "metadata": nil},
	}
	mkExport := func(name string, procs map[string]any) []byte {
		b, _ := json.Marshal(map[string]any{
			"caseSensitive": false,
			"processes":     procs,
			"connections":   []any{},
			"inports":       map[string]any{},
			"outports":      map[string]any{},
			"properties":    map[string]any{"name": name},
			"groups":        []any{},
		})
		return b
	}

	req := ComposeRequest{
		Graphs:      []json.RawMessage{mkExport("left", leftProcs), mkExport("right", rightProcs)},
		Connections: nil,
		Properties:  map[string]any{"name": "composed"},
	}
	out, err := ComposeGraphs(req)
	if err != nil {
		t.Fatalf("ComposeGraphs: %v", err)
	}
	var v map[string]any
	if err := json.Unmarshal(out, &v); err != nil {
		t.Fatalf("unmarshal composed: %v", err)
	}
	processes, _ := v["processes"].(map[string]any)
	if processes == nil {
		t.Fatalf("no processes in composed: %s", out)
	}
	found := map[string]bool{}
	for k := range processes {
		if strings.HasSuffix(k, "/a") || k == "a" {
			found["a"] = true
		}
		if strings.HasSuffix(k, "/b") || k == "b" {
			found["b"] = true
		}
	}
	if !found["a"] || !found["b"] {
		t.Fatalf("expected composed to contain a + b, got keys: %v", processes)
	}
}

func TestComposeGraphsWithCrossConnection(t *testing.T) {
	gsrc, _ := json.Marshal(map[string]any{
		"caseSensitive": false,
		"processes":     map[string]any{"src": map[string]any{"id": "src", "component": "tpl_passthrough", "metadata": nil}},
		"connections":   []any{},
		"inports":       map[string]any{},
		"outports":      map[string]any{},
		"properties":    map[string]any{"name": "gsrc"},
		"groups":        []any{},
	})
	gsink, _ := json.Marshal(map[string]any{
		"caseSensitive": false,
		"processes":     map[string]any{"sink": map[string]any{"id": "sink", "component": "tpl_passthrough", "metadata": nil}},
		"connections":   []any{},
		"inports":       map[string]any{},
		"outports":      map[string]any{},
		"properties":    map[string]any{"name": "gsink"},
		"groups":        []any{},
	})

	out, err := ComposeGraphs(ComposeRequest{
		Graphs: []json.RawMessage{gsrc, gsink},
		Connections: []ComposeConnection{
			{
				From: ComposeEndpoint{Process: "gsrc/src", Port: "out"},
				To:   ComposeEndpoint{Process: "gsink/sink", Port: "in"},
			},
		},
		Properties: map[string]any{"name": "pipeline"},
	})
	if err != nil {
		t.Fatalf("ComposeGraphs: %v", err)
	}
	var v map[string]any
	_ = json.Unmarshal(out, &v)
	conns, _ := v["connections"].([]any)
	if len(conns) == 0 {
		t.Fatalf("expected at least one composed connection, got: %s", out)
	}
}
