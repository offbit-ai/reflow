//! Stream handle producer + consumer bindings.
//!
//! Wraps the [`StreamRegistry`] so C callers can:
//! - create a new stream on the producer side
//! - attach frames / terminate it
//! - receive frames from a `Message::StreamHandle` arriving on an inport.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use std::sync::Arc;

use reflow_rt::actor_runtime::message::Message;
use reflow_rt::actor_runtime::stream::{StreamFrame, StreamHandle, StreamId, STREAM_REGISTRY};

use crate::message::rfl_message;
use crate::{rfl_status, set_last_error};

// ─── producer side ─────────────────────────────────────────────────────────

/// Opaque producer handle to a stream. Create with `rfl_stream_new`, hand
/// chunks in via `rfl_stream_send_bytes`, terminate with
/// `rfl_stream_end` / `rfl_stream_error`. Free with `rfl_stream_free`.
pub struct rfl_stream {
    id: StreamId,
    sender: flume::Sender<StreamFrame>,
    origin_actor: String,
    origin_port: String,
    content_type: Option<String>,
}

/// Allocate a new stream. `buffer_size == 0` creates an unbounded channel;
/// any positive value sets a bounded buffer (backpressure).
///
/// `origin_actor` and `origin_port` are metadata attached to the
/// StreamHandle that lets consumers trace where the stream came from.
/// Either may be NULL to use empty strings.
///
/// `content_type` is an optional MIME hint (NULL for none).
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_new(
    buffer_size: usize,
    origin_actor: *const c_char,
    origin_port: *const c_char,
    content_type: *const c_char,
) -> *mut rfl_stream {
    crate::clear_last_error();
    let buf = if buffer_size == 0 {
        None
    } else {
        Some(buffer_size)
    };
    let (id, sender) = STREAM_REGISTRY.create_stream(buf);

    let origin_actor = cstr_or_default(origin_actor);
    let origin_port = cstr_or_default(origin_port);
    let content_type = if content_type.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(content_type) }
            .to_str()
            .ok()
            .map(str::to_owned)
    };

    Box::into_raw(Box::new(rfl_stream {
        id,
        sender,
        origin_actor,
        origin_port,
        content_type,
    }))
}

fn cstr_or_default(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }
            .to_str()
            .map(str::to_owned)
            .unwrap_or_default()
    }
}

/// Send a Data frame. The buffer is copied into a refcounted allocation.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_send_bytes(
    s: *mut rfl_stream,
    data: *const u8,
    len: usize,
) -> rfl_status {
    crate::clear_last_error();
    if s.is_null() {
        return rfl_status::NullArg;
    }
    let buf: Vec<u8> = if len == 0 {
        Vec::new()
    } else if data.is_null() {
        set_last_error("data pointer is null with len > 0");
        return rfl_status::NullArg;
    } else {
        unsafe { std::slice::from_raw_parts(data, len) }.to_vec()
    };
    let handle = unsafe { &*s };
    match handle.sender.send(StreamFrame::Data(Arc::new(buf))) {
        Ok(_) => rfl_status::Ok,
        Err(e) => {
            set_last_error(format!("stream send: {e}"));
            rfl_status::Runtime
        }
    }
}

/// Send a Begin frame (stream metadata). Optional — use before the first
/// `send_bytes` if you want consumers to see `content_type` / `size_hint`
/// / `metadata_json` before any data.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_send_begin(
    s: *mut rfl_stream,
    content_type: *const c_char,
    size_hint: u64,
    has_size_hint: c_int,
    metadata_json: *const c_char,
) -> rfl_status {
    crate::clear_last_error();
    if s.is_null() {
        return rfl_status::NullArg;
    }
    let ct = if content_type.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(content_type) }
            .to_str()
            .ok()
            .map(str::to_owned)
    };
    let sh = if has_size_hint != 0 {
        Some(size_hint)
    } else {
        None
    };
    let meta = if metadata_json.is_null() {
        None
    } else {
        let s = match unsafe { CStr::from_ptr(metadata_json) }.to_str() {
            Ok(s) => s,
            Err(_) => {
                set_last_error("metadata_json is not valid UTF-8");
                return rfl_status::InvalidUtf8;
            }
        };
        match serde_json::from_str::<serde_json::Value>(s) {
            Ok(v) => Some(v),
            Err(e) => {
                set_last_error(format!("metadata_json parse: {e}"));
                return rfl_status::InvalidJson;
            }
        }
    };
    let handle = unsafe { &*s };
    match handle.sender.send(StreamFrame::Begin {
        content_type: ct,
        size_hint: sh,
        metadata: meta,
    }) {
        Ok(_) => rfl_status::Ok,
        Err(e) => {
            set_last_error(format!("stream send: {e}"));
            rfl_status::Runtime
        }
    }
}

/// Terminate the stream with success.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_end(s: *mut rfl_stream) -> rfl_status {
    crate::clear_last_error();
    if s.is_null() {
        return rfl_status::NullArg;
    }
    let handle = unsafe { &*s };
    match handle.sender.send(StreamFrame::End) {
        Ok(_) => rfl_status::Ok,
        Err(e) => {
            set_last_error(format!("stream end: {e}"));
            rfl_status::Runtime
        }
    }
}

