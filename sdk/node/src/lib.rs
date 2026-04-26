//! Node.js bindings for the Reflow runtime.
//!
//! Design mirrors the frozen `reflow_rt_capi` surface so the SDK contract
//! stays stable across language bindings:
//!
//!  - Typed `Message` / `Network` / `Graph` / `Subgraph` classes.
//!  - Actor registration via JS callback (wrapped in a ThreadsafeFunction).
//!  - Template catalog and event stream exposed as idiomatic Node APIs.

#![allow(clippy::missing_safety_doc)]

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::{
    ErrorStrategy, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi_derive::napi;
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
            .thread_name("reflow-node")
            .build()
            .expect("failed to build tokio runtime"),
    )
});

fn enter_runtime<R>(f: impl FnOnce() -> R) -> R {
    let _g = RUNTIME.enter();
    f()
}

// ─── Message ────────────────────────────────────────────────────────────────

/// A Reflow `Message`. Construct with the static helpers; inspect with
/// `kind` / `asBoolean` / `asInteger` / `asFloat` / `asString` / `asJson`.
#[napi]
pub struct ReflowMessage {
    inner: Message,
}

#[napi]
impl ReflowMessage {
    #[napi(factory)]
    pub fn flow() -> Self {
        Self {
            inner: Message::Flow,
        }
    }

    #[napi(factory)]
    pub fn boolean(v: bool) -> Self {
        Self {
            inner: Message::Boolean(v),
        }
    }

    #[napi(factory)]
    pub fn integer(v: i64) -> Self {
        Self {
            inner: Message::Integer(v),
        }
    }

    #[napi(factory)]
    pub fn float(v: f64) -> Self {
        Self {
            inner: Message::Float(v),
        }
    }

    #[napi(factory)]
    pub fn string(v: String) -> Self {
        Self {
            inner: Message::String(Arc::new(v)),
        }
    }

    #[napi(factory)]
    pub fn bytes(v: Buffer) -> Self {
        Self {
            inner: Message::Bytes(Arc::new(v.to_vec())),
        }
    }

    /// Build a `Message::Object` from any JS value (serialized through JSON).
    #[napi(factory)]
    pub fn object(value: serde_json::Value) -> Self {
        Self {
            inner: Message::Object(Arc::new(value.into())),
        }
    }

    /// Build a `Message::Array` from a JS array (serialized through JSON).
    #[napi(factory)]
    pub fn array(value: serde_json::Value) -> Result<Self> {
        let arr = match value {
            serde_json::Value::Array(a) => a.into_iter().map(|v| v.into()).collect::<Vec<_>>(),
            _ => return Err(Error::from_reason("value is not a JSON array")),
        };
        Ok(Self {
            inner: Message::Array(Arc::new(arr)),
        })
    }

    #[napi(factory)]
    pub fn error(v: String) -> Self {
        Self {
            inner: Message::Error(Arc::new(v)),
        }
    }

    /// Fallback constructor accepting any `Message` in its canonical JSON
    /// shape (`{ type, data? }`).
    #[napi(factory, js_name = "fromJson")]
    pub fn from_json(value: serde_json::Value) -> Result<Self> {
        serde_json::from_value::<Message>(value)
            .map(|m| Self { inner: m })
            .map_err(|e| Error::from_reason(format!("message parse: {e}")))
    }

    /// Returns the variant name (e.g. "Flow", "Integer", "Bytes").
    #[napi(getter)]
    pub fn kind(&self) -> &'static str {
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

    #[napi(js_name = "asBoolean")]
    pub fn as_boolean(&self) -> Option<bool> {
        if let Message::Boolean(v) = &self.inner {
            Some(*v)
        } else {
            None
        }
    }

    #[napi(js_name = "asInteger")]
    pub fn as_integer(&self) -> Option<i64> {
        if let Message::Integer(v) = &self.inner {
            Some(*v)
        } else {
            None
        }
    }

    #[napi(js_name = "asFloat")]
    pub fn as_float(&self) -> Option<f64> {
        if let Message::Float(v) = &self.inner {
            Some(*v)
        } else {
            None
        }
    }

    #[napi(js_name = "asString")]
    pub fn as_string(&self) -> Option<String> {
        match &self.inner {
            Message::String(s) => Some(s.as_str().to_owned()),
            Message::Error(s) => Some(s.as_str().to_owned()),
            _ => None,
        }
    }

    #[napi(js_name = "asBytes")]
    pub fn as_bytes(&self) -> Option<Buffer> {
        if let Message::Bytes(b) = &self.inner {
            Some(b.as_slice().into())
        } else {
            None
        }
    }

    #[napi(js_name = "asJson")]
    pub fn as_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(&self.inner)
            .map_err(|e| Error::from_reason(format!("serialize: {e}")))
    }

    /// For `StreamHandle` messages, take the receiver. Throws if the
    /// message is not a stream, or if the receiver has already been taken.
    #[napi(js_name = "takeStream")]
    pub fn take_stream(&self) -> Result<StreamReader> {
        match &self.inner {
            Message::StreamHandle(h) => {
                let rx = STREAM_REGISTRY.take_receiver(h.stream_id).ok_or_else(|| {
                    Error::from_reason(format!(
                        "no receiver for stream {} (already taken?)",
                        h.stream_id
                    ))
                })?;
                Ok(StreamReader { rx })
            }
            _ => Err(Error::from_reason("message is not a StreamHandle")),
        }
    }
}

