#[cfg(target_arch = "wasm32")]
use futures::Future;
use futures::StreamExt;
#[cfg(target_arch = "wasm32")]
use futures::future::lazy;
#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
#[cfg(not(target_arch = "wasm32"))]
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use rusty_pool::ThreadPool;
use std::borrow::BorrowMut;
use std::collections::HashMap;
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

#[cfg(target_arch = "wasm32")]
use crate::actor::ExternActor;

use crate::actor::{Actor, ActorState, MemoryState, Port};

#[cfg(target_arch = "wasm32")]
use crate::actor::ActorChannel;
use crate::graph::Graph;
use crate::graph::types::{GraphConnection, GraphEdge, GraphEvents, GraphIIP, GraphNode};
use crate::message::{CompressionConfig, EncodedMessage, Message, MessageError};

use crate::connector::{ConnectionPoint, Connector, InitialPacket};
use std::sync::{Arc, Mutex};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct NetworkConfig {
    /// Size of worker threads this network should use
    pub pool_size: usize,
    /// Max size of worker threads
    pub max_pool_size: usize,
    /// How long  should a worker run before it is shutdown
    pub keep_alive: Duration,

    pub compression: CompressionConfig,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            pool_size: num_cpus::get(),
            max_pool_size: num_cpus::get() * 2,
            keep_alive: Duration::from_secs(3 * 60),
            compression: CompressionConfig::default(),
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
    Actor {
        actor_id: String,
        port: String,
        data: Value,
    },
    #[serde(rename_all = "camelCase")]
    FlowTrace { from: FlowStub, to: FlowStub },
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
pub struct Network {
    config: NetworkConfig,
    pub(crate) actors: HashMap<String, Box<dyn Actor>>,
    pub(crate) nodes: HashMap<String, String>,
    connectors: Vec<Connector>,
    initials: Vec<InitialPacket>,
    pub(crate) network_event_emitter: (flume::Sender<NetworkEvent>, flume::Receiver<NetworkEvent>),
    #[cfg(target_arch = "wasm32")]
    event_handle: Vec<JsValue>,
    pub(crate) thread_pool: Arc<Mutex<ThreadPool>>,
    compression_config: CompressionConfig,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_class = Network))]
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
        use crate::actor::WasmActor;

        self.register_actor(name, WasmActor::new(actor))
            .map_err(|err| JsValue::from_str(format!("{}", err.to_string()).as_str()))?;
        Ok(())
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
        self.nodes.insert(id.to_string(), process.to_string());
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
        if let Some(process) = self.nodes.get(&actor_id) {
            let actor = self.actors.get(process).unwrap();
            let outports = actor.get_outports();
            let messages = packet
                .into_serde::<HashMap<String, serde_json::Value>>()
                .expect("Expected packet to be mapped")
                .iter()
                .map(|(port, msg)| (port.to_owned(), Message::from(msg.clone())))
                .collect::<HashMap<String, Message>>();
            Self::send_outport_msg(outports, messages).unwrap();
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
    }

    /// Get an actor node from the network
    #[cfg(target_arch = "wasm32")]
    #[wasm_bindgen(js_name = getNode)]
    pub fn get_node(&self, actor_id: &str) -> JsValue {
        if let Some(actor) = self.nodes.get(actor_id) {
            return actor.into();
        }
        JsValue::null()
    }
}

impl Network {
    pub fn new(config: NetworkConfig) -> Self {
        Self {
            config: config.clone(),
            nodes: HashMap::new(),
            actors: HashMap::new(),
            connectors: Vec::new(),
            initials: Vec::new(),
            network_event_emitter: flume::unbounded(),
            #[cfg(target_arch = "wasm32")]
            event_handle: Vec::new(),
            thread_pool: Arc::new(Mutex::new(ThreadPool::new(
                config.pool_size,
                config.max_pool_size,
                config.keep_alive,
            ))),
            compression_config: config.compression,
        }
    }

