use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex;
use reflow_tracing_protocol::client::TracingIntegration;

use crate::message::Message;
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
        let inports_count = self.inport_names.len();
        let actor_id = self.config.get_node_id();
        let total_connections: usize = self.config.inport_connection_counts.values().sum();
        let mut tick_message_count: usize = 0;
        loop {
            let packet = match self.inport_rx.clone().stream().next().await {
                Some(p) => p,
                None => break,
            };

            self.load.inc();

            // ── Accumulate if awaiting inports ──────────────────────
            let payload = if self.await_all_inports {
                // Wait for ALL connected inports (uses graph topology)
                accumulated.extend(packet);
                tick_message_count += 1;
                let needed = if total_connections > 0 {
                    total_connections
                } else {
                    inports_count
                };
                if tick_message_count < needed {
                    continue;
                }
                tick_message_count = 0;
                std::mem::take(&mut accumulated)
            } else if !self.required_inports.is_empty() {
                // Wait for SPECIFIC required inports using connection counts
                accumulated.extend(packet);
                tick_message_count += 1;

                let has_all_required = self
                    .required_inports
                    .iter()
                    .all(|req| accumulated.contains_key(req));
                if !has_all_required {
                    continue;
                }
                // Keep cached values, only clear required ports
                tick_message_count = 0;
                let payload = accumulated.clone();
                for req in &self.required_inports {
                    accumulated.remove(req);
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
                        let _ = self.outports.0.send(result);
                    }
                    self.load.reset();
                    if let Some(ref tracing) = self.tracing {
                        let _ = tracing.trace_actor_completed(actor_id).await;
                    }
                }
                Err(e) => {
                    self.load.reset();
                    eprintln!("[{}] behavior error: {:?}", self.node_id, e);
                    if let Some(ref tracing) = self.tracing {
                        let _ = tracing.trace_actor_failed(actor_id, e.to_string()).await;
                    }
                }
            }
        }
    }

    /// Consume self and return a boxed, pinned future suitable for
    /// `tokio::spawn` or the Actor trait's `create_process` return type.
    pub fn into_future(
        self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(self.run())
    }
}
