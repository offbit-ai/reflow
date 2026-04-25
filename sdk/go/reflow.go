// Package reflow is the Go SDK for the Reflow runtime.
//
// Reflow is a modular flow-based programming runtime built on the actor
// model. Graphs are declarative DAGs: each node is an actor with named
// in/out ports, edges route messages, and a network executor runs the
// whole thing with bounded backpressure and a tracing stream.
//
// This package binds to the runtime's C ABI (reflow_rt_capi) via cgo.
// It exposes idiomatic Go types — Message, Actor, Network, Graph,
// SubgraphBuilder, Stream, StreamReader, EventStream — that mirror the
// other language SDKs one-for-one.
//
// # Quick start
//
//	package main
//
//	import "github.com/offbit-ai/reflow/sdk/go"
//
//	type Doubler struct{ reflow.BaseActor }
//
//	func NewDoubler() *Doubler {
//	    return &Doubler{BaseActor: reflow.BaseActor{
//	        ComponentName: "doubler",
//	        InportsList:   []string{"in"},
//	        OutportsList:  []string{"out"},
//	    }}
//	}
//
//	func (d *Doubler) Run(ctx *reflow.ActorContext) error {
//	    n := ctx.Input("in").AsInteger()
//	    ctx.Emit("out", reflow.MessageInteger(n*2))
//	    return nil
//	}
//
//	func main() {
//	    net := reflow.NewNetwork()
//	    defer net.Close()
//	    _ = net.RegisterActor("tpl_doubler", NewDoubler())
//	    _ = net.AddNode("a", "tpl_doubler", nil)
//	    // ... wire, start, shutdown.
//	}
package reflow

// Both dev and published paths share the same module-local layout:
//
//	sdk/go/include/reflow_rt.h        C header
//	sdk/go/lib/<goos>_<goarch>/...    libreflow_rt_capi.{so,dylib,dll}
//
// For repo-local dev, run scripts/link_dev_lib.sh to symlink the
// just-built target/<profile>/libreflow_rt_capi.* into the right
// triple dir. For published use, scripts/install_lib.sh <version>
// fetches the matching tarball from a GitHub Release and unpacks it
// here.

// #cgo CFLAGS: -I${SRCDIR}/include
// #cgo darwin,arm64  LDFLAGS: -L${SRCDIR}/lib/darwin_arm64  -lreflow_rt_capi -Wl,-rpath,${SRCDIR}/lib/darwin_arm64
// #cgo darwin,amd64  LDFLAGS: -L${SRCDIR}/lib/darwin_amd64  -lreflow_rt_capi -Wl,-rpath,${SRCDIR}/lib/darwin_amd64
// #cgo linux,amd64   LDFLAGS: -L${SRCDIR}/lib/linux_amd64   -lreflow_rt_capi -Wl,-rpath,${SRCDIR}/lib/linux_amd64
// #cgo linux,arm64   LDFLAGS: -L${SRCDIR}/lib/linux_arm64   -lreflow_rt_capi -Wl,-rpath,${SRCDIR}/lib/linux_arm64
// #cgo windows,amd64 LDFLAGS: -L${SRCDIR}/lib/windows_amd64 -lreflow_rt_capi
// #include <stdlib.h>
// #include "reflow_rt.h"
import "C"

import (
	"errors"
	"fmt"
	"unsafe"
)

// Error returns the last runtime error attached to the calling goroutine
// (thread-local on the Rust side).
func lastError() error {
	p := C.rfl_last_error_message()
	if p == nil {
		return errors.New("reflow: unknown error")
	}
	defer C.rfl_string_free(p)
	return errors.New(C.GoString(p))
}

// rfl_status_Ok is 0; every non-Ok status is a failure. The Go-side
// callers pass the status as int32 because cgo emits distinct
// C.rfl_status types per file — using the underlying integer type keeps
// the helper callable from every package file.
func statusToError(s int32, op string) error {
	if s == 0 {
		return nil
	}
	return fmt.Errorf("reflow.%s: %w", op, lastError())
}

// Version reports the runtime's semver.
func Version() string {
	p := C.rfl_version()
	if p == nil {
		return "unknown"
	}
	defer C.rfl_string_free(p)
	return C.GoString(p)
}

// Shutdown tears down the shared tokio runtime. Normally runs on library
// unload; call explicitly to release threads early.
func Shutdown() {
	C.rfl_runtime_shutdown()
}

// cstringArray holds C strings so the caller can pass them to the C ABI
// and free them after the call.
type cstringArray struct {
	ptrs []*C.char
}

func newCStringArray(values []string) *cstringArray {
	a := &cstringArray{ptrs: make([]*C.char, len(values))}
	for i, v := range values {
		a.ptrs[i] = C.CString(v)
	}
	return a
}

func (a *cstringArray) data() **C.char {
	if len(a.ptrs) == 0 {
		return nil
	}
	return &a.ptrs[0]
}

func (a *cstringArray) free() {
	for _, p := range a.ptrs {
		C.free(unsafe.Pointer(p))
	}
}

// cstr allocates a C string; caller frees via freeCStr.
func cstr(s string) *C.char { return C.CString(s) }
func freeCStr(p *C.char)    { C.free(unsafe.Pointer(p)) }

// nullableCStr returns nil if s is "", else a fresh C string (caller frees).
func nullableCStr(s string) *C.char {
	if s == "" {
		return nil
	}
	return C.CString(s)
}
