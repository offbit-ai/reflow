//! Python bindings for the Reflow runtime (pyo3).
//!
//! The native module mirrors the Node / Go / C ABI surface. A tiny
//! Python wrapper (`reflow/__init__.py`) adds the idiomatic `Actor`
//! base class pattern on top.

#![allow(non_local_definitions)]
#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use once_cell::sync::Lazy;
use parking_lot::Mutex as PlMutex;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict};
use pythonize::{depythonize, pythonize};
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
use reflow_rt::network::multi_graph::{
    CompositionConnection, CompositionEndpoint, GraphComposition, GraphComposer,
    GraphSource, SharedResource,
};
use reflow_rt::network::subgraph::SubgraphActor;
use serde::Deserialize;

// ─── shared tokio runtime ──────────────────────────────────────────────────

static RUNTIME: Lazy<Arc<tokio::runtime::Runtime>> = Lazy::new(|| {
    Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("reflow-rt-worker")
            .build()
            .expect("failed to build tokio runtime"),
    )
});

fn enter_runtime<R>(f: impl FnOnce() -> R) -> R {
    let _g = RUNTIME.enter();
    f()
}

/// Live network registry — populated by `Network.start()`, drained by
/// the `atexit` hook installed at module init. Without this, the
/// process can begin Python finalization while the Tokio worker is
/// still scheduling actor callbacks; the trampoline then tries to
/// acquire the GIL on a half-dead interpreter and panics inside pyo3.
static LIVE_NETWORKS: Lazy<PlMutex<Vec<std::sync::Weak<Mutex<RtNetwork>>>>> =
    Lazy::new(|| PlMutex::new(Vec::new()));

fn register_live_network(net: &Arc<Mutex<RtNetwork>>) {
    let mut g = LIVE_NETWORKS.lock();
    // Sweep dead weakrefs while we're here.
    g.retain(|w| w.strong_count() > 0);
    g.push(Arc::downgrade(net));
}

fn shutdown_all_live_networks() {
    let nets: Vec<Arc<Mutex<RtNetwork>>> = {
        let mut g = LIVE_NETWORKS.lock();
        let upgraded: Vec<_> = g.iter().filter_map(|w| w.upgrade()).collect();
        g.clear();
        upgraded
    };
    if nets.is_empty() {
        return;
    }
    let _g = RUNTIME.enter();
    for net in &nets {
        if let Ok(mut n) = net.lock() {
            n.shutdown();
        }
    }
    // shutdown() aborts spawned tasks but doesn't wait for them — the
    // task may already be inside `with_gil` when abort fires. Give the
    // runtime a brief moment for abort signals to actually unwind those
    // tasks before atexit returns and Python finalizes. 50 ms is well
    // below human-perceptible exit lag and dwarfs the per-tick budget.
    std::thread::sleep(std::time::Duration::from_millis(50));
}

/// Cheap finalize check — returns true while the Python interpreter is
/// still alive and safe to call into.
fn python_alive() -> bool {
    // SAFETY: Py_IsInitialized is documented as safe to call at any
    // time, including before init / after finalize. Returns 0 when
    // the interpreter has been finalized; the actor trampoline uses
    // this to short-circuit GIL acquisition rather than panic.
    unsafe { pyo3::ffi::Py_IsInitialized() != 0 }
}

fn map_err(e: impl std::fmt::Display) -> PyErr {
    PyRuntimeError::new_err(format!("{e}"))
}

// ─── Message ───────────────────────────────────────────────────────────────

#[pyclass(module = "reflow._native", name = "Message")]
pub struct PyMessage {
    inner: Message,
}

fn py_to_json(py: Python<'_>, v: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    depythonize::<serde_json::Value>(v)
        .map_err(|e| PyValueError::new_err(format!("python → json: {e}")))
}

#[pymethods]
impl PyMessage {
    #[staticmethod]
    fn flow() -> Self {
        Self { inner: Message::Flow }
    }
    #[staticmethod]
    fn boolean(v: bool) -> Self {
        Self { inner: Message::Boolean(v) }
    }
    #[staticmethod]
    fn integer(v: i64) -> Self {
        Self { inner: Message::Integer(v) }
    }
    #[staticmethod]
    fn float(v: f64) -> Self {
        Self { inner: Message::Float(v) }
    }
    #[staticmethod]
    fn string(v: String) -> Self {
        Self { inner: Message::String(Arc::new(v)) }
    }
    #[staticmethod]
    fn bytes(v: &Bound<'_, PyBytes>) -> Self {
        Self { inner: Message::Bytes(Arc::new(v.as_bytes().to_vec())) }
    }
    #[staticmethod]
    fn error(v: String) -> Self {
        Self { inner: Message::Error(Arc::new(v)) }
    }
    #[staticmethod]
    fn object(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let v = py_to_json(py, value)?;
        Ok(Self { inner: Message::Object(Arc::new(v.into())) })
    }
    #[staticmethod]
    fn array(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let v = py_to_json(py, value)?;
        match v {
            serde_json::Value::Array(a) => {
                let conv: Vec<_> = a.into_iter().map(|v| v.into()).collect();
                Ok(Self { inner: Message::Array(Arc::new(conv)) })
            }
            _ => Err(PyValueError::new_err("value is not a JSON array")),
        }
    }
    /// Parse a tagged Message JSON (`{"type": "...", "data": ...}`).
    #[staticmethod]
    fn from_json(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let v = py_to_json(py, value)?;
        let m: Message = serde_json::from_value(v).map_err(map_err)?;
        Ok(Self { inner: m })
    }

