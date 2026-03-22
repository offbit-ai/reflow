#[cfg(target_arch = "wasm32")]
use futures::Future;
use futures::StreamExt;
#[cfg(target_arch = "wasm32")]
use futures::TryFutureExt;
#[cfg(target_arch = "wasm32")]
use futures::future::lazy;
#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
use reflow_actor::ActorContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

// use rusty_pool::ThreadPool;
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

#[cfg(target_arch = "wasm32")]
use crate::actor::ExternActor;

use crate::actor::{Actor, ActorConfig, MemoryState};

#[cfg(target_arch = "wasm32")]
use crate::actor::ActorChannel;
use crate::graph::Graph;
use crate::graph::types::{ComponentSpec, GraphConnection, GraphEvents, GraphIIP, GraphNode};
use crate::message::{CompressionConfig, EncodableValue, EncodedMessage, Message};
use crate::tracing::{TracingClient, TracingConfig, TracingIntegration};

use crate::connector::{ConnectionPoint, Connector, InitialPacket};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct NetworkConfig {
    pub compression: CompressionConfig,
    pub tracing: TracingConfig,
}

#[allow(clippy::derivable_impls)]
impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            compression: CompressionConfig::default(),
            tracing: TracingConfig::default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(tag = "_type")]
pub enum NetworkEvent {
    #[serde(rename_all = "camelCase")]
    ActorEmit {
        actor_id: String,
        message: EncodableValue,
    },

    // Actor lifecycle events
    #[serde(rename_all = "camelCase")]
    ActorStarted {
        actor_id: String,
        component: String,
        timestamp: u64,
    },

    #[serde(rename_all = "camelCase")]
    ActorCompleted {
        actor_id: String,
        component: String,
        outputs: Option<Value>,
        timestamp: u64,
    },

    #[serde(rename_all = "camelCase")]
    ActorFailed {
        actor_id: String,
        component: String,
        error: String,
        timestamp: u64,
    },

    // Message flow events
    #[serde(rename_all = "camelCase")]
    MessageSent {
        from_actor: String,
        from_port: String,
        to_actor: String,
        to_port: String,
        message: EncodableValue,
        timestamp: u64,
    },

    #[serde(rename_all = "camelCase")]
    MessageReceived {
        actor_id: String,
        port: String,
        message: EncodableValue,
        timestamp: u64,
    },

    // Network lifecycle events
    #[serde(rename_all = "camelCase")]
    NetworkStarted { timestamp: u64 },

    #[serde(rename_all = "camelCase")]
    NetworkIdle { timestamp: u64 },

