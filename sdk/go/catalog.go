package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
)

// TemplateActor instantiates an actor from the bundled reflow_components
// catalog. Returns nil + error if the id is unknown. Requires the
// `components` Cargo feature in the underlying shared library (on by
// default).
func TemplateActor(templateID string) (*ActorHandle, error) {
	cs := cstr(templateID)
	defer freeCStr(cs)
	p := C.rfl_template_actor_new(cs)
	if p == nil {
		return nil, lastError()
	}
	return &ActorHandle{ptr: p}, nil
}

// TemplateList returns every template id registered in the bundled
// catalog.
func TemplateList() ([]string, error) {
	p := C.rfl_template_list_json()
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	var out []string
	if err := json.Unmarshal([]byte(C.GoString(p)), &out); err != nil {
		return nil, fmt.Errorf("decode template list: %w", err)
	}
	return out, nil
}

// SubgraphBuilder composes a SubgraphActor out of a GraphExport plus an
// explicit actor map.
type SubgraphBuilder struct {
	ptr *C.rfl_subgraph_builder
}

// NewSubgraphBuilder parses a GraphExport JSON document and returns a
// builder.
func NewSubgraphBuilder(graphExport []byte) (*SubgraphBuilder, error) {
	cs := cstr(string(graphExport))
	defer freeCStr(cs)
	p := C.rfl_subgraph_builder_new(cs)
	if p == nil {
		return nil, lastError()
	}
	b := &SubgraphBuilder{ptr: p}
	runtime.SetFinalizer(b, (*SubgraphBuilder).Free)
	return b, nil
}

// RegisterActor maps `componentName` (as referenced inside the export)
// to a user-supplied actor handle. Transfers ownership.
func (b *SubgraphBuilder) RegisterActor(componentName string, handle *ActorHandle) error {
	if handle == nil {
		return fmt.Errorf("reflow.SubgraphBuilder.RegisterActor: nil handle")
	}
	cs := cstr(componentName)
	defer freeCStr(cs)
	st := C.rfl_subgraph_builder_register_actor(b.ptr, cs, handle.release())
	return statusToError(int32(st), "SubgraphBuilder.RegisterActor")
}

// RegisterGoActor is sugar.
func (b *SubgraphBuilder) RegisterGoActor(componentName string, actor Actor) error {
	return b.RegisterActor(componentName, NewActor(actor))
}

// FillFromCatalog resolves any still-unregistered components from the
// bundled catalog.
func (b *SubgraphBuilder) FillFromCatalog() error {
	st := C.rfl_subgraph_builder_fill_from_catalog(b.ptr)
	return statusToError(int32(st), "SubgraphBuilder.FillFromCatalog")
}

// Build consumes the builder and returns a subgraph actor handle.
func (b *SubgraphBuilder) Build() (*ActorHandle, error) {
	if b == nil || b.ptr == nil {
		return nil, fmt.Errorf("reflow.SubgraphBuilder.Build: nil builder")
	}
	p := C.rfl_subgraph_builder_build(b.ptr)
	b.ptr = nil
	runtime.SetFinalizer(b, nil)
	if p == nil {
		return nil, lastError()
	}
	return &ActorHandle{ptr: p}, nil
}

// Free discards the builder without building.
func (b *SubgraphBuilder) Free() {
	if b == nil || b.ptr == nil {
		return
	}
	C.rfl_subgraph_builder_free(b.ptr)
	b.ptr = nil
	runtime.SetFinalizer(b, nil)
}