    #[getter]
    fn kind(&self) -> &'static str {
        match &self.inner {
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
        }
    }

    fn as_boolean(&self) -> Option<bool> {
        if let Message::Boolean(v) = &self.inner { Some(*v) } else { None }
    }
    fn as_integer(&self) -> Option<i64> {
        if let Message::Integer(v) = &self.inner { Some(*v) } else { None }
    }
    fn as_float(&self) -> Option<f64> {
        if let Message::Float(v) = &self.inner { Some(*v) } else { None }
    }
    fn as_string(&self) -> Option<String> {
        match &self.inner {
            Message::String(s) => Some(s.as_str().to_owned()),
            Message::Error(s) => Some(s.as_str().to_owned()),
            _ => None,
        }
    }
    fn as_bytes<'py>(&self, py: Python<'py>) -> Option<Bound<'py, PyBytes>> {
        if let Message::Bytes(b) = &self.inner {
            Some(PyBytes::new_bound(py, b.as_slice()))
        } else {
            None
        }
    }
    fn as_json<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = serde_json::to_value(&self.inner).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    /// Inner data payload as a native Python value, with the runtime's
    /// EncodableValue wrappers transparently decoded. Covers every
    /// variant whose payload has a useful JSON form: primitives,
    /// Object, Array, Optional, Event, Any, Error; StreamHandle and
    /// RemoteReference return their serializable locator metadata;
    /// NetworkEvent returns its `{event_type, data}` shape; Encoded
    /// is decoded back to its inner Message.
    ///
    /// Returns None for Flow (control signal, no data) and Bytes
    /// (use as_bytes — exposing the buffer as a JSON array would
    /// just bloat the wire).
    fn data<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyAny>>> {
        match self.inner.data_value() {
            Some(v) => Ok(Some(pythonize(py, &v).map_err(map_err)?)),
            None => Ok(None),
        }
    }

    /// StreamHandle: take the consumer side.
    fn take_stream(&self) -> PyResult<PyStreamReader> {
        match &self.inner {
            Message::StreamHandle(h) => {
                let rx = STREAM_REGISTRY
                    .take_receiver(h.stream_id)
                    .ok_or_else(|| {
                        PyRuntimeError::new_err(format!(
                            "no receiver for stream {} (already taken?)",
                            h.stream_id
                        ))
                    })?;
                Ok(PyStreamReader { rx })
            }
            _ => Err(PyValueError::new_err("message is not a StreamHandle")),
        }
    }
}

// ─── Stream producer / consumer ────────────────────────────────────────────

#[pyclass(module = "reflow._native", name = "Stream")]
pub struct PyStream {
    inner: PlMutex<Option<StreamInner>>,
}

struct StreamInner {
    id: StreamId,
    sender: flume::Sender<StreamFrame>,
    origin_actor: String,
    origin_port: String,
    content_type: Option<String>,
}

#[pymethods]
impl PyStream {
    #[staticmethod]
    #[pyo3(signature = (buffer_size=0, origin_actor=None, origin_port=None, content_type=None))]
    fn create(
        buffer_size: u32,
        origin_actor: Option<String>,
        origin_port: Option<String>,
        content_type: Option<String>,
    ) -> Self {
        let buf = if buffer_size == 0 { None } else { Some(buffer_size as usize) };
        let (id, sender) = STREAM_REGISTRY.create_stream(buf);
        Self {
            inner: PlMutex::new(Some(StreamInner {
                id,
                sender,
                origin_actor: origin_actor.unwrap_or_default(),
                origin_port: origin_port.unwrap_or_default(),
                content_type,
            })),
        }
    }

    fn send_bytes(&self, data: &Bound<'_, PyBytes>) -> PyResult<()> {
        let guard = self.inner.lock();
        let i = guard.as_ref().ok_or_else(|| PyRuntimeError::new_err("stream consumed"))?;
        i.sender
            .send(StreamFrame::Data(Arc::new(data.as_bytes().to_vec())))
            .map_err(map_err)
    }

    #[pyo3(signature = (content_type=None, size_hint=None, metadata=None))]
    fn send_begin(
        &self,
        py: Python<'_>,
        content_type: Option<String>,
        size_hint: Option<u64>,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let meta = match metadata {
            Some(m) => Some(py_to_json(py, m)?),
            None => None,
        };
        let guard = self.inner.lock();
        let i = guard.as_ref().ok_or_else(|| PyRuntimeError::new_err("stream consumed"))?;
        i.sender
            .send(StreamFrame::Begin {
                content_type,
                size_hint,
                metadata: meta,
            })
            .map_err(map_err)
    }

    fn end(&self) -> PyResult<()> {
        let guard = self.inner.lock();
        let i = guard.as_ref().ok_or_else(|| PyRuntimeError::new_err("stream consumed"))?;
        i.sender.send(StreamFrame::End).map_err(map_err)
    }

    fn error(&self, message: String) -> PyResult<()> {
        let guard = self.inner.lock();
        let i = guard.as_ref().ok_or_else(|| PyRuntimeError::new_err("stream consumed"))?;
        i.sender.send(StreamFrame::Error(message)).map_err(map_err)
    }

    /// Consume and return a Message.StreamHandle.
    fn into_message(&self) -> PyResult<PyMessage> {
        let mut guard = self.inner.lock();
        let inner = guard
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("stream already consumed"))?;
        let handle = RtStreamHandle {
            stream_id: inner.id,
            origin_actor: inner.origin_actor,
            origin_port: inner.origin_port,
            content_type: inner.content_type,
            size_hint: None,
        };
        Ok(PyMessage {
            inner: Message::StreamHandle(Arc::new(handle)),
        })
    }
}

#[pyclass(module = "reflow._native", name = "StreamReader")]
pub struct PyStreamReader {
    rx: flume::Receiver<StreamFrame>,
}

