use std::{
    any::Any, collections::HashMap, pin::Pin, rc::Rc, sync::Arc
};

#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
use parking_lot::Mutex;
#[cfg(not(target_arch = "wasm32"))]
use rayon::ThreadPool;
use serde_json::Value;
#[cfg(target_arch = "wasm32")]
use std::fmt::Debug;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::convert::FromWasmAbi;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::{message::Message, network::Network};

// #[cfg(not(target_arch = "wasm32"))]
pub type ActorBehavior = Box<
    dyn Fn(
            ActorPayload,
            Arc<Mutex<dyn ActorState>>,
            Port,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<HashMap<String, Message>, anyhow::Error>> + Send + 'static>>
        + Send
        + Sync
       
        + 'static,
>;

pub type ActorPayload = HashMap<String, Message>;
pub type ActorChannel = (
    flume::Sender<crate::message::Message>,
    flume::Receiver<crate::message::Message>,
);

// #[cfg(not(target_arch = "wasm32"))]
pub type Port = (
    flume::Sender<HashMap<String, crate::message::Message>>,
    flume::Receiver<HashMap<String, crate::message::Message>>,
);

// #[cfg(not(target_arch = "wasm32"))]
pub trait Actor: Send + Sync + 'static {
    /// Trait method to get actor's behavior
    fn get_behavior(&self) -> ActorBehavior;
    /// Access all output ports
    fn get_outports(&self) -> Port;
    /// Access all input ports
    fn get_inports(&self) -> Port;

    fn create_process(
        &self,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>>;
}

pub trait ActorState: Send + Sync + 'static {
    fn as_any(&self) -> &dyn Any;
    fn as_mut_any(&mut self) -> &mut dyn Any;
}

#[derive(Default, Clone)]
pub struct MemoryState(pub HashMap<String, Value>);
impl ActorState for MemoryState {
    fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn as_mut_any(&mut self) -> &mut dyn Any {
        self as &mut dyn Any
    }
}

impl MemoryState {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.0.get_mut(key)
    }

    pub fn insert(&mut self, key: &str, value: Value) {
        self.0.insert(key.to_string(), value);
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_class = Actor)]
    pub type ExternActor;

    #[wasm_bindgen(method, getter, structural)]
    pub fn inports(this: &ExternActor) -> Vec<String>;

    #[wasm_bindgen(method, getter, structural)]
    pub fn outports(this: &ExternActor) -> Vec<String>;

    #[wasm_bindgen(method, getter, structural)]
    pub fn state(this: &ExternActor) -> JsValue;

    #[wasm_bindgen(method, setter, structural)]
    pub fn set_state(this: &ExternActor, state: &JsValue);

    #[wasm_bindgen(method, getter, structural)]
    pub fn config(this: &ExternActor) -> JsValue;

    #[wasm_bindgen(method, structural)]
    pub fn run(this: &ExternActor, data: &JsValue, callback: &Closure<dyn FnMut(JsValue)>);

}

trait WasmActorState: ActorState {
    fn get_object(&self) -> HashMap<String, Value>;
    fn set_object(&mut self, state: HashMap<String, Value>);
}

impl WasmActorState for MemoryState {
    fn get_object(&self) -> HashMap<String, Value> {
        self.0.clone()
    }

    fn set_object(&mut self, state: HashMap<String, Value>) {
        self.0 = state;
    }
}

#[cfg(target_arch = "wasm32")]
pub struct WasmActor {
    inports: Port,
    outports: Port,
    inports_size: usize,
    outports_size: usize,
    load: Arc<Mutex<usize>>,
    state: Arc<Mutex<dyn ActorState>>,
    behavior: Arc<ActorBehavior>,
    config: HashMap<String, Value>,
}