// ─── Stream producer / consumer ────────────────────────────────────────────

/// Producer-side handle for a `Message::StreamHandle`. Create with
/// `ReflowStream.create`, push frames with `sendBytes` / `sendBegin`,
/// terminate with `end` or `error`, and convert to a `ReflowMessage`
/// with `intoMessage`.
#[napi]
pub struct ReflowStream {
    inner: PlMutex<Option<StreamInner>>,
}

struct StreamInner {
    id: StreamId,
    sender: flume::Sender<StreamFrame>,
    origin_actor: String,
    origin_port: String,
    content_type: Option<String>,
}

/// Options accepted by `ReflowStream.create`.
#[napi(object)]
pub struct StreamOptions {
    pub buffer_size: Option<u32>,
    pub origin_actor: Option<String>,
    pub origin_port: Option<String>,
    pub content_type: Option<String>,
}

/// Options accepted by `ReflowStream.sendBegin`.
#[napi(object)]
pub struct StreamBeginOptions {
    pub content_type: Option<String>,
    pub size_hint: Option<u32>,
    pub metadata: Option<serde_json::Value>,
}

#[napi]
impl ReflowStream {
    /// Allocate a new stream. `bufferSize == 0` creates an unbounded
    /// channel; any positive value enables backpressure with that
    /// bound. Default: 0 (unbounded).
    #[napi(factory)]
    pub fn create(opts: Option<StreamOptions>) -> Self {
        let opts = opts.unwrap_or(StreamOptions {
            buffer_size: None,
            origin_actor: None,
            origin_port: None,
            content_type: None,
        });
        let bs = opts.buffer_size.unwrap_or(0);
        let buf = if bs == 0 { None } else { Some(bs as usize) };
        let (id, sender) = STREAM_REGISTRY.create_stream(buf);
        Self {
            inner: PlMutex::new(Some(StreamInner {
                id,
                sender,
                origin_actor: opts.origin_actor.unwrap_or_default(),
                origin_port: opts.origin_port.unwrap_or_default(),
                content_type: opts.content_type,
            })),
        }
    }

    fn with_inner<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&StreamInner) -> Result<R>,
    {
        let guard = self.inner.lock();
        match guard.as_ref() {
            Some(i) => f(i),
            None => Err(Error::from_reason("stream has already been consumed")),
        }
    }

    #[napi(js_name = "sendBegin")]
    pub fn send_begin(&self, opts: Option<StreamBeginOptions>) -> Result<()> {
        let opts = opts.unwrap_or(StreamBeginOptions {
            content_type: None,
            size_hint: None,
            metadata: None,
        });
        self.with_inner(|i| {
            i.sender
                .send(StreamFrame::Begin {
                    content_type: opts.content_type.clone(),
                    size_hint: opts.size_hint.map(|v| v as u64),
                    metadata: opts.metadata.clone(),
                })
                .map_err(|e| Error::from_reason(format!("stream send: {e}")))
        })
    }

    #[napi(js_name = "sendBytes")]
    pub fn send_bytes(&self, data: Buffer) -> Result<()> {
        self.with_inner(|i| {
            i.sender
                .send(StreamFrame::Data(Arc::new(data.to_vec())))
                .map_err(|e| Error::from_reason(format!("stream send: {e}")))
        })
    }

    #[napi]
    pub fn end(&self) -> Result<()> {
        self.with_inner(|i| {
            i.sender
                .send(StreamFrame::End)
                .map_err(|e| Error::from_reason(format!("stream end: {e}")))
        })
    }

    #[napi]
    pub fn error(&self, message: String) -> Result<()> {
        self.with_inner(|i| {
            i.sender
                .send(StreamFrame::Error(message.clone()))
                .map_err(|e| Error::from_reason(format!("stream error: {e}")))
        })
    }

    /// Consume this producer and return a `Message::StreamHandle` the
    /// runtime can route on an output port. After this call, the
    /// stream cannot send more frames via `sendBytes` / `end` — the
    /// actor on the other side of the port owns lifetime.
    #[napi(js_name = "intoMessage")]
    pub fn into_message(&self) -> Result<ReflowMessage> {
        let mut guard = self.inner.lock();
        let inner = guard
            .take()
            .ok_or_else(|| Error::from_reason("stream has already been consumed"))?;
        let handle = RtStreamHandle {
            stream_id: inner.id,
            origin_actor: inner.origin_actor,
            origin_port: inner.origin_port,
            content_type: inner.content_type,
            size_hint: None,
        };
        Ok(ReflowMessage {
            inner: Message::StreamHandle(Arc::new(handle)),
        })
    }
}

/// Consumer-side reader for a stream. Obtained via
/// `ReflowMessage.takeStream()`.
#[napi]
pub struct StreamReader {
    rx: flume::Receiver<StreamFrame>,
}

