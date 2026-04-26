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

// ─── Mutators (renames) ────────────────────────────────────────────────────

// RenameNode changes a node's id and updates every connection that
// referenced it.
func (g *Graph) RenameNode(oldID, newID string) error {
	a := cstr(oldID)
	defer freeCStr(a)
	b := cstr(newID)
	defer freeCStr(b)
	st := C.rfl_graph_rename_node(g.ptr, a, b)
	return statusToError(int32(st), "Graph.RenameNode")
}

// RenameInport renames a graph-level inport (subgraph boundary).
func (g *Graph) RenameInport(oldPort, newPort string) error {
	a := cstr(oldPort)
	defer freeCStr(a)
	b := cstr(newPort)
	defer freeCStr(b)
	st := C.rfl_graph_rename_inport(g.ptr, a, b)
	return statusToError(int32(st), "Graph.RenameInport")
}

// RenameOutport renames a graph-level outport.
func (g *Graph) RenameOutport(oldPort, newPort string) error {
	a := cstr(oldPort)
	defer freeCStr(a)
	b := cstr(newPort)
	defer freeCStr(b)
	st := C.rfl_graph_rename_outport(g.ptr, a, b)
	return statusToError(int32(st), "Graph.RenameOutport")
}

// ─── Mutators (port lifecycle) ─────────────────────────────────────────────

// AddInport exposes a node's inport at the graph level. portType may be
// nil for `Any`, otherwise any value json.Marshal-able to a PortType.
func (g *Graph) AddInport(portID, nodeID, portKey string, portType any, metadata map[string]any) error {
	pid := cstr(portID)
	defer freeCStr(pid)
	nid := cstr(nodeID)
	defer freeCStr(nid)
	pk := cstr(portKey)
	defer freeCStr(pk)
	pt, err := nullableJSON(portType)
	if err != nil {
		return err
	}
	defer freeIfNotNil(pt)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeIfNotNil(md)
	st := C.rfl_graph_add_inport(g.ptr, pid, nid, pk, pt, md)
	return statusToError(int32(st), "Graph.AddInport")
}

// AddOutport exposes a node's outport at the graph level.
func (g *Graph) AddOutport(portID, nodeID, portKey string, portType any, metadata map[string]any) error {
	pid := cstr(portID)
	defer freeCStr(pid)
	nid := cstr(nodeID)
	defer freeCStr(nid)
	pk := cstr(portKey)
	defer freeCStr(pk)
	pt, err := nullableJSON(portType)
	if err != nil {
		return err
	}
	defer freeIfNotNil(pt)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeIfNotNil(md)
	st := C.rfl_graph_add_outport(g.ptr, pid, nid, pk, pt, md)
	return statusToError(int32(st), "Graph.AddOutport")
}

func (g *Graph) RemoveInport(portID string) error {
	cs := cstr(portID)
	defer freeCStr(cs)
	st := C.rfl_graph_remove_inport(g.ptr, cs)
	return statusToError(int32(st), "Graph.RemoveInport")
}

func (g *Graph) RemoveOutport(portID string) error {
	cs := cstr(portID)
	defer freeCStr(cs)
	st := C.rfl_graph_remove_outport(g.ptr, cs)
	return statusToError(int32(st), "Graph.RemoveOutport")
}

// ─── Mutators (groups) ─────────────────────────────────────────────────────

func (g *Graph) AddGroup(groupID string, nodes []string, metadata map[string]any) error {
	gid := cstr(groupID)
	defer freeCStr(gid)
	rawNodes, err := json.Marshal(nodes)
	if err != nil {
		return fmt.Errorf("reflow.Graph.AddGroup nodes marshal: %w", err)
	}
	cn := cstr(string(rawNodes))
	defer freeCStr(cn)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeIfNotNil(md)
	st := C.rfl_graph_add_group(g.ptr, gid, cn, md)
	return statusToError(int32(st), "Graph.AddGroup")
}

func (g *Graph) RemoveGroup(groupID string) error {
	cs := cstr(groupID)
	defer freeCStr(cs)
	st := C.rfl_graph_remove_group(g.ptr, cs)
	return statusToError(int32(st), "Graph.RemoveGroup")
}

func (g *Graph) AddToGroup(groupID, nodeID string) error {
	a := cstr(groupID)
	defer freeCStr(a)
	b := cstr(nodeID)
	defer freeCStr(b)
	st := C.rfl_graph_add_to_group(g.ptr, a, b)
	return statusToError(int32(st), "Graph.AddToGroup")
}