/// Terminate the stream with an error.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_error(
    s: *mut rfl_stream,
    message: *const c_char,
) -> rfl_status {
    crate::clear_last_error();
    if s.is_null() {
        return rfl_status::NullArg;
    }
    let msg = cstr_or_default(message);
    let handle = unsafe { &*s };
    match handle.sender.send(StreamFrame::Error(msg)) {
        Ok(_) => rfl_status::Ok,
        Err(e) => {
            set_last_error(format!("stream error: {e}"));
            rfl_status::Runtime
        }
    }
}

/// Convert this stream producer into a `Message::StreamHandle` that can be
/// emitted on an output port. The producer is **consumed** — free is not
/// necessary after this call.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_into_message(s: *mut rfl_stream) -> *mut rfl_message {
    crate::clear_last_error();
    if s.is_null() {
        return std::ptr::null_mut();
    }
    let handle = unsafe { Box::from_raw(s) };
    let stream_handle = StreamHandle {
        stream_id: handle.id,
        origin_actor: handle.origin_actor.clone(),
        origin_port: handle.origin_port.clone(),
        content_type: handle.content_type.clone(),
        size_hint: None,
    };
    Box::into_raw(Box::new(rfl_message {
        inner: Message::StreamHandle(Arc::new(stream_handle)),
    }))
}

/// Free a producer handle without emitting it as a message. If the stream
/// has live consumers, they will see an `End` frame when the sender is
/// dropped.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_free(s: *mut rfl_stream) {
    if !s.is_null() {
        drop(unsafe { Box::from_raw(s) });
    }
}

// ─── consumer side ─────────────────────────────────────────────────────────

/// Opaque receiver for a stream's data channel.
pub struct rfl_stream_recv {
    rx: flume::Receiver<StreamFrame>,
    /// Last frame received — held so callers can borrow the bytes until
    /// the next `recv` call.
    last_bytes: Option<Arc<Vec<u8>>>,
}

/// Variant tag returned by `rfl_stream_recv_next`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum rfl_stream_frame_kind {
    Begin = 0,
    Data = 1,
    End = 2,
    Error = 3,
    Timeout = 4,
    Closed = 5,
}

/// Take the receiver for a `StreamHandle` message. Transfers ownership —
/// only one call succeeds per stream. Returns NULL if the message is not
/// a StreamHandle or the receiver has already been taken.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_stream_take(m: *mut rfl_message) -> *mut rfl_stream_recv {
    crate::clear_last_error();
    if m.is_null() {
        return std::ptr::null_mut();
    }
    let handle = match &unsafe { &*m }.inner {
        Message::StreamHandle(h) => Arc::clone(h),
        _ => {
            set_last_error("message is not a StreamHandle");
            return std::ptr::null_mut();
        }
    };
    match STREAM_REGISTRY.take_receiver(handle.stream_id) {
        Some(rx) => Box::into_raw(Box::new(rfl_stream_recv {
            rx,
            last_bytes: None,
        })),
        None => {
            set_last_error(format!(
                "no receiver available for stream {} (already taken?)",
                handle.stream_id
            ));
            std::ptr::null_mut()
        }
    }
}

/// Block up to `timeout_ms` for the next frame.
///
/// Writes the frame kind into `*out_kind`. On `Data` / `Error`, also
/// populates `*out_data` / `*out_len` / `*out_err` as appropriate. The
/// pointers are valid until the next call to `rfl_stream_recv_next` on
/// the same receiver.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_recv_next(
    r: *mut rfl_stream_recv,
    timeout_ms: u32,
    out_kind: *mut rfl_stream_frame_kind,
    out_data: *mut *const u8,
    out_len: *mut usize,
    out_err: *mut *mut c_char,
) -> rfl_status {
    crate::clear_last_error();
    if r.is_null() || out_kind.is_null() {
        return rfl_status::NullArg;
    }
    let recv = unsafe { &mut *r };
    recv.last_bytes = None;

    let deadline = std::time::Duration::from_millis(timeout_ms as u64);
    let frame = match recv.rx.recv_timeout(deadline) {
        Ok(f) => f,
        Err(flume::RecvTimeoutError::Timeout) => {
            unsafe { *out_kind = rfl_stream_frame_kind::Timeout };
            return rfl_status::Ok;
        }
        Err(flume::RecvTimeoutError::Disconnected) => {
            unsafe { *out_kind = rfl_stream_frame_kind::Closed };
            return rfl_status::Ok;
        }
    };

    match frame {
        StreamFrame::Begin { .. } => {
            // For Begin we don't populate out_data; callers can re-poll
            // for JSON metadata via a separate accessor if they need it.
            unsafe { *out_kind = rfl_stream_frame_kind::Begin };
        }
        StreamFrame::Data(buf) => {
            unsafe {
                *out_kind = rfl_stream_frame_kind::Data;
                if !out_data.is_null() {
                    *out_data = buf.as_ptr();
                }
                if !out_len.is_null() {
                    *out_len = buf.len();
                }
            }
            recv.last_bytes = Some(buf);
        }
        StreamFrame::End => {
            unsafe { *out_kind = rfl_stream_frame_kind::End };
        }
        StreamFrame::Error(msg) => unsafe {
            *out_kind = rfl_stream_frame_kind::Error;
            if !out_err.is_null() {
                let c = std::ffi::CString::new(msg)
                    .unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
                *out_err = c.into_raw();
            }
        },
    }
    rfl_status::Ok
}

/// Free a stream receiver. Safe on NULL.
#[no_mangle]
pub unsafe extern "C" fn rfl_stream_recv_free(r: *mut rfl_stream_recv) {
    if !r.is_null() {
        drop(unsafe { Box::from_raw(r) });
    }
}
