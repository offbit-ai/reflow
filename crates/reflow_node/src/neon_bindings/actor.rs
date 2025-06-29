//! Actor-related Neon bindings
//!
//! Provides Node.js bindings for Actor functionality

use crate::neon_bindings::utils::*;
use crate::runtime;
use neon::prelude::*;
use parking_lot::{lock_api::RwLock, Mutex, RwLock as ParkingRwLock};
use reflow_network::{
    actor::{
        Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, ActorState, MemoryState, Port,
    },
    connector::{ConnectionPoint, Connector, InitialPacket},
    graph::{types::GraphExport, Graph},
    message::{EncodableValue, Message},
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value as JsonValue};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// JavaScript Actor Bridge - implements Actor trait for JS objects
pub struct JavaScriptActor {
    /// JavaScript actor object (constructor function or instance)
    js_actor: Arc<ParkingRwLock<Root<JsObject>>>,
    /// Actor configuration
    config: HashMap<String, JsonValue>,
    /// Input ports
    inports: Vec<String>,
    inports_channel: Port,

    /// Output ports  
    outports: Vec<String>,
    outports_channel: Port,
    /// Shared state
    state: Arc<Mutex<MemoryState>>,
    /// Load counter
    load: Arc<Mutex<ActorLoad>>,
    /// Event channel for Neon callbacks
    channel: Channel,
}

impl JavaScriptActor {
    pub fn new(
        js_actor: Arc<ParkingRwLock<Root<JsObject>>>,
        config: HashMap<String, JsonValue>,
        inports: Vec<String>,
        outports: Vec<String>,
        channel: Channel,
    ) -> Self {
        Self {
            config: config.clone(),
            inports,
            inports_channel: flume::unbounded(),
            outports_channel: flume::unbounded(),
            outports,
            state: Arc::new(Mutex::new(MemoryState::default())),
            load: Arc::new(Mutex::new(ActorLoad::new(0))),
            channel: channel.clone(),
            js_actor: js_actor.clone(),
        }
    }
}

impl Actor for JavaScriptActor {
    fn get_behavior(&self) -> reflow_network::actor::ActorBehavior {
        let js_actor = self.js_actor.clone();
        let channel = self.channel.clone();
        let state = Arc::clone(&self.state);

        Box::new(move |context| {
            let js_actor = js_actor.clone();
            let channel = channel.clone();
            let state = Arc::clone(&state);

            Box::pin(async move {
                // Create a promise to handle the async JS call
                let (sender, receiver) = flume::unbounded();

                // Call JavaScript actor on the main thread
                channel.send(move |mut cx| {
                    let result = call_js_actor(&mut cx, js_actor, &context, &state).map_or(
                        Err(anyhow::Error::msg("Failed to call JavaScript actor")),
                        |outputs| Ok(outputs),
                    );
                    let _ = sender.send(result);
                    Ok(())
                });

                // Wait for the result
                match receiver.recv_async().await {
                    Ok(Ok(outputs)) => Ok(outputs),
                    Ok(Err(e)) => Err(anyhow::Error::msg(format!("JavaScript actor error: {}", e))),
                    Err(_) => Err(anyhow::Error::msg("JavaScript actor call failed")),
                }
            })
        })
    }

    fn get_outports(&self) -> Port {
        self.outports_channel.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports_channel.clone()
    }

    fn create_process(
        &self,
        mut config: ActorConfig,
    ) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        use futures_util::StreamExt;

        let behavior = self.get_behavior();
        let inports = self.inports_channel.clone();
        let outports = self.outports_channel.clone();
        let state = Arc::clone(&self.state);
        let load = Arc::clone(&self.load);

        config.config.extend(self.config.clone().into_iter());

        let inports_size = self.inports.len();
        let inport_keys = self.inports.clone();

        let await_all_inports = config
            .config
            .get("await_all_inports")
            .unwrap_or(&json!(false))
            .as_bool()
            .unwrap();