#[pymethods]
impl PyStreamReader {
    /// Block up to `timeout_ms` ms for a frame. Returns a dict
    /// `{kind: "begin"|"data"|"end"|"error"|"timeout"|"closed", ...}`.
    fn recv<'py>(&self, py: Python<'py>, timeout_ms: u32) -> PyResult<Bound<'py, PyDict>> {
        // Release the GIL for the blocking recv, then reacquire to build the dict.
        let frame_outcome = py.allow_threads(|| {
            let d = std::time::Duration::from_millis(timeout_ms as u64);
            self.rx.recv_timeout(d)
        });
        let out = PyDict::new_bound(py);
        match frame_outcome {
            Ok(StreamFrame::Begin { content_type, size_hint, .. }) => {
                out.set_item("kind", "begin")?;
                if let Some(ct) = content_type {
                    out.set_item("content_type", ct)?;
                }
                if let Some(sh) = size_hint {
                    out.set_item("size_hint", sh)?;
                }
            }
            Ok(StreamFrame::Data(buf)) => {
                out.set_item("kind", "data")?;
                out.set_item("data", PyBytes::new_bound(py, buf.as_slice()))?;
            }
            Ok(StreamFrame::End) => {
                out.set_item("kind", "end")?;
            }
            Ok(StreamFrame::Error(msg)) => {
                out.set_item("kind", "error")?;
                out.set_item("error", msg)?;
            }
            Err(flume::RecvTimeoutError::Timeout) => {
                out.set_item("kind", "timeout")?;
            }
            Err(flume::RecvTimeoutError::Disconnected) => {
                out.set_item("kind", "closed")?;
            }
        }
        Ok(out)
    }
}

// ─── Actor bridging — Python callable as an Actor ──────────────────────────

#[pyclass(module = "reflow._native", name = "ActorCallContext")]
pub struct PyActorCallContext {
    inputs_value: serde_json::Value,
    config_value: serde_json::Value,
    /// Pending outputs queued via `emit`. Flushed to the outport
    /// channel on `done`.
    outputs: PlMutex<HashMap<String, Message>>,
    reply: PlMutex<Option<flume::Sender<CallbackReply>>>,
    /// Direct handle to the outport sender, for `ctx.send` —
    /// flush-immediately semantics that match the JS / browser shim.
    /// Streaming actors push per-chunk packets through this without
    /// waiting for the tick to complete.
    outport_tx: flume::Sender<HashMap<String, Message>>,
}

enum CallbackReply {
    Ok(HashMap<String, Message>),
    Err(String),
}

#[pymethods]
impl PyActorCallContext {
    #[getter]
    fn inputs<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pythonize(py, &self.inputs_value).map_err(map_err)
    }

    #[getter]
    fn config<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pythonize(py, &self.config_value).map_err(map_err)
    }

    /// Queue one output packet on `port`. `message` is either a
    /// `Message` instance or a tagged JSON dict (`{"type": ...,
    /// "data": ...}`). Emits accumulate until `done` fires.
    fn emit(&self, py: Python<'_>, port: String, message: &Bound<'_, PyAny>) -> PyResult<()> {
        // Accept a Message handle or a JSON-shaped Message.
        let msg: Message = if let Ok(m) = message.extract::<PyRef<PyMessage>>() {
            m.inner.clone()
        } else {
            let v = py_to_json(py, message)?;
            serde_json::from_value(v).map_err(map_err)?
        };
        self.outputs.lock().insert(port, msg);
        Ok(())
    }

    /// Flush a packet to the outport **immediately**. `messages` is a
    /// dict keyed by port; each value is a `Message` handle or a
    /// tagged JSON dict. Use this for streaming actors that emit
    /// many packets per tick (LLM chunks, timer pulses, sensor
    /// readings) and want each packet to reach the consumer before
    /// the tick completes. Mirrors the JS / browser SDK's
    /// `ctx.send(...)`.
    fn send(&self, py: Python<'_>, messages: &Bound<'_, PyAny>) -> PyResult<()> {
        let dict = messages.downcast::<PyDict>().map_err(|_| {
            PyValueError::new_err("ctx.send(messages) expects a dict keyed by port")
        })?;
        let mut packet: HashMap<String, Message> = HashMap::new();
        for (k, v) in dict.iter() {
            let port: String = k.extract()?;
            let msg: Message = if let Ok(m) = v.extract::<PyRef<PyMessage>>() {
                m.inner.clone()
            } else {
                let j = py_to_json(py, &v)?;
                serde_json::from_value(j).map_err(map_err)?
            };
            packet.insert(port, msg);
        }
        if !packet.is_empty() {
            self.outport_tx
                .send(packet)
                .map_err(|e| PyRuntimeError::new_err(format!("outport closed: {e}")))?;
        }
        Ok(())
    }

    /// Resolve the tick. `outputs` is optional — any packets already
    /// queued via `emit` are always flushed. If `outputs` is supplied,
    /// its entries are merged on top of what was emitted. Values may be
    /// `Message` handles or JSON-shaped `{"type": ..., "data": ...}`
    /// dicts.
    #[pyo3(signature = (outputs=None))]
    fn done(&self, py: Python<'_>, outputs: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let mut hmap = std::mem::take(&mut *self.outputs.lock());
        if let Some(obj) = outputs {
            if !obj.is_none() {
                let dict = obj.downcast::<PyDict>().map_err(|_| {
                    PyValueError::new_err("ctx.done(outputs) expects a dict keyed by port")
                })?;
                for (k, v) in dict.iter() {
                    let port: String = k.extract()?;
                    let msg: Message = if let Ok(m) = v.extract::<PyRef<PyMessage>>() {
                        m.inner.clone()
                    } else {
                        let j = py_to_json(py, &v)?;
                        serde_json::from_value(j).map_err(map_err)?
                    };
                    hmap.insert(port, msg);
                }
            }
        }
        let tx = self.reply.lock().take().ok_or_else(|| {
            PyRuntimeError::new_err("actor context already resolved")
        })?;
        let _ = tx.send(CallbackReply::Ok(hmap));
        Ok(())
    }

    fn fail(&self, message: String) -> PyResult<()> {
        let tx = self.reply.lock().take().ok_or_else(|| {
            PyRuntimeError::new_err("actor context already resolved")
        })?;
        let _ = tx.send(CallbackReply::Err(message));
        Ok(())
    }
}

