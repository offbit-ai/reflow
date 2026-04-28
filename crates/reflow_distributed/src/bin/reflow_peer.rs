//! `reflow-peer` — runs a single member of a distributed Reflow
//! network from a TOML config file.
//!
//! What it does on startup:
//!   1. Loads `peer.toml` — bind address, discovery endpoints,
//!      auth token, peer connect list, remote-actor proxies.
//!   2. Builds a `DistributedNetwork` and starts the bridge.
//!   3. Registers a built-in `recorder` actor on the local
//!      network. Every message landing on `recorder.in` is logged
//!      via `tracing` — useful as a smoke target without writing
//!      any glue code.
//!   4. Dials each `[[connect]]` entry.
//!   5. Registers each `[[remote_actor]]` proxy.
//!   6. Idles until Ctrl-C.
//!
//! With two peers + one `reflow-discovery` server, this binary
//! alone is enough to demonstrate federation end-to-end.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use futures::StreamExt;
use reflow_actor::{
    Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, MemoryState, Port,
    message::Message,
};
use reflow_distributed::peer_config::PeerConfig;
use reflow_network::distributed_network::DistributedNetwork;
use reflow_network::tracing::TracingIntegration;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(version, about = "Reflow distributed peer")]
struct Args {
    /// Path to a TOML peer config file.
    #[arg(short, long)]
    config: PathBuf,

    /// One-shot test send: `<network_id>:<actor_id>:<port>:<text>`.
    /// Fires after federation is up; the peer keeps running
    /// afterward.
    #[arg(long)]
    send: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let cfg = PeerConfig::from_path(&args.config)?;

    tracing::info!(
        "starting peer: network_id={}, instance_id={}, bind={}:{}",
        cfg.network_id,
        cfg.instance_id,
        cfg.bind_address,
        cfg.bind_port,
    );

    let dist_cfg = cfg.to_distributed_config();
    let mut net = DistributedNetwork::new(dist_cfg).await?;

    // Built-in recorder actor — logs every inbound message. Useful
    // as a smoke-test target without anyone having to write code.
    net.register_local_actor("recorder", RecorderActor::new(), None)
        .context("register recorder")?;

    net.start().await?;

    // Brief settle window — bridge needs a beat to start accepting
    // before we dial outwards.
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    for connect in &cfg.connect {
        match net.connect_to_network(&connect.endpoint).await {
            Ok(()) => tracing::info!("connected to peer at {}", connect.endpoint),
            Err(e) => tracing::warn!("failed to connect to {}: {}", connect.endpoint, e),
        }
    }

    for ra in &cfg.remote_actor {
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

    if let Some(spec) = args.send {
        send_one_shot(&net, &spec).await?;
    }

    tracing::info!("peer ready, awaiting messages (Ctrl-C to stop)");
    tokio::signal::ctrl_c().await?;
    tracing::info!("shutting down");
    net.shutdown().await?;
    Ok(())
}

async fn send_one_shot(net: &DistributedNetwork, spec: &str) -> Result<()> {
    // Format: <network_id>:<actor_id>:<port>:<text>
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
