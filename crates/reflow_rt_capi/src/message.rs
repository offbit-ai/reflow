//! Opaque message handle + typed constructors/accessors.
//!
//! Passing `Message` through the ABI as a JSON string works for prototyping
//! but taxes every hot-path emit. The `rfl_message` handle wraps a real
//! `reflow_rt::actor_runtime::message::Message` so:
//!
//! - binary payloads stay as `Arc<Vec<u8>>`, zero-copy on the way through;
//! - stream handles live alongside their registry allocation instead of
//!   being serialized and re-resolved;
//! - numeric / string variants skip a JSON round-trip entirely.
//!
//! Ownership: constructors return a `*mut rfl_message` you own. Passing
//! the handle to `rfl_ctx_emit` **transfers** ownership to the runtime —
//! do not free it afterwards. To discard without emitting, call
//! `rfl_message_free`.

#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::sync::Arc;

use reflow_rt::actor_runtime::message::{EncodableValue, Message};

use crate::set_last_error;

/// Opaque Reflow message handle.
pub struct rfl_message {
    pub(crate) inner: Message,
}

impl rfl_message {
    pub(crate) fn into_message(self) -> Message {
        self.inner
    }
}

/// Variant tag — matches the `Message` enum in `reflow_actor`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum rfl_message_kind {
    Flow = 0,
    Boolean = 1,
    Integer = 2,
    Float = 3,
    String = 4,
    Object = 5,
    Array = 6,
    Bytes = 7,
    Error = 8,
    StreamHandle = 9,
    Optional = 10,
    /// Anything else — use `rfl_message_as_json` to inspect.
    Other = 99,
}

// ─── constructors ──────────────────────────────────────────────────────────

fn wrap(m: Message) -> *mut rfl_message {
    Box::into_raw(Box::new(rfl_message { inner: m }))
}

#[no_mangle]
pub extern "C" fn rfl_message_flow() -> *mut rfl_message {
    wrap(Message::Flow)
}

#[no_mangle]
pub extern "C" fn rfl_message_boolean(v: c_int) -> *mut rfl_message {
    wrap(Message::Boolean(v != 0))
}

#[no_mangle]
pub extern "C" fn rfl_message_integer(v: i64) -> *mut rfl_message {
    wrap(Message::Integer(v))
}

#[no_mangle]
pub extern "C" fn rfl_message_float(v: f64) -> *mut rfl_message {
    wrap(Message::Float(v))
}

/// UTF-8 string; copied.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_string(s: *const c_char) -> *mut rfl_message {
    crate::clear_last_error();
    if s.is_null() {
        set_last_error("string pointer is null");
        return std::ptr::null_mut();
    }
    match unsafe { CStr::from_ptr(s) }.to_str() {
        Ok(v) => wrap(Message::String(Arc::new(v.to_owned()))),
        Err(_) => {
            set_last_error("string is not valid UTF-8");
            std::ptr::null_mut()
        }
    }
}

/// Binary payload; the buffer is copied into a refcounted allocation.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_bytes(data: *const u8, len: usize) -> *mut rfl_message {
    crate::clear_last_error();
    let buf: Vec<u8> = if len == 0 {
        Vec::new()
    } else if data.is_null() {
        set_last_error("data pointer is null with len > 0");
        return std::ptr::null_mut();
    } else {
        unsafe { std::slice::from_raw_parts(data, len) }.to_vec()
    };
    wrap(Message::Bytes(Arc::new(buf)))
}

