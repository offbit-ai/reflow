package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
	"time"
)

// Network wraps a runtime network.
type Network struct {
	ptr *C.rfl_network
}

// NewNetwork constructs a network with default config.
func NewNetwork() *Network {
	p := C.rfl_network_new()
	if p == nil {
		return nil
	}
	n := &Network{ptr: p}
	runtime.SetFinalizer(n, (*Network).Close)
	return n
}

// NewNetworkWithConfig constructs a network with a serialized NetworkConfig.
func NewNetworkWithConfig(config any) (*Network, error) {
	raw, err := json.Marshal(config)
	if err != nil {
		return nil, fmt.Errorf("reflow.NewNetworkWithConfig marshal: %w", err)
	}
	cs := cstr(string(raw))
	defer freeCStr(cs)
	p := C.rfl_network_new_with_config(cs)
	if p == nil {
		return nil, lastError()
	}
	n := &Network{ptr: p}
	runtime.SetFinalizer(n, (*Network).Close)
	return n, nil
}

// NewNetworkFromGraph consumes a Graph and returns a network wired with it.
func NewNetworkFromGraph(g *Graph) *Network {
	if g == nil || g.ptr == nil {
		return nil
	}
	p := C.rfl_network_from_graph(g.ptr)
	g.ptr = nil
	runtime.SetFinalizer(g, nil)
	if p == nil {
		return nil
	}
	n := &Network{ptr: p}
	runtime.SetFinalizer(n, (*Network).Close)
	return n
}

// Close shuts the network down and releases the handle.
func (n *Network) Close() {
	if n == nil || n.ptr == nil {
		return
	}
	C.rfl_network_free(n.ptr)
	n.ptr = nil
	runtime.SetFinalizer(n, nil)
}

// RegisterActor publishes `actor` under `templateId`. The network takes
// ownership of the handle — do not reuse it afterwards.
func (n *Network) RegisterActor(templateID string, handle *ActorHandle) error {
	if handle == nil {
		return fmt.Errorf("reflow.RegisterActor: nil handle")
	}
	cs := cstr(templateID)
	defer freeCStr(cs)
	st := C.rfl_network_register_actor(n.ptr, cs, handle.release())
	return statusToError(int32(st), "Network.RegisterActor")
}

// RegisterGoActor is sugar for NewActor + RegisterActor.
func (n *Network) RegisterGoActor(templateID string, actor Actor) error {
	h := NewActor(actor)
	if h == nil {
		return fmt.Errorf("reflow.RegisterGoActor: NewActor returned nil")
	}
	return n.RegisterActor(templateID, h)
}

// AddNode instantiates a node referencing `templateId`.
func (n *Network) AddNode(id, templateID string, config map[string]any) error {
	a := cstr(id)
	defer freeCStr(a)
	b := cstr(templateID)
	defer freeCStr(b)
	var cfg *C.char
	if config != nil {
		raw, err := json.Marshal(config)
		if err != nil {
			return fmt.Errorf("reflow.AddNode marshal config: %w", err)
		}
		cfg = cstr(string(raw))
		defer freeCStr(cfg)
	}
	st := C.rfl_network_add_node(n.ptr, a, b, cfg)
	return statusToError(int32(st), "Network.AddNode")
}

// AddConnection wires fromActor.fromPort → toActor.toPort.
func (n *Network) AddConnection(fromActor, fromPort, toActor, toPort string) error {
	a := cstr(fromActor)
	defer freeCStr(a)
	b := cstr(fromPort)
	defer freeCStr(b)
	c := cstr(toActor)
	defer freeCStr(c)
	d := cstr(toPort)
	defer freeCStr(d)
	st := C.rfl_network_add_connection(n.ptr, a, b, c, d)
	return statusToError(int32(st), "Network.AddConnection")
}

// AddInitial seeds an initial message on a node's port.
func (n *Network) AddInitial(actor, port string, msg any) error {
	var raw []byte
	var err error
	switch v := msg.(type) {
	case []byte:
		raw = v
	case string:
		raw = []byte(v)
	case *Message:
		raw, err = v.AsJSON()
		if err != nil {
			return err
		}
	default:
		raw, err = json.Marshal(msg)
		if err != nil {
			return fmt.Errorf("reflow.AddInitial marshal: %w", err)
		}
	}
	a := cstr(actor)
	defer freeCStr(a)
	b := cstr(port)
	defer freeCStr(b)
	c := cstr(string(raw))
	defer freeCStr(c)
	st := C.rfl_network_add_initial(n.ptr, a, b, c)
	return statusToError(int32(st), "Network.AddInitial")
}

// Start launches the network. Non-blocking.
func (n *Network) Start() error {
	return statusToError(int32(C.rfl_network_start(n.ptr)), "Network.Start")
}

// Shutdown signals the network to stop.
func (n *Network) Shutdown() error {
	return statusToError(int32(C.rfl_network_shutdown(n.ptr)), "Network.Shutdown")
}

// Events subscribes to the network's event stream.
func (n *Network) Events() *EventStream {
	p := C.rfl_network_events(n.ptr)
	if p == nil {
		return nil
	}
	e := &EventStream{ptr: p}
	runtime.SetFinalizer(e, (*EventStream).Close)
	return e
}

// EventStream delivers NetworkEvent objects as JSON.
type EventStream struct {
	ptr *C.rfl_events
}

// Recv blocks up to `timeout` for the next event. On timeout returns
// (nil, nil). On channel close, returns an error.
func (e *EventStream) Recv(timeout time.Duration) (map[string]any, error) {
	if e == nil || e.ptr == nil {
		return nil, fmt.Errorf("reflow.EventStream.Recv: reader is closed")
	}
	ms := timeout.Milliseconds()
	if ms < 0 {
		ms = 0
	}
	var out *C.char
	st := C.rfl_events_recv(e.ptr, C.uint32_t(ms), &out)
	switch st {
	case C.rfl_status_Ok:
		if out == nil {
			return nil, nil
		}
		defer C.rfl_string_free(out)
		raw := []byte(C.GoString(out))
		m := map[string]any{}
		if err := json.Unmarshal(raw, &m); err != nil {
			return nil, fmt.Errorf("decode event: %w", err)
		}
		return m, nil
	case C.rfl_status_InvalidState:
		return nil, nil // timeout
	default:
		return nil, statusToError(int32(st), "EventStream.Recv")
	}
}

// Close releases the subscription.
func (e *EventStream) Close() {
	if e == nil || e.ptr == nil {
		return
	}
	C.rfl_events_free(e.ptr)
	e.ptr = nil
	runtime.SetFinalizer(e, nil)
}
