package reflow

// #include <stdlib.h>
// #include "reflow_rt.h"
import "C"

import (
	"encoding/json"
	"fmt"
	"runtime"
	"time"
	"unsafe"
)

// StreamOptions configures a stream producer.
type StreamOptions struct {
	// BufferSize. Zero means unbounded.
	BufferSize  int
	OriginActor string
	OriginPort  string
	ContentType string
}

// Stream is the producer side of a Message.StreamHandle. Call SendBytes /
// SendBegin / End / Error to push frames, then IntoMessage to convert to
// a Message for emission on an output port.
type Stream struct {
	ptr *C.rfl_stream
}

// NewStream allocates a fresh stream.
func NewStream(opts StreamOptions) *Stream {
	origin := nullableCStr(opts.OriginActor)
	port := nullableCStr(opts.OriginPort)
	ct := nullableCStr(opts.ContentType)
	defer func() {
		if origin != nil {
			freeCStr(origin)
		}
		if port != nil {
			freeCStr(port)
		}
		if ct != nil {
			freeCStr(ct)
		}
	}()
	p := C.rfl_stream_new(C.size_t(opts.BufferSize), origin, port, ct)
	if p == nil {
		return nil
	}
	s := &Stream{ptr: p}
	runtime.SetFinalizer(s, (*Stream).Free)
	return s
}

// BeginOptions mirrors the StreamFrame::Begin payload.
type BeginOptions struct {
	ContentType string
	SizeHint    uint64
	HasSizeHint bool
	Metadata    any // marshaled via encoding/json; use nil to omit
}

// SendBegin sends an optional Begin frame.
func (s *Stream) SendBegin(opts BeginOptions) error {
	if s == nil || s.ptr == nil {
		return fmt.Errorf("reflow.Stream.SendBegin: stream is closed")
	}
	ct := nullableCStr(opts.ContentType)
	defer func() {
		if ct != nil {
			freeCStr(ct)
		}
	}()
	var metaPtr *C.char
	if opts.Metadata != nil {
		raw, err := json.Marshal(opts.Metadata)
		if err != nil {
			return fmt.Errorf("reflow.Stream.SendBegin: %w", err)
		}
		metaPtr = cstr(string(raw))
		defer freeCStr(metaPtr)
	}
	var has C.int
	if opts.HasSizeHint {
		has = 1
	}
	st := C.rfl_stream_send_begin(s.ptr, ct, C.uint64_t(opts.SizeHint), has, metaPtr)
	return statusToError(int32(st), "Stream.SendBegin")
}

// SendBytes pushes a Data frame. The buffer is copied.
func (s *Stream) SendBytes(data []byte) error {
	if s == nil || s.ptr == nil {
		return fmt.Errorf("reflow.Stream.SendBytes: stream is closed")
	}
	var ptr *C.uint8_t
	if len(data) > 0 {
		ptr = (*C.uint8_t)(unsafe.Pointer(&data[0]))
	}
	st := C.rfl_stream_send_bytes(s.ptr, ptr, C.size_t(len(data)))
	return statusToError(int32(st), "Stream.SendBytes")
}

// End terminates the stream successfully.
func (s *Stream) End() error {
	if s == nil || s.ptr == nil {
		return fmt.Errorf("reflow.Stream.End: stream is closed")
	}
	return statusToError(int32(C.rfl_stream_end(s.ptr)), "Stream.End")
}

// Error terminates the stream with an error message.
func (s *Stream) Error(msg string) error {
	if s == nil || s.ptr == nil {
		return fmt.Errorf("reflow.Stream.Error: stream is closed")
	}
	cs := cstr(msg)
	defer freeCStr(cs)
	return statusToError(int32(C.rfl_stream_error(s.ptr, cs)), "Stream.Error")
}