/// Object from JSON. The JSON must parse as any valid serde value.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_object_from_json(json: *const c_char) -> *mut rfl_message {
    crate::clear_last_error();
    if json.is_null() {
        set_last_error("json pointer is null");
        return std::ptr::null_mut();
    }
    let s = match unsafe { CStr::from_ptr(json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("json is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    match serde_json::from_str::<serde_json::Value>(s) {
        Ok(v) => wrap(Message::Object(Arc::new(EncodableValue::from(v)))),
        Err(e) => {
            set_last_error(format!("json parse: {e}"));
            std::ptr::null_mut()
        }
    }
}

/// Array from a JSON array string.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_array_from_json(json: *const c_char) -> *mut rfl_message {
    crate::clear_last_error();
    if json.is_null() {
        set_last_error("json pointer is null");
        return std::ptr::null_mut();
    }
    let s = match unsafe { CStr::from_ptr(json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("json is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    let v: serde_json::Value = match serde_json::from_str(s) {
        Ok(v) => v,
        Err(e) => {
            set_last_error(format!("json parse: {e}"));
            return std::ptr::null_mut();
        }
    };
    let arr = match v {
        serde_json::Value::Array(a) => a.into_iter().map(EncodableValue::from).collect::<Vec<_>>(),
        _ => {
            set_last_error("json value is not an array");
            return std::ptr::null_mut();
        }
    };
    wrap(Message::Array(Arc::new(arr)))
}

#[no_mangle]
pub unsafe extern "C" fn rfl_message_error(msg: *const c_char) -> *mut rfl_message {
    crate::clear_last_error();
    if msg.is_null() {
        set_last_error("msg pointer is null");
        return std::ptr::null_mut();
    }
    match unsafe { CStr::from_ptr(msg) }.to_str() {
        Ok(v) => wrap(Message::Error(Arc::new(v.to_owned()))),
        Err(_) => {
            set_last_error("msg is not valid UTF-8");
            std::ptr::null_mut()
        }
    }
}

/// Fallback: parse a fully-tagged `Message` JSON (i.e. the same shape the
/// legacy `rfl_ctx_emit`-by-JSON path consumes). Useful for tests /
/// debugging — prefer the typed constructors in production.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_from_json(json: *const c_char) -> *mut rfl_message {
    crate::clear_last_error();
    if json.is_null() {
        set_last_error("json pointer is null");
        return std::ptr::null_mut();
    }
    let s = match unsafe { CStr::from_ptr(json) }.to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("json is not valid UTF-8");
            return std::ptr::null_mut();
        }
    };
    match serde_json::from_str::<Message>(s) {
        Ok(m) => wrap(m),
        Err(e) => {
            set_last_error(format!("json parse (Message): {e}"));
            std::ptr::null_mut()
        }
    }
}

// ─── accessors ─────────────────────────────────────────────────────────────

#[no_mangle]
pub unsafe extern "C" fn rfl_message_get_kind(m: *const rfl_message) -> rfl_message_kind {
    if m.is_null() {
        return rfl_message_kind::Other;
    }
    match &unsafe { &*m }.inner {
        Message::Flow => rfl_message_kind::Flow,
        Message::Boolean(_) => rfl_message_kind::Boolean,
        Message::Integer(_) => rfl_message_kind::Integer,
        Message::Float(_) => rfl_message_kind::Float,
        Message::String(_) => rfl_message_kind::String,
        Message::Object(_) => rfl_message_kind::Object,
        Message::Array(_) => rfl_message_kind::Array,
        Message::Bytes(_) => rfl_message_kind::Bytes,
        Message::Error(_) => rfl_message_kind::Error,
        Message::StreamHandle(_) => rfl_message_kind::StreamHandle,
        Message::Optional(_) => rfl_message_kind::Optional,
        _ => rfl_message_kind::Other,
    }
}

