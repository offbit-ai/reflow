use std::{collections::HashMap, pin::Pin};

use crate::{message::Message, network::Network};
#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Serialize, Deserialize, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct ConnectionPoint {
    pub actor: String,
    pub port: String,
    pub initial_data: Option<Message>,
}

impl ConnectionPoint {
    pub fn new(actor: &str, port: &str, initial_data: Option<Message>) -> ConnectionPoint {
        ConnectionPoint {
            actor: actor.to_owned(),
            port: port.to_owned(),
            initial_data,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct Connector {
    pub from: ConnectionPoint,
    pub to: ConnectionPoint,
}

impl Connector {
    pub fn new(from: ConnectionPoint, to: ConnectionPoint) -> Self {
        Connector { from, to }
    }
}

impl Connector {
    pub async fn init(&self, network: &Network) {
        use futures::{Stream, StreamExt, task::Poll};

        use crate::network::FlowStub;
        use crate::network::NetworkEvent;
        let network_event_emitter = network.network_event_emitter.clone();

        let from_process = network
            .nodes
            .get(&self.from.actor.to_owned())
            .expect("Expected to get actor process from node");

        let to_process = network
            .nodes
            .get(&self.to.actor.to_owned())
            .expect("Expected to get actor process from connected node");

        let from_actor = network.initialized_actors.get(&from_process.id).expect(&format!("Expected to find intitialized Actor for id {}", from_process.id));
        let from_actor_load_count = from_actor.load_count();
        let from_actor_id = self.from.actor.clone();

        let to_actor = network.initialized_actors.get(&to_process.id).expect(&format!("Expected to find intitialized Actor for id {}", from_process.id));

        let to_actor_id = self.to.actor.clone();

        let to_port = self.to.port.clone();

        let _from_port = self.from.port.clone();

        let out_ports = from_actor.get_outports();
        let in_ports = to_actor.get_inports();

        let mut routine = Box::pin(async move {
            while let Some(mut outport_packet) = out_ports.1.clone().stream().next().await {
                let _from_port = _from_port.clone();
                let to_port = to_port.clone();
                let from_actor_id = from_actor_id.clone();
                let to_actor_id = to_actor_id.clone();

                let msg = outport_packet
                    .remove(&_from_port)
                    .unwrap_or_else(|| Message::Optional(None));
                in_ports
                    .clone()
                    .0
                    .send_async(HashMap::from_iter([(
                        to_port.clone().to_owned(),
                        msg.clone(),
                    )]))
                    .await
                    .expect(
                        format!(
                            "Expected to send message from Actor '{}' to Actor '{}'",
                            &from_actor_id,
                            &to_actor_id
                        )
                        .as_str(),
                    );
                from_actor_load_count.clone().lock().dec();

                #[cfg(feature = "flowtrace")]
                {
                    // Send flow event
                    let (network_sender, _) = network_event_emitter.clone();
                    let _ = network_sender.clone().send(NetworkEvent::FlowTrace {
                        from: FlowStub {
                            actor_id: from_actor_id,
                            port: _from_port,
                            data: Some(json!(msg)),
                        },
                        to: FlowStub {
                            actor_id: to_actor_id,
                            port: to_port,
                            data: None,
                        },
                    });
                }
            }
        });

        // Start a loop to recieve messages from the first and send to second actor
        #[cfg(not(target_arch = "wasm32"))]
        let _ = tokio::spawn(async move { (&mut routine).await });
        // network.thread_pool.lock().unwrap().spawn(routine);

        #[cfg(target_arch = "wasm32")]
        spawn_local(routine);
    }
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct InitialPacket {
    pub to: ConnectionPoint,
}
