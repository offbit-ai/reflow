use anyhow::Result;
use async_trait::async_trait;

use crate::config::StorageConfig;
use reflow_tracing_protocol::{FlowTrace, TraceId, TraceQuery};

pub mod memory;
pub mod sqlite;

#[async_trait]
pub trait TraceStorage: Send + Sync {
    async fn store_trace(&self, trace: FlowTrace) -> Result<TraceId>;
    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>>;
    async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>>;
    async fn delete_trace(&self, trace_id: TraceId) -> Result<bool>;
    async fn get_stats(&self) -> Result<StorageStats>;
}

#[derive(Debug, Clone)]
pub struct StorageStats {
    pub total_traces: usize,
    pub total_events: usize,
    pub storage_size_bytes: usize,
    pub oldest_trace_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_trace_timestamp: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct StorageBackend;

impl StorageBackend {
    pub async fn create(config: &StorageConfig) -> Result<Box<dyn TraceStorage>> {
        match config.backend.as_str() {
            "memory" => {
                let memory_config = config.memory.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Memory storage config missing"))?;
                Ok(Box::new(memory::MemoryStorage::new(memory_config.clone())))
            }
            "sqlite" => {
                let sqlite_config = config.sqlite.as_ref()
                    .ok_or_else(|| anyhow::anyhow!("SQLite storage config missing"))?;
                let storage = sqlite::SqliteStorage::new(sqlite_config.clone()).await?;
                Ok(Box::new(storage))
            }
            _ => Err(anyhow::anyhow!("Unsupported storage backend: {}", config.backend)),
        }
    }
}