struct PyActorImpl {
    callable: PyObject,
    inports: Port,
    outports: Port,
    inport_names: Vec<String>,
    outport_names: Vec<String>,
    load: Arc<ActorLoad>,
    await_all_inports: bool,
}

impl RtActor for PyActorImpl {
    fn get_behavior(&self) -> ActorBehavior {
        let callable = Python::with_gil(|py| self.callable.clone_ref(py));
        Box::new(move |ctx: ActorContext| -> Pin<Box<dyn Future<Output = anyhow::Result<HashMap<String, Message>>> + Send + 'static>> {
            // Per-tick clone of the Python callable. This runs on the
            // network's scheduler thread BEFORE any async work. We
            // need to acquire the GIL here, but Python may be
            // finalizing concurrently — `with_gil` panics inside pyo3
            // (gil.rs check) when that happens. `Py_IsInitialized` is
            // a fast pre-filter; `catch_unwind` is the backstop for
            // the racy case where Python is alive at the check and
            // dead at the actual ffi call. Either way we surface an
            // anyhow::Error and let the actor task wind down quietly.
            if !python_alive() {
                return Box::pin(async {
                    Err(anyhow::anyhow!("python interpreter finalized"))
                });
            }
            let callable = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                Python::with_gil(|py| callable.clone_ref(py))
            })) {
                Ok(c) => c,
                Err(_) => {
                    return Box::pin(async {
                        Err(anyhow::anyhow!("python interpreter finalized"))
                    });
                }
            };
            Box::pin(async move {
                let payload = ctx.get_payload();
                let cfg = ctx.get_config().as_hashmap();
                let inputs_value: serde_json::Value =
                    serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);
                let config_value: serde_json::Value =
                    serde_json::to_value(cfg).unwrap_or(serde_json::Value::Null);

                let (reply_tx, reply_rx) = flume::bounded::<CallbackReply>(1);
                let call_ctx = PyActorCallContext {
                    inputs_value,
                    config_value,
                    outputs: PlMutex::new(HashMap::new()),
                    reply: PlMutex::new(Some(reply_tx)),
                    outport_tx: ctx.outports.0.clone(),
                };

                // Call the Python function with the GIL held. Skip if
                // the interpreter is already finalizing — touching pyo3
                // here would panic in `gil.rs:check_gil`. catch_unwind
                // covers the race window where the interpreter is
                // alive at the pre-check and dead at the ffi call.
                if !python_alive() {
                    return Err(anyhow::anyhow!("python interpreter finalized"));
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Python::with_gil(|py| {
                        let ctx_obj = Py::new(py, call_ctx)
                            .map_err(|e| anyhow::anyhow!("wrap ctx: {e}"))?;
                        match callable.call1(py, (ctx_obj,)) {
                            Ok(_) => Ok::<(), anyhow::Error>(()),
                            Err(e) => Err(anyhow::anyhow!("python actor callback raised: {e}")),
                        }
                    })
                }));
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {
                        return Err(anyhow::anyhow!("python interpreter finalized"));
                    }
                }

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
        let callable = Python::with_gil(|py| self.callable.clone_ref(py));
        Arc::new(Self {
            callable,
            inports: flume::bounded(50),
            outports: flume::bounded(50),
            inport_names: self.inport_names.clone(),
            outport_names: self.outport_names.clone(),
            load: Arc::new(ActorLoad::new(0)),
            await_all_inports: self.await_all_inports,
        })
    }
}

#[pyclass(module = "reflow._native", name = "Actor")]
pub struct PyActor {
    inner: Arc<dyn RtActor>,
}

#[pymethods]
impl PyActor {
    /// Wrap a Python callable as an Actor. `callable(ctx)` must resolve
    /// by calling `ctx.done(outputs=None)` or `ctx.fail(message)`.
    #[staticmethod]
    #[pyo3(signature = (component, inports, outports, callable, await_all_inports=false))]
    fn from_callback(
        component: String,
        inports: Vec<String>,
        outports: Vec<String>,
        callable: PyObject,
        await_all_inports: bool,
    ) -> Self {
        let _ = component; // currently only used for diagnostics
        let actor = Arc::new(PyActorImpl {
            callable,
            inports: flume::bounded(50),
            outports: flume::bounded(50),
            inport_names: inports,
            outport_names: outports,
            load: Arc::new(ActorLoad::new(0)),
            await_all_inports,
        }) as Arc<dyn RtActor>;
        Self { inner: actor }
    }
}

// ─── Template catalog ──────────────────────────────────────────────────────

#[pyfunction]
fn template_actor(template_id: String) -> PyResult<PyActor> {
    // Packs win over the bundled catalog — matches the C ABI order in
    // `rfl_template_actor_new`.
    if let Some(a) = reflow_rt::pack_loader::instantiate(&template_id) {
        return Ok(PyActor { inner: a });
    }
    match reflow_components::get_actor_for_template(&template_id) {
        Some(a) => Ok(PyActor { inner: a }),
        None => Err(PyValueError::new_err(format!(
            "unknown template id: '{template_id}' — no loaded pack or bundled catalog entry"
        ))),
    }
}

#[pyfunction]
fn template_list() -> Vec<String> {
    let mut ids: Vec<String> = reflow_rt::pack_loader::PACK_REGISTRY.template_ids();
    for k in reflow_components::get_template_mapping().into_keys() {
        if !ids.contains(&k) {
            ids.push(k);
        }
    }
    ids.sort();
    ids
}

// ─── Actor packs ───────────────────────────────────────────────────────────

#[pyfunction]
fn load_pack(path: String) -> PyResult<Vec<String>> {
    reflow_rt::pack_loader::load_pack(&path)
        .map_err(|e| PyRuntimeError::new_err(format!("load pack '{path}': {e:#}")))
}

