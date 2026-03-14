use std::collections::HashMap;

use crate::{actor::message::Message, network::Network};
#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
use serde::{Deserialize, Serialize};
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
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

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
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
    /// Initialize this connector using the source actor's outport receiver directly.
    /// Used on WASM or as a fallback when broadcast fan-out is not available.
    pub fn init(&self, network: &Network) {
        use futures::StreamExt;

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

        let from_actor = network
            .initialized_actors
            .get(&from_process.id)
            .unwrap_or_else(|| {
                panic!(
                    "Expected to find intitialized Actor for id {}",
                    from_process.id
                )
            });
        let from_actor_load_count = from_actor.load_count();
        let from_actor_id = self.from.actor.clone();

        let to_actor = network
            .initialized_actors
            .get(&to_process.id)
            .unwrap_or_else(|| {
                panic!(
                    "Expected to find intitialized Actor for id {}",
                    from_process.id
                )
            });

        let to_actor_id = self.to.actor.clone();

        let to_port = self.to.port.clone();

        let _from_port = self.from.port.clone();

        let out_ports = from_actor.get_outports();
        let in_ports = to_actor.get_inports();

        // Clone tracing integration before moving into async block
        let tracing_integration = network.tracing_integration.clone();

        let routine = Box::pin(async move {
            while let Some(mut outport_packet) = out_ports.1.clone().stream().next().await {
                let _from_port = _from_port.clone();
                let to_port = to_port.clone();
                let from_actor_id = from_actor_id.clone();
                let to_actor_id = to_actor_id.clone();

                let msg = outport_packet
                    .remove(&_from_port)
                    .unwrap_or(Message::Optional(None));

                // Emit MessageSent event
                let value: serde_json::Value = msg.clone().into();
                let encodable = crate::message::EncodableValue::from(value);
                let timestamp = chrono::Utc::now().timestamp_millis() as u64;
                let _ = network_event_emitter.0.send(NetworkEvent::MessageSent {
                    from_actor: from_actor_id.clone(),
                    from_port: _from_port.clone(),
                    to_actor: to_actor_id.clone(),
                    to_port: to_port.clone(),
                    message: encodable,
                    timestamp,
                });

                in_ports
                    .clone()
                    .0
                    .send_async(HashMap::from_iter([(
                        to_port.clone().to_owned(),
                        msg.clone(),
                    )]))
                    .await
                    .unwrap_or_else(|_| {
                        panic!(
                            "Expected to send message from Actor '{}' to Actor '{}'",
                            &from_actor_id, &to_actor_id
                        )
                    });
                from_actor_load_count.dec();

                // Send tracing event if tracing is enabled
                if let Some(ref tracing) = tracing_integration {
                    let message_size = std::mem::size_of_val(&msg);
                    let _ = tracing
                        .trace_message_sent(
                            from_actor_id.clone(),
                            _from_port.clone(),
                            format!("{:?}", std::mem::discriminant(&msg)),
                            message_size,
                        )
                        .await;

                    // Trace the data flow between actors
                    let _ = tracing
                        .trace_data_flow(
                            from_actor_id.clone(),
                            _from_port.clone(),
                            to_actor_id.clone(),
                            to_port.clone(),
                            format!("{:?}", std::mem::discriminant(&msg)),
                            message_size,
                        )
                        .await;
                }
            }
        });

        // Start a loop to recieve messages from the first and send to second actor
        #[cfg(not(target_arch = "wasm32"))]
        tokio::spawn(routine);
        // network.thread_pool.lock().unwrap().spawn(routine);

        #[cfg(target_arch = "wasm32")]
        spawn_local(routine);
    }
}

/// Broadcast-based connector initialization for fan-out support.
/// A single forwarder task reads from the source actor's flume outport and
/// broadcasts to all downstream connectors via `tokio::sync::broadcast`,
/// ensuring every connector receives every message.
#[cfg(not(target_arch = "wasm32"))]
impl Connector {
    /// Initialize this connector using a broadcast receiver for fan-out support.
    /// All connectors sharing the same source actor receive every outport message.
    pub fn init_broadcast(
        &self,
        network: &Network,
        mut broadcast_rx: tokio::sync::broadcast::Receiver<
            std::sync::Arc<HashMap<String, Message>>,
        >,
    ) {
        use crate::network::NetworkEvent;
        let network_event_emitter = network.network_event_emitter.clone();

        let to_process = network
            .nodes
            .get(&self.to.actor.to_owned())
            .expect("Expected to get actor process from connected node");

        let to_actor = network
            .initialized_actors
            .get(&to_process.id)
            .unwrap_or_else(|| {
                panic!(
                    "Expected to find initialized Actor for id {}",
                    to_process.id
                )
            });

        let from_actor_id = self.from.actor.clone();
        let to_actor_id = self.to.actor.clone();
        let to_port = self.to.port.clone();
        let from_port = self.from.port.clone();
        let in_ports = to_actor.get_inports();
        let tracing_integration = network.tracing_integration.clone();

        let routine = Box::pin(async move {
            loop {
                match broadcast_rx.recv().await {
                    Ok(outport_packet) => {
                        // Arc<HashMap> — only clone the single Message we need
                        let msg = outport_packet
                            .get(&from_port)
                            .cloned()
                            .unwrap_or(Message::Optional(None));

                        // Capture tracing info from &msg before moving
                        let message_size = std::mem::size_of_val(&msg);
                        let msg_discriminant = format!("{:?}", std::mem::discriminant(&msg));

                        // Emit MessageSent event
                        let value: serde_json::Value = msg.clone().into();
                        let encodable = crate::message::EncodableValue::from(value);
                        let timestamp = chrono::Utc::now().timestamp_millis() as u64;
                        let _ = network_event_emitter.0.send(NetworkEvent::MessageSent {
                            from_actor: from_actor_id.clone(),
                            from_port: from_port.clone(),
                            to_actor: to_actor_id.clone(),
                            to_port: to_port.clone(),
                            message: encodable,
                            timestamp,
                        });

                        // StreamHandle messages pass through the normal inport
                        // channel. The actual data flows out-of-band via the
                        // StreamRegistry; the downstream actor takes the receiver
                        // via ActorContext::take_stream_receiver().

                        // Move msg into the inport send — no clone needed
                        in_ports
                            .clone()
                            .0
                            .send_async(HashMap::from([(to_port.clone(), msg)]))
                            .await
                            .unwrap_or_else(|_| {
                                panic!(
                                    "Expected to send message from Actor '{}' to Actor '{}'",
                                    &from_actor_id, &to_actor_id
                                )
                            });

                        // Send tracing event if tracing is enabled
                        if let Some(ref tracing) = tracing_integration {
                            let _ = tracing
                                .trace_message_sent(
                                    from_actor_id.clone(),
                                    from_port.clone(),
                                    msg_discriminant.clone(),
                                    message_size,
                                )
                                .await;

                            let _ = tracing
                                .trace_data_flow(
                                    from_actor_id.clone(),
                                    from_port.clone(),
                                    to_actor_id.clone(),
                                    to_port.clone(),
                                    msg_discriminant,
                                    message_size,
                                )
                                .await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "[CONNECTOR] Broadcast receiver lagged, missed {} messages ({}:{} → {}:{})",
                            n,
                            from_actor_id,
                            from_port,
                            to_actor_id,
                            to_port
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
        });

        tokio::spawn(routine);
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct InitialPacket {
    pub to: ConnectionPoint,
}
