package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
//
// // Trampoline: cgo cannot assign Go functions directly to rfl_actor_fn,
// // and the cgo preamble is file-local. The forward declarations below
// // reference the //export'd Go functions further down in this file.
// extern enum rfl_status reflow_go_actor_trampoline(void* user_data, struct rfl_actor_ctx* ctx);
// extern void           reflow_go_actor_drop(void* user_data);
//
// static inline struct rfl_actor* go_rfl_actor_new(
//   const char* component_name,
//   const char* const* inports,  size_t n_inports,
//   const char* const* outports, size_t n_outports,
//   int await_all_inports,
//   void* user_data
// ) {
//   return rfl_actor_new(component_name, inports, n_inports, outports, n_outports,
//                        await_all_inports,
//                        reflow_go_actor_trampoline,
//                        user_data,
//                        reflow_go_actor_drop);
// }
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
	"runtime/cgo"
	"sync"
	"unsafe"
)

// Actor is the interface a Go actor implementation satisfies. Embed
// BaseActor in your struct to get port declarations "for free"; override
// Run to implement behavior.
type Actor interface {
	Component() string
	Inports() []string
	Outports() []string
	AwaitAllInports() bool
	Run(ctx *ActorContext) error
}

// BaseActor is a convenience embedding. Set the public fields and Actor
// is satisfied for everything except Run.
type BaseActor struct {
	ComponentName string
	InportsList   []string
	OutportsList  []string
	AwaitAll      bool
}

func (b *BaseActor) Component() string     { return b.ComponentName }
func (b *BaseActor) Inports() []string     { return b.InportsList }
func (b *BaseActor) Outports() []string    { return b.OutportsList }
func (b *BaseActor) AwaitAllInports() bool { return b.AwaitAll }

// ActorHandle wraps a Rust-side actor (either callback-driven or a
// bundled template). Handed to Network.RegisterActor / SubgraphBuilder.
type ActorHandle struct {
	ptr *C.rfl_actor
	// If the handle wraps a Go actor, we keep a cgo.Handle alive so the
	// Rust side's user_data pointer stays valid. The handle is released
	// from the Rust-side drop hook (see reflow_go_actor_drop below).
	goHandle cgo.Handle
	hasGo    bool
}

// Free discards the handle without registering it. Safe on nil.
func (a *ActorHandle) Free() {
	if a == nil || a.ptr == nil {
		return
	}
	C.rfl_actor_free(a.ptr)
	a.ptr = nil
}

// release relinquishes ownership — used when the runtime consumes the
// handle (Network.RegisterActor / SubgraphBuilder.RegisterActor).
func (a *ActorHandle) release() *C.rfl_actor {
	if a == nil {
		return nil
	}
	p := a.ptr
	a.ptr = nil
	return p
}

// NewActor wraps a Go Actor into an ActorHandle. The returned handle
// owns a cgo.Handle kept alive until the Rust side releases it.
func NewActor(a Actor) *ActorHandle {
	reg := &actorReg{actor: a}
	h := cgo.NewHandle(reg)

	component := cstr(a.Component())
	defer freeCStr(component)

	inArr := newCStringArray(a.Inports())
	defer inArr.free()
	outArr := newCStringArray(a.Outports())
	defer outArr.free()

	var awaitAll C.int
	if a.AwaitAllInports() {
		awaitAll = 1
	}

	ptr := C.go_rfl_actor_new(
		component,
		inArr.data(), C.size_t(len(inArr.ptrs)),
		outArr.data(), C.size_t(len(outArr.ptrs)),
		awaitAll,
		unsafe.Pointer(h),
	)
	if ptr == nil {
		h.Delete()
		return nil
	}
	return &ActorHandle{ptr: ptr, goHandle: h, hasGo: true}
}

// ─── cgo callback trampoline ───────────────────────────────────────────────

// actorReg is what's addressed by the cgo.Handle; plain wrapper around a
// Go Actor.
type actorReg struct {
	actor Actor
}

// ActorContext is what an Actor.Run sees. Read inputs / config, then
// either Emit outputs and return nil, or return an error to fail the
// tick.
type ActorContext struct {
	ctx *C.rfl_actor_ctx

	// We memoize decoded config on first access.
	decodeOnce  sync.Once
	configCache map[string]json.RawMessage
	decodeErr   error
}

func (c *ActorContext) decode() {
	c.decodeOnce.Do(func() {
		cfgPtr := C.rfl_ctx_config_json(c.ctx)
		if cfgPtr != nil {
			raw := []byte(C.GoString(cfgPtr))
			C.rfl_string_free(cfgPtr)
			if len(raw) > 0 && string(raw) != "null" {
				c.configCache = map[string]json.RawMessage{}
				if err := json.Unmarshal(raw, &c.configCache); err != nil {
					c.decodeErr = fmt.Errorf("config decode: %w", err)
				}
			}
		}
	})
}

// Input returns the message arriving on `port`, or nil if no packet came
// in on this tick. The message handle is owned by the runtime — use
// AsJSON / AsInteger / etc. to read, don't emit it elsewhere.
func (c *ActorContext) Input(port string) *Message {
	cs := cstr(port)
	defer freeCStr(cs)
	p := C.rfl_ctx_take_input_message(c.ctx, cs)
	if p == nil {
		return nil
	}
	return wrapMessage(p)
}

// HasInput reports whether a packet came in on `port` (does not consume).
func (c *ActorContext) HasInput(port string) bool {
	cs := cstr(port)
	defer freeCStr(cs)
	return C.rfl_ctx_has_input(c.ctx, cs) != 0
}

