package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
)

// Graph wraps a reflow Graph handle.
type Graph struct {
	ptr *C.rfl_graph
}

// NewGraph allocates an empty graph.
func NewGraph(name string, caseSensitive bool) *Graph {
	cs := cstr(name)
	defer freeCStr(cs)
	var cf C.int
	if caseSensitive {
		cf = 1
	}
	p := C.rfl_graph_new(cs, cf)
	if p == nil {
		return nil
	}
	g := &Graph{ptr: p}
	runtime.SetFinalizer(g, (*Graph).Close)
	return g
}

// LoadGraph parses a GraphExport JSON document into a Graph.
func LoadGraph(raw []byte) (*Graph, error) {
	cs := cstr(string(raw))
	defer freeCStr(cs)
	p := C.rfl_graph_load_json(cs)
	if p == nil {
		return nil, lastError()
	}
	g := &Graph{ptr: p}
	runtime.SetFinalizer(g, (*Graph).Close)
	return g, nil
}

// Close frees the graph.
func (g *Graph) Close() {
	if g == nil || g.ptr == nil {
		return
	}
	C.rfl_graph_free(g.ptr)
	g.ptr = nil
	runtime.SetFinalizer(g, nil)
}

func nullableMetadata(md map[string]any) (*C.char, error) {
	if md == nil {
		return nil, nil
	}
	raw, err := json.Marshal(md)
	if err != nil {
		return nil, fmt.Errorf("metadata marshal: %w", err)
	}
	return cstr(string(raw)), nil
}

// AddNode adds a node referencing a component template id.
func (g *Graph) AddNode(id, component string, metadata map[string]any) error {
	cid := cstr(id)
	defer freeCStr(cid)
	cc := cstr(component)
	defer freeCStr(cc)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer func() {
		if md != nil {
			freeCStr(md)
		}
	}()
	st := C.rfl_graph_add_node(g.ptr, cid, cc, md)
	return statusToError(int32(st), "Graph.AddNode")
}

// RemoveNode removes a node by id.
func (g *Graph) RemoveNode(id string) error {
	cs := cstr(id)
	defer freeCStr(cs)
	st := C.rfl_graph_remove_node(g.ptr, cs)
	return statusToError(int32(st), "Graph.RemoveNode")
}

// AddConnection wires outPort of outNode into inPort of inNode.
func (g *Graph) AddConnection(outNode, outPort, inNode, inPort string, metadata map[string]any) error {
	a := cstr(outNode)
	defer freeCStr(a)
	b := cstr(outPort)
	defer freeCStr(b)
	c := cstr(inNode)
	defer freeCStr(c)
	d := cstr(inPort)
	defer freeCStr(d)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer func() {
		if md != nil {
			freeCStr(md)
		}
	}()
	st := C.rfl_graph_add_connection(g.ptr, a, b, c, d, md)
	return statusToError(int32(st), "Graph.AddConnection")
}

// AddInitial seeds an initial packet on a node's port.
func (g *Graph) AddInitial(node, port string, data any, metadata map[string]any) error {
	raw, err := json.Marshal(data)
	if err != nil {
		return fmt.Errorf("reflow.Graph.AddInitial marshal: %w", err)
	}
	a := cstr(node)
	defer freeCStr(a)
	b := cstr(port)
	defer freeCStr(b)
	c := cstr(string(raw))
	defer freeCStr(c)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer func() {
		if md != nil {
			freeCStr(md)
		}
	}()
	st := C.rfl_graph_add_initial(g.ptr, a, b, c, md)
	return statusToError(int32(st), "Graph.AddInitial")
}

// ToJSON serializes the graph to its GraphExport form.
func (g *Graph) ToJSON() ([]byte, error) {
	p := C.rfl_graph_to_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}
