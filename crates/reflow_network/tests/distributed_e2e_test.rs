//! End-to-end integration tests for `DistributedNetwork`.
//!
//! Boots two real `DistributedNetwork` instances on loopback ports,
//! has them connect over the bridge, and asserts that messages sent
//! via `send_to_remote_actor` (or via the proxy in the local network)
//! arrive at the receiving actor's inport.
//!
//! These tests are the floor: every protocol-level fix needs to keep
//! them green, and every new distributed-runtime feature should add
//! a case here before it can be considered shipped.

use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reflow_network::actor::{
    Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, MemoryState, Port,
};
use reflow_network::distributed_network::{DistributedConfig, DistributedNetwork};
use reflow_network::message::Message;
use reflow_network::network::NetworkConfig;
use reflow_network::tracing::TracingIntegration;

// ─── test actor: records every inbound packet into a flume channel ────────

struct RecorderActor {
    inports: Port,
    outports: Port,
    load: Arc<ActorLoad>,
    sink: flume::Sender<HashMap<String, Message>>,
}

impl RecorderActor {
    fn new(sink: flume::Sender<HashMap<String, Message>>) -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ActorLoad::new(0)),
            sink,
        }
    }
}

impl Actor for RecorderActor {
    fn get_behavior(&self) -> ActorBehavior {
        let sink = self.sink.clone();
        Box::new(move |context: ActorContext| {
            let sink = sink.clone();
            Box::pin(async move {
                let payload = context.get_payload();
                // Forward every (port, message) into the test's channel
                // so assertions can read what arrived.
                let _ = sink.send_async(payload.clone()).await;
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
        Arc::new(Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            load: Arc::new(ActorLoad::new(0)),
            sink: self.sink.clone(),
        })
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

// ─── helpers ───────────────────────────────────────────────────────────────

fn make_config(network_id: &str, instance_id: &str, port: u16) -> DistributedConfig {
    make_config_with_auth(network_id, instance_id, port, None)
}

fn make_config_with_auth(
    network_id: &str,
    instance_id: &str,
    port: u16,
    auth_token: Option<&str>,
) -> DistributedConfig {
    DistributedConfig {
        network_id: network_id.to_string(),
        instance_id: instance_id.to_string(),
        bind_address: "127.0.0.1".to_string(),
        bind_port: port,
        discovery_endpoints: vec![],
        auth_token: auth_token.map(String::from),
        max_connections: 10,
        heartbeat_interval_ms: 1000,
        local_network_config: NetworkConfig::default(),
    }
}

// ─── tests ─────────────────────────────────────────────────────────────────

/// Two networks federate; one sends a message addressed to the other's
/// local actor; the receiving actor sees it on its inport.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn message_crosses_the_bridge() {
    let (sink_tx, sink_rx) = flume::unbounded::<HashMap<String, Message>>();

    // ── Server: hosts a recorder actor ────────────────────────────────
    let server = DistributedNetwork::new(make_config("server-net", "server-1", 9100))
        .await
        .expect("server::new");
    server
        .register_local_actor("recorder", RecorderActor::new(sink_tx), None)
        .expect("register recorder");
    let mut server = server;
    server.start().await.expect("server::start");

    // ── Client: connects to server, then sends ────────────────────────
    let mut client = DistributedNetwork::new(make_config("client-net", "client-1", 9101))
        .await
        .expect("client::new");
    client.start().await.expect("client::start");

    // Connection takes a beat to settle (handshake + heartbeat).
    tokio::time::sleep(Duration::from_millis(150)).await;
    client
        .connect_to_network("127.0.0.1:9100")
        .await
        .expect("connect_to_network");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Send a message addressed to server's `recorder` actor on its `in` port.
    let payload = Message::String(Arc::new("hello-from-client".to_string()));
    client
        .send_to_remote_actor(
            "server-net",
            "recorder",
            "in",
            payload.clone(),
            Some("client-source"),
        )
        .await
        .expect("send_to_remote_actor");

    // Recorder fires within a couple of seconds.
    let received = sink_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("message should arrive at server's recorder actor");
    assert_eq!(received.get("in"), Some(&payload),
        "expected the inport to carry the original payload, got: {:?}",
        received);

    server.shutdown().await.ok();
    client.shutdown().await.ok();
}

/// Same shape as above, but uses the local-network proxy path:
/// register the remote actor, then push a message to its proxy via the
/// local `send_to_actor`. Exercises the full proxy path.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn message_via_remote_actor_proxy() {
    let (sink_tx, sink_rx) = flume::unbounded::<HashMap<String, Message>>();

    let server = DistributedNetwork::new(make_config("server-net", "server-2", 9102))
        .await
        .expect("server::new");
    server
        .register_local_actor("recorder", RecorderActor::new(sink_tx), None)
        .expect("register recorder");
    let mut server = server;
    server.start().await.expect("server::start");

    let mut client = DistributedNetwork::new(make_config("client-net", "client-2", 9103))
        .await
        .expect("client::new");
    client.start().await.expect("client::start");

    tokio::time::sleep(Duration::from_millis(150)).await;
    client
        .connect_to_network("127.0.0.1:9102")
        .await
        .expect("connect_to_network");
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Register a proxy actor on client for the remote `recorder@server-net`.
    client
        .register_remote_actor("recorder", "server-net")
        .await
        .expect("register_remote_actor");

    // Push through the local network — the proxy forwards across the bridge.
    let payload = Message::String(Arc::new("hello-via-proxy".to_string()));
    {
        let local = client.get_local_network();
        let net = local.read();
        net.send_to_actor("recorder@server-net", "in", payload.clone())
            .expect("send_to_actor via proxy");
    }

    let received = sink_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("proxy should forward the message");
    assert_eq!(received.get("in"), Some(&payload));

    server.shutdown().await.ok();
    client.shutdown().await.ok();
}

/// When server and client share the same auth_token, federation
/// works exactly as without auth.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auth_matching_tokens_succeed() {
    let (sink_tx, sink_rx) = flume::unbounded::<HashMap<String, Message>>();

    let server = DistributedNetwork::new(make_config_with_auth(
        "server-net", "server-3", 9104, Some("shared-secret"),
    ))
    .await
    .expect("server::new");
    server
        .register_local_actor("recorder", RecorderActor::new(sink_tx), None)
        .expect("register recorder");
    let mut server = server;
    server.start().await.expect("server::start");

    let mut client = DistributedNetwork::new(make_config_with_auth(
        "client-net", "client-3", 9105, Some("shared-secret"),
    ))
    .await
    .expect("client::new");
    client.start().await.expect("client::start");

    tokio::time::sleep(Duration::from_millis(150)).await;
    client
        .connect_to_network("127.0.0.1:9104")
        .await
        .expect("connect should succeed with matching tokens");
    tokio::time::sleep(Duration::from_millis(150)).await;

    let payload = Message::String(Arc::new("authed".to_string()));
    client
        .send_to_remote_actor("server-net", "recorder", "in", payload.clone(), None)
        .await
        .expect("send_to_remote_actor");

    let received = sink_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("authed message should arrive");
    assert_eq!(received.get("in"), Some(&payload));

    server.shutdown().await.ok();
    client.shutdown().await.ok();
}

/// Server has an auth_token; client presents a different one.
/// connect_to_network must fail loudly; no message round-trip
/// possible because no connection was registered.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auth_mismatch_rejects_handshake() {
    let server = DistributedNetwork::new(make_config_with_auth(
        "server-net", "server-4", 9106, Some("server-secret"),
    ))
    .await
    .expect("server::new");
    let mut server = server;
    server.start().await.expect("server::start");

    let mut client = DistributedNetwork::new(make_config_with_auth(
        "client-net", "client-4", 9107, Some("wrong-secret"),
    ))
    .await
    .expect("client::new");
    client.start().await.expect("client::start");

    tokio::time::sleep(Duration::from_millis(150)).await;
    let connect = client.connect_to_network("127.0.0.1:9106").await;
    assert!(
        connect.is_err(),
        "client.connect_to_network should fail when tokens don't match"
    );
    let err = connect.unwrap_err().to_string();
    assert!(
        err.contains("auth_token") || err.contains("refused"),
        "error should mention auth refusal, got: {err}"
    );

    server.shutdown().await.ok();
    client.shutdown().await.ok();
}

/// Helper: same as `make_config` but lets the test pick a custom
/// heartbeat interval so timeout-driven tests can run in seconds
/// rather than the default 1s.
fn make_config_fast_heartbeat(
    network_id: &str,
    instance_id: &str,
    port: u16,
    heartbeat_ms: u64,
) -> DistributedConfig {
    let mut cfg = make_config(network_id, instance_id, port);
    cfg.heartbeat_interval_ms = heartbeat_ms;
    cfg
}

/// When the server vanishes, the client's heartbeat monitor must
/// notice the dead connection within `3 * heartbeat_interval_ms`
/// (the timeout threshold) and evict it from the connections map.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn heartbeat_timeout_evicts_dead_peer() {
    let server = DistributedNetwork::new(make_config_fast_heartbeat(
        "server-net",
        "server-6",
        9110,
        100,
    ))
    .await
    .expect("server::new");
    let mut server = server;
    server.start().await.expect("server::start");