/// Shape of each frame emitted by `StreamReader.recv`. `kind` is one of
/// `"begin" | "data" | "end" | "error" | "timeout" | "closed"`.
#[napi(object)]
pub struct StreamFrameValue {
    pub kind: String,
    pub data: Option<Buffer>,
    pub error: Option<String>,
    pub content_type: Option<String>,
    pub size_hint: Option<u32>,
}

#[napi]
impl StreamReader {
    /// Await the next frame, blocking up to `timeoutMs` milliseconds.
    /// Resolves to a frame whose `kind` distinguishes all possible
    /// outcomes. `kind === "closed"` means the producer has been dropped
    /// without terminating; iteration should stop.
    #[napi]
    pub async fn recv(&self, timeout_ms: u32) -> Result<StreamFrameValue> {
        let d = std::time::Duration::from_millis(timeout_ms as u64);
        let fut = self.rx.recv_async();
        let outcome = tokio::time::timeout(d, fut).await;
        let frame = match outcome {
            Err(_) => {
                return Ok(StreamFrameValue {
                    kind: "timeout".into(),
                    data: None,
                    error: None,
                    content_type: None,
                    size_hint: None,
                });
            }
            Ok(Err(_)) => {
                return Ok(StreamFrameValue {
                    kind: "closed".into(),
                    data: None,
                    error: None,
                    content_type: None,
                    size_hint: None,
                });
            }
            Ok(Ok(f)) => f,
        };
        Ok(match frame {
            StreamFrame::Begin {
                content_type,
                size_hint,
                ..
            } => StreamFrameValue {
                kind: "begin".into(),
                data: None,
                error: None,
                content_type,
                size_hint: size_hint.map(|v| v as u32),
            },
            StreamFrame::Data(buf) => StreamFrameValue {
                kind: "data".into(),
                data: Some(buf.as_slice().into()),
                error: None,
                content_type: None,
                size_hint: None,
            },
            StreamFrame::End => StreamFrameValue {
                kind: "end".into(),
                data: None,
                error: None,
                content_type: None,
                size_hint: None,
            },
            StreamFrame::Error(msg) => StreamFrameValue {
                kind: "error".into(),
                data: None,
                error: Some(msg),
                content_type: None,
                size_hint: None,
            },
        })
    }
}

// ─── Actor call context — what JS sees inside a callback ───────────────────

/// Per-tick context passed to JS actor callbacks. Read `inputs` / `config`;
/// resolve with `done({...})` (outputs keyed by port) or `fail("reason")`.
/// Exactly one resolution per call — calling both is an error.
#[napi]
pub struct ActorCallContext {
    inputs_value: serde_json::Value,
    config_value: serde_json::Value,
    reply: PlMutex<Option<flume::Sender<CallbackReply>>>,
}

enum CallbackReply {
    Ok(HashMap<String, Message>),
    Err(String),
}

#[napi]
impl ActorCallContext {
    #[napi(getter)]
    pub fn inputs(&self) -> serde_json::Value {
        self.inputs_value.clone()
    }

    #[napi(getter)]
    pub fn config(&self) -> serde_json::Value {
        self.config_value.clone()
    }

    /// Complete the tick by emitting zero or more output packets. `outputs`
    /// is an object keyed by output port name; each value may be either a
    /// `Message` handle or a JSON-shaped `Message`.
    #[napi]
    pub fn done(&self, outputs: Option<serde_json::Value>) -> Result<()> {
        let mut hmap: HashMap<String, Message> = HashMap::new();
        if let Some(serde_json::Value::Object(m)) = outputs {
            for (port, val) in m {
                let msg: Message = serde_json::from_value(val).map_err(|e| {
                    Error::from_reason(format!("output '{port}' not a Message: {e}"))
                })?;
                hmap.insert(port, msg);
            }
        }
        let tx = self
            .reply
            .lock()
            .take()
            .ok_or_else(|| Error::from_reason("actor context already resolved"))?;
        let _ = tx.send(CallbackReply::Ok(hmap));
        Ok(())
    }

    #[napi]
    pub fn fail(&self, message: String) -> Result<()> {
        let tx = self
            .reply
            .lock()
            .take()
            .ok_or_else(|| Error::from_reason("actor context already resolved"))?;
        let _ = tx.send(CallbackReply::Err(message));
        Ok(())
    }
}

// ─── Actor bridging — JS callback as an Actor ──────────────────────────────

type JsCallback = ThreadsafeFunction<ActorCallContext, ErrorStrategy::Fatal>;

struct JsActor {
    callback: Arc<JsCallback>,
    inports: Port,
    outports: Port,
    inport_names: Vec<String>,
    outport_names: Vec<String>,
    load: Arc<ActorLoad>,
    await_all_inports: bool,
}