#[cfg(target_arch = "wasm32")]
impl WasmActor {
    pub fn new(extern_actor: ExternActor) -> Self {
        use serde_json::json;

        // let inports = extern_actor
        //     .inports()
        //     .iter()
        //     .map(|port| (port.clone(), Message::default()))
        //     .map(|d| )
        //     .collect::<Vec<Port>>();
        // let outports = extern_actor
        //     .outports()
        //     .iter()
        //     .map(|port| (port.clone(), flume::unbounded::<HashMap<String, Message>>()))
        //     .collect::<Vec<Port>>();

        let inports = flume::unbounded();
        let outports = flume::unbounded();

        let state = if extern_actor.state().is_object() {
            MemoryState(
                extern_actor
                    .state()
                    .into_serde::<HashMap<String, Value>>()
                    .unwrap_or(Default::default()),
            )
        } else {
            MemoryState::default()
        };

        let actor = extern_actor.clone();


        Self {
            inports,
            outports,
            inports_size: extern_actor.inports().len(),
            outports_size: extern_actor.outports().len(),
            load: Arc::new(Mutex::new(0)),
            state: Arc::new(Mutex::new(state)),
            config:  extern_actor.config().into_serde::<HashMap<String, Value>>().unwrap(),
            behavior: Arc::new(Box::new(
                move |payload: ActorPayload,
                      state: Arc<Mutex<dyn ActorState>>,
                      outport_channels: Port| {
                    // let (port, data) = payload;
                    
                    
                    let inputs = JsValue::from_serde(&payload)?;
                    let outports = outport_channels.clone();
                    let closure = Closure::new(move |value: JsValue| {
                        let _messages = value
                            .into_serde::<HashMap<String, serde_json::Value>>()
                            .expect("Expected packet to be mapped");

                        let messages = _messages
                            .iter()
                            .map(|(port, val)| (port.to_owned(), Message::from(val.clone())))
                            .collect::<HashMap<String, Message>>();

                        Network::send_outport_msg(outports.clone(), messages)
                            .expect("Expected to send outport messages ");
                    });

                    if let Ok(state) = state.try_lock() {
                        if let Some(state_) = state.as_any().downcast_ref::<MemoryState>() {
                            actor.set_state(&JsValue::from_serde(&json!(state_.0))?);
                        }
                    }

                    actor.run(&inputs, &closure);

                    if let Ok(state) = state.try_lock().as_mut() {
                        if let Some(state_) = state.as_mut_any().downcast_mut::<MemoryState>() {
                            state_
                                .set_object(actor.state().into_serde::<HashMap<String, Value>>()?);
                        }
                    }

                    Ok([].into())
                },
            )),
        }
    }

    fn get_config(&self) -> HashMap<String, Value> {
       self.config.clone()
    }
    fn get_state(&self) -> Arc<Mutex<dyn ActorState>> {
        self.state.clone()
    }

    fn load_count(&self) -> usize {
        *self.load.lock().unwrap()
    }
}

#[cfg(target_arch = "wasm32")]
impl Actor for WasmActor {
    fn get_behavior(&self) -> ActorBehavior {
        let behavior = self.behavior.clone();
        Box::new(move |payload, state, port| {
            (*behavior)(payload, state, port)
        })
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn create_process(
        &self,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        use serde_json::json;
        use futures::StreamExt;

        let outports = self.outports.clone();
        let behavior = self.get_behavior();
        let actor_state = self.get_state();

        let inports_size = self.inports_size;

        let (_, receiver) = self.inports.clone();

        let config = self.get_config();
        
        let await_all_inports = config.get("await_all_inports").unwrap_or(&json!(false)).as_bool().unwrap();
        

        Box::pin(async move {
            let behavior_func = behavior;
            let mut all_inports = std::collections::HashMap::new();
            loop {
                if let Some(packet) = receiver.clone().stream().next().await {
                    if await_all_inports {
                        if all_inports.keys().len() < inports_size  {
                            all_inports.extend(packet.iter().map(|(k, v)| {(k.clone(), v.clone())}));
                            if all_inports.keys().len() == inports_size  {
                                // Run the behavior function
                                if let Ok(result) = (behavior_func)(all_inports.clone(), actor_state.clone(), outports.clone())
                                {
                                    if !result.is_empty() {
                                        let _ = outports.0.send(result)
                                            .expect("Expected to send message via outport");
                                    }
                                }
                            }
                            continue;
                        }
                    }

                    if !await_all_inports {
                        // Run the behavior function
                        if let Ok(result) = (behavior_func)(packet, actor_state.clone(), outports.clone())
                        {
                            if !result.is_empty() {
                                let _ = outports.0.send(result)
                                    .expect("Expected to send message via outport");
                            }
                        }
                    }
                }

                
            }
        })
    }
}

#[cfg(target_arch = "wasm32")]
impl Clone for ExternActor {
    fn clone(&self) -> Self {
        Self {
            obj: self.obj.clone(),
        }
    }
}
#[cfg(target_arch = "wasm32")]
impl Debug for ExternActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExternActor")
            .field("obj", &self.obj)
            .finish()
    }
}
#[cfg(target_arch = "wasm32")]
unsafe impl Send for ExternActor {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for ExternActor {}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
interface Actor {
    inports:Array<any>;
    outports:Array<any>;
    run(input:{data:any}, send:(message:any)=>void):void;
    get state():any;
    set state(value);
}
"#;