    let mut client = DistributedNetwork::new(make_config_fast_heartbeat(
        "client-net",
        "client-6",
        9111,
        100,
    ))
    .await
    .expect("client::new");
    client.start().await.expect("client::start");

    tokio::time::sleep(Duration::from_millis(150)).await;
    client
        .connect_to_network("127.0.0.1:9110")
        .await
        .expect("connect_to_network");

    // Connection should be in the map now.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        client.is_connected_to("server-net"),
        "expected client to track the server connection right after handshake"
    );

    // Tear the server down. The TCP close propagates to the client's
    // message-loop task, which marks the connection failed and
    // removes it. The heartbeat monitor would catch a silent peer
    // even without the close, but the close path is the fast one.
    server.shutdown().await.ok();

    // Within ~3 heartbeat intervals (300ms) the client should have
    // dropped the entry. Give a generous margin to absorb scheduler
    // jitter on slower CI hosts.
    let deadline = std::time::Instant::now() + Duration::from_millis(1500);
    while client.is_connected_to("server-net") && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !client.is_connected_to("server-net"),
        "server-net entry should have been evicted after the peer died"
    );

    client.shutdown().await.ok();
}

/// After a peer drops and comes back on the same endpoint, the
/// client's reconnect dispatcher should re-establish the connection
/// without any explicit user action.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn client_reconnects_after_server_restart() {
    let (sink_tx_initial, _sink_rx_initial) =
        flume::unbounded::<HashMap<String, Message>>();

    // ── Round 1: bring up server #1 + client; verify federation ──
    let server1 = DistributedNetwork::new(make_config_fast_heartbeat(
        "server-net",
        "server-7",
        9112,
        150,
    ))
    .await
    .expect("server1::new");
    server1
        .register_local_actor("recorder", RecorderActor::new(sink_tx_initial), None)
        .expect("register recorder");
    let mut server1 = server1;
    server1.start().await.expect("server1::start");

    let mut client = DistributedNetwork::new(make_config_fast_heartbeat(
        "client-net",
        "client-7",
        9113,
        150,
    ))
    .await
    .expect("client::new");
    client.start().await.expect("client::start");

    tokio::time::sleep(Duration::from_millis(150)).await;
    client
        .connect_to_network("127.0.0.1:9112")
        .await
        .expect("initial connect_to_network");
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(
        client.is_connected_to("server-net"),
        "client should track the server-net connection after the initial handshake"
    );

    // ── Drop server #1 ──────────────────────────────────────────────
    server1.shutdown().await.ok();
    drop(server1);

    // Wait for the client to notice the close and remove its
    // tracking entry. Give a generous margin — the cleanup chain is
    // async (Close frame → handle_incoming_messages breaks →
    // `connections.write().remove`).
    let drop_deadline = std::time::Instant::now() + Duration::from_millis(2000);
    while client.is_connected_to("server-net") && std::time::Instant::now() < drop_deadline {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !client.is_connected_to("server-net"),
        "client should drop the entry after server #1 closes"
    );

    // ── Bring up server #2 on the same port ────────────────────────
    let (sink_tx, sink_rx) = flume::unbounded::<HashMap<String, Message>>();
    let server2 = loop {
        match DistributedNetwork::new(make_config_fast_heartbeat(
            "server-net",
            "server-7-restart",
            9112,
            150,
        ))
        .await
        {
            Ok(s) => break s,
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        }
    };
    server2
        .register_local_actor("recorder", RecorderActor::new(sink_tx), None)
        .expect("register recorder #2");
    let mut server2 = server2;
    server2.start().await.expect("server2::start");

    // Client's reconnect loop sleeps with backoff (200, 400, 800,
    // 1600, 3200 ms). Total budget for reconnect ≈ 6.5s.
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    while !client.is_connected_to("server-net") && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(
        client.is_connected_to("server-net"),
        "client should auto-reconnect to server-net after server #2 is up"
    );

    // Verify the link works end-to-end after reconnect.
    let payload = Message::String(Arc::new("hello-after-reconnect".to_string()));
    client
        .send_to_remote_actor("server-net", "recorder", "in", payload.clone(), None)
        .await
        .expect("send_to_remote_actor after reconnect");

    let received = sink_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("recorder on server #2 should receive the post-reconnect message");
    assert_eq!(received.get("in"), Some(&payload));

    server2.shutdown().await.ok();
    client.shutdown().await.ok();
}

/// Server demands auth; client presents no token at all. Same
/// rejection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn auth_missing_token_rejected() {
    let server = DistributedNetwork::new(make_config_with_auth(
        "server-net", "server-5", 9108, Some("required"),
    ))
    .await
    .expect("server::new");
    let mut server = server;
    server.start().await.expect("server::start");

    let mut client = DistributedNetwork::new(make_config("client-net", "client-5", 9109))
        .await
        .expect("client::new");
    client.start().await.expect("client::start");

    tokio::time::sleep(Duration::from_millis(150)).await;
    let connect = client.connect_to_network("127.0.0.1:9108").await;
    assert!(
        connect.is_err(),
        "anonymous client should be refused when server requires auth"
    );

    server.shutdown().await.ok();
    client.shutdown().await.ok();
}
