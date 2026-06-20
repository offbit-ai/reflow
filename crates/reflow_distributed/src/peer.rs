//! Runs a single member of a distributed Reflow network from a [`PeerConfig`].
//!
//! Extracted so both the `reflow-peer` binary and the unified `reflow peer
//! spawn` CLI drive the exact same logic.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures::StreamExt;
use reflow_actor::{
    Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, MemoryState, Port,
    message::Message,
};
use reflow_network::distributed_network::DistributedNetwork;
use reflow_network::tracing::TracingIntegration;

use crate::peer_config::PeerConfig;

/// Bring up a peer: build the `DistributedNetwork`, register a built-in
/// `recorder` actor, start the bridge, dial `[[connect]]` entries, register
/// `[[remote_actor]]` proxies, optionally fire a one-shot `send`, then idle
/// until Ctrl-C.
pub async fn run_peer(config: PeerConfig, send: Option<String>) -> Result<()> {
    tracing::info!(
        "starting peer: network_id={}, instance_id={}, bind={}:{}",
        config.network_id,
        config.instance_id,
        config.bind_address,
        config.bind_port,
    );

    let mut net = DistributedNetwork::new(config.to_distributed_config()).await?;

    // Built-in recorder actor — logs every inbound message. A smoke-test target
    // without anyone having to write code.
    net.register_local_actor("recorder", RecorderActor::new(), None)
        .context("register recorder")?;

    net.start().await?;

    // Brief settle window — the bridge needs a beat to start accepting before
    // we dial outwards.
    tokio::time::sleep(Duration::from_millis(150)).await;

    for connect in &config.connect {
        match net.connect_to_network(&connect.endpoint).await {
            Ok(()) => tracing::info!("connected to peer at {}", connect.endpoint),
            Err(e) => tracing::warn!("failed to connect to {}: {}", connect.endpoint, e),
        }
    }

    for ra in &config.remote_actor {
        match net.register_remote_actor(&ra.actor_id, &ra.network_id).await {
            Ok(()) => tracing::info!(
                "registered remote actor proxy: {}@{}",
                ra.actor_id,
                ra.network_id
            ),
            Err(e) => tracing::warn!(
                "failed to register {}@{}: {}",
                ra.actor_id,
                ra.network_id,
                e
            ),
        }
    }

    if let Some(spec) = send {
        send_one_shot(&net, &spec).await?;
    }

    tracing::info!("peer ready, awaiting messages (Ctrl-C to stop)");
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    net.shutdown().await?;
    Ok(())
}

/// Fire a one-shot message: `<network_id>:<actor_id>:<port>:<text>`.
pub async fn send_one_shot(net: &DistributedNetwork, spec: &str) -> Result<()> {
    let mut parts = spec.splitn(4, ':');
    let network = parts.next().context("--send: missing network_id")?;
    let actor = parts.next().context("--send: missing actor_id")?;
    let port = parts.next().context("--send: missing port")?;
    let text = parts
        .next()
        .context("--send: missing text payload (after the third ':')")?;

    let payload = Message::String(Arc::new(text.to_string()));
    net.send_to_remote_actor(network, actor, port, payload, None)
        .await
        .with_context(|| format!("send_to_remote_actor: {network}:{actor}:{port}"))?;
    tracing::info!("sent test message to {network}/{actor}.{port}: {text:?}");
    Ok(())
}

// ─── built-in recorder actor ──────────────────────────────────────

struct RecorderActor {
    inports: Port,
    outports: Port,
    load: Arc<ActorLoad>,
}

impl RecorderActor {
    fn new() -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ActorLoad::new(0)),
        }
    }
}

impl Actor for RecorderActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(move |context: ActorContext| {
            Box::pin(async move {
                let payload = context.get_payload();
                for (port, msg) in payload.iter() {
                    tracing::info!("recorder.{port}: {msg:?}");
                }
                Ok(HashMap::new())
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
    fn create_instance(&self) -> Arc<dyn Actor> {
        Arc::new(Self::new())
    }
    fn create_process(
        &self,
        actor_config: ActorConfig,
        _tracing: Option<TracingIntegration>,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + Send + 'static>> {
        let behavior = self.get_behavior();
        let (_, receiver) = self.get_inports();
        let outports = self.get_outports();
        let load = self.load_count();

        Box::pin(async move {
            while let Some(packet) = receiver.stream().next().await {
                let context = ActorContext::new(
                    packet,
                    outports.clone(),
                    Arc::new(parking_lot::Mutex::new(MemoryState::default())),
                    actor_config.clone(),
                    load.clone(),
                );
                let _ = behavior(context).await;
                load.reset();
            }
        })
    }
}