    #[serde(rename_all = "camelCase")]
    NetworkShutdown { timestamp: u64 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(tag = "_type")]
#[serde(rename_all = "camelCase")]
pub struct FlowStub {
    pub(crate) actor_id: String,
    pub(crate) port: String,
    pub(crate) data: Option<Value>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
#[derive(Clone)]
pub struct Network {
    #[allow(dead_code)]
    config: NetworkConfig,
    pub(crate) actors: HashMap<String, Arc<dyn Actor>>,
    pub(crate) initialized_actors: HashMap<String, Arc<dyn Actor>>,
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) processes: HashMap<String, Arc<JoinHandle<()>>>,
    pub(crate) nodes: HashMap<String, GraphNode>,
    connectors: Vec<Connector>,
    initials: Vec<InitialPacket>,
    pub(crate) network_event_emitter: (flume::Sender<NetworkEvent>, flume::Receiver<NetworkEvent>),
    #[cfg(target_arch = "wasm32")]
    event_handle: Vec<JsValue>,
    compression_config: CompressionConfig,
    pub(crate) tracing_integration: Option<TracingIntegration>,
}

unsafe impl Send for Network {}
unsafe impl Sync for Network {}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_class = Network)]
impl Network {
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(constructor)]
    pub fn _new() -> Self {
        let config = NetworkConfig::default();
        return Self::new(config);
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = registerActor))]
    pub fn _register_actor(&mut self, name: &str, actor: ExternActor) -> Result<(), JsValue> {
        use crate::actor::JsBrowserActor;

        self.register_actor(name, JsBrowserActor::new(actor))
            .map_err(|err| JsValue::from_str(format!("{}", err.to_string()).as_str()))?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = createActor)]
    pub fn create_actor(&mut self, name: &str, actor: ExternActor) -> Result<JsValue, JsValue> {
        use crate::actor::{BrowserActor, JsBrowserActor};

        // Register the actor in the network
        self._register_actor(name, actor.clone())?;

        // Create a JsBrowserActor wrapper for the actor
        let js_actor = JsBrowserActor::new(actor);

        Ok(JsValue::from(js_actor))
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = getActorNames)]
    pub fn get_actor_names(&self) -> Vec<String> {
        self.actors.keys().cloned().collect()
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = getActiveActors)]
    pub fn _get_active_actors(&self) -> Vec<String> {
        self.get_active_actors()
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = getActorCount)]
    pub fn _get_actor_count(&self) -> usize {
        self.actor_count()
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = getMessageQueueSize)]
    pub fn _get_message_queue_size(&self) -> usize {
        self.get_message_queue_size()
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addNode))]
    pub fn _add_node(&mut self, id: &str, process: &str) {
        if !self.actors.contains_key(process) {
            web_sys::console::error_1(
                &format!("Could not find process {} in Network", process).into(),
            );
            return;
        }
        let node = GraphNode {
            id: id.to_string(),
            component: process.to_string(),
            metadata: None,
        };
        self.nodes.insert(id.to_string(), node);
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addConnection))]
    pub fn _add_connection(&mut self, connector: Connector) {
        self.add_connection(connector);
    }

    #[cfg(target_arch = "wasm32")]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addInitial))]
    pub fn _add_initial(&mut self, connector: InitialPacket) {
        self.add_initial(connector);
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name= start)]
    pub async fn _start(&mut self) -> Result<(), JsValue> {
        self.start()
            .map_err(|err| JsValue::from_str(err.to_string().as_str()))?;

        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen]
    pub fn emit(&mut self, actor_id: String, packet: JsValue) {
        if let Some(node) = self.nodes.get(&actor_id) {
            let actor = self.actors.get(&node.component).unwrap();
            let outports = actor.get_outports();
            let messages = packet
                .into_serde::<HashMap<String, serde_json::Value>>()
                .expect("Expected packet to be mapped")
                .iter()
                .map(|(port, msg)| (port.to_owned(), Message::from(msg.clone())))
                .collect::<HashMap<String, Message>>();
            outports.0.send(messages);
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = sendToActor)]
    pub fn _send_to_actor(&self, actor_id: &str, port: &str, data: JsValue) -> Result<(), JsValue> {
        if let Ok(value) = data.into_serde::<Value>() {
            self.send_to_actor(actor_id, port, Message::from(value))
                .map_err(|e| JsValue::from_str(&format!("Failed to send message: {}", e)))?;
            Ok(())
        } else {
            Err(JsValue::from_str("Failed to parse message data"))
        }
    }

    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = executeActor)]
    pub async fn _execute_actor(&self, actor_id: &str, data: JsValue) -> Result<JsValue, JsValue> {
        if let Ok(value) = data.into_serde::<Value>() {
            let mut payload = HashMap::new();
            payload.insert("input".to_string(), Message::from(value));
            let result = self
                .execute_actor(actor_id, payload)
                .await
                .map_err(|e| JsValue::from_str(&format!("Failed to execute actor: {}", e)))?;

            Ok(
                JsValue::from_serde(&serde_json::to_value(result).unwrap())
                    .unwrap_or(JsValue::NULL),
            )
        } else {
            Err(JsValue::from_str("Failed to parse message data"))
        }
    }

    /// Subscribe to the network's events
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen]
    pub fn next(&mut self, callback: web_sys::js_sys::Function) {
        let receiver = self.network_event_emitter.1.clone();

        let cl: Closure<dyn Fn()> = Closure::new(move || {
            if let Ok(event) = receiver.clone().try_recv() {
                let _ = callback.call1(
                    &JsValue::null(),
                    &JsValue::from_serde(&serde_json::json!(event.clone())).unwrap_or_default(),
                );
            }
        });
        if let Ok(set_interval) =
            web_sys::js_sys::Reflect::get(&web_sys::js_sys::global().into(), &"setInterval".into())
        {
            let _set_interval: web_sys::js_sys::Function = set_interval.into();
            if let Ok(handle) = _set_interval.call2(
                &JsValue::null(),
                cl.as_ref().clone().as_ref().unchecked_ref(),
                &0.into(),
            ) {
                self.event_handle.push(handle);
            }
        }
        cl.forget();
    }

    /// Shutdown the network and turn off events
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = shutdown)]
    pub fn _shutdown(&mut self) {
        // clear all pending events
        if let Ok(clear_interval) = web_sys::js_sys::Reflect::get(
            &web_sys::js_sys::global().into(),
            &"clearInterval".into(),
        ) {
            let _clear_interval: web_sys::js_sys::Function = clear_interval.into();
            for handle in &self.event_handle {
                let _ = _clear_interval.call1(&JsValue::null(), handle);
            }
        }

        // Shutdown the network
        self.shutdown();
    }

    /// Get an actor node from the network
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = getNode)]
    pub fn get_node(&self, actor_id: &str) -> JsValue {
        if let Some(actor) = self.nodes.get(actor_id) {
            use wasm_bindgen_test::__rt::wasm_bindgen::JsValue;

            return JsValue::from_serde(&json!(actor)).unwrap_or(JsValue::null());
        }
        JsValue::null()
    }
}

impl Network {
    pub fn new(config: NetworkConfig) -> Self {
        // Initialize tracing if enabled
        let tracing_integration = if config.tracing.enabled {
            let client = TracingClient::new(config.tracing.clone());
            Some(TracingIntegration::new(client))
        } else {
            None
        };

        Self {
            config: config.clone(),
            nodes: HashMap::new(),
            actors: HashMap::new(),
            initialized_actors: HashMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            processes: HashMap::new(),
            connectors: Vec::new(),
            initials: Vec::new(),
            network_event_emitter: flume::unbounded(),
            #[cfg(target_arch = "wasm32")]
            event_handle: Vec::new(),
            compression_config: config.compression,
            tracing_integration,
        }
    }

