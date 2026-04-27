package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
	"unsafe"
)

// MessageKind matches the Rust `Message` enum's variant tag.
type MessageKind int

const (
	KindFlow         MessageKind = iota // 0
	KindBoolean                         // 1
	KindInteger                         // 2
	KindFloat                           // 3
	KindString                          // 4
	KindObject                          // 5
	KindArray                           // 6
	KindBytes                           // 7
	KindError                           // 8
	KindStreamHandle                    // 9
	KindOptional                        // 10
	KindOther        MessageKind = 99
)

func (k MessageKind) String() string {
	switch k {
	case KindFlow:
		return "Flow"
	case KindBoolean:
		return "Boolean"
	case KindInteger:
		return "Integer"
	case KindFloat:
		return "Float"
	case KindString:
		return "String"
	case KindObject:
		return "Object"
	case KindArray:
		return "Array"
	case KindBytes:
		return "Bytes"
	case KindError:
		return "Error"
	case KindStreamHandle:
		return "StreamHandle"
	case KindOptional:
		return "Optional"
	default:
		return "Other"
	}
}

// Message wraps an opaque rfl_message handle. A Message owns its handle
// until transferred to the runtime (via Emit / Initial / etc.) or until
// Free is called. The finalizer will free it automatically; relying on
// that is not recommended for hot paths.
type Message struct {
	ptr *C.rfl_message
}

func wrapMessage(p *C.rfl_message) *Message {
	if p == nil {
		return nil
	}
	m := &Message{ptr: p}
	runtime.SetFinalizer(m, (*Message).Free)
	return m
}

// MessageFlow constructs a Flow signal.
func MessageFlow() *Message { return wrapMessage(C.rfl_message_flow()) }

// MessageBoolean constructs a Boolean message.
func MessageBoolean(v bool) *Message {
	var iv C.int
	if v {
		iv = 1
	}
	return wrapMessage(C.rfl_message_boolean(iv))
}

// MessageInteger constructs an Integer message (i64).
func MessageInteger(v int64) *Message {
	return wrapMessage(C.rfl_message_integer(C.int64_t(v)))
}

// MessageFloat constructs a Float message (f64).
func MessageFloat(v float64) *Message {
	return wrapMessage(C.rfl_message_float(C.double(v)))
}

// MessageString constructs a String message.
func MessageString(s string) *Message {
	cs := cstr(s)
	defer freeCStr(cs)
	p := C.rfl_message_string(cs)
	if p == nil {
		return nil
	}
	return wrapMessage(p)
}

// MessageBytes constructs a binary Bytes message. The buffer is copied.
func MessageBytes(b []byte) *Message {
	if len(b) == 0 {
		return wrapMessage(C.rfl_message_bytes(nil, 0))
	}
	return wrapMessage(C.rfl_message_bytes(
		(*C.uint8_t)(unsafe.Pointer(&b[0])),
		C.size_t(len(b)),
	))
}

// MessageError constructs an Error message.
func MessageError(s string) *Message {
	cs := cstr(s)
	defer freeCStr(cs)
	p := C.rfl_message_error(cs)
	if p == nil {
		return nil
	}
	return wrapMessage(p)
}

// MessageObject constructs an Object message from any JSON-serializable value.
func MessageObject(v any) (*Message, error) {
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, fmt.Errorf("reflow.MessageObject marshal: %w", err)
	}
	cs := cstr(string(raw))
	defer freeCStr(cs)
	p := C.rfl_message_object_from_json(cs)
	if p == nil {
		return nil, lastError()
	}
	return wrapMessage(p), nil
}

// MessageArray constructs an Array message from a JSON array value.
func MessageArray(v any) (*Message, error) {
	raw, err := json.Marshal(v)
	if err != nil {
		return nil, fmt.Errorf("reflow.MessageArray marshal: %w", err)
	}
	cs := cstr(string(raw))
	defer freeCStr(cs)
	p := C.rfl_message_array_from_json(cs)
	if p == nil {
		return nil, lastError()
	}
	return wrapMessage(p), nil
}

// MessageFromJSON parses a Rust-shaped Message JSON (e.g.
// `{"type":"Integer","data":3}`). Prefer the typed constructors.
func MessageFromJSON(raw []byte) (*Message, error) {
	cs := cstr(string(raw))
	defer freeCStr(cs)
	p := C.rfl_message_from_json(cs)
	if p == nil {
		return nil, lastError()
	}
	return wrapMessage(p), nil
}

// Kind returns the message variant.
func (m *Message) Kind() MessageKind {
	if m == nil || m.ptr == nil {
		return KindOther
	}
	return MessageKind(C.rfl_message_get_kind(m.ptr))
}