#[pyfunction]
fn inspect_pack(py: Python<'_>, path: String) -> PyResult<PyObject> {
    let manifest = reflow_rt::pack_loader::inspect_pack(&path)
        .map_err(|e| PyRuntimeError::new_err(format!("inspect pack '{path}': {e:#}")))?;
    let value = serde_json::to_value(&manifest)
        .map_err(|e| PyRuntimeError::new_err(format!("serialize manifest: {e}")))?;
    Ok(pythonize(py, &value)
        .map_err(|e| PyRuntimeError::new_err(format!("pythonize manifest: {e}")))?
        .into())
}

#[pyfunction]
fn list_packs(py: Python<'_>) -> PyResult<PyObject> {
    let list = reflow_rt::pack_loader::PACK_REGISTRY.loaded_packs();
    let value = serde_json::to_value(&list)
        .map_err(|e| PyRuntimeError::new_err(format!("serialize list: {e}")))?;
    Ok(pythonize(py, &value)
        .map_err(|e| PyRuntimeError::new_err(format!("pythonize list: {e}")))?
        .into())
}

#[pyfunction]
fn pack_abi_version() -> u32 {
    reflow_rt::pack_loader::REFLOW_PACK_ABI_VERSION
}

// ─── Multi-graph composition ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct PyComposeRequest {
    #[serde(default)]
    graphs: Vec<reflow_rt::graph::types::GraphExport>,
    #[serde(default)]
    connections: Vec<PyComposeConn>,
    #[serde(default)]
    shared_resources: Vec<PyComposeShared>,
    #[serde(default)]
    properties: HashMap<String, serde_json::Value>,
    #[serde(default)]
    case_sensitive: Option<bool>,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct PyComposeConn {
    from: PyComposeEndpoint,
    to: PyComposeEndpoint,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct PyComposeEndpoint {
    process: String,
    port: String,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct PyComposeShared {
    name: String,
    component: String,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Compose N `GraphExport` dicts into a single `GraphExport` dict.
#[pyfunction]
fn compose_graphs<'py>(
    py: Python<'py>,
    composition: &Bound<'_, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let v = py_to_json(py, composition)?;
    let req: PyComposeRequest = serde_json::from_value(v).map_err(|e| {
        PyValueError::new_err(format!("composition parse: {e}"))
    })?;

    let composition = GraphComposition {
        sources: req
            .graphs
            .into_iter()
            .map(GraphSource::GraphExport)
            .collect(),
        connections: req
            .connections
            .into_iter()
            .map(|c| CompositionConnection {
                from: CompositionEndpoint { process: c.from.process, port: c.from.port, index: c.from.index },
                to:   CompositionEndpoint { process: c.to.process,   port: c.to.port,   index: c.to.index },
                metadata: c.metadata,
            })
            .collect(),
        shared_resources: req
            .shared_resources
            .into_iter()
            .map(|r| SharedResource { name: r.name, component: r.component, metadata: r.metadata })
            .collect(),
        properties: req.properties,
        case_sensitive: req.case_sensitive,
        metadata: req.metadata,
    };

    let composed = enter_runtime(|| {
        RUNTIME.block_on(async {
            let mut composer = GraphComposer::new();
            composer.compose_graphs(composition).await
        })
    })
    .map_err(|e| map_err(format!("compose_graphs: {e}")))?;

    let export = composed.export();
    let raw = serde_json::to_value(&export).map_err(map_err)?;
    pythonize(py, &raw).map_err(map_err)
}

// ─── Graph ─────────────────────────────────────────────────────────────────

#[pyclass(module = "reflow._native", name = "Graph")]
pub struct PyGraph {
    inner: PlMutex<RtGraph>,
}

#[pymethods]
impl PyGraph {
    #[new]
    #[pyo3(signature = (name="", case_sensitive=false))]
    fn new(name: &str, case_sensitive: bool) -> Self {
        Self {
            inner: PlMutex::new(RtGraph::new(name, case_sensitive, None)),
        }
    }

    #[staticmethod]
    fn from_json(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Self> {
        let v = py_to_json(py, value)?;
        let export: GraphExport = serde_json::from_value(v).map_err(map_err)?;
        Ok(Self {
            inner: PlMutex::new(RtGraph::load(export, None)),
        })
    }

    fn to_json<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let export = self.inner.lock().export();
        let v = serde_json::to_value(&export).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    #[pyo3(signature = (id, component, metadata=None))]
    fn add_node(
        &self,
        py: Python<'_>,
        id: String,
        component: String,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let md = parse_metadata(py, metadata)?;
        self.inner.lock().add_node(&id, &component, md);
        Ok(())
    }

    fn remove_node(&self, id: String) {
        self.inner.lock().remove_node(&id);
    }

    #[pyo3(signature = (out_node, out_port, in_node, in_port, metadata=None))]
    fn add_connection(
        &self,
        py: Python<'_>,
        out_node: String,
        out_port: String,
        in_node: String,
        in_port: String,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let md = parse_metadata(py, metadata)?;
        self.inner.lock().add_connection(&out_node, &out_port, &in_node, &in_port, md);
        Ok(())
    }

    #[pyo3(signature = (node, port, data, metadata=None))]
    fn add_initial(
        &self,
        py: Python<'_>,
        node: String,
        port: String,
        data: &Bound<'_, PyAny>,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let data_v = py_to_json(py, data)?;
        let md = parse_metadata(py, metadata)?;
        self.inner.lock().add_initial(data_v, &node, &port, md);
        Ok(())
    }

    // ── mutators (renames) ────────────────────────────────────────────────

    fn rename_node(&self, old_id: String, new_id: String) {
        self.inner.lock().rename_node(&old_id, &new_id);
    }

    fn rename_inport(&self, old_port: String, new_port: String) {
        self.inner.lock().rename_inport(&old_port, &new_port);
    }

    fn rename_outport(&self, old_port: String, new_port: String) {
        self.inner.lock().rename_outport(&old_port, &new_port);
    }

    // ── mutators (port lifecycle) ─────────────────────────────────────────

    #[pyo3(signature = (port_id, node_id, port_key, port_type=None, metadata=None))]
    fn add_inport(
        &self,
        py: Python<'_>,
        port_id: String,
        node_id: String,
        port_key: String,
        port_type: Option<&Bound<'_, PyAny>>,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let pt = parse_port_type(py, port_type)?;
        let md = parse_metadata(py, metadata)?;
        self.inner.lock().add_inport(&port_id, &node_id, &port_key, pt, md);
        Ok(())
    }

    #[pyo3(signature = (port_id, node_id, port_key, port_type=None, metadata=None))]
    fn add_outport(
        &self,
        py: Python<'_>,
        port_id: String,
        node_id: String,
        port_key: String,
        port_type: Option<&Bound<'_, PyAny>>,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let pt = parse_port_type(py, port_type)?;
        let md = parse_metadata(py, metadata)?;
        self.inner.lock().add_outport(&port_id, &node_id, &port_key, pt, md);
        Ok(())
    }

    fn remove_inport(&self, port_id: String) {
        self.inner.lock().remove_inport(&port_id);
    }

    fn remove_outport(&self, port_id: String) {
        self.inner.lock().remove_outport(&port_id);
    }

    // ── mutators (groups) ─────────────────────────────────────────────────

    #[pyo3(signature = (group_id, nodes, metadata=None))]
    fn add_group(
        &self,
        py: Python<'_>,
        group_id: String,
        nodes: Vec<String>,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let md = parse_metadata(py, metadata)?;
        self.inner.lock().add_group(&group_id, nodes, md);
        Ok(())
    }

    fn remove_group(&self, group_id: String) {
        self.inner.lock().remove_group(&group_id);
    }

    fn add_to_group(&self, group_id: String, node_id: String) {
        self.inner.lock().add_to_group(&group_id, &node_id);
    }

    fn remove_from_group(&self, group_id: String, node_id: String) {
        self.inner.lock().remove_from_group(&group_id, &node_id);
    }

    // ── mutators (connection / initial removal + indexed initials) ────────

    fn remove_connection(
        &self,
        out_node: String,
        out_port: String,
        in_node: String,
        in_port: String,
    ) {
        self.inner
            .lock()
            .remove_connection(&out_node, &out_port, &in_node, &in_port);
    }

    fn remove_initial(&self, node: String, port: String) {
        self.inner.lock().remove_initial(&node, &port);
    }

    #[pyo3(signature = (node, port, data, index, metadata=None))]
    fn add_initial_index(
        &self,
        py: Python<'_>,
        node: String,
        port: String,
        data: &Bound<'_, PyAny>,
        index: usize,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let data_v = py_to_json(py, data)?;
        let md = parse_metadata(py, metadata)?;
        self.inner
            .lock()
            .add_initial_index(data_v, &node, &port, index, md);
        Ok(())
    }

    #[pyo3(signature = (inport, data, metadata=None))]
    fn add_graph_initial(
        &self,
        py: Python<'_>,
        inport: String,
        data: &Bound<'_, PyAny>,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let data_v = py_to_json(py, data)?;
        let md = parse_metadata(py, metadata)?;
        self.inner.lock().add_graph_initial(data_v, &inport, md);
        Ok(())
    }

    #[pyo3(signature = (inport, data, index, metadata=None))]
    fn add_graph_initial_index(
        &self,
        py: Python<'_>,
        inport: String,
        data: &Bound<'_, PyAny>,
        index: usize,
        metadata: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let data_v = py_to_json(py, data)?;
        let md = parse_metadata(py, metadata)?;
        self.inner
            .lock()
            .add_graph_initial_index(data_v, &inport, index, md);
        Ok(())
    }

    fn remove_graph_initial(&self, inport: String) {
        self.inner.lock().remove_graph_initial(&inport);
    }

    // ── mutators (metadata setters + properties) ──────────────────────────

    fn set_node_metadata(
        &self,
        py: Python<'_>,
        id: String,
        metadata: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let md = parse_metadata_required(py, metadata)?;
        self.inner.lock().set_node_metadata(&id, md);
        Ok(())
    }

    fn set_connection_metadata(
        &self,
        py: Python<'_>,
        out_node: String,
        out_port: String,
        in_node: String,
        in_port: String,
        metadata: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let md = parse_metadata_required(py, metadata)?;
        self.inner
            .lock()
            .set_connection_metadata(&out_node, &out_port, &in_node, &in_port, md);
        Ok(())
    }

    fn set_inport_metadata(
        &self,
        py: Python<'_>,
        port_id: String,
        metadata: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let md = parse_metadata_required(py, metadata)?;
        self.inner.lock().set_inport_metadata(&port_id, md);
        Ok(())
    }

    fn set_outport_metadata(
        &self,
        py: Python<'_>,
        port_id: String,
        metadata: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let md = parse_metadata_required(py, metadata)?;
        self.inner.lock().set_outport_metadata(&port_id, md);
        Ok(())
    }

    fn set_group_metadata(
        &self,
        py: Python<'_>,
        group_id: String,
        metadata: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let md = parse_metadata_required(py, metadata)?;
        self.inner.lock().set_group_metadata(&group_id, md);
        Ok(())
    }

    fn set_properties(
        &self,
        py: Python<'_>,
        properties: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let md = parse_metadata_required(py, properties)?;
        self.inner.lock().set_properties(md);
        Ok(())
    }

    /// Replace this graph's state with another GraphExport.
    /// (`reflow_graph::Graph::import` is destructive.)
    fn import_graph(
        &self,
        py: Python<'_>,
        export: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let v = py_to_json(py, export)?;
        let exp: GraphExport = serde_json::from_value(v).map_err(map_err)?;
        self.inner.lock().import(exp);
        Ok(())
    }

    // ── queries ───────────────────────────────────────────────────────────

    fn get_node<'py>(&self, py: Python<'py>, id: String) -> PyResult<Bound<'py, PyAny>> {
        match self.inner.lock().get_node(&id) {
            Some(n) => {
                let v = serde_json::to_value(n).map_err(map_err)?;
                pythonize(py, &v).map_err(map_err)
            }
            None => Ok(py.None().into_bound(py)),
        }
    }

    fn nodes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = serde_json::to_value(self.inner.lock().get_nodes()).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    fn get_connection<'py>(
        &self,
        py: Python<'py>,
        out_node: String,
        out_port: String,
        in_node: String,
        in_port: String,
    ) -> PyResult<Bound<'py, PyAny>> {
        match self
            .inner
            .lock()
            .get_connection(&out_node, &out_port, &in_node, &in_port)
        {
            Some(c) => {
                let v = serde_json::to_value(&c).map_err(map_err)?;
                pythonize(py, &v).map_err(map_err)
            }
            None => Ok(py.None().into_bound(py)),
        }
    }

    fn connections<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = serde_json::to_value(self.inner.lock().get_connections()).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    fn groups<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = serde_json::to_value(&self.inner.lock().groups).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    fn inports<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let exp = self.inner.lock().export();
        let v = serde_json::to_value(&exp.inports).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    fn outports<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let exp = self.inner.lock().export();
        let v = serde_json::to_value(&exp.outports).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    fn initializers<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = serde_json::to_value(&self.inner.lock().initializers).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }

    fn properties<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let v = serde_json::to_value(self.inner.lock().get_properties()).map_err(map_err)?;
        pythonize(py, &v).map_err(map_err)
    }
}