func (g *Graph) RemoveFromGroup(groupID, nodeID string) error {
	a := cstr(groupID)
	defer freeCStr(a)
	b := cstr(nodeID)
	defer freeCStr(b)
	st := C.rfl_graph_remove_from_group(g.ptr, a, b)
	return statusToError(int32(st), "Graph.RemoveFromGroup")
}

// ─── Mutators (connection / initial removal + indexed initials) ────────────

func (g *Graph) RemoveConnection(outNode, outPort, inNode, inPort string) error {
	a := cstr(outNode)
	defer freeCStr(a)
	b := cstr(outPort)
	defer freeCStr(b)
	c := cstr(inNode)
	defer freeCStr(c)
	d := cstr(inPort)
	defer freeCStr(d)
	st := C.rfl_graph_remove_connection(g.ptr, a, b, c, d)
	return statusToError(int32(st), "Graph.RemoveConnection")
}

func (g *Graph) RemoveInitial(node, port string) error {
	a := cstr(node)
	defer freeCStr(a)
	b := cstr(port)
	defer freeCStr(b)
	st := C.rfl_graph_remove_initial(g.ptr, a, b)
	return statusToError(int32(st), "Graph.RemoveInitial")
}

func (g *Graph) AddInitialIndex(node, port string, data any, index uint, metadata map[string]any) error {
	raw, err := json.Marshal(data)
	if err != nil {
		return fmt.Errorf("reflow.Graph.AddInitialIndex marshal: %w", err)
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
	defer freeIfNotNil(md)
	st := C.rfl_graph_add_initial_index(g.ptr, a, b, c, C.size_t(index), md)
	return statusToError(int32(st), "Graph.AddInitialIndex")
}

// AddGraphInitial pushes an initial packet through one of the graph's
// exposed inports.
func (g *Graph) AddGraphInitial(inport string, data any, metadata map[string]any) error {
	raw, err := json.Marshal(data)
	if err != nil {
		return fmt.Errorf("reflow.Graph.AddGraphInitial marshal: %w", err)
	}
	a := cstr(inport)
	defer freeCStr(a)
	b := cstr(string(raw))
	defer freeCStr(b)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeIfNotNil(md)
	st := C.rfl_graph_add_graph_initial(g.ptr, a, b, md)
	return statusToError(int32(st), "Graph.AddGraphInitial")
}

func (g *Graph) AddGraphInitialIndex(inport string, data any, index uint, metadata map[string]any) error {
	raw, err := json.Marshal(data)
	if err != nil {
		return fmt.Errorf("reflow.Graph.AddGraphInitialIndex marshal: %w", err)
	}
	a := cstr(inport)
	defer freeCStr(a)
	b := cstr(string(raw))
	defer freeCStr(b)
	md, err := nullableMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeIfNotNil(md)
	st := C.rfl_graph_add_graph_initial_index(g.ptr, a, b, C.size_t(index), md)
	return statusToError(int32(st), "Graph.AddGraphInitialIndex")
}

func (g *Graph) RemoveGraphInitial(inport string) error {
	cs := cstr(inport)
	defer freeCStr(cs)
	st := C.rfl_graph_remove_graph_initial(g.ptr, cs)
	return statusToError(int32(st), "Graph.RemoveGraphInitial")
}

// ─── Mutators (metadata setters + properties) ──────────────────────────────

func (g *Graph) SetNodeMetadata(id string, metadata map[string]any) error {
	cs := cstr(id)
	defer freeCStr(cs)
	md, err := requiredMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeCStr(md)
	st := C.rfl_graph_set_node_metadata(g.ptr, cs, md)
	return statusToError(int32(st), "Graph.SetNodeMetadata")
}

func (g *Graph) SetConnectionMetadata(outNode, outPort, inNode, inPort string, metadata map[string]any) error {
	a := cstr(outNode)
	defer freeCStr(a)
	b := cstr(outPort)
	defer freeCStr(b)
	c := cstr(inNode)
	defer freeCStr(c)
	d := cstr(inPort)
	defer freeCStr(d)
	md, err := requiredMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeCStr(md)
	st := C.rfl_graph_set_connection_metadata(g.ptr, a, b, c, d, md)
	return statusToError(int32(st), "Graph.SetConnectionMetadata")
}

func (g *Graph) SetInportMetadata(portID string, metadata map[string]any) error {
	cs := cstr(portID)
	defer freeCStr(cs)
	md, err := requiredMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeCStr(md)
	st := C.rfl_graph_set_inport_metadata(g.ptr, cs, md)
	return statusToError(int32(st), "Graph.SetInportMetadata")
}

func (g *Graph) SetOutportMetadata(portID string, metadata map[string]any) error {
	cs := cstr(portID)
	defer freeCStr(cs)
	md, err := requiredMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeCStr(md)
	st := C.rfl_graph_set_outport_metadata(g.ptr, cs, md)
	return statusToError(int32(st), "Graph.SetOutportMetadata")
}

func (g *Graph) SetGroupMetadata(groupID string, metadata map[string]any) error {
	cs := cstr(groupID)
	defer freeCStr(cs)
	md, err := requiredMetadata(metadata)
	if err != nil {
		return err
	}
	defer freeCStr(md)
	st := C.rfl_graph_set_group_metadata(g.ptr, cs, md)
	return statusToError(int32(st), "Graph.SetGroupMetadata")
}

func (g *Graph) SetProperties(properties map[string]any) error {
	md, err := requiredMetadata(properties)
	if err != nil {
		return err
	}
	defer freeCStr(md)
	st := C.rfl_graph_set_properties(g.ptr, md)
	return statusToError(int32(st), "Graph.SetProperties")
}

// Import replaces this graph's contents with a GraphExport JSON document.
// (Destructive — the existing graph state is cleared first.)
func (g *Graph) Import(graphExport []byte) error {
	cs := cstr(string(graphExport))
	defer freeCStr(cs)
	st := C.rfl_graph_import(g.ptr, cs)
	return statusToError(int32(st), "Graph.Import")
}

// ─── Queries (return JSON-decoded data) ────────────────────────────────────

// GetNodeJSON returns the JSON representation of a node, or nil if no
// node with that id exists.
func (g *Graph) GetNodeJSON(id string) ([]byte, error) {
	cs := cstr(id)
	defer freeCStr(cs)
	p := C.rfl_graph_get_node_json(g.ptr, cs)
	if p == nil {
		// Not found is also signalled by NULL — disambiguate by
		// checking whether last_error is set.
		if err := lastError(); err != nil {
			// Distinguish "no such node" (a query miss) from a
			// real error by string match. The capi sets a
			// specific message in that case; everything else is
			// an actual error.
			if err.Error() == fmt.Sprintf("reflow runtime: no node with id '%s'", id) {
				return nil, nil
			}
			return nil, err
		}
		return nil, nil
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) NodesJSON() ([]byte, error) {
	p := C.rfl_graph_list_nodes_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) GetConnectionJSON(outNode, outPort, inNode, inPort string) ([]byte, error) {
	a := cstr(outNode)
	defer freeCStr(a)
	b := cstr(outPort)
	defer freeCStr(b)
	c := cstr(inNode)
	defer freeCStr(c)
	d := cstr(inPort)
	defer freeCStr(d)
	p := C.rfl_graph_get_connection_json(g.ptr, a, b, c, d)
	if p == nil {
		// query miss vs error — same convention as GetNodeJSON
		if err := lastError(); err != nil {
			return nil, nil
		}
		return nil, nil
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) ConnectionsJSON() ([]byte, error) {
	p := C.rfl_graph_list_connections_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) GroupsJSON() ([]byte, error) {
	p := C.rfl_graph_list_groups_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) InportsJSON() ([]byte, error) {
	p := C.rfl_graph_list_inports_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) OutportsJSON() ([]byte, error) {
	p := C.rfl_graph_list_outports_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) InitializersJSON() ([]byte, error) {
	p := C.rfl_graph_list_initializers_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

func (g *Graph) PropertiesJSON() ([]byte, error) {
	p := C.rfl_graph_get_properties_json(g.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

// ─── helpers ───────────────────────────────────────────────────────────────

func nullableJSON(v any) (*C.char, error) {
	if v == nil {
		return nil, nil
	}
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, fmt.Errorf("json marshal: %w", err)
	}
	return cstr(string(raw)), nil
}

func requiredMetadata(md map[string]any) (*C.char, error) {
	if md == nil {
		md = map[string]any{}
	}
	raw, err := json.Marshal(md)
	if err != nil {
		return nil, fmt.Errorf("metadata marshal: %w", err)
	}
	return cstr(string(raw)), nil
}

func freeIfNotNil(p *C.char) {
	if p != nil {
		freeCStr(p)
	}
}