impl RtActor for JsActor {
    fn get_behavior(&self) -> ActorBehavior {
        let callback = Arc::clone(&self.callback);
        Box::new(move |ctx: ActorContext| -> Pin<Box<dyn Future<Output = anyhow::Result<HashMap<String, Message>>> + Send + 'static>> {
            let callback = Arc::clone(&callback);
            Box::pin(async move {
                let payload = ctx.get_payload();
                let cfg = ctx.get_config().as_hashmap();

                let inputs_value: serde_json::Value = serde_json::to_value(payload)
                    .unwrap_or(serde_json::Value::Null);
                let config_value: serde_json::Value = serde_json::to_value(cfg)
                    .unwrap_or(serde_json::Value::Null);

                let (reply_tx, reply_rx) = flume::bounded::<CallbackReply>(1);
                let call_ctx = ActorCallContext {
                    inputs_value,
                    config_value,
                    reply: PlMutex::new(Some(reply_tx)),
                };
                callback.call(call_ctx, ThreadsafeFunctionCallMode::NonBlocking);

                match reply_rx.recv_async().await {
                    Ok(CallbackReply::Ok(out)) => Ok(out),
                    Ok(CallbackReply::Err(msg)) => Err(anyhow::anyhow!(msg)),
                    Err(e) => Err(anyhow::anyhow!("actor callback channel: {e}")),
                }
            })
        })
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn inport_names(&self) -> Vec<String> {
        self.inport_names.clone()
    }

    fn outport_names(&self) -> Vec<String> {
        self.outport_names.clone()
    }

    fn await_all_inports(&self) -> bool {
        self.await_all_inports
    }

    fn create_state(&self) -> Arc<PlMutex<dyn ActorState>> {
        Arc::new(PlMutex::new(MemoryState::default()))
    }

    fn load_count(&self) -> Arc<ActorLoad> {
        Arc::clone(&self.load)
    }

    fn create_instance(&self) -> Arc<dyn RtActor> {
        Arc::new(Self {
            callback: Arc::clone(&self.callback),
            inports: flume::bounded(50),
            outports: flume::bounded(50),
            inport_names: self.inport_names.clone(),
            outport_names: self.outport_names.clone(),
            load: Arc::new(ActorLoad::new(0)),
            await_all_inports: self.await_all_inports,
        })
    }
}

/// Handle to an actor registered on the Rust side. Used with
/// `Network.registerActor` / subgraph builders. Also wraps bundled
/// template actors returned by `templateActor`.
#[napi]
pub struct ReflowActor {
    inner: Arc<dyn RtActor>,
}

/// Options describing a JS-authored actor.
#[napi(object)]
pub struct ActorOptions {
    pub component: String,
    pub inports: Vec<String>,
    pub outports: Vec<String>,
    pub await_all_inports: Option<bool>,
}

#[napi]
impl ReflowActor {
    /// Wrap a JS callback as a Reflow actor. The callback receives an
    /// `ActorCallContext` object and must resolve it with `done(outputs)`
    /// or reject with `fail(message)`.
    #[napi(factory, js_name = "fromCallback")]
    pub fn from_callback(
        options: ActorOptions,
        callback: napi::JsFunction,
    ) -> Result<Self> {
        let tsfn: ThreadsafeFunction<ActorCallContext, ErrorStrategy::Fatal> =
            callback.create_threadsafe_function(0, |ctx| Ok(vec![ctx.value]))?;

        let actor = Arc::new(JsActor {
            callback: Arc::new(tsfn),
            inports: flume::bounded(50),
            outports: flume::bounded(50),
            inport_names: options.inports,
            outport_names: options.outports,
            load: Arc::new(ActorLoad::new(0)),
            await_all_inports: options.await_all_inports.unwrap_or(false),
        }) as Arc<dyn RtActor>;

        Ok(ReflowActor { inner: actor })
    }
}

// ─── Bundled template catalog ──────────────────────────────────────────────

/// Instantiate an actor from the bundled `reflow_components` catalog.
/// Throws if the template id is unknown.
#[napi(js_name = "templateActor")]
pub fn template_actor(template_id: String) -> Result<ReflowActor> {
    match reflow_components::get_actor_for_template(&template_id) {
        Some(a) => Ok(ReflowActor { inner: a }),
        None => Err(Error::from_reason(format!(
            "unknown template id: '{template_id}'"
        ))),
    }
}

