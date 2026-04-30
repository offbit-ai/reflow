//! End-to-end test: real `reflow_distributed::serve()` HTTP server +
//! two real `DiscoveryService` clients, asserting that
//! self-registration, periodic refresh, and topology events all
//! work over the loopback HTTP contract.

use std::net::SocketAddr;
use std::time::Duration;

use reflow_distributed::{ServerConfig, serve};
use reflow_network::discovery::{DiscoveryEvent, DiscoveryService};
use reflow_network::distributed_network::DistributedConfig;
use reflow_network::network::NetworkConfig;
use tokio::net::TcpListener;

async fn pick_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    port
}

fn make_client_config(network_id: &str, instance_id: &str, port: u16, server: &str) -> DistributedConfig {
    DistributedConfig {
        network_id: network_id.to_string(),
        instance_id: instance_id.to_string(),
        bind_address: "127.0.0.1".to_string(),
        bind_port: port,
        discovery_endpoints: vec![format!("http://{server}")],
        auth_token: None,
        max_connections: 10,
        heartbeat_interval_ms: 1000,
        local_network_config: NetworkConfig::default(),
    }
}

async fn spawn_server() -> (String, tokio::task::JoinHandle<()>) {
    let port = pick_port().await;
    let bind: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
    let endpoint = format!("127.0.0.1:{port}");
    let handle = tokio::spawn(async move {
        let _ = serve(ServerConfig {
            bind,
            entry_ttl: Duration::from_secs(30),
            prune_interval: Duration::from_secs(60),
        })
        .await;
    });
    // Wait for the listener to actually be bound.
    for _ in 0..40 {
        if reqwest::get(format!("http://{endpoint}/networks")).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    (endpoint, handle)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::single_match)]
#[allow(clippy::collapsible_match)]
async fn two_clients_see_each_other_via_discovery() {
    let (server_addr, server_task) = spawn_server().await;

    // Client A registers and refreshes every 75ms.
    let svc_a = DiscoveryService::new(make_client_config("alpha", "alpha-1", 9200, &server_addr))
        .with_refresh_interval(Duration::from_millis(75));
    let mut events_a = svc_a.subscribe();
    svc_a.start().await.unwrap();

    // Client B does the same.
    let svc_b = DiscoveryService::new(make_client_config("beta", "beta-1", 9201, &server_addr))
        .with_refresh_interval(Duration::from_millis(75));
    let mut events_b = svc_b.subscribe();
    svc_b.start().await.unwrap();

    // Within a few ticks, both clients should have observed the
    // other via the server. A's view should include "beta", B's
    // view should include "alpha".
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        let a_sees_b = svc_a.known_networks().contains_key("beta");
        let b_sees_a = svc_b.known_networks().contains_key("alpha");
        if a_sees_b && b_sees_a {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!(
                "timed out waiting for cross-visibility: a_sees_b={}, b_sees_a={}",
                a_sees_b, b_sees_a
            );
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    // Both clients should have emitted at least one Added event.
    let mut a_saw_added = false;
    for _ in 0..5 {
        if let Ok(Ok(ev)) = tokio::time::timeout(Duration::from_millis(200), events_a.recv()).await && (matches!(ev, DiscoveryEvent::Added(ref info) if info.network_id == "beta")
                || matches!(ev, DiscoveryEvent::Added(ref info) if info.network_id == "alpha"))
        {
            a_saw_added = true;
            break;
        }
    }
    assert!(a_saw_added, "client A should have received an Added event");

    let mut b_saw_added = false;
    for _ in 0..5 {
        match tokio::time::timeout(Duration::from_millis(200), events_b.recv()).await {
            Ok(Ok(ev)) => {
                if matches!(ev, DiscoveryEvent::Added(ref info) if info.network_id == "alpha")
                    || matches!(ev, DiscoveryEvent::Added(ref info) if info.network_id == "beta")
                {
                    b_saw_added = true;
                    break;
                }
            }
            _ => (),
        }
    }
    assert!(b_saw_added, "client B should have received an Added event");

    svc_a.stop();
    svc_b.stop();
    server_task.abort();
}