        Box::pin(async move {
            let mut all_inports = std::collections::HashMap::new();
            loop {
                if let Some(packet) = inports.1.clone().stream().next().await {
                    // Increment load counter
                    load.lock().inc();

                    if await_all_inports {
                        if all_inports.keys().len() < inports_size {
                            all_inports.extend(
                                packet
                                    .iter()
                                    .filter(|(k, _)| inport_keys.contains(k))
                                    .map(|(k, v)| (k.clone(), v.clone())),
                            );
                            if all_inports.keys().len() == inports_size {
                                // Run the behavior function
                                let context = ActorContext::new(
                                    all_inports.clone(),
                                    outports.clone(),
                                    state.clone(),
                                    config.clone(),
                                    load.clone(),
                                );

                                if let Ok(result) = behavior(context).await {
                                    if !result.is_empty() {
                                        let _ = outports
                                            .0
                                            .send(result)
                                            .expect("Expected to send message via outport");
                                        load.lock().reset();
                                    }
                                }
                            }
                            continue;
                        }
                    }

                    if !await_all_inports {
                        // Run the behavior function
                        let context = ActorContext::new(
                            packet,
                            outports.clone(),
                            state.clone(),
                            config.clone(),
                            load.clone(),
                        );

                        if let Ok(result) = behavior(context).await {
                            if !result.is_empty() {
                                let _ = outports
                                    .0
                                    .send(result)
                                    .expect("Expected to send message via outport");
                                load.lock().reset();
                            }
                        }
                    }
                }
            }
        })
    }

    fn shutdown(&self) {
        // Cleanup any resources
    }

    fn load_count(&self) -> Arc<Mutex<ActorLoad>> {
        Arc::clone(&self.load)
    }
}

/// ActorRunContext - the context object passed to JavaScript actors
pub struct ActorRunContext {
    input: JsonValue,
    state: JavaScriptActorState,
    config: JsonValue,
    outputs: Arc<Mutex<HashMap<String, JsonValue>>>,
}

/// JavaScript-accessible state object
pub struct JavaScriptActorState {
    inner: Arc<Mutex<MemoryState>>,
}

impl Finalize for JavaScriptActorState {}

impl JavaScriptActorState {
    pub fn new(state: Arc<Mutex<MemoryState>>) -> Self {
        Self { inner: state }
    }

    pub fn get(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let key = cx.argument::<JsString>(0)?.value(&mut cx);

        let state = this.inner.lock();
        match state.get(&key) {
            Some(value) => json_value_to_js(&mut cx, value),
            None => Ok(cx.undefined().upcast()),
        }
    }

    pub fn set(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let key = cx.argument::<JsString>(0)?.value(&mut cx);
        let value = cx.argument::<JsValue>(1)?;

        let value_json = js_to_json_value(&mut cx, value)?;
        let mut state = this.inner.lock();
        state.insert(&key, value_json);

        Ok(cx.undefined())
    }

    pub fn has(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let key = cx.argument::<JsString>(0)?.value(&mut cx);

        let state = this.inner.lock();
        Ok(cx.boolean(state.has_key(&key)))
    }

    pub fn remove(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let key = cx.argument::<JsString>(0)?.value(&mut cx);

        let mut state = this.inner.lock();
        state.remove(&key);
        Ok(cx.boolean(true))
    }

    pub fn clear(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let mut state = this.inner.lock();
        state.clear();
        Ok(cx.undefined())
    }

    pub fn size(mut cx: FunctionContext) -> JsResult<JsNumber> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let state = this.inner.lock();
        Ok(cx.number(state.len() as f64))
    }

    pub fn get_all(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let state = this.inner.lock();
        let all_data = serde_json::json!(state.0);
        json_value_to_js(&mut cx, &all_data)
    }

    pub fn set_all(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let data = cx.argument::<JsValue>(0)?;

        let data_json = data
            .downcast::<JsObject, _>(&mut cx)
            .map_err(|_| cx.throw_error("Expected an object").unwrap())
            .and_then(|v| js_object_to_map(&mut cx, v))?;
        let mut state = this.inner.lock();
        (*state).0 = data_json;

        Ok(cx.undefined())
    }

    pub fn keys(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx.this::<JsBox<JavaScriptActorState>>()?;
        let state = this.inner.lock();
        let keys = state.0.keys();

        let js_array = cx.empty_array();
        for (i, key) in keys.enumerate() {
            let js_key = cx.string(key);
            js_array.set(&mut cx, i as u32, js_key)?;
        }

        Ok(js_array)
    }
}

