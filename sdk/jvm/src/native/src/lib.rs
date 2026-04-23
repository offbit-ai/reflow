//! JNI bindings for the Reflow runtime.
//!
//! Each Java class in `ai.offbit.reflow` owns a `long nativePtr` that
//! points at a Box-allocated Rust-side handle. Every `nativeFree` frees
//! the Box. The JVM's finalization / `Cleaner` path can drive this, or
//! the Java code can call `close()` explicitly.

#![allow(clippy::missing_safety_doc)]
#![allow(non_snake_case)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use jni::JNIEnv;
use jni::objects::{GlobalRef, JByteArray, JClass, JObject, JObjectArray, JString, JValue};
use jni::sys::{jboolean, jbyteArray, jdouble, jint, jlong, jobject};
use jni::{JavaVM, NativeMethod};
use once_cell::sync::Lazy;
use parking_lot::Mutex as PlMutex;
use std::sync::Mutex;

use reflow_rt::actor_runtime::message::Message;
use reflow_rt::actor_runtime::stream::{
    STREAM_REGISTRY, StreamFrame, StreamHandle as RtStreamHandle, StreamId,
};
use reflow_rt::actor_runtime::{
    Actor as RtActor, ActorBehavior, ActorContext, ActorLoad, ActorState, MemoryState, Port,
};
use reflow_rt::graph::Graph as RtGraph;
use reflow_rt::graph::types::GraphExport;
use reflow_rt::network::connector::{ConnectionPoint, Connector, InitialPacket};
use reflow_rt::network::network::{
    Network as RtNetwork, NetworkConfig, NetworkEvent,
};
use reflow_rt::network::subgraph::SubgraphActor;

// ─── shared tokio runtime ──────────────────────────────────────────────────

static RUNTIME: Lazy<Arc<tokio::runtime::Runtime>> = Lazy::new(|| {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("reflow-jvm")
            .build()
            .expect("failed to build tokio runtime"),
    )
});

fn enter_runtime<R>(f: impl FnOnce() -> R) -> R {
    let _g = RUNTIME.enter();
    f()
}

// ─── JVM pointer stashed once per process so tokio workers can attach ──────

static JVM: Lazy<PlMutex<Option<Arc<JavaVM>>>> = Lazy::new(|| PlMutex::new(None));

#[no_mangle]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _reserved: *mut std::ffi::c_void) -> jint {
    *JVM.lock() = Some(Arc::new(vm));
    jni::sys::JNI_VERSION_1_8
}

fn jvm() -> Arc<JavaVM> {
    JVM.lock()
        .as_ref()
        .cloned()
        .expect("JNI_OnLoad did not run — JVM handle missing")
}

// ─── helpers ───────────────────────────────────────────────────────────────

fn throw_runtime(env: &mut JNIEnv, msg: impl AsRef<str>) {
    let _ = env.throw_new("java/lang/RuntimeException", msg.as_ref());
}

fn jstring_to_string(env: &mut JNIEnv, s: &JString) -> Result<String, String> {
    let jstr = env
        .get_string(s)
        .map_err(|e| format!("get_string: {e}"))?;
    jstr.to_str()
        .map(str::to_owned)
        .map_err(|e| format!("utf-8: {e}"))
}

fn new_jstring<'e>(env: &mut JNIEnv<'e>, s: &str) -> Result<JString<'e>, String> {
    env.new_string(s).map_err(|e| format!("new_string: {e}"))
}

/// Down-cast a Box'd pointer back into its Rust type.
unsafe fn as_ref_mut<'a, T>(ptr: jlong) -> Option<&'a mut T> {
    if ptr == 0 {
        None
    } else {
        Some(&mut *(ptr as *mut T))
    }
}
unsafe fn as_ref<'a, T>(ptr: jlong) -> Option<&'a T> {
    if ptr == 0 {
        None
    } else {
        Some(&*(ptr as *const T))
    }
}
unsafe fn box_from_ptr<T>(ptr: jlong) -> Option<Box<T>> {
    if ptr == 0 {
        None
    } else {
        Some(Box::from_raw(ptr as *mut T))
    }
}

// ─── Message ───────────────────────────────────────────────────────────────