// IntoMessage consumes the producer and returns a Message.StreamHandle
// that can be emitted on an output port.
func (s *Stream) IntoMessage() *Message {
	if s == nil || s.ptr == nil {
		return nil
	}
	p := C.rfl_stream_into_message(s.ptr)
	// into_message consumes the producer on the C side; clear our handle.
	s.ptr = nil
	runtime.SetFinalizer(s, nil)
	return wrapMessage(p)
}

// Free releases the handle early. End-of-stream is implied if no End
// frame was sent.
func (s *Stream) Free() {
	if s == nil || s.ptr == nil {
		return
	}
	C.rfl_stream_free(s.ptr)
	s.ptr = nil
	runtime.SetFinalizer(s, nil)
}

// ─── consumer side ─────────────────────────────────────────────────────────

// StreamReader is the consumer-side reader for a Message.StreamHandle.
type StreamReader struct {
	ptr *C.rfl_stream_recv
}

// FrameKind identifies what came out of a StreamReader.Recv call.
type FrameKind string

const (
	FrameBegin   FrameKind = "begin"
	FrameData    FrameKind = "data"
	FrameEnd     FrameKind = "end"
	FrameError   FrameKind = "error"
	FrameTimeout FrameKind = "timeout"
	FrameClosed  FrameKind = "closed"
)

// Frame is a single frame produced by StreamReader.Recv.
type Frame struct {
	Kind        FrameKind
	Data        []byte
	Error       string
	ContentType string
	SizeHint    uint32
	HasSizeHint bool
}

func takeStreamFromMessage(m *Message) (*StreamReader, error) {
	if m == nil || m.ptr == nil {
		return nil, fmt.Errorf("reflow.TakeStream: nil message")
	}
	p := C.rfl_message_stream_take(m.ptr)
	if p == nil {
		return nil, lastError()
	}
	r := &StreamReader{ptr: p}
	runtime.SetFinalizer(r, (*StreamReader).Close)
	return r, nil
}

// Recv blocks up to timeout for the next frame.
func (r *StreamReader) Recv(timeout time.Duration) (*Frame, error) {
	if r == nil || r.ptr == nil {
		return nil, fmt.Errorf("reflow.StreamReader.Recv: reader is closed")
	}
	ms := timeout.Milliseconds()
	if ms < 0 {
		ms = 0
	}

	// cgo emits enum*-returning signatures with the underlying integer
	// type (uint32 here). Keep `kind` as the underlying type and compare
	// to uint32(C.rfl_stream_frame_kind_*).
	var kind uint32
	var data *C.uint8_t
	var n C.size_t
	var errPtr *C.char

	st := C.rfl_stream_recv_next(r.ptr, C.uint32_t(ms), &kind, &data, &n, &errPtr)
	if err := statusToError(int32(st), "StreamReader.Recv"); err != nil {
		return nil, err
	}
	f := &Frame{}
	switch kind {
	case uint32(C.rfl_stream_frame_kind_Begin):
		f.Kind = FrameBegin
	case uint32(C.rfl_stream_frame_kind_Data):
		f.Kind = FrameData
		if data != nil && n > 0 {
			f.Data = C.GoBytes(unsafe.Pointer(data), C.int(n))
		} else {
			f.Data = []byte{}
		}
	case uint32(C.rfl_stream_frame_kind_End):
		f.Kind = FrameEnd
	case uint32(C.rfl_stream_frame_kind_Error):
		f.Kind = FrameError
		if errPtr != nil {
			f.Error = C.GoString(errPtr)
			C.rfl_string_free(errPtr)
		}
	case uint32(C.rfl_stream_frame_kind_Timeout):
		f.Kind = FrameTimeout
	case uint32(C.rfl_stream_frame_kind_Closed):
		f.Kind = FrameClosed
	default:
		f.Kind = FrameTimeout
	}
	return f, nil
}

// Close frees the reader.
func (r *StreamReader) Close() {
	if r == nil || r.ptr == nil {
		return
	}
	C.rfl_stream_recv_free(r.ptr)
	r.ptr = nil
	runtime.SetFinalizer(r, nil)
}
