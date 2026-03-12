use parking_lot::Mutex;
use reflow_tracing_protocol::client::TracingIntegration;

use crate::{
    actor::{Actor, ActorConfig, ActorContext, ActorLoad, MemoryState, Port},
    bridge::NetworkBridge,
    message::Message,
};
use std::{collections::HashMap, sync::Arc, time::Duration};

/// Default timeout for forwarding messages to remote actors.
const PROXY_FORWARD_TIMEOUT: Duration = Duration::from_secs(30);

/// Proxy actor that represents a remote actor in the local network.
///
/// When a message arrives on any inport, the proxy forwards it to the remote actor
/// via the network bridge. The result map includes either:
/// - `"status"` → `"forwarded"` on success
/// - `"error"` → error description on failure (timeout, connection loss, etc.)
pub struct RemoteActorProxy {
    proxy_id: String,
    remote_network_id: String,
    remote_actor_id: String,
    bridge: Arc<NetworkBridge>,
    inports: Port,
    outports: Port,
    load: Arc<ActorLoad>,
}

impl RemoteActorProxy {
    pub fn new(
        remote_network_id: String,
        remote_actor_id: String,
        bridge: Arc<NetworkBridge>,
    ) -> Self {
        let proxy_id = format!("{}@{}", remote_actor_id, remote_network_id);
        RemoteActorProxy {
            proxy_id,
            remote_network_id,
            remote_actor_id,
            bridge,
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ActorLoad::new(0)),
        }
    }
}

impl Actor for RemoteActorProxy {
    fn get_behavior(&self) -> crate::actor::ActorBehavior {
        let bridge = self.bridge.clone();
        let remote_network_id = self.remote_network_id.clone();
        let remote_actor_id = self.remote_actor_id.clone();
        let proxy_id = self.proxy_id.clone();

        Box::new(move |context| {
            let payload = context.get_payload().clone();
            let bridge = bridge.clone();
            let remote_network_id = remote_network_id.clone();
            let remote_actor_id = remote_actor_id.clone();
            let proxy_id = proxy_id.clone();

            Box::pin(async move {
                let mut result = HashMap::new();
                let mut had_error = false;

                // Forward all input messages to remote actor
                for (port, message) in payload.iter() {
                    tracing::info!(
                        "[PROXY {}] Forwarding to {}::{} on port {}",
                        proxy_id,
                        remote_network_id,
                        remote_actor_id,
                        port
                    );

                    let send_future = bridge.send_remote_message(
                        &remote_network_id,
                        &remote_actor_id,
                        port,
                        message.clone(),
                        Some(&proxy_id),
                    );

                    // Wrap the send with a timeout to prevent indefinite hangs
                    match tokio::time::timeout(PROXY_FORWARD_TIMEOUT, send_future).await {
                        Ok(Ok(_)) => {
                            tracing::info!(
                                "[PROXY {}] Successfully forwarded to remote actor",
                                proxy_id
                            );
                        }
                        Ok(Err(e)) => {
                            tracing::error!(
                                "[PROXY {}] Failed to forward to remote actor: {}",
                                proxy_id,
                                e
                            );
                            had_error = true;
                            result.insert(
                                "error".to_string(),
                                Message::String(Arc::new(format!(
                                    "proxy:{}: forward failed: {}",
                                    proxy_id, e
                                ))),
                            );
                        }
                        Err(_) => {
                            tracing::error!(
                                "[PROXY {}] Timed out forwarding to {}::{} ({}s)",
                                proxy_id,
                                remote_network_id,
                                remote_actor_id,
                                PROXY_FORWARD_TIMEOUT.as_secs()
                            );
                            had_error = true;
                            result.insert(
                                "error".to_string(),
                                Message::String(Arc::new(format!(
                                    "proxy:{}: forward timed out after {}s",
                                    proxy_id,
                                    PROXY_FORWARD_TIMEOUT.as_secs()
                                ))),
                            );
                        }
                    }
                }

                if !had_error {
                    result.insert(
                        "status".to_string(),
                        Message::String(Arc::new("forwarded".to_string())),
                    );
                }

                Ok(result)
            })
        })
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn load_count(&self) -> Arc<ActorLoad> {
        self.load.clone()
    }

    fn create_process(
        &self,
        config: ActorConfig,
        _tracing_integration: Option<TracingIntegration>,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        use futures::StreamExt;

        let behavior = self.get_behavior();
        let (_, receiver) = self.get_inports();
        let outports = self.get_outports();
        let load = self.load_count();

        Box::pin(async move {
            loop {
                if let Some(packet) = receiver.clone().stream().next().await {
                    let context = ActorContext::new(
                        packet,
                        outports.clone(),
                        Arc::new(Mutex::new(MemoryState::default())),
                        config.clone(),
                        load.clone(),
                    );

                    match behavior(context).await {
                        Ok(result) if !result.is_empty() => {
                            outports
                                .0
                                .send(result)
                                .expect("Expected to send message via outport");
                            load.reset();
                        }
                        Err(e) => {
                            tracing::error!("[PROXY] Behavior error: {}", e);
                            // Propagate error as output so downstream actors are notified
                            let mut error_result = HashMap::new();
                            error_result.insert(
                                "error".to_string(),
                                Message::String(Arc::new(format!("proxy behavior error: {}", e))),
                            );
                            let _ = outports.0.send(error_result);
                            load.reset();
                        }
                        _ => {}
                    }
                }
            }
        })
    }
}
