//! TOML-shaped config for the `reflow-peer` CLI.

use std::path::Path;

use anyhow::{Context, Result};
use reflow_network::distributed_network::DistributedConfig;
use reflow_network::network::NetworkConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    /// Logical name of this network. Other peers address remote
    /// actors via `<actor_id>@<network_id>`.
    pub network_id: String,

    /// Instance identifier — distinguishes multiple processes that
    /// share a `network_id` (e.g. for HA).
    #[serde(default = "default_instance_id")]
    pub instance_id: String,

    /// Address the bridge listens on.
    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    /// Port the bridge listens on. WebSocket handshakes from other
    /// peers land here.
    pub bind_port: u16,

    /// Discovery server endpoints for self-registration and peer
    /// lookup. Empty list means "don't register; rely on
    /// `[[connect]]` entries below."
    #[serde(default)]
    pub discovery_endpoints: Vec<String>,

    /// Optional shared secret. When set, only peers presenting the
    /// same token are accepted.
    pub auth_token: Option<String>,

    #[serde(default = "default_max_connections")]
    pub max_connections: usize,

    #[serde(default = "default_heartbeat_ms")]
    pub heartbeat_interval_ms: u64,

    /// Peers to dial on startup. Useful when not using a discovery
    /// server, or to bootstrap before the discovery refresh kicks
    /// in.
    #[serde(default)]
    pub connect: Vec<ConnectEntry>,

    /// Remote actors to register as proxies in the local network.
    /// Each entry creates a `<actor_id>@<network_id>` proxy that
    /// forwards messages to the named remote network.
    #[serde(default)]
    pub remote_actor: Vec<RemoteActorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectEntry {
    /// `host:port` of a peer's bridge listener.
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteActorEntry {
    /// Name of the actor on the remote side.
    pub actor_id: String,
    /// `network_id` of the network hosting the actor.
    pub network_id: String,
}

fn default_instance_id() -> String {
    format!("peer-{}", &uuid())
}

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_max_connections() -> usize {
    32
}

fn default_heartbeat_ms() -> u64 {
    1000
}

fn uuid() -> String {
    // Lightweight unique id: `time-rand`. Avoids pulling in `uuid`
    // just for this default.
    let now = chrono::Utc::now().timestamp_micros() as u64;
    format!("{now:x}")
}

impl PeerConfig {
    pub fn from_path(path: &Path) -> Result<Self> {
        let body = std::fs::read_to_string(path)
            .with_context(|| format!("read peer config: {}", path.display()))?;
        let cfg: PeerConfig = toml::from_str(&body)
            .with_context(|| format!("parse peer config: {}", path.display()))?;
        Ok(cfg)
    }

    pub fn to_distributed_config(&self) -> DistributedConfig {
        DistributedConfig {
            network_id: self.network_id.clone(),
            instance_id: self.instance_id.clone(),
            bind_address: self.bind_address.clone(),
            bind_port: self.bind_port,
            discovery_endpoints: self.discovery_endpoints.clone(),
            auth_token: self.auth_token.clone(),
            max_connections: self.max_connections,
            heartbeat_interval_ms: self.heartbeat_interval_ms,
            local_network_config: NetworkConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_config() {
        let toml = r#"
            network_id = "alpha"
            bind_port = 9100
        "#;
        let cfg: PeerConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.network_id, "alpha");
        assert_eq!(cfg.bind_port, 9100);
        assert_eq!(cfg.bind_address, "0.0.0.0");
        assert!(cfg.discovery_endpoints.is_empty());
        assert!(cfg.connect.is_empty());
        assert!(cfg.remote_actor.is_empty());
    }

    #[test]
    fn parses_full_config() {
        let toml = r#"
            network_id = "alpha"
            instance_id = "alpha-1"
            bind_address = "127.0.0.1"
            bind_port = 9100
            discovery_endpoints = ["http://localhost:9000"]
            auth_token = "shared-secret"
            heartbeat_interval_ms = 500

            [[connect]]
            endpoint = "127.0.0.1:9101"

            [[remote_actor]]
            actor_id = "data_processor"
            network_id = "beta"
        "#;
        let cfg: PeerConfig = toml::from_str(toml).unwrap();
        assert_eq!(cfg.discovery_endpoints, vec!["http://localhost:9000"]);
        assert_eq!(cfg.auth_token.as_deref(), Some("shared-secret"));
        assert_eq!(cfg.connect.len(), 1);
        assert_eq!(cfg.remote_actor[0].network_id, "beta");
    }

    #[test]
    fn round_trips_through_distributed_config() {
        let cfg = PeerConfig {
            network_id: "alpha".into(),
            instance_id: "alpha-1".into(),
            bind_address: "127.0.0.1".into(),
            bind_port: 9100,
            discovery_endpoints: vec![],
            auth_token: None,
            max_connections: 32,
            heartbeat_interval_ms: 1000,
            connect: vec![],
            remote_actor: vec![],
        };
        let dc = cfg.to_distributed_config();
        assert_eq!(dc.network_id, "alpha");
        assert_eq!(dc.bind_port, 9100);
    }
}
