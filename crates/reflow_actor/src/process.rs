use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use reflow_tracing_protocol::client::TracingIntegration;
use serde_json::Value;

use crate::message::{EncodableValue, Message};
use crate::{ActorBehavior, ActorConfig, ActorContext, ActorLoad, ActorState, Port};

/// Runtime-managed execution loop for an actor node.
///
/// Instead of each actor implementing its own receive-loop in `create_process`,
/// `ActorProcess` provides the single, canonical dispatch loop. The runtime
/// constructs one per node and spawns it — actors only need to declare their
/// behavior and port names.
///
/// # Event-driven model
///
/// Actors are passive: they declare reactions to data via [`ActorBehavior`].
/// The runtime (this struct) drives execution by:
///
/// 1. Waiting for data on the actor's inport channel
/// 2. Optionally accumulating until all declared inports have data
/// 3. Building an [`ActorContext`] and invoking the behavior
/// 4. Forwarding non-empty results to the outport channel
/// 5. Managing load counters and tracing
pub struct ActorProcess {
    node_id: String,
    behavior: ActorBehavior,
    inport_names: Vec<String>,
    await_all_inports: bool,
    required_inports: Vec<String>,
    inport_rx: flume::Receiver<HashMap<String, Message>>,
    outports: Port,
    state: Arc<Mutex<dyn ActorState>>,
    load: Arc<ActorLoad>,
    config: ActorConfig,
    tracing: Option<TracingIntegration>,
}

impl ActorProcess {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        node_id: String,
        behavior: ActorBehavior,
        inport_names: Vec<String>,
        await_all_inports: bool,
        required_inports: Vec<String>,
        inport_rx: flume::Receiver<HashMap<String, Message>>,
        outports: Port,
        state: Arc<Mutex<dyn ActorState>>,
        load: Arc<ActorLoad>,
        config: ActorConfig,
        tracing: Option<TracingIntegration>,
    ) -> Self {
        Self {
            node_id,
            behavior,
            inport_names,
            await_all_inports,
            required_inports,
            inport_rx,
            outports,
            state,
            load,
            config,
            tracing,
        }
    }

    /// Run the canonical dispatch loop.
    ///
    /// This replaces the per-actor `create_process` boilerplate. The loop
    /// exits when the inport channel closes (all senders dropped).
    pub async fn run(self) {
        use futures::StreamExt;

        let mut accumulated: HashMap<String, Message> = HashMap::new();
        let actor_id = self.config.get_node_id();
        let mut tick_message_count: usize = 0;
        // Per-port message counter for connection-count-aware synchronization
        let mut port_counts: HashMap<String, usize> = HashMap::new();
        loop {
            let packet = match self.inport_rx.clone().stream().next().await {
                Some(p) => p,
                None => {
                    tracing::debug!(node = %self.node_id, "inport channel closed");
                    break;
                }
            };

            self.load.inc();

            // ── Accumulate if awaiting inports ──────────────────────
            let payload = if self.await_all_inports {
                // Wait until every *declared* inport has received at
                // least one packet, then fire. Earlier this counted
                // total packets against the sum of inport connection
                // counts, which mis-fires when an inport is fed only
                // via `add_initial` (zero topology connections — the
                // count would never include it, and the actor would
                // fire after fewer packets than declared inports).
                // Fan-in: `merge_accumulate` merges Object messages
                // on the same port instead of overwriting, so multiple
                // sources are preserved.
                merge_accumulate(&mut accumulated, packet);
                let all_inports_ready = self
                    .inport_names
                    .iter()
                    .all(|name| accumulated.contains_key(name));
                if !all_inports_ready {
                    continue;
                }
                std::mem::take(&mut accumulated)
            } else if !self.required_inports.is_empty() {
                // Wait for SPECIFIC required inports.
                // Connection-count-aware: for each required port, wait
                // until ALL upstream connections on that port have sent.
                merge_accumulate(&mut accumulated, packet.clone());
                tick_message_count += 1;
                for port in packet.keys() {
                    *port_counts.entry(port.clone()).or_insert(0) += 1;
                }

                let has_all_required = self.required_inports.iter().all(|req| {
                    let needed = self
                        .config
                        .inport_connection_counts
                        .get(req)
                        .copied()
                        .unwrap_or(1);
                    let received = port_counts.get(req).copied().unwrap_or(0);
                    received >= needed
                });
                if !has_all_required {
                    continue;
                }
                // Keep cached values, only clear required ports and counts
                tick_message_count = 0;
                let payload = accumulated.clone();
                for req in &self.required_inports {
                    accumulated.remove(req);
                    port_counts.remove(req);
                }
                payload
            } else {
                // Fire on any input
                packet
            };

            // ── Build context & invoke behavior ──────────────────────
            let context = ActorContext::new(
                payload,
                self.outports.clone(),
                self.state.clone(),
                self.config.clone(),
                self.load.clone(),
            );

            match (self.behavior)(context).await {
                Ok(result) => {
                    if !result.is_empty() {
                        let _ = self.outports.0.send_async(result).await;
                    }
                    self.load.reset();
                    if let Some(ref tracing) = self.tracing {
                        let _ = tracing.trace_actor_completed(actor_id).await;
                    }
                }
                Err(e) => {
                    self.load.reset();
                    tracing::warn!(node = %self.node_id, error = ?e, "actor behavior failed");
                    if let Some(ref tracing) = self.tracing {
                        let _ = tracing.trace_actor_failed(actor_id, e.to_string()).await;
                    }
                }
            }
        }
    }

    /// Consume self and return a boxed, pinned future suitable for
    /// `tokio::spawn` or the Actor trait's `create_process` return type.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn into_future(
        self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(self.run())
    }

    /// wasm32 variant — no `Send` because the runtime is single-threaded
    /// (`spawn_local`) and browser-only types are typically `!Send`.
    #[cfg(target_arch = "wasm32")]
    pub fn into_future(self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static>> {
        Box::pin(self.run())
    }
}

/// Merge-aware accumulation for fan-in synchronization.
///
/// When multiple connections fan-in to the same port and both carry
/// Object messages, their keys are merged (shallow) so no data is lost.
/// For non-Object types, last-write-wins (same as HashMap::extend).
fn merge_accumulate(accumulated: &mut HashMap<String, Message>, packet: HashMap<String, Message>) {
    for (port, msg) in packet {
        match accumulated.get(&port) {
            Some(Message::Object(existing_obj)) => {
                if let Message::Object(new_obj) = &msg {
                    // Both Objects: merge keys
                    let mut merged: Value = existing_obj.as_ref().clone().into();
                    let new_v: Value = new_obj.as_ref().clone().into();
                    if let (Some(m), Some(n)) = (merged.as_object_mut(), new_v.as_object()) {
                        for (k, v) in n {
                            m.insert(k.clone(), v.clone());
                        }
                    }
                    accumulated.insert(
                        port,
                        Message::Object(Arc::new(EncodableValue::from(merged))),
                    );
                } else {
                    accumulated.insert(port, msg);
                }
            }
            _ => {
                accumulated.insert(port, msg);
            }
        }
    }
}
