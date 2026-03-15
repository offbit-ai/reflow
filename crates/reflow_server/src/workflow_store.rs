//! Workflow graph persistence.
//!
//! Stores published workflow graphs for webhook triggering.
//! Uses Redis when available, falls back to in-memory DashMap.

use anyhow::Result;
use dashmap::DashMap;
use log::{info, warn};
use std::sync::Arc;
use tokio::sync::OnceCell;

/// Workflow store — persists graphs to Redis with in-memory fallback.
pub struct WorkflowStore {
    /// In-memory cache (always active).
    memory: Arc<DashMap<String, serde_json::Value>>,
    /// Redis connection (lazily initialized, optional).
    redis: OnceCell<Option<redis::aio::ConnectionManager>>,
    /// Redis URL from config.
    redis_url: Option<String>,
}

impl WorkflowStore {
    /// Create a new workflow store.
    ///
    /// If `redis_url` is provided, the store will attempt to connect to Redis.
    /// Operations always read/write the in-memory cache first, then sync to Redis.
    pub fn new(redis_url: Option<String>) -> Self {
        Self {
            memory: Arc::new(DashMap::new()),
            redis: OnceCell::new(),
            redis_url,
        }
    }

    /// Get or initialize the Redis connection.
    async fn redis_conn(&self) -> Option<&redis::aio::ConnectionManager> {
        let conn = self
            .redis
            .get_or_init(|| async {
                if let Some(url) = &self.redis_url {
                    match redis::Client::open(url.as_str()) {
                        Ok(client) => match redis::aio::ConnectionManager::new(client).await {
                            Ok(mgr) => {
                                info!("Workflow store connected to Redis at {}", url);
                                Some(mgr)
                            }
                            Err(e) => {
                                warn!("Redis connection failed (using in-memory): {}", e);
                                None
                            }
                        },
                        Err(e) => {
                            warn!("Invalid Redis URL '{}': {} (using in-memory)", url, e);
                            None
                        }
                    }
                } else {
                    None
                }
            })
            .await;
        conn.as_ref()
    }

    /// Redis key for a workflow graph.
    fn redis_key(workflow_id: &str) -> String {
        format!("reflow:workflow:{}", workflow_id)
    }

    /// Store a workflow graph.
    pub async fn store(&self, workflow_id: &str, graph: serde_json::Value) {
        // Always write to memory
        self.memory.insert(workflow_id.to_string(), graph.clone());

        // Sync to Redis if available
        if let Some(conn) = self.redis_conn().await {
            let mut conn = conn.clone();
            let key = Self::redis_key(workflow_id);
            let json = serde_json::to_string(&graph).unwrap_or_default();
            let result: Result<(), redis::RedisError> =
                redis::cmd("SET").arg(&key).arg(&json).query_async(&mut conn).await;
            if let Err(e) = result {
                warn!("Redis SET failed for '{}': {}", workflow_id, e);
            }
        }
    }

    /// Get a workflow graph. Tries memory first, then Redis.
    pub async fn get(&self, workflow_id: &str) -> Option<serde_json::Value> {
        // Try memory first
        if let Some(graph) = self.memory.get(workflow_id) {
            return Some(graph.value().clone());
        }

        // Try Redis
        if let Some(conn) = self.redis_conn().await {
            let mut conn = conn.clone();
            let key = Self::redis_key(workflow_id);
            let result: Result<Option<String>, redis::RedisError> =
                redis::cmd("GET").arg(&key).query_async(&mut conn).await;
            if let Ok(Some(json)) = result {
                if let Ok(graph) = serde_json::from_str::<serde_json::Value>(&json) {
                    // Populate memory cache
                    self.memory.insert(workflow_id.to_string(), graph.clone());
                    return Some(graph);
                }
            }
        }

        None
    }

    /// Remove a workflow graph.
    pub async fn remove(&self, workflow_id: &str) {
        self.memory.remove(workflow_id);

        if let Some(conn) = self.redis_conn().await {
            let mut conn = conn.clone();
            let key = Self::redis_key(workflow_id);
            let result: Result<(), redis::RedisError> =
                redis::cmd("DEL").arg(&key).query_async(&mut conn).await;
            if let Err(e) = result {
                warn!("Redis DEL failed for '{}': {}", workflow_id, e);
            }
        }
    }

    /// List all stored workflow IDs.
    pub async fn list(&self) -> Vec<String> {
        // Combine memory keys with Redis keys
        let mut ids: Vec<String> = self.memory.iter().map(|e| e.key().clone()).collect();

        if let Some(conn) = self.redis_conn().await {
            let mut conn = conn.clone();
            let result: Result<Vec<String>, redis::RedisError> =
                redis::cmd("KEYS").arg("reflow:workflow:*").query_async(&mut conn).await;
            if let Ok(keys) = result {
                for key in keys {
                    let id = key.strip_prefix("reflow:workflow:").unwrap_or(&key).to_string();
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }

        ids
    }

    /// Check if a workflow exists.
    pub async fn exists(&self, workflow_id: &str) -> bool {
        if self.memory.contains_key(workflow_id) {
            return true;
        }

        if let Some(conn) = self.redis_conn().await {
            let mut conn = conn.clone();
            let key = Self::redis_key(workflow_id);
            let result: Result<bool, redis::RedisError> =
                redis::cmd("EXISTS").arg(&key).query_async(&mut conn).await;
            return result.unwrap_or(false);
        }

        false
    }
}
