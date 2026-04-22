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

/// Enumerate every template id registered in the bundled catalog.
#[napi(js_name = "templateList")]
pub fn template_list() -> Vec<String> {
    reflow_components::get_template_mapping().into_keys().collect()
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