fn parse_metadata(
    py: Python<'_>,
    v: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<HashMap<String, serde_json::Value>>> {
    match v {
        None => Ok(None),
        Some(b) if b.is_none() => Ok(None),
        Some(b) => {
            let json = py_to_json(py, b)?;
            match json {
                serde_json::Value::Null => Ok(None),
                serde_json::Value::Object(m) => Ok(Some(m.into_iter().collect())),
                _ => Err(PyValueError::new_err("metadata must be a dict or None")),
            }
        }
    }
}

/// `set_*_metadata` and `set_properties` on the underlying Graph
/// take an owned HashMap, not Option. Coerce None / empty to empty.
fn parse_metadata_required(
    py: Python<'_>,
    v: &Bound<'_, PyAny>,
) -> PyResult<HashMap<String, serde_json::Value>> {
    if v.is_none() {
        return Ok(HashMap::new());
    }
    match py_to_json(py, v)? {
        serde_json::Value::Null => Ok(HashMap::new()),
        serde_json::Value::Object(m) => Ok(m.into_iter().collect()),
        _ => Err(PyValueError::new_err("metadata must be a dict")),
    }
}

/// `None`/`null` → `PortType::Any`. Otherwise must deserialize to a
/// `PortType` (e.g. `{"type":"flow"}`, `"All"` is NOT valid since the
/// enum is adjacently tagged).
fn parse_port_type(
    py: Python<'_>,
    v: Option<&Bound<'_, PyAny>>,
) -> PyResult<reflow_rt::graph::types::PortType> {
    use reflow_rt::graph::types::PortType;
    match v {
        None => Ok(PortType::Any),
        Some(b) if b.is_none() => Ok(PortType::Any),
        Some(b) => {
            let j = py_to_json(py, b)?;
            if j.is_null() {
                return Ok(PortType::Any);
            }
            serde_json::from_value::<PortType>(j).map_err(map_err)
        }
    }
}

// ─── Subgraph builder ──────────────────────────────────────────────────────

#[pyclass(module = "reflow._native", name = "SubgraphBuilder", subclass)]
pub struct PySubgraphBuilder {
    export: PlMutex<GraphExport>,
    actors: PlMutex<HashMap<String, Arc<dyn RtActor>>>,
}

#[pymethods]
impl PySubgraphBuilder {
    #[new]
    fn new(py: Python<'_>, export: &Bound<'_, PyAny>) -> PyResult<Self> {
        let v = py_to_json(py, export)?;
        let export: GraphExport = serde_json::from_value(v).map_err(map_err)?;
        Ok(Self {
            export: PlMutex::new(export),
            actors: PlMutex::new(HashMap::new()),
        })
    }

    fn register_actor(&self, component: String, actor: &PyActor) {
        self.actors.lock().insert(component, Arc::clone(&actor.inner));
    }

    fn fill_from_catalog(&self) -> PyResult<()> {
        let export = self.export.lock();
        let mut actors = self.actors.lock();
        let needed: Vec<String> = export
            .processes
            .values()
            .map(|n| n.component.clone())
            .filter(|c| !actors.contains_key(c))
            .collect();
        for c in needed {
            match reflow_components::get_actor_for_template(&c) {
                Some(a) => {
                    actors.insert(c, a);
                }
                None => {
                    return Err(PyRuntimeError::new_err(format!(
                        "subgraph references unknown component '{c}'"
                    )));
                }
            }
        }
        Ok(())
    }

    fn build(&self) -> PyResult<PyActor> {
        let export = self.export.lock().clone();
        let actors = self.actors.lock().clone();
        for node in export.processes.values() {
            if !actors.contains_key(&node.component) {
                return Err(PyRuntimeError::new_err(format!(
                    "subgraph references unregistered component '{}'",
                    node.component
                )));
            }
        }
        let sg = SubgraphActor::from_graph_export(&export, actors).map_err(map_err)?;
        Ok(PyActor {
            inner: Arc::new(sg) as Arc<dyn RtActor>,
        })
    }
}

// ─── Network ───────────────────────────────────────────────────────────────

#[pyclass(module = "reflow._native", name = "Network", subclass)]
pub struct PyNetwork {
    inner: Arc<Mutex<RtNetwork>>,
}

#[pymethods]
impl PyNetwork {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(py: Python<'_>, config: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let cfg: NetworkConfig = match config {
            None => NetworkConfig::default(),
            Some(v) if v.is_none() => NetworkConfig::default(),
            Some(v) => {
                let json = py_to_json(py, v)?;
                serde_json::from_value(json).map_err(map_err)?
            }
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(RtNetwork::new(cfg))),
        })
    }

    #[staticmethod]
    fn from_graph(graph: &PyGraph) -> Self {
        let g = graph.inner.lock();
        let net_arc = RtNetwork::with_graph(NetworkConfig::default(), &g);
        Self { inner: net_arc }
    }

    fn register_actor(&self, template_id: String, actor: &PyActor) -> PyResult<()> {
        self.inner
            .lock()
            .unwrap()
            .register_actor_arc(&template_id, Arc::clone(&actor.inner))
            .map_err(map_err)
    }

    #[pyo3(signature = (id, template_id, config=None))]
    fn add_node(
        &self,
        py: Python<'_>,
        id: String,
        template_id: String,
        config: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let md = parse_metadata(py, config)?;
        self.inner
            .lock()
            .unwrap()
            .add_node(&id, &template_id, md)
            .map_err(map_err)
    }

    fn add_connection(
        &self,
        from_actor: String,
        from_port: String,
        to_actor: String,
        to_port: String,
    ) {
        self.inner.lock().unwrap().add_connection(Connector {
            from: ConnectionPoint {
                actor: from_actor,
                port: from_port,
                initial_data: None,
            },
            to: ConnectionPoint {
                actor: to_actor,
                port: to_port,
                initial_data: None,
            },
        });
    }

    fn add_initial(
        &self,
        py: Python<'_>,
        actor: String,
        port: String,
        message: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let v = py_to_json(py, message)?;
        let msg: Message = serde_json::from_value(v).map_err(map_err)?;
        self.inner.lock().unwrap().add_initial(InitialPacket {
            to: ConnectionPoint::new(&actor, &port, Some(msg)),
        });
        Ok(())
    }

    fn start(&self) -> PyResult<()> {
        enter_runtime(|| {
            self.inner.lock().unwrap().start().map_err(map_err)
        })?;
        register_live_network(&self.inner);
        Ok(())
    }

    fn shutdown(&self) {
        enter_runtime(|| {
            self.inner.lock().unwrap().shutdown();
        });
    }

    fn events(&self) -> PyEventStream {
        let rx = self.inner.lock().unwrap().get_event_receiver();
        PyEventStream { rx }
    }
}

