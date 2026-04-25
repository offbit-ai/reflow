package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
import "C"

import (
	"encoding/json"
	"fmt"
)

// LoadPack loads an actor pack from either a `.rflpack` bundle or a raw
// cdylib path. Returns the list of template ids the pack published.
// Repeat calls with the same pack are no-ops.
func LoadPack(path string) ([]string, error) {
	cs := cstr(path)
	defer freeCStr(cs)
	var js *C.char
	st := C.rfl_pack_load(cs, &js)
	if err := statusToError(int32(st), "LoadPack"); err != nil {
		return nil, err
	}
	if js == nil {
		return nil, nil
	}
	defer C.rfl_string_free(js)
	var out []string
	if err := json.Unmarshal([]byte(C.GoString(js)), &out); err != nil {
		return nil, fmt.Errorf("decode pack load result: %w", err)
	}
	return out, nil
}

// PackManifest is the subset of a `.rflpack` manifest surfaced by
// InspectPack + ListPacks. Unknown keys are preserved via Extra.
type PackManifest struct {
	ManifestVersion       uint32                 `json:"manifest_version"`
	Name                  string                 `json:"name"`
	Version               string                 `json:"version"`
	Authors               []string               `json:"authors"`
	Description           *string                `json:"description,omitempty"`
	License               *string                `json:"license,omitempty"`
	ReflowPackABIVersion  uint32                 `json:"reflow_pack_abi_version"`
	Entrypoint            string                 `json:"entrypoint"`
	Targets               map[string]PackTarget  `json:"targets"`
	Templates             []string               `json:"templates"`
}

// PackTarget describes one per-triple dylib inside a `.rflpack`.
type PackTarget struct {
	File string `json:"file"`
}

// InspectPack reads a `.rflpack` manifest without loading any code.
// Fails for raw dylib paths.
func InspectPack(path string) (*PackManifest, error) {
	cs := cstr(path)
	defer freeCStr(cs)
	js := C.rfl_pack_inspect_json(cs)
	if js == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(js)
	var m PackManifest
	if err := json.Unmarshal([]byte(C.GoString(js)), &m); err != nil {
		return nil, fmt.Errorf("decode pack manifest: %w", err)
	}
	return &m, nil
}

// LoadedPack is one row of `ListPacks`.
type LoadedPack struct {
	Name       string   `json:"name"`
	Version    string   `json:"version"`
	SourcePath string   `json:"source_path"`
	Templates  []string `json:"templates"`
}

// ListPacks returns every pack currently loaded into the process.
func ListPacks() ([]LoadedPack, error) {
	js := C.rfl_pack_list_json()
	if js == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(js)
	var out []LoadedPack
	if err := json.Unmarshal([]byte(C.GoString(js)), &out); err != nil {
		return nil, fmt.Errorf("decode pack list: %w", err)
	}
	return out, nil
}

// PackABIVersion is the ABI version this SDK was built against. Pack
// authors must stamp the same value into their `.rflpack` manifests.
func PackABIVersion() uint32 {
	return uint32(C.rfl_pack_abi_version())
}