/// Enumerate every template id registered in the bundled catalog and
/// any loaded `.rflpack` packs.
#[napi(js_name = "templateList")]
pub fn template_list() -> Vec<String> {
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

/// Load a Reflow actor pack from either a `.rflpack` bundle or a raw
/// cdylib path. Returns the list of template ids the pack published.
/// Safe to call repeatedly with the same pack name — the second call is
/// a no-op.
#[napi(js_name = "loadPack")]
pub fn load_pack(path: String) -> Result<Vec<String>> {
    reflow_rt::pack_loader::load_pack(&path)
        .map_err(|e| Error::from_reason(format!("load pack '{path}': {e:#}")))
}

/// Read the manifest from a `.rflpack` without loading its code. Useful
/// for showing a pack's contents in UI before the user accepts it.
#[napi(js_name = "inspectPack")]
pub fn inspect_pack(path: String) -> Result<serde_json::Value> {
    let manifest = reflow_rt::pack_loader::inspect_pack(&path)
        .map_err(|e| Error::from_reason(format!("inspect pack '{path}': {e:#}")))?;
    serde_json::to_value(&manifest)
        .map_err(|e| Error::from_reason(format!("serialize manifest: {e}")))
}

/// List every pack currently loaded into this process, with their
/// manifest name / version / templates.
#[napi(js_name = "listPacks")]
pub fn list_packs() -> Result<serde_json::Value> {
    let list = reflow_rt::pack_loader::PACK_REGISTRY.loaded_packs();
    serde_json::to_value(&list).map_err(|e| Error::from_reason(format!("serialize list: {e}")))
}

/// The pack ABI version this SDK was compiled against. Pack authors
/// must build their `.rflpack` with a matching value.
#[napi(js_name = "packAbiVersion")]
pub fn pack_abi_version() -> u32 {
    reflow_rt::pack_loader::REFLOW_PACK_ABI_VERSION
}

// ─── Multi-graph composition ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ComposeRequest {
    #[serde(default)]
    graphs: Vec<reflow_rt::graph::types::GraphExport>,
    #[serde(default)]
    connections: Vec<ComposeConn>,
    #[serde(default)]
    shared_resources: Vec<ComposeShared>,
    #[serde(default)]
    properties: HashMap<String, serde_json::Value>,
    #[serde(default)]
    case_sensitive: Option<bool>,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ComposeConn {
    from: ComposeEndpoint,
    to: ComposeEndpoint,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize)]