/// If the message is a Boolean, writes its value into `*out` and returns 1.
/// Returns 0 otherwise.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_as_boolean(m: *const rfl_message, out: *mut c_int) -> c_int {
    if m.is_null() || out.is_null() {
        return 0;
    }
    if let Message::Boolean(v) = &unsafe { &*m }.inner {
        unsafe { *out = if *v { 1 } else { 0 } };
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfl_message_as_integer(m: *const rfl_message, out: *mut i64) -> c_int {
    if m.is_null() || out.is_null() {
        return 0;
    }
    if let Message::Integer(v) = &unsafe { &*m }.inner {
        unsafe { *out = *v };
        1
    } else {
        0
    }
}

#[no_mangle]
pub unsafe extern "C" fn rfl_message_as_float(m: *const rfl_message, out: *mut f64) -> c_int {
    if m.is_null() || out.is_null() {
        return 0;
    }
    if let Message::Float(v) = &unsafe { &*m }.inner {
        unsafe { *out = *v };
        1
    } else {
        0
    }
}

/// String access — returns a newly allocated C string on String/Error
/// variants, else NULL. Caller frees via `rfl_string_free`.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_as_string(m: *const rfl_message) -> *mut c_char {
    if m.is_null() {
        return std::ptr::null_mut();
    }
    let s: &str = match &unsafe { &*m }.inner {
        Message::String(s) => s.as_str(),
        Message::Error(s) => s.as_str(),
        _ => return std::ptr::null_mut(),
    };
    CString::new(s)
        .map(|c| c.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

/// Full message serialized as JSON. Always succeeds for representable
/// variants. Caller frees via `rfl_string_free`.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_as_json(m: *const rfl_message) -> *mut c_char {
    crate::clear_last_error();
    if m.is_null() {
        return std::ptr::null_mut();
    }
    match serde_json::to_string(&unsafe { &*m }.inner) {
        Ok(s) => CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("message serialize: {e}"));
            std::ptr::null_mut()
        }
    }
}

/// Inner data payload as JSON, with `EncodableValue` wrappers
/// transparently decoded. Covers every variant whose payload has a
/// useful JSON form: primitives, Object, Array, Optional, Event, Any,
/// Error; StreamHandle and RemoteReference return their serializable
/// locator metadata; NetworkEvent returns its `{event_type, data}`
/// shape; Encoded is decoded back to its inner Message and recursed.
/// Callers can `json.Unmarshal` straight into a typed Go struct /
/// Python dict / JS object without dealing with the `{type, data}`
/// envelope or the EncodableValue bitcode shape.
///
/// Returns NULL for Flow (control signal, no data) and Bytes (use the
/// bytes accessor). Caller frees via `rfl_string_free`.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_data_json(m: *const rfl_message) -> *mut c_char {
    crate::clear_last_error();
    if m.is_null() {
        return std::ptr::null_mut();
    }
    let value = match unsafe { &*m }.inner.data_value() {
        Some(v) => v,
        None => return std::ptr::null_mut(),
    };
    match serde_json::to_string(&value) {
        Ok(s) => CString::new(s)
            .map(|c| c.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            set_last_error(format!("message data serialize: {e}"));
            std::ptr::null_mut()
        }
    }
}

/// Zero-copy borrow of a `Bytes` payload. Writes a pointer into the
/// Arc'd buffer and its length. The pointer is valid until the message
/// handle is freed (or ownership transferred). Returns 1 on success, 0
/// if the message is not a Bytes variant.
#[no_mangle]
pub unsafe extern "C" fn rfl_message_bytes_borrow(
    m: *const rfl_message,
    out_data: *mut *const u8,
    out_len: *mut usize,
) -> c_int {
    if m.is_null() || out_data.is_null() || out_len.is_null() {
        return 0;
    }
    if let Message::Bytes(buf) = &unsafe { &*m }.inner {
        unsafe {
            *out_data = buf.as_ptr();
            *out_len = buf.len();
        }
        1
    } else {
        0
    }
}

/// Free a message handle. Safe on NULL.
///
/// Do **not** call this after handing the message to `rfl_ctx_emit`
/// (which transfers ownership).
#[no_mangle]
pub unsafe extern "C" fn rfl_message_free(m: *mut rfl_message) {
    if !m.is_null() {
        drop(unsafe { Box::from_raw(m) });
    }
}
