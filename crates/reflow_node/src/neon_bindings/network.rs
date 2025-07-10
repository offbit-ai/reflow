//! Network-related Neon bindings
//!
//! Provides Node.js bindings for Network and simplified GraphNetwork classes
//! with enhanced capabilities compared to WASM version

use crate::{
    neon_bindings::{js_to_json_value, json_value_to_js, JavaScriptActor, JavaScriptActorState},
    runtime,
};
use neon::prelude::*;
use parking_lot::{lock_api::RwLock, Mutex as ParkingMutex, RwLock as ParkingRwLock};
use reflow_network::{
    connector::{ConnectionPoint, Connector, InitialPacket},
    graph::{types::GraphExport, Graph},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::Value as JsonValue;
use std::sync::Mutex as StdMutex;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub struct NodeNetwork {
    inner: Arc<ParkingRwLock<Network>>,
    channel: Channel,
}

impl Finalize for NodeNetwork {}

impl NodeNetwork {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeNetwork>> {
        let config_js = cx.argument_opt(0);

        let config = if let Some(config_value) = config_js {
            if let Ok(config_obj) = config_value.downcast::<JsObject, _>(&mut cx) {
                let config_json = super::js_to_json_value(&mut cx, config_obj.upcast())?;
                serde_json::from_value(config_json).unwrap_or_default()
            } else {
                NetworkConfig::default()
            }
        } else {
            NetworkConfig::default()
        };

        let network = Network::new(config);
        let channel = cx.channel();

        Ok(cx.boxed(NodeNetwork {
            inner: Arc::new(ParkingRwLock::new(network)),
            channel,
        }))
    }

    fn register_actor(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let name = cx.argument::<JsString>(1)?.value(&mut cx);
        let actor_js = cx.argument::<JsObject>(2)?;

        // Extract actor properties
        let inports = extract_ports(&mut cx, &actor_js, "inports")?;
        let outports = extract_ports(&mut cx, &actor_js, "outports")?;
        let config = extract_config(&mut cx, &actor_js)?;

        // Create JavaScript actor bridge
        let js_actor = Arc::new(RwLock::new(actor_js.root(&mut cx)));
        let js_actor_bridge =
            super::JavaScriptActor::new(js_actor, config, inports, outports, this.channel.clone());

        // Register with the network
        {
            let mut network = this.inner.write();
            if let Err(e) = network.register_actor(&name, js_actor_bridge) {
                return cx.throw_error(format!("Failed to register actor: {}", e));
            }
        }

        Ok(cx.undefined())
    }

    fn start(mut cx: FunctionContext) -> JsResult<JsPromise> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let network = Arc::clone(&this.inner);
        let channel = this.channel.clone();
        let (deferred, promise) = cx.promise();

        runtime::spawn(async move {
            let result = {
                let mut net = network.write();
                let res = net.start();
                res
            };

            deferred.settle_with(&channel, move |mut cx| match result {
                Ok(_) => Ok(cx.undefined()),
                Err(e) => cx.throw_error(format!("Failed to start network: {}", e)),
            });
        });

        Ok(promise)
    }

    fn stop(mut cx: FunctionContext) -> JsResult<JsPromise> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let network = Arc::clone(&this.inner);
        let channel = this.channel.clone();
        let (deferred, promise) = cx.promise();

        runtime::spawn(async move {
            {
                let net = network.read();
                net.shutdown();
            }

            deferred.settle_with(&channel, move |mut cx| Ok(cx.undefined()));
        });

        Ok(promise)
    }

    fn shutdown(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let network = this.inner.read();
        network.shutdown();
        Ok(cx.undefined())
    }

    fn add_node(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let id = cx.argument::<JsString>(1)?.value(&mut cx);
        let process = cx.argument::<JsString>(2)?.value(&mut cx);
        let metadata = cx
            .argument_opt(3)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .and_then(|v| {
                if let JsonValue::Object(map) = v {
                    Some(map.into_iter().collect())
                } else {
                    None
                }
            });

        let result = {
            let mut network = this.inner.write();
            network.add_node(&id, &process, metadata)
        };

        match result {
            Ok(_) => Ok(cx.undefined()),
            Err(e) => cx.throw_error(format!("Failed to add node: {}", e)),
        }
    }

    fn add_connection(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let connection_js = cx.argument::<JsObject>(1)?;

        // Parse connection object
        let from_js: Handle<JsValue> = connection_js.get(&mut cx, "from")?;
        let to_js: Handle<JsValue> = connection_js.get(&mut cx, "to")?;

        if let (Ok(from_obj), Ok(to_obj)) = (
            from_js.downcast::<JsObject, _>(&mut cx),
            to_js.downcast::<JsObject, _>(&mut cx),
        ) {
            let from_actor: Handle<JsValue> = from_obj.get(&mut cx, "actor")?;
            let from_port: Handle<JsValue> = from_obj.get(&mut cx, "port")?;
            let to_actor: Handle<JsValue> = to_obj.get(&mut cx, "actor")?;
            let to_port: Handle<JsValue> = to_obj.get(&mut cx, "port")?;

            let from_actor_str = from_actor
                .downcast::<JsString, _>(&mut cx)
                .expect("Expected string")
                .value(&mut cx);
            let from_port_str = from_port
                .downcast::<JsString, _>(&mut cx)
                .expect("Expected string")
                .value(&mut cx);
            let to_actor_str = to_actor
                .downcast::<JsString, _>(&mut cx)
                .expect("Expected string")
                .value(&mut cx);
            let to_port_str = to_port
                .downcast::<JsString, _>(&mut cx)
                .expect("Expected string")
                .value(&mut cx);

            let connector = Connector {
                from: ConnectionPoint {
                    actor: from_actor_str,
                    port: from_port_str,
                    initial_data: None,
                },
                to: ConnectionPoint {
                    actor: to_actor_str,
                    port: to_port_str,
                    initial_data: None,
                },
            };

            {
                let mut network = this.inner.write();
                network.add_connection(connector);
            }
        }

        Ok(cx.undefined())
    }

    fn add_initial(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let initial_js = cx.argument::<JsObject>(1)?;

        let to_js: Handle<JsValue> = initial_js.get(&mut cx, "to")?;
        if let Ok(to_obj) = to_js.downcast::<JsObject, _>(&mut cx) {
            let actor: Handle<JsValue> = to_obj.get(&mut cx, "actor")?;
            let port: Handle<JsValue> = to_obj.get(&mut cx, "port")?;
            let initial_data: Handle<JsValue> = to_obj.get(&mut cx, "initial_data")?;

            let actor_str = actor
                .downcast::<JsString, _>(&mut cx)
                .expect("Expected string")
                .value(&mut cx);
            let port_str = port
                .downcast::<JsString, _>(&mut cx)
                .expect("Expected string")
                .value(&mut cx);
            let data_json = js_to_json_value(&mut cx, initial_data)?;

            let initial_packet = InitialPacket {
                to: ConnectionPoint {
                    actor: actor_str,
                    port: port_str,
                    initial_data: Some(Message::from(data_json)),
                },
            };

            {
                let mut network = this.inner.write();
                network.add_initial(initial_packet);
            }
        }

        Ok(cx.undefined())
    }

    fn get_state(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;

        let state = cx.empty_object();

        // Add network status (simplified)
        let is_running = cx.boolean(true); // TODO: Get actual running state
        state.set(&mut cx, "running", is_running)?;

        let node_count = cx.number(0.0); // TODO: Get actual node count
        state.set(&mut cx, "nodeCount", node_count)?;

        let connection_count = cx.number(0.0); // TODO: Get actual connection count
        state.set(&mut cx, "connectionCount", connection_count)?;

        Ok(state.upcast())
    }

    fn is_running(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        let this = cx.argument::<JsBox<NodeNetwork>>(0)?;
        let size = this.inner.read().get_active_actors().len();

        Ok(cx.boolean(size > 0))
    }
}

