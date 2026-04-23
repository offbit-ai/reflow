package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
// extern char* rfl_compose_graphs(const char* composition_json);
import "C"

import (
	"encoding/json"
	"fmt"
)

// ComposeRequest describes a multi-graph composition. Every field
// mirrors the C ABI's JSON contract.
type ComposeRequest struct {
	Graphs          []json.RawMessage      `json:"graphs"`
	Connections     []ComposeConnection    `json:"connections,omitempty"`
	SharedResources []ComposeShared        `json:"shared_resources,omitempty"`
	Properties      map[string]any         `json:"properties,omitempty"`
	CaseSensitive   *bool                  `json:"case_sensitive,omitempty"`
	Metadata        map[string]any         `json:"metadata,omitempty"`
}

type ComposeConnection struct {
	From     ComposeEndpoint `json:"from"`
	To       ComposeEndpoint `json:"to"`
	Metadata map[string]any  `json:"metadata,omitempty"`
}

type ComposeEndpoint struct {
	Process string `json:"process"`
	Port    string `json:"port"`
	Index   *int   `json:"index,omitempty"`
}

type ComposeShared struct {
	Name      string         `json:"name"`
	Component string         `json:"component"`
	Metadata  map[string]any `json:"metadata,omitempty"`
}

// ComposeGraphs merges N GraphExport documents into a single
// GraphExport, namespaces collisions, and wires cross-graph connections.
// The returned bytes are a GraphExport JSON document ready for
// LoadGraph.
func ComposeGraphs(req ComposeRequest) ([]byte, error) {
	raw, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("reflow.ComposeGraphs marshal: %w", err)
	}
	cs := cstr(string(raw))
	defer freeCStr(cs)
	out := C.rfl_compose_graphs(cs)
	if out == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(out)
	return []byte(C.GoString(out)), nil
}