// ─── Event stream ──────────────────────────────────────────────────────────

#[pyclass(module = "reflow._native", name = "EventStream")]
pub struct PyEventStream {
    rx: flume::Receiver<NetworkEvent>,
}

#[pymethods]
impl PyEventStream {
    /// Block up to `timeout_ms` for the next event. Returns `None` on
    /// timeout; raises on channel close.
    #[pyo3(signature = (timeout_ms=0))]
    fn recv<'py>(&self, py: Python<'py>, timeout_ms: u32) -> PyResult<Option<Bound<'py, PyAny>>> {
        let outcome = py.allow_threads(|| {
            if timeout_ms == 0 {
                self.rx.recv().map_err(|_| flume::RecvTimeoutError::Disconnected)
            } else {
                self.rx
                    .recv_timeout(std::time::Duration::from_millis(timeout_ms as u64))
            }
        });
        match outcome {
            Ok(evt) => {
                let v = serde_json::to_value(&evt).map_err(map_err)?;
                Ok(Some(pythonize(py, &v).map_err(map_err)?))
            }
            Err(flume::RecvTimeoutError::Timeout) => Ok(None),
            Err(flume::RecvTimeoutError::Disconnected) => {
                Err(PyRuntimeError::new_err("event channel closed"))
            }
        }
    }
}