struct ComposeEndpoint {
    process: String,
    port: String,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ComposeShared {
    name: String,
    component: String,
    #[serde(default)]
    metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Compose N `GraphExport` objects into a single `GraphExport`, merging
/// namespaces and wiring cross-graph connections. See the C ABI docs
/// for the request shape.
#[napi(js_name = "composeGraphs")]
pub fn compose_graphs(composition: serde_json::Value) -> Result<serde_json::Value> {
    let req: ComposeRequest = serde_json::from_value(composition)
        .map_err(|e| Error::from_reason(format!("composition parse: {e}")))?;
    let composition = GraphComposition {
        sources: req.graphs.into_iter().map(GraphSource::GraphExport).collect(),
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
    .map_err(|e| Error::from_reason(format!("compose_graphs: {e}")))?;

    let export = composed.export();
    serde_json::to_value(&export)
        .map_err(|e| Error::from_reason(format!("serialize composed graph: {e}")))
}

// ─── Graph ─────────────────────────────────────────────────────────────────

#[napi]
pub struct ReflowGraph {
    inner: PlMutex<RtGraph>,
}

#[napi]
impl ReflowGraph {
    #[napi(constructor)]
    pub fn new(name: Option<String>, case_sensitive: Option<bool>) -> Self {
        Self {
            inner: PlMutex::new(RtGraph::new(
                name.as_deref().unwrap_or(""),
                case_sensitive.unwrap_or(false),
                None,
            )),
        }
    }

    #[napi(factory, js_name = "fromJson")]
    pub fn from_json(value: serde_json::Value) -> Result<Self> {
        let export: GraphExport = serde_json::from_value(value)
            .map_err(|e| Error::from_reason(format!("GraphExport parse: {e}")))?;
        Ok(Self {
            inner: PlMutex::new(RtGraph::load(export, None)),
        })
    }

    #[napi(js_name = "toJson")]
    pub fn to_json(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.inner.lock().export())
            .map_err(|e| Error::from_reason(format!("serialize: {e}")))
    }

    #[napi(js_name = "addNode")]
    pub fn add_node(
        &self,
        id: String,
        component: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(metadata)?;
        self.inner.lock().add_node(&id, &component, md);
        Ok(())
    }

    #[napi(js_name = "removeNode")]
    pub fn remove_node(&self, id: String) {
        self.inner.lock().remove_node(&id);
    }

    #[napi(js_name = "addConnection")]
    pub fn add_connection(
        &self,
        out_node: String,
        out_port: String,
        in_node: String,
        in_port: String,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(metadata)?;
        self.inner
            .lock()
            .add_connection(&out_node, &out_port, &in_node, &in_port, md);
        Ok(())
    }

    #[napi(js_name = "addInitial")]
    pub fn add_initial(
        &self,
        node: String,
        port: String,
        data: serde_json::Value,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(metadata)?;
        self.inner.lock().add_initial(data, &node, &port, md);
        Ok(())
    }

    // ── mutators (renames) ────────────────────────────────────────────────

    #[napi(js_name = "renameNode")]
    pub fn rename_node(&self, old_id: String, new_id: String) {
        self.inner.lock().rename_node(&old_id, &new_id);
    }

    #[napi(js_name = "renameInport")]
    pub fn rename_inport(&self, old_port: String, new_port: String) {
        self.inner.lock().rename_inport(&old_port, &new_port);
    }

    #[napi(js_name = "renameOutport")]
    pub fn rename_outport(&self, old_port: String, new_port: String) {
        self.inner.lock().rename_outport(&old_port, &new_port);
    }

    // ── mutators (port lifecycle) ─────────────────────────────────────────

    #[napi(js_name = "addInport")]
    pub fn add_inport(
        &self,
        port_id: String,
        node_id: String,
        port_key: String,
        port_type: Option<serde_json::Value>,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let pt = parse_port_type(port_type)?;
        let md = parse_metadata(metadata)?;
        self.inner
            .lock()
            .add_inport(&port_id, &node_id, &port_key, pt, md);
        Ok(())
    }

    #[napi(js_name = "addOutport")]
    pub fn add_outport(
        &self,
        port_id: String,
        node_id: String,
        port_key: String,
        port_type: Option<serde_json::Value>,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let pt = parse_port_type(port_type)?;
        let md = parse_metadata(metadata)?;
        self.inner
            .lock()
            .add_outport(&port_id, &node_id, &port_key, pt, md);
        Ok(())
    }

    #[napi(js_name = "removeInport")]
    pub fn remove_inport(&self, port_id: String) {
        self.inner.lock().remove_inport(&port_id);
    }

    #[napi(js_name = "removeOutport")]
    pub fn remove_outport(&self, port_id: String) {
        self.inner.lock().remove_outport(&port_id);
    }

    // ── mutators (groups) ─────────────────────────────────────────────────

    #[napi(js_name = "addGroup")]
    pub fn add_group(
        &self,
        group_id: String,
        nodes: Vec<String>,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(metadata)?;
        self.inner.lock().add_group(&group_id, nodes, md);
        Ok(())
    }

    #[napi(js_name = "removeGroup")]
    pub fn remove_group(&self, group_id: String) {
        self.inner.lock().remove_group(&group_id);
    }

    #[napi(js_name = "addToGroup")]
    pub fn add_to_group(&self, group_id: String, node_id: String) {
        self.inner.lock().add_to_group(&group_id, &node_id);
    }

    #[napi(js_name = "removeFromGroup")]
    pub fn remove_from_group(&self, group_id: String, node_id: String) {
        self.inner.lock().remove_from_group(&group_id, &node_id);
    }

    // ── mutators (connection / initial removal) ───────────────────────────

    #[napi(js_name = "removeConnection")]
    pub fn remove_connection(
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

    #[napi(js_name = "removeInitial")]
    pub fn remove_initial(&self, node: String, port: String) {
        self.inner.lock().remove_initial(&node, &port);
    }

    #[napi(js_name = "addInitialIndex")]
    pub fn add_initial_index(
        &self,
        node: String,
        port: String,
        data: serde_json::Value,
        index: u32,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(metadata)?;
        self.inner
            .lock()
            .add_initial_index(data, &node, &port, index as usize, md);
        Ok(())
    }

    #[napi(js_name = "addGraphInitial")]
    pub fn add_graph_initial(
        &self,
        inport: String,
        data: serde_json::Value,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(metadata)?;
        self.inner.lock().add_graph_initial(data, &inport, md);
        Ok(())
    }

    #[napi(js_name = "addGraphInitialIndex")]
    pub fn add_graph_initial_index(
        &self,
        inport: String,
        data: serde_json::Value,
        index: u32,
        metadata: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(metadata)?;
        self.inner
            .lock()
            .add_graph_initial_index(data, &inport, index as usize, md);
        Ok(())
    }

    #[napi(js_name = "removeGraphInitial")]
    pub fn remove_graph_initial(&self, inport: String) {
        self.inner.lock().remove_graph_initial(&inport);
    }

    // ── mutators (metadata setters) ───────────────────────────────────────

    #[napi(js_name = "setNodeMetadata")]
    pub fn set_node_metadata(&self, id: String, metadata: serde_json::Value) -> Result<()> {
        let md = parse_metadata_required(metadata)?;
        self.inner.lock().set_node_metadata(&id, md);
        Ok(())
    }

    #[napi(js_name = "setConnectionMetadata")]
    pub fn set_connection_metadata(
        &self,
        out_node: String,
        out_port: String,
        in_node: String,
        in_port: String,
        metadata: serde_json::Value,
    ) -> Result<()> {
        let md = parse_metadata_required(metadata)?;
        self.inner.lock().set_connection_metadata(
            &out_node, &out_port, &in_node, &in_port, md,
        );
        Ok(())
    }

    #[napi(js_name = "setInportMetadata")]
    pub fn set_inport_metadata(
        &self,
        port_id: String,
        metadata: serde_json::Value,
    ) -> Result<()> {
        let md = parse_metadata_required(metadata)?;
        self.inner.lock().set_inport_metadata(&port_id, md);
        Ok(())
    }

    #[napi(js_name = "setOutportMetadata")]
    pub fn set_outport_metadata(
        &self,
        port_id: String,
        metadata: serde_json::Value,
    ) -> Result<()> {
        let md = parse_metadata_required(metadata)?;
        self.inner.lock().set_outport_metadata(&port_id, md);
        Ok(())
    }

    #[napi(js_name = "setGroupMetadata")]
    pub fn set_group_metadata(
        &self,
        group_id: String,
        metadata: serde_json::Value,
    ) -> Result<()> {
        let md = parse_metadata_required(metadata)?;
        self.inner.lock().set_group_metadata(&group_id, md);
        Ok(())
    }

    #[napi(js_name = "setProperties")]
    pub fn set_properties(&self, properties: serde_json::Value) -> Result<()> {
        let md = parse_metadata_required(properties)?;
        self.inner.lock().set_properties(md);
        Ok(())
    }

    /// Replace this graph's state with another GraphExport. Existing
    /// nodes, connections, properties, etc. are cleared first.
    #[napi(js_name = "import")]
    pub fn import_graph(&self, export: serde_json::Value) -> Result<()> {
        let exp: GraphExport = serde_json::from_value(export)
            .map_err(|e| Error::from_reason(format!("GraphExport parse: {e}")))?;
        self.inner.lock().import(exp);
        Ok(())
    }

    // ── queries ───────────────────────────────────────────────────────────

    #[napi(js_name = "getNode")]
    pub fn get_node(&self, id: String) -> Result<Option<serde_json::Value>> {
        let g = self.inner.lock();
        match g.get_node(&id) {
            Some(n) => serde_json::to_value(n)
                .map(Some)
                .map_err(|e| Error::from_reason(format!("serialize node: {e}"))),
            None => Ok(None),
        }
    }

    #[napi(js_name = "nodes")]
    pub fn nodes(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.inner.lock().get_nodes())
            .map_err(|e| Error::from_reason(format!("serialize nodes: {e}")))
    }

    #[napi(js_name = "getConnection")]
    pub fn get_connection(
        &self,
        out_node: String,
        out_port: String,
        in_node: String,
        in_port: String,
    ) -> Result<Option<serde_json::Value>> {
        let g = self.inner.lock();
        match g.get_connection(&out_node, &out_port, &in_node, &in_port) {
            Some(c) => serde_json::to_value(&c)
                .map(Some)
                .map_err(|e| Error::from_reason(format!("serialize connection: {e}"))),
            None => Ok(None),
        }
    }

    #[napi(js_name = "connections")]
    pub fn connections(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.inner.lock().get_connections())
            .map_err(|e| Error::from_reason(format!("serialize connections: {e}")))
    }

    #[napi(js_name = "groups")]
    pub fn groups(&self) -> Result<serde_json::Value> {
        serde_json::to_value(&self.inner.lock().groups)
            .map_err(|e| Error::from_reason(format!("serialize groups: {e}")))
    }

    #[napi(js_name = "inports")]
    pub fn inports(&self) -> Result<serde_json::Value> {
        let exp = self.inner.lock().export();
        serde_json::to_value(&exp.inports)
            .map_err(|e| Error::from_reason(format!("serialize inports: {e}")))
    }

    #[napi(js_name = "outports")]
    pub fn outports(&self) -> Result<serde_json::Value> {
        let exp = self.inner.lock().export();
        serde_json::to_value(&exp.outports)
            .map_err(|e| Error::from_reason(format!("serialize outports: {e}")))
    }

    #[napi(js_name = "initializers")]
    pub fn initializers(&self) -> Result<serde_json::Value> {
        serde_json::to_value(&self.inner.lock().initializers)
            .map_err(|e| Error::from_reason(format!("serialize initializers: {e}")))
    }

    #[napi(js_name = "properties")]
    pub fn properties(&self) -> Result<serde_json::Value> {
        serde_json::to_value(self.inner.lock().get_properties())
            .map_err(|e| Error::from_reason(format!("serialize properties: {e}")))
    }
}

fn parse_metadata(
    md: Option<serde_json::Value>,
) -> Result<Option<HashMap<String, serde_json::Value>>> {
    match md {
        None => Ok(None),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Object(m)) => Ok(Some(m.into_iter().collect())),
        Some(_) => Err(Error::from_reason("metadata must be an object or null")),
    }
}

