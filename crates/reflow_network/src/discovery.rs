use std::{collections::HashMap, sync::Arc, time::Duration};

use crate::distributed_network::DistributedConfig;
use anyhow::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tokio::time::MissedTickBehavior;

/// Default cadence for re-polling discovery endpoints when the config
/// does not pin one. 15s is fast enough that a peer disappearing or
/// joining is noticed within a heartbeat or two without hammering the
/// discovery service in steady state.
const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(15);

pub struct DiscoveryService {
    config: DistributedConfig,
    known_networks: Arc<RwLock<HashMap<String, NetworkInfo>>>,
    registration_client: Option<reqwest::Client>,
    events_tx: tokio::sync::broadcast::Sender<DiscoveryEvent>,
    shutdown: Arc<tokio::sync::Notify>,
    refresh_interval: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkInfo {
    pub network_id: String,
    pub instance_id: String,
    pub endpoint: String,
    pub capabilities: Vec<String>,
    pub last_seen: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RegistrationRequest {
    pub network_id: String,
    pub instance_id: String,
    pub endpoint: String,
    pub capabilities: Vec<String>,
}

/// Emitted whenever the discovery refresh notices the topology has
/// changed since the previous poll. Subscribers receive `Added` for
/// networks that appeared, `Removed` for networks that vanished, and
/// `Updated` when an existing network's endpoint or capabilities
/// changed (instance restart, address rotation).
#[derive(Debug, Clone)]
pub enum DiscoveryEvent {
    Added(NetworkInfo),
    Removed(String),
    Updated(NetworkInfo),
}

impl DiscoveryService {
    pub fn new(config: DistributedConfig) -> Self {
        let (events_tx, _) = tokio::sync::broadcast::channel(64);
        DiscoveryService {
            config,
            known_networks: Arc::new(RwLock::new(HashMap::new())),
            registration_client: Some(reqwest::Client::new()),
            events_tx,
            shutdown: Arc::new(tokio::sync::Notify::new()),
            refresh_interval: DEFAULT_REFRESH_INTERVAL,
        }
    }

    /// Override the periodic refresh cadence. Mainly useful in tests
    /// where the default 15s would be too slow to observe.
    pub fn with_refresh_interval(mut self, interval: Duration) -> Self {
        self.refresh_interval = interval;
        self
    }

    pub async fn start(&self) -> Result<(), anyhow::Error> {
        // Register with discovery endpoints
        self.register_self().await?;

        // Start periodic discovery refresh
        self.start_discovery_refresh().await?;

        Ok(())
    }

    async fn register_self(&self) -> Result<(), anyhow::Error> {
        let registration = RegistrationRequest {
            network_id: self.config.network_id.clone(),
            instance_id: self.config.instance_id.clone(),
            endpoint: format!("{}:{}", self.config.bind_address, self.config.bind_port),
            capabilities: vec!["actor_messaging".to_string()],
        };

        for endpoint in &self.config.discovery_endpoints {
            if let Some(client) = &self.registration_client {
                let result = client
                    .post(format!("{}/register", endpoint))
                    .json(&registration)
                    .send()
                    .await;

                match result {
                    Ok(_) => tracing::info!("Registered with discovery endpoint: {}", endpoint),
                    Err(e) => tracing::warn!("Failed to register with {}: {}", endpoint, e),
                }
            }
        }

        Ok(())
    }

    pub async fn discover_networks(&self) -> Result<Vec<NetworkInfo>, anyhow::Error> {
        let mut all_networks = Vec::new();

        for endpoint in &self.config.discovery_endpoints {
            if let Some(client) = &self.registration_client {
                match client.get(format!("{}/networks", endpoint)).send().await {
                    Ok(response) => {
                        if let Ok(networks) = response.json::<Vec<NetworkInfo>>().await {
                            all_networks.extend(networks);
                        }
                    }
                    Err(e) => tracing::warn!("Discovery failed for {}: {}", endpoint, e),
                }
            }
        }

        Ok(all_networks)
    }

    /// Snapshot of every network currently visible to this service.
    pub fn known_networks(&self) -> HashMap<String, NetworkInfo> {
        self.known_networks.read().clone()
    }

    /// Subscribe to topology change events. Each subscriber gets its
    /// own queue; missed events surface as `RecvError::Lagged` — when
    /// that happens, callers should re-snapshot via `known_networks()`.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<DiscoveryEvent> {
        self.events_tx.subscribe()
    }

    /// Stop the periodic refresh loop. Idempotent.
    pub fn stop(&self) {
        self.shutdown.notify_waiters();
    }

    async fn start_discovery_refresh(&self) -> Result<()> {
        if self.config.discovery_endpoints.is_empty() {
            // Nothing to refresh — the loop would just spin.
            tracing::debug!("Discovery refresh: no endpoints configured, skipping");
            return Ok(());
        }

        let endpoints = self.config.discovery_endpoints.clone();
        let registration = RegistrationRequest {
            network_id: self.config.network_id.clone(),
            instance_id: self.config.instance_id.clone(),
            endpoint: format!("{}:{}", self.config.bind_address, self.config.bind_port),
            capabilities: vec!["actor_messaging".to_string()],
        };
        let known = self.known_networks.clone();
        let events_tx = self.events_tx.clone();
        let shutdown = self.shutdown.clone();
        let interval_dur = self.refresh_interval;
        let client = self
            .registration_client
            .clone()
            .unwrap_or_default();

        tokio::spawn(async move {
            // First tick fires immediately so the table is populated
            // before the bridge starts dispatching to it.
            let mut tick = tokio::time::interval(interval_dur);
            tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = tick.tick() => {
                        Self::refresh_once(&endpoints, &registration, &client, &known, &events_tx).await;
                    }
                    _ = shutdown.notified() => {
                        tracing::debug!("Discovery refresh: shutdown received, exiting");
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    async fn refresh_once(
        endpoints: &[String],
        registration: &RegistrationRequest,
        client: &reqwest::Client,
        known: &Arc<RwLock<HashMap<String, NetworkInfo>>>,
        events_tx: &tokio::sync::broadcast::Sender<DiscoveryEvent>,
    ) {
        // Renew our registration on every tick. Without this the
        // discovery server's TTL would eventually drop us, and other
        // peers would stop seeing this network.
        for endpoint in endpoints {
            if let Err(e) = client
                .post(format!("{}/register", endpoint))
                .json(registration)
                .send()
                .await
            {
                tracing::warn!("Discovery re-register failed for {}: {}", endpoint, e);
            }
        }

        // Aggregate every endpoint's response into a single
        // `network_id -> info` map so duplicates collapse.
        let mut latest: HashMap<String, NetworkInfo> = HashMap::new();
        for endpoint in endpoints {
            match client.get(format!("{}/networks", endpoint)).send().await {
                Ok(response) => match response.json::<Vec<NetworkInfo>>().await {
                    Ok(networks) => {
                        for n in networks {
                            latest.insert(n.network_id.clone(), n);
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Discovery refresh: invalid JSON from {}: {}", endpoint, e)
                    }
                },
                Err(e) => tracing::warn!("Discovery refresh failed for {}: {}", endpoint, e),
            }
        }

        // Diff against the previous snapshot under the write lock so
        // readers can never observe a half-applied state.
        let mut events = Vec::new();
        {
            let mut current = known.write();
            for (id, info) in &latest {
                match current.get(id) {
                    None => events.push(DiscoveryEvent::Added(info.clone())),
                    Some(prev) if prev != info => {
                        events.push(DiscoveryEvent::Updated(info.clone()))
                    }
                    _ => {}
                }
            }
            for id in current
                .keys()
                .filter(|id| !latest.contains_key(*id))
                .cloned()
                .collect::<Vec<_>>()
            {
                events.push(DiscoveryEvent::Removed(id));
            }
            *current = latest;
        }

        for event in events {
            // Send is best-effort — if no one is subscribed, drop it.
            let _ = events_tx.send(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::NetworkConfig;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    /// Tiny HTTP/1.1 server that responds to `GET /networks` with
    /// whatever JSON body is currently in `state`. Lives only for the
    /// test that spawned it.
    async fn spawn_mock_discovery(
        state: Arc<Mutex<Vec<NetworkInfo>>>,
    ) -> (String, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let endpoint = format!("http://{}", addr);

        let handle = tokio::spawn(async move {
            loop {
                let (mut stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => break,
                };
                let state = state.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    if req.starts_with("GET /networks") {
                        let body = serde_json::to_string(&*state.lock().await).unwrap();
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes()).await;
                    } else if req.starts_with("POST /register") {
                        let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                });
            }
        });

        (endpoint, handle)
    }

    fn make_info(id: &str, port: u16) -> NetworkInfo {
        NetworkInfo {
            network_id: id.to_string(),
            instance_id: format!("{id}-1"),
            endpoint: format!("127.0.0.1:{port}"),
            capabilities: vec!["actor_messaging".to_string()],
            last_seen: chrono::Utc::now(),
        }
    }

    fn make_config(endpoint: &str) -> DistributedConfig {
        DistributedConfig {
            network_id: "client-net".to_string(),
            instance_id: "client-1".to_string(),
            bind_address: "127.0.0.1".to_string(),
            bind_port: 0,
            discovery_endpoints: vec![endpoint.to_string()],
            auth_token: None,
            max_connections: 10,
            heartbeat_interval_ms: 1000,
            local_network_config: NetworkConfig::default(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn refresh_populates_known_networks_and_emits_added() {
        let state = Arc::new(Mutex::new(vec![make_info("alpha", 7001)]));
        let (endpoint, _server) = spawn_mock_discovery(state.clone()).await;

        let svc = DiscoveryService::new(make_config(&endpoint))
            .with_refresh_interval(Duration::from_millis(50));
        let mut events = svc.subscribe();
        svc.start().await.expect("start discovery");

        // Wait for the first refresh tick.
        let event = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("first tick should fire")
            .expect("event channel ok");
        assert!(
            matches!(event, DiscoveryEvent::Added(ref info) if info.network_id == "alpha"),
            "expected Added(alpha), got {event:?}",
        );

        let snapshot = svc.known_networks();
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.contains_key("alpha"));

        svc.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn refresh_detects_added_and_removed() {
        let state = Arc::new(Mutex::new(vec![make_info("alpha", 7001)]));
        let (endpoint, _server) = spawn_mock_discovery(state.clone()).await;

        let svc = DiscoveryService::new(make_config(&endpoint))
            .with_refresh_interval(Duration::from_millis(50));
        let mut events = svc.subscribe();
        svc.start().await.expect("start discovery");

        // First tick → Added(alpha).
        let _ = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("first tick")
            .unwrap();

        // Mock server now reports beta instead of alpha.
        {
            let mut s = state.lock().await;
            *s = vec![make_info("beta", 7002)];
        }

        // Within ~10 ticks we should observe the swap as a pair of
        // events (order is not guaranteed since both are emitted from
        // a single refresh).
        let mut saw_added_beta = false;
        let mut saw_removed_alpha = false;
        for _ in 0..20 {
            match tokio::time::timeout(Duration::from_secs(2), events.recv()).await {
                Ok(Ok(DiscoveryEvent::Added(info))) if info.network_id == "beta" => {
                    saw_added_beta = true;
                }
                Ok(Ok(DiscoveryEvent::Removed(id))) if id == "alpha" => {
                    saw_removed_alpha = true;
                }
                Ok(Ok(_)) => continue,
                Ok(Err(_)) | Err(_) => break,
            }
            if saw_added_beta && saw_removed_alpha {
                break;
            }
        }
        assert!(saw_added_beta, "should see Added(beta)");
        assert!(saw_removed_alpha, "should see Removed(alpha)");

        let snapshot = svc.known_networks();
        assert_eq!(snapshot.len(), 1);
        assert!(snapshot.contains_key("beta"));
        assert!(!snapshot.contains_key("alpha"));

        svc.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn refresh_emits_updated_when_endpoint_rotates() {
        let state = Arc::new(Mutex::new(vec![make_info("alpha", 7001)]));
        let (endpoint, _server) = spawn_mock_discovery(state.clone()).await;

        let svc = DiscoveryService::new(make_config(&endpoint))
            .with_refresh_interval(Duration::from_millis(50));
        let mut events = svc.subscribe();
        svc.start().await.expect("start discovery");

        // First tick → Added(alpha).
        let _ = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .expect("first tick")
            .unwrap();

        // Same network_id, different endpoint port — restart scenario.
        {
            let mut s = state.lock().await;
            *s = vec![make_info("alpha", 7099)];
        }

        let mut saw_updated = false;
        for _ in 0..20 {
            match tokio::time::timeout(Duration::from_secs(2), events.recv()).await {
                Ok(Ok(DiscoveryEvent::Updated(info)))
                    if info.network_id == "alpha" && info.endpoint.ends_with(":7099") =>
                {
                    saw_updated = true;
                    break;
                }
                Ok(Ok(_)) => continue,
                Ok(Err(_)) | Err(_) => break,
            }
        }
        assert!(saw_updated, "should see Updated(alpha) with new endpoint");

        svc.stop();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn no_endpoints_means_no_refresh_task() {
        // With no discovery_endpoints, start() should not spawn a
        // refresh task and stop() should be a no-op.
        let mut config = make_config("http://unused");
        config.discovery_endpoints.clear();
        let svc = DiscoveryService::new(config);
        svc.start().await.expect("start discovery");
        assert!(svc.known_networks().is_empty());
        svc.stop();
    }
}