// AsBoolean extracts a Boolean message's value. ok is false on type mismatch.
func (m *Message) AsBoolean() (val, ok bool) {
	if m == nil || m.ptr == nil {
		return false, false
	}
	var out C.int
	if C.rfl_message_as_boolean(m.ptr, &out) == 0 {
		return false, false
	}
	return out != 0, true
}

// AsInteger extracts an Integer message's value.
func (m *Message) AsInteger() (val int64, ok bool) {
	if m == nil || m.ptr == nil {
		return 0, false
	}
	var out C.int64_t
	if C.rfl_message_as_integer(m.ptr, &out) == 0 {
		return 0, false
	}
	return int64(out), true
}

// AsFloat extracts a Float message's value.
func (m *Message) AsFloat() (val float64, ok bool) {
	if m == nil || m.ptr == nil {
		return 0, false
	}
	var out C.double
	if C.rfl_message_as_float(m.ptr, &out) == 0 {
		return 0, false
	}
	return float64(out), true
}

// AsString extracts a String or Error message's text.
func (m *Message) AsString() (string, bool) {
	if m == nil || m.ptr == nil {
		return "", false
	}
	p := C.rfl_message_as_string(m.ptr)
	if p == nil {
		return "", false
	}
	defer C.rfl_string_free(p)
	return C.GoString(p), true
}

// AsBytes returns a copy of the Bytes payload.
func (m *Message) AsBytes() ([]byte, bool) {
	if m == nil || m.ptr == nil {
		return nil, false
	}
	var data *C.uint8_t
	var n C.size_t
	if C.rfl_message_bytes_borrow(m.ptr, &data, &n) == 0 {
		return nil, false
	}
	if n == 0 {
		return []byte{}, true
	}
	return C.GoBytes(unsafe.Pointer(data), C.int(n)), true
}

// AsJSON serializes the full tagged message to JSON
// (`{"type":"Array","data":[...]}` form). Round-trips with
// MessageFromJSON. For most actor code you want Data instead.
func (m *Message) AsJSON() ([]byte, error) {
	if m == nil || m.ptr == nil {
		return nil, fmt.Errorf("reflow.Message.AsJSON: nil message")
	}
	p := C.rfl_message_as_json(m.ptr)
	if p == nil {
		return nil, lastError()
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), nil
}

// Data returns the inner payload as JSON, with the runtime's
// EncodableValue wrappers transparently decoded. For Array messages
// it's a bare JSON array; for Object, a bare object; for primitives,
// the bare scalar. Pass straight to json.Unmarshal.
//
// Returns ok=false for variants without a portable payload (Flow,
// Bytes — use AsBytes — StreamHandle, Encoded, RemoteReference,
// NetworkEvent).
func (m *Message) Data() ([]byte, bool) {
	if m == nil || m.ptr == nil {
		return nil, false
	}
	p := C.rfl_message_data_json(m.ptr)
	if p == nil {
		return nil, false
	}
	defer C.rfl_string_free(p)
	return []byte(C.GoString(p)), true
}

// Free releases the handle early. The finalizer will call this
// automatically if you forget, but reclaiming eagerly is cheaper.
func (m *Message) Free() {
	if m == nil || m.ptr == nil {
		return
	}
	C.rfl_message_free(m.ptr)
	m.ptr = nil
	runtime.SetFinalizer(m, nil)
}

// release relinquishes ownership without freeing — used when the runtime
// takes over (e.g. after Emit).
func (m *Message) release() *C.rfl_message {
	if m == nil {
		return nil
	}
	p := m.ptr
	m.ptr = nil
	runtime.SetFinalizer(m, nil)
	return p
}

// TakeStream claims the receiver for a StreamHandle message.
// Returns ErrNotStream if the message is not a StreamHandle.
func (m *Message) TakeStream() (*StreamReader, error) {
	if m == nil || m.ptr == nil {
		return nil, fmt.Errorf("reflow.TakeStream: nil message")
	}
	if m.Kind() != KindStreamHandle {
		return nil, fmt.Errorf("reflow.TakeStream: message kind is %s, not StreamHandle", m.Kind())
	}
	// Re-use the C ABI: it's easier to synthesize JSON and go through
	// C's own take path, but the capi exposes this at the rfl_message*
	// level directly only when an explicit "take" function exists.
	// Since we only have the capi-side take via the underlying registry,
	// we parse the message JSON, extract the stream_id, and take the
	// receiver directly via the registry-backed Rust function in
	// stream.go.
	return takeStreamFromMessage(m)
}