/// Like `parse_metadata` but requires a non-null object — `set_*_metadata`
/// and `set_properties` on the underlying Graph take an owned HashMap.
fn parse_metadata_required(
    md: serde_json::Value,
) -> Result<HashMap<String, serde_json::Value>> {
    match md {
        serde_json::Value::Null => Ok(HashMap::new()),
        serde_json::Value::Object(m) => Ok(m.into_iter().collect()),
        _ => Err(Error::from_reason("metadata must be an object")),
    }
}

/// `null` / undefined → `PortType::Any`. Anything else must be a JSON
/// payload that deserializes to `PortType` (e.g. `"All"`, `"Flow"`,
/// `{"Event":"click"}`).
fn parse_port_type(
    pt: Option<serde_json::Value>,
) -> Result<reflow_rt::graph::types::PortType> {
    use reflow_rt::graph::types::PortType;
    match pt {
        None | Some(serde_json::Value::Null) => Ok(PortType::Any),
        Some(v) => serde_json::from_value::<PortType>(v)
            .map_err(|e| Error::from_reason(format!("port_type parse: {e}"))),
    }
}

// ─── Subgraph builder ──────────────────────────────────────────────────────

#[napi]
pub struct SubgraphBuilder {
    export: GraphExport,
    actors: HashMap<String, Arc<dyn RtActor>>,
}