/// Function to call JavaScript actor from Rust
fn call_js_actor(
    cx: &mut TaskContext,
    js_actor: Arc<ParkingRwLock<Root<JsObject>>>,
    context: &reflow_network::actor::ActorContext,
    state: &Arc<Mutex<MemoryState>>,
) -> Result<HashMap<String, Message>, String> {
    // Convert inputs to JavaScript
    let input_json = match serde_json::to_value(
        &context
            .payload
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect::<HashMap<String, JsonValue>>(),
    ) {
        Ok(json) => json,
        Err(e) => return Err(format!("Failed to serialize inputs: {}", e)),
    };

    let config_json = JsonValue::Object(context.config.as_hashmap().into_iter().collect());

    // Create state object for JavaScript
    let js_state = cx.boxed(JavaScriptActorState::new(Arc::clone(state)));

    // Create the output collection

    let outputs_clone = context.outports.clone();

    // Create send function
    let send_fn = JsFunction::new(cx, move |mut cx| {
        let outputs_data = cx.argument::<JsValue>(0)?;
        let outputs_json = js_to_json_value(&mut cx, outputs_data)?;
        let mut outputs_map = HashMap::<String, Message>::new();
        if let JsonValue::Object(obj) = outputs_json {
            for (port, data) in obj {
                outputs_map.insert(port, Message::from(data));
            }
        }
        match outputs_clone
            .0
            .send(outputs_map)
            .map_err(|e| format!("Failed to send outputs: {:?}", e))
        {
            Ok(_) => Ok(cx.undefined()),
            Err(e) => return cx.throw_error(format!("Send function error: {}", e)),
        }
    })
    .map_err(|e| format!("Send function creation error: {:?}", e))?;

    // Create context object
    let context_obj = cx.empty_object();

    // Set context properties
    let input_js = json_value_to_js(cx, &input_json)
        .map_err(|e| format!("Input conversion error: {:?}", e))?;
    context_obj
        .set(cx, "input", input_js)
        .map_err(|e| format!("Context input error: {:?}", e))?;

    context_obj
        .set(cx, "state", js_state)
        .map_err(|e| format!("Context state error: {:?}", e))?;

    let config_js = json_value_to_js(cx, &config_json)
        .map_err(|e| format!("Config conversion error: {:?}", e))?;
    context_obj
        .set(cx, "config", config_js)
        .map_err(|e| format!("Context config error: {:?}", e))?;

    context_obj
        .set(cx, "send", send_fn)
        .map_err(|e| format!("Context send error: {:?}", e))?;

    // Get the JavaScript actor object
    let binding = js_actor.clone();
    let js_actor_clone = binding.read();
    let actor = js_actor_clone.to_inner(cx);

    // Call the run method on the JavaScript actor
    let run_method: Handle<JsValue> = actor
        .get(cx, "run")
        .map_err(|e| format!("No run method: {:?}", e))?;

    // let outputs_clone = context.outports.clone();

    if let Ok(run_fn) = run_method.downcast::<JsFunction, _>(cx) {
        let this = actor.upcast::<JsValue>();
        let args = vec![context_obj.upcast::<JsValue>()];

        // Call the run function
        let result = run_fn
            .call(cx, this, args)
            .map_err(|e| format!("Actor run call failed: {:?}", e))?;

        // Check if the result is a JsObject
        if !result.is_a::<JsNull, _>(cx) || !result.is_a::<JsUndefined, _>(cx) {
            // let null = cx.null();
            let result_obj = result
                .downcast::<JsObject, _>(cx)
                .map_err(|e| format!("Actor run result is not an object: {:?}", e))?;
            let result_map = js_object_to_map(cx, result_obj)
                .map_err(|e| format!("Failed to convert actor run result to map: {:?}", e))?
                .iter()
                .map(|(k, v)| (k.clone(), Message::from(v.clone())))
                .collect::<HashMap<String, Message>>();

            return Ok(result_map);
        }

        Ok(HashMap::new())
    } else {
        Err("Actor run method is not a function".to_string())
    }
}