/// GraphNetwork implementation
pub struct NodeGraphNetwork {
    network: Arc<StdMutex<Network>>,
    graph: Arc<ParkingRwLock<Graph>>,
    channel: Channel,
}

impl Finalize for NodeGraphNetwork {}

impl NodeGraphNetwork {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphNetwork>> {
        let graph_js = cx.argument::<JsValue>(0)?;
        let config_js = cx.argument_opt(1);

        // Parse the graph
        let graph_json = js_to_json_value(&mut cx, graph_js)?;
        let graph_export: GraphExport = match serde_json::from_value(graph_json) {
            Ok(export) => export,
            Err(e) => return cx.throw_error(format!("Failed to parse graph: {}", e)),
        };

        let graph = Graph::load(graph_export, None);

        // Parse the config
        let config = if let Some(config_value) = config_js {
            if let Ok(config_obj) = config_value.downcast::<JsObject, _>(&mut cx) {
                let config_json = js_to_json_value(&mut cx, config_obj.upcast())?;
                serde_json::from_value(config_json).unwrap_or_default()
            } else {
                NetworkConfig::default()
            }
        } else {
            NetworkConfig::default()
        };

        // Create network with the graph
        let network = Network::with_graph(config, &graph);
        // let network = match Arc::try_unwrap(network) {
        //     Ok(mutex) => Arc::new(ParkingRwLock::new(mutex.into_inner().unwrap())),
        //     Err(_) => return cx.throw_error("Failed to create network")
        // };

