
use std::{collections::HashMap, sync::Arc, time::Duration};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::distributed_network::DistributedConfig;

pub struct DiscoveryService {
    config: DistributedConfig,
    known_networks: Arc<RwLock<HashMap<String, NetworkInfo>>>,
    registration_client: Option<reqwest::Client>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl DiscoveryService {
    pub fn new(config: DistributedConfig) -> Self {
        DiscoveryService {
            config,
            known_networks: Arc::new(RwLock::new(HashMap::new())),
            registration_client: Some(reqwest::Client::new()),
        }
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
                    .post(&format!("{}/register", endpoint))
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
                match client.get(&format!("{}/networks", endpoint)).send().await {
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

    async fn start_discovery_refresh(&self)-> Result<()> {
        // For this example, we'll just log that discovery refresh is disabled
        // In a real implementation, this would periodically refresh the known networks
        tracing::info!("Discovery refresh started (stub implementation)");
        Ok(())
    }
}