// ─── Module init ───────────────────────────────────────────────────────────

/// Drains every live Network. Wired into Python's `atexit` so the
/// Tokio worker stops scheduling actor callbacks before the
/// interpreter finalizes — otherwise the trampoline races finalize
/// and panics inside pyo3's GIL check (gil.rs:198).
#[pyfunction]
fn _shutdown_all_networks() {
    shutdown_all_live_networks();
}

#[pymodule]
fn _native(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyMessage>()?;
    m.add_class::<PyStream>()?;
    m.add_class::<PyStreamReader>()?;
    m.add_class::<PyActor>()?;
    m.add_class::<PyActorCallContext>()?;
    m.add_class::<PyGraph>()?;
    m.add_class::<PySubgraphBuilder>()?;
    m.add_class::<PyNetwork>()?;
    m.add_class::<PyEventStream>()?;
    m.add_function(wrap_pyfunction!(template_actor, m)?)?;
    m.add_function(wrap_pyfunction!(template_list, m)?)?;
    m.add_function(wrap_pyfunction!(load_pack, m)?)?;
    m.add_function(wrap_pyfunction!(inspect_pack, m)?)?;
    m.add_function(wrap_pyfunction!(list_packs, m)?)?;
    m.add_function(wrap_pyfunction!(pack_abi_version, m)?)?;
    m.add_function(wrap_pyfunction!(compose_graphs, m)?)?;
    m.add_function(wrap_pyfunction!(_shutdown_all_networks, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Register an atexit handler so live networks drain before the
    // interpreter starts finalizing. atexit fires while the GIL and
    // module state are still valid, so calling shutdown() on each
    // RtNetwork is safe; once it returns the Tokio worker no longer
    // touches Python.
    let atexit = py.import_bound("atexit")?;
    let cb = m.getattr("_shutdown_all_networks")?;
    atexit.call_method1("register", (cb,))?;

    Ok(())
}