// /// Node.js Actor wrapper
// pub struct NodeActor {
//     inner: Arc<ParkingRwLock<Box<dyn Actor>>>,
//     config: HashMap<String, JsonValue>,
// }

// impl Finalize for NodeActor {}

// impl NodeActor {
//     fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeActor>> {
//         // Get actor configuration from arguments
//         let config:HashMap<String, JsonValue> = cx
//             .argument_opt(0)
//             .and_then(|v| js_to_json_value(&mut cx, v).ok())
//             .and_then(|v| {
//                 if let JsonValue::Object(map) = v {
//                     Some(map.into_iter().collect())
//                 } else {
//                     None
//                 }
//             })
//             .unwrap_or_default();

//         // Create a basic memory actor for now
//         let actor = Box::new(BasicMemoryActor::new(config.clone()));

//         Ok(cx.boxed(NodeActor {
//             inner: Arc::new(ParkingRwLock::new(actor)),
//             config,
//         }))
//     }

//     fn get_config(mut cx: FunctionContext) -> JsResult<JsValue> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         json_value_to_js(&mut cx, &JsonValue::Object(
//             this.config.clone().into_iter().collect()
//         ))
//     }

//     fn set_config(mut cx: FunctionContext) -> JsResult<JsUndefined> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         let config_js = cx.argument::<JsValue>(0)?;

//         if let Ok(config_json) = js_to_json_value(&mut cx, config_js) {
//             if let JsonValue::Object(map) = config_json {
//                 let mut actor = this.inner.write();
//                 // Update the actor's configuration
//                 // Note: This would need to be implemented in the actual actor
//             }
//         }

//         Ok(cx.undefined())
//     }

//     fn get_state(mut cx: FunctionContext) -> JsResult<JsValue> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         let actor = this.inner.read();

//         // Get state from the actor
//         // For now, return an empty object
//         let state = JsonValue::Object(serde_json::Map::new());
//         json_value_to_js(&mut cx, &state)
//     }

//     fn set_state(mut cx: FunctionContext) -> JsResult<JsUndefined> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         let state_js = cx.argument::<JsValue>(0)?;

//         if let Ok(state_json) = js_to_json_value(&mut cx, state_js) {
//             let mut actor = this.inner.write();
//             // Update the actor's state
//             // Note: This would need to be implemented in the actual actor
//         }

//         Ok(cx.undefined())
//     }

//     fn get_inports(mut cx: FunctionContext) -> JsResult<JsArray> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         let actor = this.inner.read();

//         // Get inports from the actor
//         // For now, return an empty array
//         let inports = cx.empty_array();
//         Ok(inports)
//     }

//     fn get_outports(mut cx: FunctionContext) -> JsResult<JsArray> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         let actor = this.inner.read();

//         // Get outports from the actor
//         // For now, return an empty array
//         let outports = cx.empty_array();
//         Ok(outports)
//     }

//     fn run(mut cx: FunctionContext) -> JsResult<JsPromise> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         let input_js = cx.argument::<JsValue>(0)?;
//         let send_js = cx.argument::<JsFunction>(1)?;

//         // Convert input to Rust value
//         let input = js_to_json_value(&mut cx, input_js)?;

//         // Create a promise for the async operation
//         let (deferred, promise) = cx.promise();
//         let actor = Arc::clone(&this.inner);

//         // Execute asynchronously
//         runtime::spawn(async move {
//             // Process the input with the actor
//             // For now, just resolve with the input
//             deferred.settle_with(&cx.channel(), move |mut cx| {
//                 Ok(cx.undefined())
//             });
//         });

//         Ok(promise)
//     }