        let channel = cx.channel();

        Ok(cx.boxed(NodeGraphNetwork {
            network,
            graph: Arc::new(ParkingRwLock::new(graph)),
            channel,
        }))
    }

    fn register_actor(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraphNetwork>>(0)?;
        let name = cx.argument::<JsString>(1)?.value(&mut cx);
        let actor_js = cx.argument::<JsObject>(2)?;

        // Extract actor properties
        let inports = extract_ports(&mut cx, &actor_js, "inports")?;
        let outports = extract_ports(&mut cx, &actor_js, "outports")?;
        let config = extract_config(&mut cx, &actor_js)?;

        // Create JavaScript actor bridge
        let js_actor = Arc::new(RwLock::new(actor_js.root(&mut cx)));
        let js_actor_bridge =
            JavaScriptActor::new(js_actor, config, inports, outports, this.channel.clone());

        // Register with the network
        {
            let mut network = this.network.lock().unwrap();
            if let Err(e) = network.register_actor(&name, js_actor_bridge) {
                return cx.throw_error(format!("Failed to register actor: {}", e));
            }
        }

        Ok(cx.undefined())
    }

    fn start(mut cx: FunctionContext) -> JsResult<JsPromise> {
        let this = cx.argument::<JsBox<NodeGraphNetwork>>(0)?;
        let network = Arc::clone(&this.network);
        let channel = this.channel.clone();
        let (deferred, promise) = cx.promise();

        runtime::spawn(async move {
            let mut net = network.lock().unwrap();
            let result = net.start();
            deferred.settle_with(&channel, move |mut cx| match result {
                Ok(_) => Ok(cx.undefined()),
                Err(e) => cx.throw_error(format!("Failed to start network: {}", e)),
            });
        });

        Ok(promise)
    }

    fn shutdown(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraphNetwork>>(0)?;
        let network = this.network.lock().unwrap();
        network.shutdown();
        drop(network);
        Ok(cx.undefined())
    }

    fn get_graph(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.argument::<JsBox<NodeGraphNetwork>>(0)?;
        let graph = this.graph.read();

        let exported = graph.export();
        match serde_json::to_value(&exported) {
            Ok(json_value) => json_value_to_js(&mut cx, &json_value),
            Err(e) => cx.throw_error(format!("Failed to export graph: {}", e)),
        }
    }
}

fn extract_ports(
    cx: &mut FunctionContext,
    actor: &JsObject,
    port_type: &str,
) -> NeonResult<Vec<String>> {
    let ports_val: Handle<JsValue> = actor.get(cx, port_type)?;
    if let Ok(ports_array) = ports_val.downcast::<JsArray, _>(cx) {
        let length = ports_array.len(cx);
        let mut ports = Vec::new();

        for i in 0..length {
            let port: Handle<JsValue> = ports_array.get(cx, i)?;
            if let Ok(port_str) = port.downcast::<JsString, _>(cx) {
                ports.push(port_str.value(cx));
            }
        }

        Ok(ports)
    } else {
        Ok(Vec::new())
    }
}

fn extract_config(
    cx: &mut FunctionContext,
    actor: &JsObject,
) -> NeonResult<HashMap<String, JsonValue>> {
    let config_val: Handle<JsValue> = actor
        .get(cx, "config")
        .unwrap_or_else(|_| cx.undefined().upcast());

    if let Ok(config_obj) = config_val.downcast::<JsObject, _>(cx) {
        let config_json = js_to_json_value(cx, config_obj.upcast())?;
        if let JsonValue::Object(map) = config_json {
            Ok(map.into_iter().collect())
        } else {
            Ok(HashMap::new())
        }
    } else {
        Ok(HashMap::new())
    }
}