pub struct MessageHandle {
    inner: Message,
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeFlow(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    Box::into_raw(Box::new(MessageHandle { inner: Message::Flow })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeBoolean(
    _env: JNIEnv,
    _class: JClass,
    v: jboolean,
) -> jlong {
    Box::into_raw(Box::new(MessageHandle { inner: Message::Boolean(v != 0) })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeInteger(
    _env: JNIEnv,
    _class: JClass,
    v: jlong,
) -> jlong {
    Box::into_raw(Box::new(MessageHandle { inner: Message::Integer(v) })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeFloat(
    _env: JNIEnv,
    _class: JClass,
    v: jdouble,
) -> jlong {
    Box::into_raw(Box::new(MessageHandle { inner: Message::Float(v) })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeString<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    s: JString<'local>,
) -> jlong {
    match jstring_to_string(&mut env, &s) {
        Ok(v) => Box::into_raw(Box::new(MessageHandle {
            inner: Message::String(Arc::new(v)),
        })) as jlong,
        Err(e) => {
            throw_runtime(&mut env, &e);
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeError<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    s: JString<'local>,
) -> jlong {
    match jstring_to_string(&mut env, &s) {
        Ok(v) => Box::into_raw(Box::new(MessageHandle {
            inner: Message::Error(Arc::new(v)),
        })) as jlong,
        Err(e) => {
            throw_runtime(&mut env, &e);
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeBytes<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    bytes: JByteArray<'local>,
) -> jlong {
    let buf = match env.convert_byte_array(&bytes) {
        Ok(b) => b,
        Err(e) => {
            throw_runtime(&mut env, format!("bytes: {e}"));
            return 0;
        }
    };
    Box::into_raw(Box::new(MessageHandle {
        inner: Message::Bytes(Arc::new(buf)),
    })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeFromJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    json: JString<'local>,
) -> jlong {
    let s = match jstring_to_string(&mut env, &json) {
        Ok(v) => v,
        Err(e) => {
            throw_runtime(&mut env, &e);
            return 0;
        }
    };
    match serde_json::from_str::<Message>(&s) {
        Ok(m) => Box::into_raw(Box::new(MessageHandle { inner: m })) as jlong,
        Err(e) => {
            throw_runtime(&mut env, format!("Message parse: {e}"));
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeKind<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jobject {
    let name = match unsafe { as_ref::<MessageHandle>(ptr) } {
        None => "Other",
        Some(h) => match &h.inner {
            Message::Flow => "Flow",
            Message::Boolean(_) => "Boolean",
            Message::Integer(_) => "Integer",
            Message::Float(_) => "Float",
            Message::String(_) => "String",
            Message::Object(_) => "Object",
            Message::Array(_) => "Array",
            Message::Bytes(_) => "Bytes",
            Message::Error(_) => "Error",
            Message::StreamHandle(_) => "StreamHandle",
            Message::Optional(_) => "Optional",
            Message::Event(_) => "Event",
            Message::Encoded(_) => "Encoded",
            Message::Any(_) => "Any",
            Message::RemoteReference { .. } => "RemoteReference",
            Message::NetworkEvent { .. } => "NetworkEvent",
        },
    };
    env.new_string(name)
        .map(|j| j.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeAsJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jobject {
    let h = match unsafe { as_ref::<MessageHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match serde_json::to_string(&h.inner) {
        Ok(s) => env
            .new_string(&s)
            .map(|j| j.into_raw())
            .unwrap_or(std::ptr::null_mut()),
        Err(e) => {
            throw_runtime(&mut env, format!("serialize: {e}"));
            std::ptr::null_mut()
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeAsString<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jobject {
    let h = match unsafe { as_ref::<MessageHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let s: &str = match &h.inner {
        Message::String(s) => s.as_str(),
        Message::Error(s) => s.as_str(),
        _ => return std::ptr::null_mut(),
    };
    env.new_string(s)
        .map(|j| j.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeAsInteger<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jlong {
    if let Some(h) = unsafe { as_ref::<MessageHandle>(ptr) } {
        if let Message::Integer(v) = &h.inner {
            return *v;
        }
    }
    jlong::MIN // sentinel for "not an Integer"; Java side checks via kind
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeAsFloat<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jdouble {
    if let Some(h) = unsafe { as_ref::<MessageHandle>(ptr) } {
        if let Message::Float(v) = &h.inner {
            return *v;
        }
    }
    f64::NAN
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeAsBoolean<'local>(
    _env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jboolean {
    if let Some(h) = unsafe { as_ref::<MessageHandle>(ptr) } {
        if let Message::Boolean(v) = &h.inner {
            return if *v { 1 } else { 0 };
        }
    }
    0
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeAsBytes<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jbyteArray {
    let h = match unsafe { as_ref::<MessageHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let buf: &[u8] = match &h.inner {
        Message::Bytes(b) => b.as_slice(),
        _ => return std::ptr::null_mut(),
    };
    match env.byte_array_from_slice(buf) {
        Ok(arr) => arr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    unsafe {
        let _ = box_from_ptr::<MessageHandle>(ptr);
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Message_nativeTakeStream<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jlong {
    let h = match unsafe { as_ref::<MessageHandle>(ptr) } {
        Some(h) => h,
        None => return 0,
    };
    let handle = match &h.inner {
        Message::StreamHandle(h) => Arc::clone(h),
        _ => {
            throw_runtime(&mut env, "message is not a StreamHandle");
            return 0;
        }
    };
    match STREAM_REGISTRY.take_receiver(handle.stream_id) {
        Some(rx) => Box::into_raw(Box::new(StreamReaderHandle { rx })) as jlong,
        None => {
            throw_runtime(
                &mut env,
                format!(
                    "no receiver for stream {} (already taken?)",
                    handle.stream_id
                ),
            );
            0
        }
    }
}

// ─── Stream producer + consumer ────────────────────────────────────────────

pub struct StreamProducerHandle {
    inner: PlMutex<Option<StreamInner>>,
}
struct StreamInner {
    id: StreamId,
    sender: flume::Sender<StreamFrame>,
    origin_actor: String,
    origin_port: String,
    content_type: Option<String>,
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Stream_nativeCreate<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    buffer_size: jint,
    origin_actor: JString<'local>,
    origin_port: JString<'local>,
    content_type: JString<'local>,
) -> jlong {
    let origin_actor = jstring_to_string(&mut env, &origin_actor).unwrap_or_default();
    let origin_port = jstring_to_string(&mut env, &origin_port).unwrap_or_default();
    let content_type = jstring_to_string(&mut env, &content_type).ok();
    let buf = if buffer_size <= 0 {
        None
    } else {
        Some(buffer_size as usize)
    };
    let (id, sender) = STREAM_REGISTRY.create_stream(buf);
    Box::into_raw(Box::new(StreamProducerHandle {
        inner: PlMutex::new(Some(StreamInner {
            id,
            sender,
            origin_actor,
            origin_port,
            content_type,
        })),
    })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Stream_nativeSendBytes<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    bytes: JByteArray<'local>,
) {
    let h = match unsafe { as_ref::<StreamProducerHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let buf = match env.convert_byte_array(&bytes) {
        Ok(b) => b,
        Err(e) => {
            throw_runtime(&mut env, format!("bytes: {e}"));
            return;
        }
    };
    let guard = h.inner.lock();
    if let Some(i) = guard.as_ref() {
        let _ = i.sender.send(StreamFrame::Data(Arc::new(buf)));
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Stream_nativeEnd(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if let Some(h) = unsafe { as_ref::<StreamProducerHandle>(ptr) } {
        let guard = h.inner.lock();
        if let Some(i) = guard.as_ref() {
            let _ = i.sender.send(StreamFrame::End);
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Stream_nativeError<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    msg: JString<'local>,
) {
    let msg = jstring_to_string(&mut env, &msg).unwrap_or_default();
    if let Some(h) = unsafe { as_ref::<StreamProducerHandle>(ptr) } {
        let guard = h.inner.lock();
        if let Some(i) = guard.as_ref() {
            let _ = i.sender.send(StreamFrame::Error(msg));
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Stream_nativeIntoMessage(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jlong {
    let h = match unsafe { as_ref_mut::<StreamProducerHandle>(ptr) } {
        Some(h) => h,
        None => return 0,
    };
    let inner = match h.inner.lock().take() {
        Some(i) => i,
        None => return 0,
    };
    let handle = RtStreamHandle {
        stream_id: inner.id,
        origin_actor: inner.origin_actor,
        origin_port: inner.origin_port,
        content_type: inner.content_type,
        size_hint: None,
    };
    Box::into_raw(Box::new(MessageHandle {
        inner: Message::StreamHandle(Arc::new(handle)),
    })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Stream_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    unsafe {
        let _ = box_from_ptr::<StreamProducerHandle>(ptr);
    }
}

pub struct StreamReaderHandle {
    rx: flume::Receiver<StreamFrame>,
}

/// Returns a JSON string describing the frame, of shape
/// `{"kind":"data","data":"<base64>","error":"...","content_type":"..."}`.
/// The JNI layer would prefer to hand back typed structures, but a
/// JSON string keeps the Java parsing simple for the smoke-test tier.
#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_StreamReader_nativeRecv<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    timeout_ms: jint,
) -> jbyteArray {
    let h = match unsafe { as_ref::<StreamReaderHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let d = std::time::Duration::from_millis(timeout_ms.max(0) as u64);
    let outcome = h.rx.recv_timeout(d);

    // Build a {kind, data, error, content_type, size_hint} buffer as a
    // simple tagged byte array: first byte is the kind, then optional
    // fields. Java side decodes.
    //
    // Layout:
    //   byte 0: kind (0=begin, 1=data, 2=end, 3=error, 4=timeout, 5=closed)
    //   rest:   for data, the payload bytes (length implied by array length)
    //           for error, UTF-8 error message bytes
    //           else: empty
    let (kind, payload): (u8, Vec<u8>) = match outcome {
        Ok(StreamFrame::Begin { .. }) => (0, Vec::new()),
        Ok(StreamFrame::Data(buf)) => (1, buf.to_vec()),
        Ok(StreamFrame::End) => (2, Vec::new()),
        Ok(StreamFrame::Error(msg)) => (3, msg.into_bytes()),
        Err(flume::RecvTimeoutError::Timeout) => (4, Vec::new()),
        Err(flume::RecvTimeoutError::Disconnected) => (5, Vec::new()),
    };
    let mut out = Vec::with_capacity(payload.len() + 1);
    out.push(kind);
    out.extend(payload);
    match env.byte_array_from_slice(&out) {
        Ok(arr) => arr.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_StreamReader_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    unsafe {
        let _ = box_from_ptr::<StreamReaderHandle>(ptr);
    }
}

// ─── Actor callback — Java Actor as RtActor ────────────────────────────────

pub struct ActorHandle {
    inner: Arc<dyn RtActor>,
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Actor_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    unsafe {
        let _ = box_from_ptr::<ActorHandle>(ptr);
    }
}

/// Wrap a Java `Actor` instance as a RtActor. The Java side provides a
/// bound callback object (a lambda) that the Rust layer invokes on every
/// tick. Ports + await are also passed in.
///
/// Signature contract:
///
///   nativeRegister(
///     String component,
///     String[] inports, String[] outports,
///     boolean awaitAll,
///     Runnable-like callback: (ActorCallContext) -> void
///   )
#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Actor_nativeRegister<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    component: JString<'local>,
    inports: JObjectArray<'local>,
    outports: JObjectArray<'local>,
    await_all: jboolean,
    callback: JObject<'local>,
) -> jlong {
    let component = match jstring_to_string(&mut env, &component) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime(&mut env, &e);
            return 0;
        }
    };
    let in_names = match jobject_array_to_strings(&mut env, &inports) {
        Ok(v) => v,
        Err(e) => {
            throw_runtime(&mut env, &e);
            return 0;
        }
    };
    let out_names = match jobject_array_to_strings(&mut env, &outports) {
        Ok(v) => v,
        Err(e) => {
            throw_runtime(&mut env, &e);
            return 0;
        }
    };

    let global_cb = match env.new_global_ref(callback) {
        Ok(g) => g,
        Err(e) => {
            throw_runtime(&mut env, format!("new_global_ref: {e}"));
            return 0;
        }
    };

    let actor = Arc::new(JvmActor {
        callback: Arc::new(global_cb),
        inports: flume::bounded(50),
        outports: flume::bounded(50),
        inport_names: in_names,
        outport_names: out_names,
        component,
        load: Arc::new(ActorLoad::new(0)),
        await_all_inports: await_all != 0,
    }) as Arc<dyn RtActor>;

    Box::into_raw(Box::new(ActorHandle { inner: actor })) as jlong
}

fn jobject_array_to_strings(
    env: &mut JNIEnv,
    arr: &JObjectArray,
) -> Result<Vec<String>, String> {
    let len = env
        .get_array_length(arr)
        .map_err(|e| format!("array len: {e}"))?;
    let mut out = Vec::with_capacity(len as usize);
    for i in 0..len {
        let obj = env
            .get_object_array_element(arr, i)
            .map_err(|e| format!("elem {i}: {e}"))?;
        let s: JString = obj.into();
        out.push(jstring_to_string(env, &s)?);
    }
    Ok(out)
}

struct JvmActor {
    callback: Arc<GlobalRef>,
    inports: Port,
    outports: Port,
    inport_names: Vec<String>,
    outport_names: Vec<String>,
    component: String,
    load: Arc<ActorLoad>,
    await_all_inports: bool,
}

impl RtActor for JvmActor {
    fn get_behavior(&self) -> ActorBehavior {
        let callback = Arc::clone(&self.callback);
        let component = self.component.clone();
        Box::new(move |ctx: ActorContext| -> Pin<Box<dyn Future<Output = anyhow::Result<HashMap<String, Message>>> + Send + 'static>> {
            let callback = Arc::clone(&callback);
            let component = component.clone();
            Box::pin(async move {
                let payload = ctx.get_payload();
                let cfg = ctx.get_config().as_hashmap();
                let inputs_value: serde_json::Value =
                    serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);
                let config_value: serde_json::Value =
                    serde_json::to_value(cfg).unwrap_or(serde_json::Value::Null);

                let (reply_tx, reply_rx) = flume::bounded::<CallbackReply>(1);
                let ctx_box = Box::new(ActorCallContextHandle {
                    inputs: inputs_value,
                    config: config_value,
                    outputs: PlMutex::new(HashMap::new()),
                    reply: PlMutex::new(Some(reply_tx)),
                });
                let ctx_ptr = Box::into_raw(ctx_box) as jlong;

                // Invoke the Java dispatcher in an inner scope so the
                // (non-Send) JNIEnv drops before we await the reply on
                // the tokio worker.
                let dispatch_res: Result<(), anyhow::Error> = {
                    let jvm = jvm();
                    let mut env = jvm
                        .attach_current_thread_permanently()
                        .map_err(|e| anyhow::anyhow!("attach thread: {e}"))?;
                    let cb_obj = callback.as_obj();
                    let res = env.call_static_method(
                        "ai/offbit/reflow/Actor",
                        "__dispatch",
                        "(Ljava/lang/Object;J)V",
                        &[JValue::Object(cb_obj), JValue::Long(ctx_ptr)],
                    );
                    match res {
                        Ok(_) => Ok(()),
                        Err(e) => {
                            let _ = env.exception_clear();
                            Err(anyhow::anyhow!("java dispatch ({component}): {e}"))
                        }
                    }
                };
                dispatch_res?;

                match reply_rx.recv_async().await {
                    Ok(CallbackReply::Ok(out)) => Ok(out),
                    Ok(CallbackReply::Err(msg)) => Err(anyhow::anyhow!(msg)),
                    Err(e) => Err(anyhow::anyhow!("actor callback channel: {e}")),
                }
            })
        })
    }

    fn get_outports(&self) -> Port { self.outports.clone() }
    fn get_inports(&self) -> Port { self.inports.clone() }
    fn inport_names(&self) -> Vec<String> { self.inport_names.clone() }
    fn outport_names(&self) -> Vec<String> { self.outport_names.clone() }
    fn await_all_inports(&self) -> bool { self.await_all_inports }

    fn create_state(&self) -> Arc<PlMutex<dyn ActorState>> {
        Arc::new(PlMutex::new(MemoryState::default()))
    }

    fn load_count(&self) -> Arc<ActorLoad> { Arc::clone(&self.load) }

    fn create_instance(&self) -> Arc<dyn RtActor> {
        Arc::new(Self {
            callback: Arc::clone(&self.callback),
            inports: flume::bounded(50),
            outports: flume::bounded(50),
            inport_names: self.inport_names.clone(),
            outport_names: self.outport_names.clone(),
            component: self.component.clone(),
            load: Arc::new(ActorLoad::new(0)),
            await_all_inports: self.await_all_inports,
        })
    }
}

// ─── ActorCallContext — opaque pointer exposed to Java ─────────────────────

pub struct ActorCallContextHandle {
    inputs: serde_json::Value,
    config: serde_json::Value,
    outputs: PlMutex<HashMap<String, Message>>,
    reply: PlMutex<Option<flume::Sender<CallbackReply>>>,
}

enum CallbackReply {
    Ok(HashMap<String, Message>),
    Err(String),
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_ActorCallContext_nativeInputs<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jobject {
    let h = match unsafe { as_ref::<ActorCallContextHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match serde_json::to_string(&h.inputs) {
        Ok(s) => env.new_string(&s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(e) => { throw_runtime(&mut env, format!("{e}")); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_ActorCallContext_nativeConfig<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jobject {
    let h = match unsafe { as_ref::<ActorCallContextHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    match serde_json::to_string(&h.config) {
        Ok(s) => env.new_string(&s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(e) => { throw_runtime(&mut env, format!("{e}")); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_ActorCallContext_nativeEmit<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    port: JString<'local>,
    message_ptr: jlong,
) {
    let h = match unsafe { as_ref::<ActorCallContextHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let port = match jstring_to_string(&mut env, &port) {
        Ok(s) => s,
        Err(e) => { throw_runtime(&mut env, &e); return; }
    };
    let msg_box = match unsafe { box_from_ptr::<MessageHandle>(message_ptr) } {
        Some(b) => b,
        None => { throw_runtime(&mut env, "emit: null message pointer"); return; }
    };
    h.outputs.lock().insert(port, msg_box.inner);
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_ActorCallContext_nativeDone(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    let h = match unsafe { as_ref::<ActorCallContextHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let outputs = std::mem::take(&mut *h.outputs.lock());
    if let Some(tx) = h.reply.lock().take() {
        let _ = tx.send(CallbackReply::Ok(outputs));
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_ActorCallContext_nativeFail<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    message: JString<'local>,
) {
    let h = match unsafe { as_ref::<ActorCallContextHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let msg = jstring_to_string(&mut env, &message).unwrap_or_default();
    if let Some(tx) = h.reply.lock().take() {
        let _ = tx.send(CallbackReply::Err(msg));
    }
}

// ─── Template catalog ──────────────────────────────────────────────────────

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Templates_nativeTemplateActor<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    id: JString<'local>,
) -> jlong {
    let id = match jstring_to_string(&mut env, &id) {
        Ok(s) => s,
        Err(e) => {
            throw_runtime(&mut env, &e);
            return 0;
        }
    };
    match reflow_components::get_actor_for_template(&id) {
        Some(a) => Box::into_raw(Box::new(ActorHandle { inner: a })) as jlong,
        None => {
            throw_runtime(&mut env, format!("unknown template id: '{id}'"));
            0
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Templates_nativeTemplateList<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jobject {
    let ids: Vec<String> = reflow_components::get_template_mapping()
        .into_keys()
        .collect();
    let raw = serde_json::to_string(&ids).unwrap_or_default();
    env.new_string(&raw).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
}

// ─── Graph ─────────────────────────────────────────────────────────────────

pub struct GraphHandle {
    inner: PlMutex<RtGraph>,
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeNew<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    name: JString<'local>,
    case_sensitive: jboolean,
) -> jlong {
    let name = jstring_to_string(&mut env, &name).unwrap_or_default();
    Box::into_raw(Box::new(GraphHandle {
        inner: PlMutex::new(RtGraph::new(&name, case_sensitive != 0, None)),
    })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeFromJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    json: JString<'local>,
) -> jlong {
    let s = match jstring_to_string(&mut env, &json) {
        Ok(v) => v,
        Err(e) => { throw_runtime(&mut env, &e); return 0; }
    };
    let export: GraphExport = match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(e) => { throw_runtime(&mut env, format!("GraphExport parse: {e}")); return 0; }
    };
    Box::into_raw(Box::new(GraphHandle {
        inner: PlMutex::new(RtGraph::load(export, None)),
    })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeToJson<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jobject {
    let h = match unsafe { as_ref::<GraphHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let v = h.inner.lock().export();
    match serde_json::to_string(&v) {
        Ok(s) => env.new_string(&s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut()),
        Err(e) => { throw_runtime(&mut env, format!("{e}")); std::ptr::null_mut() }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeAddNode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    id: JString<'local>,
    component: JString<'local>,
    metadata_json: JString<'local>,
) {
    let h = match unsafe { as_ref::<GraphHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let id = jstring_to_string(&mut env, &id).unwrap_or_default();
    let component = jstring_to_string(&mut env, &component).unwrap_or_default();
    let md = parse_metadata_arg(&mut env, metadata_json);
    h.inner.lock().add_node(&id, &component, md);
}

fn parse_metadata_arg(
    env: &mut JNIEnv,
    json: JString,
) -> Option<HashMap<String, serde_json::Value>> {
    if json.is_null() {
        return None;
    }
    let s = match jstring_to_string(env, &json) {
        Ok(s) if !s.is_empty() && s != "null" => s,
        _ => return None,
    };
    match serde_json::from_str::<HashMap<String, serde_json::Value>>(&s) {
        Ok(m) => Some(m),
        Err(_) => None,
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeRemoveNode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    id: JString<'local>,
) {
    let h = match unsafe { as_ref::<GraphHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let id = jstring_to_string(&mut env, &id).unwrap_or_default();
    h.inner.lock().remove_node(&id);
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeAddConnection<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    out_node: JString<'local>,
    out_port: JString<'local>,
    in_node: JString<'local>,
    in_port: JString<'local>,
    metadata_json: JString<'local>,
) {
    let h = match unsafe { as_ref::<GraphHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let a = jstring_to_string(&mut env, &out_node).unwrap_or_default();
    let b = jstring_to_string(&mut env, &out_port).unwrap_or_default();
    let c = jstring_to_string(&mut env, &in_node).unwrap_or_default();
    let d = jstring_to_string(&mut env, &in_port).unwrap_or_default();
    let md = parse_metadata_arg(&mut env, metadata_json);
    h.inner.lock().add_connection(&a, &b, &c, &d, md);
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeAddInitial<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    node: JString<'local>,
    port: JString<'local>,
    data_json: JString<'local>,
    metadata_json: JString<'local>,
) {
    let h = match unsafe { as_ref::<GraphHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let node = jstring_to_string(&mut env, &node).unwrap_or_default();
    let port = jstring_to_string(&mut env, &port).unwrap_or_default();
    let data_s = jstring_to_string(&mut env, &data_json).unwrap_or_default();
    let data: serde_json::Value = match serde_json::from_str(&data_s) {
        Ok(v) => v,
        Err(e) => { throw_runtime(&mut env, format!("data_json: {e}")); return; }
    };
    let md = parse_metadata_arg(&mut env, metadata_json);
    h.inner.lock().add_initial(data, &node, &port, md);
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Graph_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    unsafe { let _ = box_from_ptr::<GraphHandle>(ptr); }
}

// ─── Network ───────────────────────────────────────────────────────────────

pub struct NetworkHandle {
    inner: Arc<Mutex<RtNetwork>>,
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeNew(
    _env: JNIEnv,
    _class: JClass,
) -> jlong {
    let net = RtNetwork::new(NetworkConfig::default());
    Box::into_raw(Box::new(NetworkHandle {
        inner: Arc::new(Mutex::new(net)),
    })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeFromGraph(
    _env: JNIEnv,
    _class: JClass,
    graph_ptr: jlong,
) -> jlong {
    let g = match unsafe { as_ref::<GraphHandle>(graph_ptr) } {
        Some(g) => g,
        None => return 0,
    };
    let guard = g.inner.lock();
    let net_arc = RtNetwork::with_graph(NetworkConfig::default(), &guard);
    Box::into_raw(Box::new(NetworkHandle { inner: net_arc })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    let handle = match unsafe { box_from_ptr::<NetworkHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    enter_runtime(|| handle.inner.lock().unwrap().shutdown());
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeRegisterActor<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    template_id: JString<'local>,
    actor_ptr: jlong,
) {
    let h = match unsafe { as_ref::<NetworkHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let template_id = jstring_to_string(&mut env, &template_id).unwrap_or_default();
    let actor_box = match unsafe { box_from_ptr::<ActorHandle>(actor_ptr) } {
        Some(b) => b,
        None => return,
    };
    if let Err(e) = h.inner.lock().unwrap().register_actor_arc(&template_id, actor_box.inner) {
        throw_runtime(&mut env, format!("{e}"));
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeAddNode<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    id: JString<'local>,
    template_id: JString<'local>,
    config_json: JString<'local>,
) {
    let h = match unsafe { as_ref::<NetworkHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let id = jstring_to_string(&mut env, &id).unwrap_or_default();
    let tpl = jstring_to_string(&mut env, &template_id).unwrap_or_default();
    let md = parse_metadata_arg(&mut env, config_json);
    if let Err(e) = h.inner.lock().unwrap().add_node(&id, &tpl, md) {
        throw_runtime(&mut env, format!("{e}"));
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeAddConnection<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    from_actor: JString<'local>,
    from_port: JString<'local>,
    to_actor: JString<'local>,
    to_port: JString<'local>,
) {
    let h = match unsafe { as_ref::<NetworkHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let a = jstring_to_string(&mut env, &from_actor).unwrap_or_default();
    let b = jstring_to_string(&mut env, &from_port).unwrap_or_default();
    let c = jstring_to_string(&mut env, &to_actor).unwrap_or_default();
    let d = jstring_to_string(&mut env, &to_port).unwrap_or_default();
    h.inner.lock().unwrap().add_connection(Connector {
        from: ConnectionPoint { actor: a, port: b, initial_data: None },
        to:   ConnectionPoint { actor: c, port: d, initial_data: None },
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeAddInitial<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    actor: JString<'local>,
    port: JString<'local>,
    message_json: JString<'local>,
) {
    let h = match unsafe { as_ref::<NetworkHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let actor = jstring_to_string(&mut env, &actor).unwrap_or_default();
    let port = jstring_to_string(&mut env, &port).unwrap_or_default();
    let raw = jstring_to_string(&mut env, &message_json).unwrap_or_default();
    let msg: Message = match serde_json::from_str(&raw) {
        Ok(m) => m,
        Err(e) => { throw_runtime(&mut env, format!("Message parse: {e}")); return; }
    };
    h.inner.lock().unwrap().add_initial(InitialPacket {
        to: ConnectionPoint::new(&actor, &port, Some(msg)),
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeStart<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) {
    let h = match unsafe { as_ref::<NetworkHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    enter_runtime(|| {
        if let Err(e) = h.inner.lock().unwrap().start() {
            throw_runtime(&mut env, format!("{e}"));
        }
    });
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeShutdown(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    if let Some(h) = unsafe { as_ref::<NetworkHandle>(ptr) } {
        enter_runtime(|| h.inner.lock().unwrap().shutdown());
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_Network_nativeEvents(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) -> jlong {
    let h = match unsafe { as_ref::<NetworkHandle>(ptr) } {
        Some(h) => h,
        None => return 0,
    };
    let rx = h.inner.lock().unwrap().get_event_receiver();
    Box::into_raw(Box::new(EventStreamHandle { rx })) as jlong
}

pub struct EventStreamHandle {
    rx: flume::Receiver<NetworkEvent>,
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_EventStream_nativeRecv<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    timeout_ms: jint,
) -> jobject {
    let h = match unsafe { as_ref::<EventStreamHandle>(ptr) } {
        Some(h) => h,
        None => return std::ptr::null_mut(),
    };
    let d = std::time::Duration::from_millis(timeout_ms.max(0) as u64);
    match h.rx.recv_timeout(d) {
        Ok(evt) => {
            let s = serde_json::to_string(&evt).unwrap_or_default();
            env.new_string(&s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
        }
        Err(flume::RecvTimeoutError::Timeout) => std::ptr::null_mut(),
        Err(flume::RecvTimeoutError::Disconnected) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_EventStream_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    unsafe { let _ = box_from_ptr::<EventStreamHandle>(ptr); }
}

// ─── Subgraph builder ──────────────────────────────────────────────────────

pub struct SubgraphBuilderHandle {
    export: PlMutex<GraphExport>,
    actors: PlMutex<HashMap<String, Arc<dyn RtActor>>>,
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_SubgraphBuilder_nativeNew<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    json: JString<'local>,
) -> jlong {
    let s = match jstring_to_string(&mut env, &json) {
        Ok(v) => v,
        Err(e) => { throw_runtime(&mut env, &e); return 0; }
    };
    let export: GraphExport = match serde_json::from_str(&s) {
        Ok(v) => v,
        Err(e) => { throw_runtime(&mut env, format!("GraphExport parse: {e}")); return 0; }
    };
    Box::into_raw(Box::new(SubgraphBuilderHandle {
        export: PlMutex::new(export),
        actors: PlMutex::new(HashMap::new()),
    })) as jlong
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_SubgraphBuilder_nativeRegisterActor<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
    component: JString<'local>,
    actor_ptr: jlong,
) {
    let h = match unsafe { as_ref::<SubgraphBuilderHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let component = jstring_to_string(&mut env, &component).unwrap_or_default();
    let actor_box = match unsafe { box_from_ptr::<ActorHandle>(actor_ptr) } {
        Some(b) => b,
        None => return,
    };
    h.actors.lock().insert(component, actor_box.inner);
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_SubgraphBuilder_nativeFillFromCatalog<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) {
    let h = match unsafe { as_ref::<SubgraphBuilderHandle>(ptr) } {
        Some(h) => h,
        None => return,
    };
    let export = h.export.lock();
    let mut actors = h.actors.lock();
    let needed: Vec<String> = export
        .processes
        .values()
        .map(|n| n.component.clone())
        .filter(|c| !actors.contains_key(c))
        .collect();
    for c in needed {
        match reflow_components::get_actor_for_template(&c) {
            Some(a) => { actors.insert(c, a); }
            None => { throw_runtime(&mut env, format!("unknown component '{c}'")); return; }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_SubgraphBuilder_nativeBuild<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    ptr: jlong,
) -> jlong {
    let h = match unsafe { as_ref::<SubgraphBuilderHandle>(ptr) } {
        Some(h) => h,
        None => return 0,
    };
    let export = h.export.lock().clone();
    let actors = h.actors.lock().clone();
    for node in export.processes.values() {
        if !actors.contains_key(&node.component) {
            throw_runtime(
                &mut env,
                format!("unregistered component '{}'", node.component),
            );
            return 0;
        }
    }
    match SubgraphActor::from_graph_export(&export, actors) {
        Ok(sg) => Box::into_raw(Box::new(ActorHandle {
            inner: Arc::new(sg) as Arc<dyn RtActor>,
        })) as jlong,
        Err(e) => { throw_runtime(&mut env, format!("subgraph build: {e}")); 0 }
    }
}

#[no_mangle]
pub extern "system" fn Java_ai_offbit_reflow_SubgraphBuilder_nativeFree(
    _env: JNIEnv,
    _class: JClass,
    ptr: jlong,
) {
    unsafe { let _ = box_from_ptr::<SubgraphBuilderHandle>(ptr); }
}

// Unused lints that this module doesn't trip (macros, etc.):
#[allow(dead_code)]
fn _unused_native_method() -> NativeMethod {
    unreachable!()
}