#[napi]
impl SubgraphBuilder {
    #[napi(constructor)]
    pub fn new(export: serde_json::Value) -> Result<Self> {
        let export: GraphExport = serde_json::from_value(export)
            .map_err(|e| Error::from_reason(format!("GraphExport parse: {e}")))?;
        Ok(Self {
            export,
            actors: HashMap::new(),
        })
    }

    #[napi(js_name = "registerActor")]
    pub fn register_actor(&mut self, component: String, actor: &ReflowActor) {
        self.actors.insert(component, Arc::clone(&actor.inner));
    }

    /// Pull any still-missing components from the bundled catalog.
    #[napi(js_name = "fillFromCatalog")]
    pub fn fill_from_catalog(&mut self) -> Result<()> {
        let needed: Vec<String> = self
            .export
            .processes
            .values()
            .map(|n| n.component.clone())
            .filter(|c| !self.actors.contains_key(c))
            .collect();
        for c in needed {
            match reflow_components::get_actor_for_template(&c) {
                Some(a) => {
                    self.actors.insert(c, a);
                }
                None => {
                    return Err(Error::from_reason(format!(
                        "subgraph references unknown component '{c}'"
                    )));
                }
            }
        }
        Ok(())
    }

    #[napi]
    pub fn build(&self) -> Result<ReflowActor> {
        let actors = self.actors.clone();
        for node in self.export.processes.values() {
            if !actors.contains_key(&node.component) {
                return Err(Error::from_reason(format!(
                    "subgraph references unregistered component '{}'",
                    node.component
                )));
            }
        }
        let sg = SubgraphActor::from_graph_export(&self.export, actors)
            .map_err(|e| Error::from_reason(format!("subgraph build: {e}")))?;
        Ok(ReflowActor {
            inner: Arc::new(sg) as Arc<dyn RtActor>,
        })
    }
}

// ─── Network ───────────────────────────────────────────────────────────────

#[napi]
pub struct ReflowNetwork {
    inner: Arc<Mutex<RtNetwork>>,
}

#[napi]
impl ReflowNetwork {
    #[napi(constructor)]
    pub fn new(config: Option<serde_json::Value>) -> Result<Self> {
        let cfg: NetworkConfig = match config {
            None => NetworkConfig::default(),
            Some(v) => serde_json::from_value(v)
                .map_err(|e| Error::from_reason(format!("NetworkConfig parse: {e}")))?,
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(RtNetwork::new(cfg))),
        })
    }

    #[napi(factory, js_name = "fromGraph")]
    pub fn from_graph(graph: &ReflowGraph) -> Self {
        let g = graph.inner.lock();
        let net_arc = RtNetwork::with_graph(NetworkConfig::default(), &g);
        Self { inner: net_arc }
    }

    #[napi(js_name = "registerActor")]
    pub fn register_actor(&self, template_id: String, actor: &ReflowActor) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .register_actor_arc(&template_id, Arc::clone(&actor.inner))
            .map_err(|e| Error::from_reason(format!("{e}")))
    }

    #[napi(js_name = "addNode")]
    pub fn add_node(
        &self,
        id: String,
        template_id: String,
        config: Option<serde_json::Value>,
    ) -> Result<()> {
        let md = parse_metadata(config)?;
        self.inner
            .lock()
            .unwrap()
            .add_node(&id, &template_id, md)
            .map_err(|e| Error::from_reason(format!("{e}")))
    }

    #[napi(js_name = "addConnection")]
    pub fn add_connection(
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

    #[napi(js_name = "addInitial")]
    pub fn add_initial(
        &self,
        actor: String,
        port: String,
        message: serde_json::Value,
    ) -> Result<()> {
        let msg: Message = serde_json::from_value(message)
            .map_err(|e| Error::from_reason(format!("Message parse: {e}")))?;
        self.inner.lock().unwrap().add_initial(InitialPacket {
            to: ConnectionPoint::new(&actor, &port, Some(msg)),
        });
        Ok(())
    }

    #[napi]
    pub fn start(&self) -> Result<()> {
        enter_runtime(|| {
            self.inner
                .lock()
                .unwrap()
                .start()
                .map_err(|e| Error::from_reason(format!("{e}")))
        })
    }

    #[napi]
    pub fn shutdown(&self) {
        enter_runtime(|| {
            self.inner.lock().unwrap().shutdown();
        });
    }

    /// Subscribe to network events. Returns an `EventStream` whose `recv`
    /// awaits the next event.
    #[napi]
    pub fn events(&self) -> EventStream {
        let rx = self.inner.lock().unwrap().get_event_receiver();
        EventStream { rx }
    }
}

// ─── Event stream ──────────────────────────────────────────────────────────

#[napi]
pub struct EventStream {
    rx: flume::Receiver<NetworkEvent>,
}

#[napi]
impl EventStream {
    /// Await the next event. Resolves `null` if the stream is closed.
    #[napi]
    pub async fn recv(&self) -> Result<Option<serde_json::Value>> {
        match self.rx.recv_async().await {
            Ok(evt) => serde_json::to_value(&evt)
                .map(Some)
                .map_err(|e| Error::from_reason(format!("serialize event: {e}"))),
            Err(_) => Ok(None),
        }
    }
}
