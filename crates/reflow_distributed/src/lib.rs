//! Discovery server + peer-CLI runtime for Reflow's distributed network.
//!
//! The discovery server speaks the same minimal HTTP contract that
//! `reflow_network::discovery::DiscoveryService` expects:
//!
//!   * `POST /register` — body is a `RegistrationRequest`. Each peer
//!     re-registers on every refresh tick; entries that haven't been
//!     seen in `entry_ttl` are pruned.
//!   * `GET /networks` — returns the current list of `NetworkInfo`
//!     entries.
//!
//! Embed [`DiscoveryServer`] in your own process if you want to put
//! discovery behind your auth / rate-limiting middleware. The
//! `reflow-discovery` binary is a thin wrapper that just picks an
//! address from CLI flags and runs `serve()` on the main runtime.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use parking_lot::RwLock;
use reflow_network::discovery::{NetworkInfo, RegistrationRequest};

pub mod peer_config;

/// In-memory registry. Entries decay if the peer hasn't pinged in
/// `entry_ttl`; the prune task (spawned in `serve`) sweeps the table
/// every `prune_interval`.
#[derive(Clone)]
pub struct DiscoveryRegistry {
    inner: Arc<RwLock<HashMap<String, NetworkInfo>>>,
    entry_ttl: chrono::Duration,
}

impl DiscoveryRegistry {
    pub fn new(entry_ttl: Duration) -> Self {
        DiscoveryRegistry {
            inner: Arc::new(RwLock::new(HashMap::new())),
            entry_ttl: chrono::Duration::from_std(entry_ttl)
                .unwrap_or_else(|_| chrono::Duration::seconds(60)),
        }
    }

    pub fn upsert(&self, req: RegistrationRequest) {
        let info = NetworkInfo {
            network_id: req.network_id.clone(),
            instance_id: req.instance_id,
            endpoint: req.endpoint,
            capabilities: req.capabilities,
            last_seen: chrono::Utc::now(),
        };
        self.inner.write().insert(req.network_id, info);
    }

    pub fn list(&self) -> Vec<NetworkInfo> {
        let cutoff = chrono::Utc::now() - self.entry_ttl;
        self.inner
            .read()
            .values()
            .filter(|n| n.last_seen >= cutoff)
            .cloned()
            .collect()
    }

    pub fn prune(&self) -> usize {
        let cutoff = chrono::Utc::now() - self.entry_ttl;
        let mut guard = self.inner.write();
        let before = guard.len();
        guard.retain(|_, info| info.last_seen >= cutoff);
        before - guard.len()
    }

    pub fn len(&self) -> usize {
        self.inner.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().is_empty()
    }
}

/// Build the axum router that serves the discovery contract. Useful
/// if you want to mount it under a prefix or compose with your own
/// middleware.
pub fn router(registry: DiscoveryRegistry) -> Router {
    Router::new()
        .route("/register", post(register_handler))
        .route("/networks", get(list_handler))
        .with_state(registry)
}

async fn register_handler(
    State(reg): State<DiscoveryRegistry>,
    Json(req): Json<RegistrationRequest>,
) -> impl IntoResponse {
    tracing::info!(
        "register: network_id={}, instance_id={}, endpoint={}",
        req.network_id,
        req.instance_id,
        req.endpoint
    );
    reg.upsert(req);
    StatusCode::OK
}

async fn list_handler(State(reg): State<DiscoveryRegistry>) -> Json<Vec<NetworkInfo>> {
    Json(reg.list())
}

/// Configuration for [`serve`]. Reasonable defaults for local-network
/// federation; bump `entry_ttl` if your peers refresh slowly.
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub entry_ttl: Duration,
    pub prune_interval: Duration,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            bind: "0.0.0.0:9000".parse().unwrap(),
            entry_ttl: Duration::from_secs(60),
            prune_interval: Duration::from_secs(15),
        }
    }
}

/// Boot the discovery server: starts the axum listener and a
/// background prune task. Returns when the server stops (Ctrl-C
/// handler is the caller's responsibility).
pub async fn serve(config: ServerConfig) -> Result<()> {
    let registry = DiscoveryRegistry::new(config.entry_ttl);

    // Background pruner — drops stale entries on a timer. The
    // /networks endpoint also filters on read, so this is mostly
    // about reclaiming map capacity, not visibility.
    let pruner_registry = registry.clone();
    let prune_interval = config.prune_interval;
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(prune_interval);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let removed = pruner_registry.prune();
            if removed > 0 {
                tracing::info!("pruner: removed {} stale entr(y/ies)", removed);
            }
        }
    });

    let app = router(registry);
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let actual = listener.local_addr()?;
    tracing::info!("reflow discovery server listening on http://{}", actual);

    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_then_list_round_trips() {
        let reg = DiscoveryRegistry::new(Duration::from_secs(30));
        reg.upsert(RegistrationRequest {
            network_id: "alpha".into(),
            instance_id: "alpha-1".into(),
            endpoint: "127.0.0.1:9100".into(),
            capabilities: vec!["actor_messaging".into()],
        });
        let listed = reg.list();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].network_id, "alpha");
    }

    #[test]
    fn prune_drops_stale_entries() {
        // 0s TTL means everything is immediately stale.
        let reg = DiscoveryRegistry::new(Duration::from_millis(1));
        reg.upsert(RegistrationRequest {
            network_id: "alpha".into(),
            instance_id: "alpha-1".into(),
            endpoint: "127.0.0.1:9100".into(),
            capabilities: vec![],
        });
        std::thread::sleep(Duration::from_millis(10));
        let removed = reg.prune();
        assert_eq!(removed, 1);
        assert!(reg.is_empty());
    }

    #[test]
    fn list_filters_stale_entries_without_pruning() {
        let reg = DiscoveryRegistry::new(Duration::from_millis(1));
        reg.upsert(RegistrationRequest {
            network_id: "alpha".into(),
            instance_id: "alpha-1".into(),
            endpoint: "127.0.0.1:9100".into(),
            capabilities: vec![],
        });
        std::thread::sleep(Duration::from_millis(10));
        // Without explicit prune, the entry is still in the map but
        // `list()` filters by TTL on read.
        assert_eq!(reg.len(), 1);
        assert!(reg.list().is_empty());
    }
}