//     fn initialize(mut cx: FunctionContext) -> JsResult<JsPromise> {
//         let this = cx.this::<JsBox<NodeActor>>()?;

//         let (deferred, promise) = cx.promise();
//         let actor = Arc::clone(&this.inner);

//         runtime::spawn(async move {
//             // Initialize the actor
//             deferred.settle_with(&cx.channel(), move |mut cx| {
//                 Ok(cx.undefined())
//             });
//         });

//         Ok(promise)
//     }

//     fn shutdown(mut cx: FunctionContext) -> JsResult<JsUndefined> {
//         let this = cx.this::<JsBox<NodeActor>>()?;
//         let actor = this.inner.read();

//         // Shutdown the actor
//         actor.shutdown();

//         Ok(cx.undefined())
//     }
// }

// /// Basic memory actor implementation for Node.js bindings
// struct BasicMemoryActor {
//     config: HashMap<String, JsonValue>,
//     state: Arc<Mutex<MemoryState>>,
//     inports: Port,
//     outports: Port,
// }

// impl BasicMemoryActor {
//     fn new(config: HashMap<String, JsonValue>) -> Self {
//         Self {
//             config,
//             state: Arc::new(Mutex::new(MemoryState::default())),
//             inports: flume::unbounded(),
//             outports: flume::unbounded(),
//         }
//     }
// }

// impl Actor for BasicMemoryActor {
//     fn get_behavior(&self) -> reflow_network::actor::ActorBehavior {
//         Box::new(|_context| {
//             Box::pin(async move {
//                 // Basic actor behavior - just pass through the input
//                 Ok(HashMap::new())
//             })
//         })
//     }

//     fn get_outports(&self) -> Port {
//         // Return default outports
//        self.inports.clone()
//     }

//     fn get_inports(&self) -> Port {
//         // Return default inports
//        self.outports.clone()
//     }

//     fn create_process(
//         &self,
//         _config: ActorConfig,
//     ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static + Send>> {
//         Box::pin(async move {
//             // Basic process implementation
//         })
//     }

//     fn shutdown(&self) {
//         // Shutdown implementation
//     }
// }

// /// Create Actor constructor function
// pub fn create_actor(mut cx: FunctionContext) -> JsResult<JsFunction> {
//     // Create the constructor function
//     let constructor = JsFunction::new(&mut cx, NodeActor::new)?;

//     // Add methods to the prototype
//     let prototype = constructor.get::<JsObject, _, _>(&mut cx, "prototype")?;

//     let get_config_fn = JsFunction::new(&mut cx, NodeActor::get_config)?;
//     prototype.set(&mut cx, "getConfig", get_config_fn)?;

//     let set_config_fn = JsFunction::new(&mut cx, NodeActor::set_config)?;
//     prototype.set(&mut cx, "setConfig", set_config_fn)?;

//     let get_state_fn = JsFunction::new(&mut cx, NodeActor::get_state)?;
//     prototype.set(&mut cx, "getState", get_state_fn)?;

//     let set_state_fn = JsFunction::new(&mut cx, NodeActor::set_state)?;
//     prototype.set(&mut cx, "setState", set_state_fn)?;

//     let get_inports_fn = JsFunction::new(&mut cx, NodeActor::get_inports)?;
//     prototype.set(&mut cx, "getInports", get_inports_fn)?;

//     let get_outports_fn = JsFunction::new(&mut cx, NodeActor::get_outports)?;
//     prototype.set(&mut cx, "getOutports", get_outports_fn)?;

//     let run_fn = JsFunction::new(&mut cx, NodeActor::run)?;
//     prototype.set(&mut cx, "run", run_fn)?;

//     let initialize_fn = JsFunction::new(&mut cx, NodeActor::initialize)?;
//     prototype.set(&mut cx, "initialize", initialize_fn)?;

//     let shutdown_fn = JsFunction::new(&mut cx, NodeActor::shutdown)?;
//     prototype.set(&mut cx, "shutdown", shutdown_fn)?;

//     Ok(constructor)
// }