    pub fn start(&mut self) -> Result<(), anyhow::Error> {
        // Emit NetworkStarted event
        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let _ = self
            .network_event_emitter
            .0
            .send(NetworkEvent::NetworkStarted { timestamp });

        #[cfg(not(target_arch = "wasm32"))]
        {
            let tracing_integration = self.tracing_integration.clone();
            tokio::runtime::Handle::current().spawn(async move {
                // Initialize tracing connection if enabled
                if let Some(ref tracing) = tracing_integration {
                    if let Err(e) = tracing.client().connect().await {
                        tracing::warn!("Failed to connect to tracing server: {}", e);
                    } else {
                        // Start a flow trace for this network session
                        let flow_id = format!("network_session_{}", chrono::Utc::now().timestamp());
                        if let Ok(trace_id) = tracing.start_flow_trace(&flow_id).await {
                            tracing::info!("Started network trace: {:?}", trace_id);
                        }
                    }
                }
            });
        }

        // Warm up all processes
        {
            for (id, node) in self.nodes.clone() {
                let actor = self.actors.get(&node.component).unwrap_or_else(|| {
                    panic!("Expected to find actor {} for node {}", node.component, id)
                });
                let mut config = ActorConfig::from_node(node.clone())?;

                // Count upstream connections per inport for this actor
                for conn in &self.connectors {
                    if conn.to.actor == id {
                        *config
                            .inport_connection_counts
                            .entry(conn.to.port.clone())
                            .or_insert(0) += 1;
                    }
                }

                // Create a fresh actor instance per node so that nodes sharing
                // the same template each get independent inport/outport channels.
                let instance = actor.create_instance();

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let tracing_integration = self.tracing_integration.clone();
                    self.processes.insert(
                        id.clone(),
                        Arc::new(Self::init_process(
                            instance.create_process(config, tracing_integration),
                        )),
                    );
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let tracing_integration = self.tracing_integration.clone();
                    Self::init_process(instance.create_process(config, tracing_integration));
                }

                self.initialized_actors.insert(id.clone(), instance);

                // Emit ActorStarted event
                let timestamp = chrono::Utc::now().timestamp_millis() as u64;
                let _ = self
                    .network_event_emitter
                    .0
                    .send(NetworkEvent::ActorStarted {
                        actor_id: id.clone(),
                        component: node.component.clone(),
                        timestamp,
                    });

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let tracing_integration = self.tracing_integration.clone();
                    let id = id.clone();
                    // Trace actor creation
                    tokio::runtime::Handle::current().spawn(async move {
                        if let Some(ref tracing) = tracing_integration {
                            let _ = tracing.trace_actor_created(&id).await;
                        }
                    });
                }
                #[cfg(target_arch = "wasm32")]
                {
                    let tracing_integration = self.tracing_integration.clone();
                    let id = id.clone();
                    // Trace actor creation
                    spawn_local(async move {
                        if let Some(ref tracing) = tracing_integration {
                            let _ = tracing.trace_actor_created(&id);
                        }
                    });
                }
            }
        }

        // Warm up all connectors to begin to recieve messages
        //
        // On native targets, use tokio::sync::broadcast for fan-out: a single
        // forwarder reads from each source actor's flume outport and broadcasts
        // to all downstream connectors, so every connector receives every message.
        #[cfg(not(target_arch = "wasm32"))]
        {
            use std::collections::HashMap as StdHashMap;

            // Per-connector delivery strategy.
            #[derive(Clone, Debug)]
            enum Delivery {
                Reliable, // send_async — block if full, never drop
                Latest,   // try_send — drop if full, consumer gets latest
            }

            // Fan-out: one forwarder per source actor. Each connector gets a
            // dedicated bounded channel with a delivery strategy based on the
            // target port's metadata. Channels carry Arc<HashMap> — for pool
            // ports the HashMap contains an integer slot index, not frame data.
            type FanoutEntry = (
                flume::Sender<std::sync::Arc<StdHashMap<String, Message>>>,
                Delivery,
            );
            let mut fanout_senders: StdHashMap<String, Vec<FanoutEntry>> = StdHashMap::new();

            // First pass: create per-connector channels with delivery strategy
            for connector in &self.connectors {
                let source_id = &connector.from.actor;
                let target_id = &connector.to.actor;
                let target_port = &connector.to.port;

                // Look up target actor's port delivery metadata
                let delivery = self
                    .initialized_actors
                    .get(target_id)
                    .map(|actor| {
                        let pd = actor.port_delivery();
                        match pd.get(target_port).map(|s| s.as_str()) {
                            Some("latest") => Delivery::Latest,
                            Some(s) if s.starts_with("pool:") => Delivery::Latest, // pool data = small indices, ok to skip
                            _ => Delivery::Reliable,
                        }
                    })
                    .unwrap_or(Delivery::Reliable);

                // Also check source port — if the source declares "latest", use it
                let source_delivery = self
                    .initialized_actors
                    .get(source_id)
                    .map(|actor| {
                        let pd = actor.port_delivery();
                        match pd.get(&connector.from.port).map(|s| s.as_str()) {
                            Some("latest") => Delivery::Latest,
                            Some(s) if s.starts_with("pool:") => Delivery::Latest,
                            _ => Delivery::Reliable,
                        }
                    })
                    .unwrap_or(Delivery::Reliable);

                // Use Latest if either source or target port declares it
                let final_delivery = match (&delivery, &source_delivery) {
                    (Delivery::Latest, _) | (_, Delivery::Latest) => Delivery::Latest,
                    _ => Delivery::Reliable,
                };

                let (tx, rx) = flume::bounded(64);
                fanout_senders
                    .entry(source_id.clone())
                    .or_default()
                    .push((tx, final_delivery));
                connector.init_fanout(self, rx);
            }

            // Second pass: spawn one forwarder per source actor
            for (source_id, senders) in fanout_senders {
                let from_process = self
                    .nodes
                    .get(&source_id)
                    .unwrap_or_else(|| panic!("Expected node for actor {}", source_id));

                let from_actor = self
                    .initialized_actors
                    .get(&from_process.id)
                    .unwrap_or_else(|| {
                        panic!("Expected initialized actor for id {}", from_process.id)
                    });

                let out_ports = from_actor.get_outports();
                let load_count = from_actor.load_count();

                // Forwarder: per-connector delivery strategy, concurrent sends.
                // Each connector gets its own spawned send so a slow reliable
                // connector doesn't block other connectors.
                tokio::spawn(async move {
                    while let Some(packet) = out_ports.1.clone().stream().next().await {
                        load_count.dec();
                        let arc = std::sync::Arc::new(packet);
                        let mut all_closed = true;
                        let mut futures = Vec::new();
                        for (tx, delivery) in &senders {
                            if !tx.is_disconnected() {
                                all_closed = false;
                                match delivery {
                                    Delivery::Latest => {
                                        let _ = tx.try_send(arc.clone());
                                    }
                                    Delivery::Reliable => {
                                        let tx = tx.clone();
                                        let arc = arc.clone();
                                        futures.push(tokio::spawn(async move {
                                            let _ = tx.send_async(arc).await;
                                        }));
                                    }
                                }
                            }
                        }
                        // Wait for all reliable sends to complete
                        for f in futures {
                            let _ = f.await;
                        }
                        if all_closed {
                            break;
                        }
                    }
                });
            }
        }

        // On WASM, fall back to direct flume receiver (no broadcast available)
        #[cfg(target_arch = "wasm32")]
        {
            for connector in &self.connectors {
                connector.init(self);
            }
        }

        // Send all initial packets

        {
            for iip in &self.initials {
                self.send_to_actor(
                    &iip.to.actor,
                    &iip.to.port,
                    iip.to
                        .initial_data
                        .clone()
                        .expect("Expected initial packet to have data"),
                )
                .expect("Expected to send initial packet");
            }
        }

        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn init_process(
        actor_process: std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>>,
    ) -> JoinHandle<()> {
        tokio::spawn(actor_process)
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn init_process(
        actor_process: std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>>,
    ) {
        spawn_local(actor_process);
    }

    pub fn set_compression_config(&mut self, config: CompressionConfig) {
        self.compression_config = config.clone();
    }

    pub fn send_to_actor(&self, id: &str, port: &str, data: Message) -> Result<(), anyhow::Error> {
        if let Some(_node) = self.nodes.get(id) {
            let actor = self.initialized_actors.get(id).unwrap();

            // Emit MessageReceived event for the target actor
            let timestamp = chrono::Utc::now().timestamp_millis() as u64;
            let value: serde_json::Value = data.clone().into();
            let encodable = EncodableValue::from(value);
            let _ = self
                .network_event_emitter
                .0
                .send(NetworkEvent::MessageReceived {
                    actor_id: id.to_string(),
                    port: port.to_string(),
                    message: encodable,
                    timestamp,
                });

            // Trace the message being sent
            if let Some(ref tracing) = self.tracing_integration {
                let message_type = format!("{:?}", std::mem::discriminant(&data));
                let size_bytes = serde_json::to_string(&data).unwrap_or_default().len();
                let tracing_clone = tracing.clone();
                let id_clone = id.to_string();
                let port_clone = port.to_string();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    tokio::runtime::Handle::current().spawn(async move {
                        let _ = tracing_clone
                            .trace_message_sent(&id_clone, &port_clone, &message_type, size_bytes)
                            .await;
                    });
                }
                #[cfg(target_arch = "wasm32")]
                {
                    spawn_local(async move {
                        let _ = tracing_clone
                            .trace_message_sent(&id_clone, &port_clone, &message_type, size_bytes)
                            .await;
                    });
                }
            }

            actor
                .get_inports()
                .0
                .clone()
                .send(HashMap::from_iter([(port.to_owned(), data)]))
                .unwrap_or_else(|_| panic!("Expected initial packet to Actor '{}'", id));

            return Ok(());
        }
        Err(anyhow::Error::msg("Actor not found"))
    }

    pub fn send_to_actor_encoded(
        &self,
        id: &str,
        port: &str,
        data: EncodedMessage,
    ) -> Result<(), anyhow::Error> {
        if let Some(_node) = self.nodes.get(id) {
            let actor = self.initialized_actors.get(id).unwrap();

            // Emit MessageReceived event for encoded messages
            let timestamp = chrono::Utc::now().timestamp_millis() as u64;
            let msg = Message::encoded(data.0.clone());
            let value: serde_json::Value = msg.into();
            let encodable = EncodableValue::from(value);
            let _ = self
                .network_event_emitter
                .0
                .send(NetworkEvent::MessageReceived {
                    actor_id: id.to_string(),
                    port: port.to_string(),
                    message: encodable,
                    timestamp,
                });

            actor
                .get_inports()
                .0
                .clone()
                .send(HashMap::from_iter([(
                    port.to_owned(),
                    Message::encoded(data.0),
                )]))
                .unwrap_or_else(|_| panic!("Expected initial packet to Actor '{}'", id));

            return Ok(());
        }
        Err(anyhow::Error::msg("Actor not found"))
    }

    /// Read all available output messages from an actor's outport without blocking.
    ///
    /// Returns a `Vec` of `(port_name, message)` tuples for every packet
    /// currently sitting in the actor's outport channel. Useful for
    /// inspecting results after a pipeline has finished.
    pub fn read_actor_output(&self, id: &str) -> Vec<(String, Message)> {
        let component = match self.nodes.get(id) {
            Some(node) => &node.component,
            None => return Vec::new(),
        };
        let actor = match self.actors.get(component) {
            Some(a) => a,
            None => return Vec::new(),
        };
        let outport_rx = actor.get_outports().1;
        let mut results = Vec::new();
        while let Ok(pkt) = outport_rx.try_recv() {
            for (port, msg) in pkt {
                results.push((port, msg));
            }
        }
        results
    }

    pub fn register_actor(&mut self, name: &str, actor: impl Actor) -> Result<(), anyhow::Error> {
        if self.actors.contains_key(name) {
            return Err(anyhow::Error::msg(format!(
                "Process '{}' already extists in Network",
                name
            )));
        }

        self.actors.insert(name.to_string(), Arc::new(actor));
        Ok(())
    }

    pub fn register_actor_arc(
        &mut self,
        name: &str,
        actor: Arc<dyn Actor>,
    ) -> Result<(), anyhow::Error> {
        if self.actors.contains_key(name) {
            return Err(anyhow::Error::msg(format!(
                "Process '{}' already extists in Network",
                name
            )));
        }

        self.actors.insert(name.to_string(), actor);
        Ok(())
    }

    pub fn add_node(
        &mut self,
        id: &str,
        process: &str,
        metadata: Option<HashMap<String, Value>>,
    ) -> Result<(), anyhow::Error> {
        if !self.actors.contains_key(process) {
            return Err(anyhow::Error::msg(format!(
                "Could not find process '{}' in Network",
                process
            )));
        }

        let node = GraphNode {
            id: id.to_string(),
            component: process.to_string(),
            metadata,
        };
        self.nodes.insert(id.to_string(), node);

        Ok(())
    }

    pub fn add_connection(&mut self, connector: Connector) {
        self.connectors.push(connector);
    }

    /// Check how many connections target a specific (actor, port) pair.
    pub fn connection_count_to_port(&self, actor_id: &str, port: &str) -> usize {
        self.connectors
            .iter()
            .filter(|c| c.to.actor == actor_id && c.to.port == port)
            .count()
    }

    /// Get all ports on an actor that have multiple incoming connections (fan-in).
    /// Returns a map of port_name → connection_count for ports with count > 1.
    pub fn fan_in_ports(&self, actor_id: &str) -> HashMap<String, usize> {
        let mut port_counts: HashMap<String, usize> = HashMap::new();
        for c in &self.connectors {
            if c.to.actor == actor_id {
                *port_counts.entry(c.to.port.clone()).or_insert(0) += 1;
            }
        }
        port_counts.retain(|_, count| *count > 1);
        port_counts
    }

    /// Inject an input event into all actors matching a component type.
    ///
    /// The runtime calls this when system/browser events occur.
    /// Finds all nodes whose component matches `component_type` and
    /// sends the event data as `Message::Object` on the `_event` port.
    pub fn inject_input_event(
        &self,
        component_type: &str,
        event_data: serde_json::Value,
    ) -> Result<(), anyhow::Error> {
        let msg = Message::from(event_data);
        let mut found = false;

        // Find all nodes with matching component and send the event
        let node_ids: Vec<String> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.component == component_type)
            .map(|(id, _)| id.clone())
            .collect();

        for node_id in &node_ids {
            self.send_to_actor(node_id, "_event", msg.clone())?;
            found = true;
        }

        if !found {
            // Also try matching by actor name (template_id or component name)
            let alt_ids: Vec<String> = self
                .nodes
                .iter()
                .filter(|(_, node)| {
                    node.component.contains(component_type)
                        || node
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("template_id"))
                            .and_then(|v| v.as_str())
                            .map(|t| t == component_type)
                            .unwrap_or(false)
                })
                .map(|(id, _)| id.clone())
                .collect();

            for node_id in &alt_ids {
                self.send_to_actor(node_id, "_event", msg.clone())?;
            }
        }

        Ok(())
    }

    /// Get all node IDs matching a component type.
    pub fn find_nodes_by_component(&self, component_type: &str) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.component == component_type)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn add_initial(&mut self, connector: InitialPacket) {
        self.initials.push(connector);
    }

    pub fn with_graph(config: NetworkConfig, graph: &Graph) -> Arc<Mutex<Network>> {
        let mut network = Self::new(config);

        for (id, node) in &graph.nodes {
            network.nodes.insert(id.to_string(), node.clone());
        }

        for iip in &graph.initializers {
            network.add_initial(InitialPacket {
                to: ConnectionPoint {
                    actor: iip.to.node_id.clone(),
                    port: iip.to.port_name.clone(),
                    initial_data: Some(iip.data.clone().into()),
                },
            });
        }

        for edge in &graph.connections {
            network.add_connection(Connector {
                from: ConnectionPoint {
                    actor: edge.from.node_id.clone(),
                    port: edge.from.port_id.clone(),
                    initial_data: edge.clone().data.map(|d| d.into()),
                },
                to: ConnectionPoint {
                    actor: edge.to.node_id.clone(),
                    port: edge.to.port_id.clone(),
                    initial_data: None,
                },
            });
        }

        let (_, graph_receiver) = graph.event_channel.clone();
        let network = Arc::new(Mutex::new(network));

        let network_clone = network.clone();

        let event_worker = async move {
            while let Some(graph_event) = graph_receiver.clone().stream().next().await {
                if let Ok(mut network) = network_clone.clone().lock() {
                    match graph_event {
                        GraphEvents::AddNode(node_value) => {
                            if let Ok(node) = serde_json::from_value::<GraphNode>(node_value) {
                                let _ = network.add_node(&node.id, &node.component, node.metadata);
                            }
                        }
                        GraphEvents::RemoveNode(node_value) => {
                            if let Ok(node) = serde_json::from_value::<GraphNode>(node_value) {
                                network.nodes.remove(&node.id);
                            }
                        }
                        GraphEvents::AddConnection(edge_value) => {
                            if let Ok(edge) = serde_json::from_value::<GraphConnection>(edge_value)
                            {
                                network.add_connection(Connector {
                                    from: ConnectionPoint {
                                        actor: edge.from.node_id,
                                        port: edge.from.port_name,
                                        initial_data: edge.data.map(Message::from),
                                    },
                                    to: ConnectionPoint {
                                        actor: edge.to.node_id,
                                        port: edge.to.port_name,
                                        initial_data: None,
                                    },
                                });
                            }
                        }
                        GraphEvents::RemoveConnection(edge_value) => {
                            if let Ok(edge) = serde_json::from_value::<GraphConnection>(edge_value)
                            {
                                network.connectors.retain(|conn| {
                                    !(conn.from.actor == edge.from.node_id
                                        && conn.from.port == edge.from.port_name
                                        && conn.to.actor == edge.to.node_id
                                        && conn.to.port == edge.to.port_name)
                                });
                            }
                        }
                        GraphEvents::AddInitial(iip_value) => {
                            if let Ok(iip) = serde_json::from_value::<GraphIIP>(iip_value) {
                                network.add_initial(InitialPacket {
                                    to: ConnectionPoint {
                                        actor: iip.to.node_id,
                                        port: iip.to.port_name,
                                        initial_data: Some(Message::from(iip.data)),
                                    },
                                });
                            }
                        }
                        GraphEvents::RemoveInitial(iip_value) => {
                            if let Ok(iip) = serde_json::from_value::<GraphIIP>(iip_value) {
                                network.initials.retain(|init| {
                                    !(init.to.actor == iip.to.node_id
                                        && init.to.port == iip.to.port_name)
                                });
                            }
                        }
                        _ => {} // Handle other events as needed
                    }
                }
            }
        };

        // Bind graph events to network
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::spawn(event_worker);
        }

        #[cfg(target_arch = "wasm32")]
        spawn_local(event_worker);

        network
    }

    /// Collect unresolved component specs from graph nodes.
    ///
    /// Returns `(node_id, component_name, spec)` for every node whose
    /// component isn't yet registered and has a `componentSpec` in metadata.
    fn unresolved_component_specs(&self) -> Vec<(String, String, ComponentSpec)> {
        self.nodes
            .iter()
            .filter(|(_, node)| !self.actors.contains_key(&node.component))
            .filter_map(|(id, node)| {
                node.component_spec()
                    .map(|spec| (id.clone(), node.component.clone(), spec))
            })
            .collect()
    }

    /// Resolve component specifications from graph node metadata.
    ///
    /// Scans all nodes for `componentSpec` metadata entries and auto-registers
    /// actors that haven't been manually registered yet. This bridges the gap
    /// between graph definition and runtime — the graph declares *what* a
    /// component is (`ComponentSpec`), and this method provisions the actual
    /// actor implementation.
    ///
    /// Call between `with_graph()` and `start()`:
    /// ```ignore
    /// let network = Network::with_graph(config, &graph);
    /// network.lock().unwrap().resolve_components(&registry).await?;
    /// network.lock().unwrap().start()?;
    /// ```
    ///
    /// Resolution rules:
    ///   - `ComponentSpec::Native` → expects actor already registered by name
    ///   - `ComponentSpec::Script { .. }` → creates actor via `ComponentRegistry`
    ///   - `ComponentSpec::Wasm { .. }` → reserved (not yet implemented)
    ///   - `ComponentSpec::Subgraph { .. }` → reserved (handled by SubgraphActor)
    ///   - No `componentSpec` → expects actor already registered by name (legacy)
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn resolve_components(
        &mut self,
        registry: &crate::script_discovery::registry::ComponentRegistry,
    ) -> Result<(), anyhow::Error> {
        use tracing::{debug, info, warn};

        let unresolved = self.unresolved_component_specs();
        let mut resolved = 0u32;

        // Warn about nodes with no spec and no actor
        for (id, node) in &self.nodes {
            if !self.actors.contains_key(&node.component) && node.component_spec().is_none() {
                warn!(
                    "Node '{}' references unregistered component '{}' with no componentSpec",
                    id, node.component
                );
            }
        }

        for (id, component, spec) in unresolved {
            match spec {
                ComponentSpec::Native => {
                    warn!(
                        "Node '{}' has componentSpec=native but component '{}' is not registered",
                        id, component
                    );
                }
                ComponentSpec::Script { .. } => {
                    let actor = registry.get_actor(&component).await.map_err(|e| {
                        anyhow::anyhow!(
                            "Failed to resolve script component '{}' for node '{}': {}",
                            component,
                            id,
                            e
                        )
                    })?;

                    self.actors.insert(component.clone(), actor);
                    info!(
                        "Resolved script component '{}' for node '{}'",
                        component, id
                    );
                    resolved += 1;
                }
                ComponentSpec::Wasm { source, handler } => {
                    warn!(
                        "Node '{}' has componentSpec=wasm (source={}, handler={}), not yet implemented",
                        id, source, handler
                    );
                }
                ComponentSpec::Subgraph { graph } => {
                    debug!(
                        "Node '{}' references subgraph '{}', expecting SubgraphActor registration",
                        id, graph
                    );
                }
            }
        }

        if resolved > 0 {
            info!("Resolved {} component(s) from graph metadata", resolved);
        }

        Ok(())
    }

    /// WASM variant of component resolution.
    ///
    /// Scans graph nodes for `componentSpec`, deploys inline scripts to
    /// dynASB via browser `fetch`, and creates `BrowserRpcClient`-backed
    /// actors that communicate over `web-sys::WebSocket`.
    ///
    /// The `dynasb_api_url` is the base URL of the dynASB instance
    /// (e.g. `"https://dynasb.example.com"`).
    #[cfg(target_arch = "wasm32")]
    pub async fn resolve_components(&mut self, dynasb_api_url: &str) -> Result<(), anyhow::Error> {
        use crate::graph::types::ScriptSourceType;
        use crate::websocket_rpc::browser_client;

        let unresolved = self.unresolved_component_specs();
        let mut errors = Vec::new();

        for (id, component, spec) in unresolved {
            match spec {
                ComponentSpec::Native => {
                    errors.push(format!(
                        "Node '{}': native component '{}' not registered",
                        id, component
                    ));
                }
                ComponentSpec::Script { script } => {
                    // Get the inline code
                    let code = match script.source {
                        ScriptSourceType::Inline => script.code.ok_or_else(|| {
                            anyhow::anyhow!("Node '{}': inline script has no code", id)
                        })?,
                        ScriptSourceType::File => {
                            errors.push(format!(
                                "Node '{}': file-based scripts not available in browser",
                                id
                            ));
                            continue;
                        }
                    };

                    // Deploy to dynASB via fetch
                    let (function_id, ws_url) = browser_client::browser_deploy_script(
                        dynasb_api_url,
                        &component,
                        &script.runtime,
                        &code,
                        &script.handler,
                        if script.dependencies.is_empty() {
                            None
                        } else {
                            Some(script.dependencies)
                        },
                        script.timeout_seconds,
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("Node '{}': deploy failed: {}", id, e))?;

                    // Inject deployment metadata into the node
                    if let Some(node) = self.nodes.get_mut(&id) {
                        node.set_metadata("dynasb.function_id", &function_id);
                        node.set_metadata("dynasb.ws_url", &ws_url);
                    }

                    // TODO: create BrowserRpcClient-backed actor and register
                    // For now, deployment metadata is set — actor creation
                    // requires a browser-compatible ScriptActor implementation
                }
                ComponentSpec::Wasm { .. } => {
                    errors.push(format!(
                        "Node '{}': WASM component auto-loading not yet implemented",
                        id
                    ));
                }
                ComponentSpec::Subgraph { .. } => {
                    // Subgraphs handled separately
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Unresolved components:\n  {}",
                errors.join("\n  ")
            ))
        }
    }

    /// Shutdown the network and finalize pending tasks
    pub fn shutdown(&self) {
        // Emit NetworkShutdown event
        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
        let _ = self
            .network_event_emitter
            .0
            .send(NetworkEvent::NetworkShutdown { timestamp });

        #[cfg(not(target_arch = "wasm32"))]
        {
            let tracing_integration = self.tracing_integration.clone();
            tokio::runtime::Handle::current().spawn(async move {
                // Shutdown tracing first to flush any pending events
                if let Some(ref tracing) = tracing_integration
                    && let Err(e) = tracing.client().shutdown().await
                {
                    tracing::warn!("Failed to shutdown tracing client: {}", e);
                }
            });
        }

        // Clear all actors
        let active_actors = self.get_active_actors();
        for actor_id in active_actors {
            if let Some(actor) = self.actors.get(&actor_id) {
                actor.shutdown();
            }
            // Abort their processes
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(process) = self.processes.get(&actor_id) {
                    process.abort();
                }
            }
        }
    }

    pub fn actor_count(&self) -> usize {
        self.actors.len()
    }

    pub async fn execute_actor(
        &self,
        actor_id: &str,
        payload: HashMap<String, Message>,
    ) -> Result<HashMap<String, Message>, anyhow::Error> {
        if let Some(actor) = self.initialized_actors.get(actor_id) {
            let config = ActorConfig::from_node(self.nodes.get(actor_id).unwrap().clone())?;

            let outports = actor.get_outports();
            #[cfg(not(target_arch = "wasm32"))]
            let state = Arc::new(parking_lot::Mutex::new(MemoryState(HashMap::new())));
            #[cfg(target_arch = "wasm32")]
            let state = Arc::new(parking_lot::Mutex::new(MemoryState::default()));
            let actor_behavior = actor.get_behavior();
            let load = actor.load_count();
            match actor_behavior(ActorContext::new(payload, outports, state, config, load)).await {
                Ok(res) => {
                    // Emit ActorCompleted event
                    let timestamp = chrono::Utc::now().timestamp_millis() as u64;
                    let outputs = if !res.is_empty() {
                        let mut output_obj = serde_json::Map::new();
                        for (port, msg) in &res {
                            if let Ok(value) = serde_json::to_value(msg) {
                                output_obj.insert(port.clone(), value);
                            }
                        }
                        Some(Value::Object(output_obj))
                    } else {
                        None
                    };

                    let component = self
                        .nodes
                        .get(actor_id)
                        .map(|n| n.component.clone())
                        .unwrap_or_default();
                    let _ = self
                        .network_event_emitter
                        .0
                        .send(NetworkEvent::ActorCompleted {
                            actor_id: actor_id.to_string(),
                            component,
                            outputs,
                            timestamp,
                        });

                    if let Some(tracing) = &self.tracing_integration {
                        let _ = tracing.trace_actor_completed(actor_id).await;
                    }

                    return Ok(res);
                }
                Err(err) => {
                    // Emit ActorFailed event
                    let timestamp = chrono::Utc::now().timestamp_millis() as u64;
                    let component = self
                        .nodes
                        .get(actor_id)
                        .map(|n| n.component.clone())
                        .unwrap_or_default();
                    let _ = self
                        .network_event_emitter
                        .0
                        .send(NetworkEvent::ActorFailed {
                            actor_id: actor_id.to_string(),
                            component,
                            error: err.to_string(),
                            timestamp,
                        });

                    if let Some(ref tracing) = self.tracing_integration {
                        let tracing_clone = tracing.clone();
                        let id_clone = actor_id.to_string();

                        let _ = tracing_clone
                            .trace_actor_failed(&id_clone, err.to_string())
                            .await;
                    }
                }
            }
        }

        Ok(HashMap::new())
    }

    pub fn get_active_actors(&self) -> Vec<String> {
        self.initialized_actors
            .iter()
            .filter(|(_, actor)| actor.load_count().get() > 0)
            .map(|(id, _)| id.clone())
            .collect()
    }

    pub fn get_message_queue_size(&self) -> usize {
        // Return current message queue size
        0 // Placeholder
    }

    /// Get the network event receiver for subscribing to network events
    pub fn get_event_receiver(&self) -> flume::Receiver<NetworkEvent> {
        self.network_event_emitter.1.clone()
    }
}