pub fn create_javascript_state(mut cx: FunctionContext) -> JsResult<JsFunction> {
    // Add state management prototype for JavaScriptActorState
    let state_constructor = JsFunction::new(&mut cx, |mut cx| -> JsResult<JsUndefined> {
        cx.throw_error("JavaScriptActorState cannot be constructed directly")
    })?;

    let state_prototype = state_constructor.get::<JsObject, _, _>(&mut cx, "prototype")?;

    let get_fn = JsFunction::new(&mut cx, JavaScriptActorState::get)?;
    state_prototype.set(&mut cx, "get", get_fn)?;

    let set_fn = JsFunction::new(&mut cx, JavaScriptActorState::set)?;
    state_prototype.set(&mut cx, "set", set_fn)?;

    let has_fn = JsFunction::new(&mut cx, JavaScriptActorState::has)?;
    state_prototype.set(&mut cx, "has", has_fn)?;

    let remove_fn = JsFunction::new(&mut cx, JavaScriptActorState::remove)?;
    state_prototype.set(&mut cx, "remove", remove_fn)?;

    let clear_fn = JsFunction::new(&mut cx, JavaScriptActorState::clear)?;
    state_prototype.set(&mut cx, "clear", clear_fn)?;

    let size_fn = JsFunction::new(&mut cx, JavaScriptActorState::size)?;
    state_prototype.set(&mut cx, "size", size_fn)?;

    let get_all_fn = JsFunction::new(&mut cx, JavaScriptActorState::get_all)?;
    state_prototype.set(&mut cx, "getAll", get_all_fn)?;

    let set_all_fn = JsFunction::new(&mut cx, JavaScriptActorState::set_all)?;
    state_prototype.set(&mut cx, "setAll", set_all_fn)?;

    let keys_fn = JsFunction::new(&mut cx, JavaScriptActorState::keys)?;
    state_prototype.set(&mut cx, "keys", keys_fn)?;

    Ok(state_constructor)
}

/// Create Network constructor function
pub fn create_network(mut cx: FunctionContext) -> JsResult<JsFunction> {
    let constructor = JsFunction::new(&mut cx, NodeNetwork::new)?;

    // Add instance methods to prototype
    let prototype = constructor.get::<JsObject, _, _>(&mut cx, "prototype")?;

    let register_actor_fn = JsFunction::new(&mut cx, NodeNetwork::register_actor)?;
    prototype.set(&mut cx, "registerActor", register_actor_fn)?;

    let start_fn = JsFunction::new(&mut cx, NodeNetwork::start)?;
    prototype.set(&mut cx, "start", start_fn)?;

    let stop_fn = JsFunction::new(&mut cx, NodeNetwork::stop)?;
    prototype.set(&mut cx, "stop", stop_fn)?;

    let shutdown_fn = JsFunction::new(&mut cx, NodeNetwork::shutdown)?;
    prototype.set(&mut cx, "shutdown", shutdown_fn)?;

    let add_node_fn = JsFunction::new(&mut cx, NodeNetwork::add_node)?;
    prototype.set(&mut cx, "addNode", add_node_fn)?;

    let add_connection_fn = JsFunction::new(&mut cx, NodeNetwork::add_connection)?;
    prototype.set(&mut cx, "addConnection", add_connection_fn)?;

    let add_initial_fn = JsFunction::new(&mut cx, NodeNetwork::add_initial)?;
    prototype.set(&mut cx, "addInitial", add_initial_fn)?;

    let get_state_fn = JsFunction::new(&mut cx, NodeNetwork::get_state)?;
    prototype.set(&mut cx, "getState", get_state_fn)?;

    let is_running_fn = JsFunction::new(&mut cx, NodeNetwork::is_running)?;
    prototype.set(&mut cx, "isRunning", is_running_fn)?;

    Ok(constructor)
}

/// Create GraphNetwork constructor function
pub fn create_graph_network(mut cx: FunctionContext) -> JsResult<JsFunction> {
    let constructor = JsFunction::new(&mut cx, NodeGraphNetwork::new)?;

    // Add instance methods to prototype
    let prototype = constructor.get::<JsObject, _, _>(&mut cx, "prototype")?;

    let register_actor_fn = JsFunction::new(&mut cx, NodeGraphNetwork::register_actor)?;
    prototype.set(&mut cx, "registerActor", register_actor_fn)?;

    let start_fn = JsFunction::new(&mut cx, NodeGraphNetwork::start)?;
    prototype.set(&mut cx, "start", start_fn)?;

    let shutdown_fn = JsFunction::new(&mut cx, NodeGraphNetwork::shutdown)?;
    prototype.set(&mut cx, "shutdown", shutdown_fn)?;

    let get_graph_fn = JsFunction::new(&mut cx, NodeGraphNetwork::get_graph)?;
    prototype.set(&mut cx, "getGraph", get_graph_fn)?;

    Ok(constructor)
}