    pub async fn start(&self) -> Result<(), anyhow::Error> {
        // Warm up all processes
        #[cfg(not(target_arch = "wasm32"))]
        {
            // use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
            //  self
            //     .actors
            //     .iter()
            //     .for_each(|(_, actor)| async {
            //         Self::init_process(actor, self.thread_pool.clone()).await
            //     });

            for actor in &self.actors {
                Self::init_process(actor.1, self.thread_pool.clone()).await;
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            for (id, actor) in &self.actors {
                Self::init_process(actor, &self.thread_pool).await;
            }
        }

        // Warm up all connectors to begin to recieve messages
        #[cfg(not(target_arch = "wasm32"))]
        {
            // use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

            for connector in &self.connectors {
                connector.init(self).await;
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            for connector in &self.connectors {
                connector.init(self);
            }
        }

        // Send all initial packets
        #[cfg(not(target_arch = "wasm32"))]
        {
            use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
            self.initials.par_iter().for_each(|iip| {
                self.send_to_actor(
                    &iip.to.actor,
                    &iip.to.port,
                    iip.to
                        .initial_data
                       .clone()
                        .expect("Expected initial packet to have data"),
                )
                .expect("Expected to send initial packet");

                #[cfg(feature = "flowtrace")]
                {
                    // Send flow event
                    let (network_sender, _) = &self.network_event_emitter;
                    let _ = network_sender.clone().send(NetworkEvent::FlowTrace {
                        from: FlowStub {
                            actor_id: "_initial_".to_owned(),
                            port: "_initial_".to_owned(),
                            data: Some(
                                iip.to
                                    .initial_data
                                    .unwrap_or(Message::Any(serde_json::json!({})))
                                    .into(),
                            ),
                        },
                        to: FlowStub {
                            actor_id: iip.to.actor.clone(),
                            port: iip.to.port.clone(),
                            data: None,
                        },
                    });
                }
            });
        }
        #[cfg(target_arch = "wasm32")]
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

                #[cfg(feature = "flowtrace")]
                {
                    // Send flow event
                    let (network_sender, _) = &self.network_event_emitter;
                    let _ = network_sender.clone().send(NetworkEvent::FlowTrace {
                        from: FlowStub {
                            actor_id: "_initial_".to_owned(),
                            port: "_initial_".to_owned(),
                            data: Some(
                                iip.to
                                    .initial_data
                                    .clone()
                                    .unwrap_or(Message::Any(serde_json::json!({})))
                                    .into(),
                            ),
                        },
                        to: FlowStub {
                            actor_id: iip.to.actor.clone(),
                            port: iip.to.port.clone(),
                            data: None,
                        },
                    });
                }
            }
        }

        Ok(())
    }

    pub(crate) async fn init_process(actor: &Box<dyn Actor>, thread_pool: Arc<Mutex<ThreadPool>>) {
       
        let process = actor.create_process();
       

        #[cfg(not(target_arch = "wasm32"))]
        //thread_pool.lock().unwrap().spawn(process);
        let _ = tokio::spawn(async move {
            process.await;
        });

        // thread_pool.lock().unwrap().spawn();

        #[cfg(target_arch = "wasm32")]
        spawn_local(process);
    }

    pub fn set_compression_config(&mut self, config: CompressionConfig) {
        self.compression_config = config.clone();
    }

    /// Send the message to all the respective outports
    #[inline]
    pub(crate) fn send_outport_msg(
        out_port: Port,
        messages: HashMap<String, Message>,
    ) -> Result<(), anyhow::Error> {
        out_port.0.send(messages)?;
        Ok(())
    }

    pub fn send_to_actor(&self, id: &str, port: &str, data: Message) -> Result<(), anyhow::Error> {
        if let Some(process) = self.nodes.get(id) {
            let actor = self.actors.get(process).unwrap();

            actor
                .get_inports()
                .0
                .clone()
                .send(HashMap::from_iter([(port.to_owned(), data)]))
                .expect(format!("Expected initial packet to Actor '{}'", id).as_str());

            return Ok(());
        }
        return Err(anyhow::Error::msg("Actor not found"));
    }

    pub fn send_to_actor_encoded(
        &self,
        id: &str,
        port: &str,
        data: EncodedMessage,
    ) -> Result<(), anyhow::Error> {
        if let Some(process) = self.nodes.get(id) {
            let actor = self.actors.get(process).unwrap();

            actor
                .get_inports()
                .0
                .clone()
                .send(HashMap::from_iter([(
                    port.to_owned(),
                    Message::encoded(data.0),
                )]))
                .expect(format!("Expected initial packet to Actor '{}'", id).as_str());

            return Ok(());
        }
        return Err(anyhow::Error::msg("Actor not found"));
    }