/// GraphNetwork
///
/// GraphNetwork is a wrapper to a Network that binds to changes in a Graph
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct GraphNetwork {
    network: Arc<Mutex<Network>>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl GraphNetwork {
    #[inline]
    fn new(config: NetworkConfig, graph: &Graph) -> GraphNetwork {
        Self {
            network: Network::with_graph(config, graph),
        }
    }

    /// Create Network from an FBP Graph
    #[wasm_bindgen(constructor)]
    pub fn _new(graph: &Graph) -> GraphNetwork {
        Self::new(NetworkConfig::default(), graph)
    }

    /// Register an actor to the network
    #[wasm_bindgen(js_name = registerActor)]
    pub fn register_actor(&mut self, name: &str, actor: ExternActor) -> Result<(), JsValue> {
        if let Ok(network) = self.network.lock().as_mut() {
            network._register_actor(name, actor)?;
        }
        Ok(())
    }

    /// Start the network
    #[wasm_bindgen]
    pub async fn start(&mut self) -> Result<(), JsValue> {
        if let Ok(network) = self.network.lock().as_mut() {
            return network
                .start()
                .map_err(|err| JsValue::from_str(err.to_string().as_str()));
        }
        Err(JsValue::from_str("Error starting Network"))
    }

    /// Listen to events in the network
    #[wasm_bindgen]
    pub fn next(&mut self, callback: web_sys::js_sys::Function) {
        if let Ok(network) = self.network.lock().as_mut() {
            network.next(callback);
        }
    }

    /// Inject an input event into actors matching a component type.
    /// Called from JavaScript when browser events occur.
    #[wasm_bindgen(js_name = injectInputEvent)]
    pub fn inject_input_event(
        &self,
        component_type: &str,
        event_data: JsValue,
    ) -> Result<(), JsValue> {
        let value: serde_json::Value = serde_wasm_bindgen::from_value(event_data)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        if let Ok(network) = self.network.lock() {
            network
                .inject_input_event(component_type, value)
                .map_err(|e| JsValue::from_str(&e.to_string()))?;
        }
        Ok(())
    }

    /// Shutdown the network
    #[wasm_bindgen]
    pub fn shutdown(&self) {
        if let Ok(network) = self.network.lock().as_mut() {
            network._shutdown();
        }
    }

    /// Create an actor
    #[wasm_bindgen(js_name = createActor)]
    pub fn create_actor(&mut self, name: &str, actor: ExternActor) -> Result<JsValue, JsValue> {
        if let Ok(mut network) = self.network.lock() {
            return network.create_actor(name, actor);
        }
        Err(JsValue::from_str("Error accessing network"))
    }

    /// Get actor names
    #[wasm_bindgen(js_name = getActorNames)]
    pub fn get_actor_names(&self) -> Vec<String> {
        if let Ok(network) = self.network.lock() {
            return network.get_actor_names();
        }
        Vec::new()
    }

    /// Get active actors
    #[wasm_bindgen(js_name = getActiveActors)]
    pub fn get_active_actors(&self) -> Vec<String> {
        if let Ok(network) = self.network.lock() {
            return network._get_active_actors();
        }
        Vec::new()
    }

    /// Get actor count
    #[wasm_bindgen(js_name = getActorCount)]
    pub fn get_actor_count(&self) -> usize {
        if let Ok(network) = self.network.lock() {
            return network._get_actor_count();
        }
        0
    }

    /// Get message queue size
    #[wasm_bindgen(js_name = getMessageQueueSize)]
    pub fn get_message_queue_size(&self) -> usize {
        if let Ok(network) = self.network.lock() {
            return network._get_message_queue_size();
        }
        0
    }

    /// Add a node
    #[wasm_bindgen(js_name = addNode)]
    pub fn add_node(&mut self, id: &str, process: &str) {
        if let Ok(mut network) = self.network.lock() {
            network._add_node(id, process);
        }
    }

    /// Add a connection
    #[wasm_bindgen(js_name = addConnection)]
    pub fn add_connection(&mut self, connector: Connector) {
        if let Ok(mut network) = self.network.lock() {
            network._add_connection(connector);
        }
    }

    /// Add initial data
    #[wasm_bindgen(js_name = addInitial)]
    pub fn add_initial(&mut self, connector: InitialPacket) {
        if let Ok(mut network) = self.network.lock() {
            network._add_initial(connector);
        }
    }

    /// Emit data to an actor
    #[wasm_bindgen]
    pub fn emit(&mut self, actor_id: String, packet: JsValue) {
        if let Ok(mut network) = self.network.lock() {
            network.emit(actor_id, packet);
        }
    }

    /// Send data to an actor
    #[wasm_bindgen(js_name = sendToActor)]
    pub fn send_to_actor(&self, actor_id: &str, port: &str, data: JsValue) -> Result<(), JsValue> {
        if let Ok(network) = self.network.lock() {
            return network._send_to_actor(actor_id, port, data);
        }
        Err(JsValue::from_str("Error accessing network"))
    }

    /// Execute an actor
    #[wasm_bindgen(js_name = executeActor)]
    pub async fn execute_actor(&self, actor_id: &str, data: JsValue) -> Result<JsValue, JsValue> {
        if let Ok(network) = self.network.lock() {
            return network._execute_actor(actor_id, data).await;
        }
        Err(JsValue::from_str("Error accessing network"))
    }

    /// Get a node
    #[wasm_bindgen(js_name = getNode)]
    pub fn get_node(&self, actor_id: &str) -> JsValue {
        if let Ok(network) = self.network.lock() {
            return network.get_node(actor_id);
        }
        JsValue::null()
    }
}