// Config returns the JSON-decoded config.
func (c *ActorContext) Config() map[string]json.RawMessage {
	c.decode()
	return c.configCache
}

// ConfigJSON returns the raw config JSON.
func (c *ActorContext) ConfigJSON() []byte {
	p := C.rfl_ctx_config_json(c.ctx)
	if p == nil {
		return nil
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p))
}

// Emit queues an output packet on `port`. The message is transferred to
// the runtime — do not Free it afterwards.
func (c *ActorContext) Emit(port string, m *Message) error {
	if m == nil {
		return fmt.Errorf("reflow.Emit: nil message")
	}
	cs := cstr(port)
	defer freeCStr(cs)
	st := C.rfl_ctx_emit_message(c.ctx, cs, m.release())
	return statusToError(int32(st), "Emit")
}

// StateGet returns the JSON-encoded value stored at `key`, or nil if
// absent.
func (c *ActorContext) StateGet(key string) []byte {
	cs := cstr(key)
	defer freeCStr(cs)
	p := C.rfl_ctx_state_get(c.ctx, cs)
	if p == nil {
		return nil
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p))
}

// StateSet writes a JSON value into the actor's MemoryState under `key`.
func (c *ActorContext) StateSet(key string, value any) error {
	raw, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("reflow.StateSet marshal: %w", err)
	}
	ck := cstr(key)
	defer freeCStr(ck)
	cv := cstr(string(raw))
	defer freeCStr(cv)
	st := C.rfl_ctx_state_set(c.ctx, ck, cv)
	return statusToError(int32(st), "StateSet")
}

// Send is the mid-tick flush variant of Emit — writes the message
// straight to the outport channel instead of buffering until the
// callback returns. Use this when one Run publishes several values
// on the same port (a Kafka reader, a per-block driver, an event
// stream) — Emit's per-tick HashMap collapses repeated writes to
// the same port to the last one. Consumes the Message handle.
func (c *ActorContext) Send(port string, m *Message) error {
	if m == nil {
		return fmt.Errorf("reflow.Send: nil message")
	}
	cs := cstr(port)
	defer freeCStr(cs)
	st := C.rfl_ctx_send_message(c.ctx, cs, m.release())
	return statusToError(int32(st), "Send")
}

// PoolUpsert stores `value` under `id` in the named pool. Pools are
// per-actor `{id: value}` maps that persist across ticks — the right
// tool for variable fan-in where N upstreams write under stable ids
// and the consumer reads the whole map. `value` is JSON-encoded.
func (c *ActorContext) PoolUpsert(poolName, id string, value any) error {
	raw, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("reflow.PoolUpsert marshal: %w", err)
	}
	cp := cstr(poolName)
	defer freeCStr(cp)
	ci := cstr(id)
	defer freeCStr(ci)
	cv := cstr(string(raw))
	defer freeCStr(cv)
	st := C.rfl_ctx_pool_upsert(c.ctx, cp, ci, cv)
	return statusToError(int32(st), "PoolUpsert")
}

// PoolRemove drops the entry under `id` from the named pool.
// Idempotent — a missing entry is not an error.
func (c *ActorContext) PoolRemove(poolName, id string) error {
	cp := cstr(poolName)
	defer freeCStr(cp)
	ci := cstr(id)
	defer freeCStr(ci)
	st := C.rfl_ctx_pool_remove(c.ctx, cp, ci)
	return statusToError(int32(st), "PoolRemove")
}

// PoolGetJSON returns the entire pool as a JSON object
// `{id: value, …}`. Empty/absent pools come back as `{}`.
func (c *ActorContext) PoolGetJSON(poolName string) []byte {
	cp := cstr(poolName)
	defer freeCStr(cp)
	p := C.rfl_ctx_pool_get_json(c.ctx, cp)
	if p == nil {
		return []byte("{}")
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p))
}

// PoolCount returns the number of entries in the named pool. Zero
// for empty or absent pools.
func (c *ActorContext) PoolCount(poolName string) int {
	cp := cstr(poolName)
	defer freeCStr(cp)
	return int(C.rfl_ctx_pool_count(c.ctx, cp))
}

// PoolClear drops the entire pool. Idempotent.
func (c *ActorContext) PoolClear(poolName string) error {
	cp := cstr(poolName)
	defer freeCStr(cp)
	st := C.rfl_ctx_pool_clear(c.ctx, cp)
	return statusToError(int32(st), "PoolClear")
}

// The exports below must not be methods — cgo requires free functions.
//
// reflow_go_actor_trampoline dispatches into the registered Go Actor's
// Run method via the cgo.Handle stored in user_data.

//export reflow_go_actor_trampoline
func reflow_go_actor_trampoline(userData unsafe.Pointer, ctx *C.rfl_actor_ctx) C.rfl_status {
	defer func() {
		if r := recover(); r != nil {
			// Swallow panics so a mis-behaved actor doesn't crash the
			// whole process. The runtime will log the failure.
			_ = r
		}
	}()
	h := cgo.Handle(userData)
	reg, ok := h.Value().(*actorReg)
	if !ok || reg == nil || reg.actor == nil {
		return C.rfl_status_Runtime
	}
	actorCtx := &ActorContext{ctx: ctx}
	if err := reg.actor.Run(actorCtx); err != nil {
		runtime.KeepAlive(reg)
		return C.rfl_status_Runtime
	}
	runtime.KeepAlive(reg)
	return C.rfl_status_Ok
}

//export reflow_go_actor_drop
func reflow_go_actor_drop(userData unsafe.Pointer) {
	if userData == nil {
		return
	}
	h := cgo.Handle(userData)
	h.Delete()
}