    pub fn register_actor(&mut self, name: &str, actor: impl Actor) -> Result<(), anyhow::Error> {
        if self.actors.contains_key(name) {
            return Err(anyhow::Error::msg(format!(
                "Process '{}' already extists in Network",
                name
            )));
        }

        self.actors.insert(name.to_string(), Box::new(actor));
        Ok(())
    }

    pub fn add_node(&mut self, id: &str, process: &str) -> Result<(), anyhow::Error> {
        if !self.actors.contains_key(process) {
            return Err(anyhow::Error::msg(format!(
                "Could not find process '{}' in Network",
                process
            )));
        }

        self.nodes.insert(id.to_string(), process.to_string());
        Ok(())
    }

    pub fn add_connection(&mut self, connector: Connector) {
        self.connectors.push(connector);
    }

    pub fn add_initial(&mut self, connector: InitialPacket) {
        self.initials.push(connector);
    }

    pub fn with_graph(config: NetworkConfig, graph: &Graph) -> Arc<Mutex<Network>> {
        let mut network = Self::new(config);

        for (id, node) in &graph.nodes {
            network
                .add_node(id, &node.component)
                .expect("Expected to add node to Network");
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
                    port: edge.from.port_name.clone(),
                    initial_data: edge.clone().data.map(|d| d.into()),
                },
                to: ConnectionPoint {
                    actor: edge.to.node_id.clone(),
                    port: edge.to.port_name.clone(),
                    initial_data: None,
                },
            });
        }

        let (_, graph_receiver) = graph.event_channel.clone();
        let network = Arc::new(Mutex::new(network));

        let network_clone = network.clone();

        let _event_worker = async move {
            while let Some(graph_event) = graph_receiver.clone().stream().next().await {
                if let Ok(mut network) = network_clone.lock() {
                    match graph_event {
                        GraphEvents::AddNode(node_value) => {
                            if let Ok(node) = serde_json::from_value::<GraphNode>(node_value) {
                                let _ = network.add_node(&node.id, &node.component);
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
                                        initial_data: edge.data.map(|d| Message::from(d)),
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

        // TODO: Bind graph events to network
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(network) = network.clone().lock() {
                network.thread_pool.lock().unwrap().spawn(_event_worker);
            }
        }

        #[cfg(target_arch = "wasm32")]
        spawn_local(_event_worker);

        return network;
    }

    /// Shutdown the network and finalize pending tasks
    pub fn shutdown(&self) {
        // // Wait for all active workers to be cleared
        // self.thread_pool
        //     .lock()
        //     .unwrap()
        //     .clone()
        //     .shutdown_join_timeout(Duration::from_millis(2500));
        // Clear all actors
        
        let active_actors = self.get_active_actors();
        for actor_id in active_actors {
            if let Some(actor) = self.actors.get(&actor_id) {
                actor.shutdown();
            }
        }

    }

    pub fn actor_count(&self) -> usize {
       self.actors.len()
    }

    pub async fn execute_actor(&self, actor_id: &str, message: Message) -> Result<Message, anyhow::Error> {


        println!(
            "🎭 Executing actor: {} with message type: {:?}",
            actor_id,
            std::mem::discriminant(&message)
        );

        // Placeholder implementation
        Ok(Message::object(
            serde_json::json!({
                "status": "success",
                "actor_id": actor_id,
                "timestamp": chrono::Utc::now().timestamp_millis()
            })
            .into(),
        ))
    }

    pub fn get_active_actors(&self) -> Vec<String> {
        self.actors.iter().filter(|(_, actor)| actor.load_count().lock().get() > 0 ).map(|(id, _)| id.clone()).collect()
    }

    pub fn get_message_queue_size(&self) -> usize {
        // Return current message queue size
        0 // Placeholder
    }
}

/// GraphNetwork
///
/// GraphNetwork is a wrapper to a Network that binds to changes in a Graph
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct GraphNetwork {
    network: Mutex<Network>,
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
    pub fn start(&mut self) -> Result<(), JsValue> {
        if let Ok(network) = self.network.lock() {
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

    /// Shutdown the network
    #[wasm_bindgen]
    pub fn shutdown(&self) {
        if let Ok(network) = self.network.lock().as_mut() {
            network._shutdown();
        }
    }
}
