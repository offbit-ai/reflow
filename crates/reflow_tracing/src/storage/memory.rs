use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use std::sync::Arc;

use crate::config::MemoryConfig;
use reflow_tracing_protocol::{FlowTrace, TraceId, TraceQuery};
use crate::storage::{StorageStats, TraceStorage};

pub struct MemoryStorage {
    traces: Arc<DashMap<TraceId, FlowTrace>>,
    config: MemoryConfig,
}

impl MemoryStorage {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            traces: Arc::new(DashMap::new()),
            config,
        }
    }

    fn check_capacity(&self) -> Result<()> {
        if self.traces.len() >= self.config.max_traces {
            // Apply eviction policy
            match self.config.eviction_policy.as_str() {
                "lru" => self.evict_lru()?,
                "fifo" => self.evict_fifo()?,
                "random" => self.evict_random()?,
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unknown eviction policy: {}",
                        self.config.eviction_policy
                    ));
                }
            }
        }
        Ok(())
    }

    fn evict_lru(&self) -> Result<()> {
        // Find the oldest trace by start_time
        let mut oldest_key: Option<TraceId> = None;
        let mut oldest_time: Option<DateTime<Utc>> = None;

        for entry in self.traces.iter() {
            let trace = entry.value();
            if oldest_time.is_none() || trace.start_time < oldest_time.unwrap() {
                oldest_time = Some(trace.start_time);
                oldest_key = Some(entry.key().clone());
            }
        }

        if let Some(key) = oldest_key {
            self.traces.remove(&key);
        }

        Ok(())
    }

    fn evict_fifo(&self) -> Result<()> {
        // Same as LRU for now, could be improved with insertion order tracking
        self.evict_lru()
    }

    fn evict_random(&self) -> Result<()> {
        // Remove a random entry
        if let Some(entry) = self.traces.iter().next() {
            let key = entry.key().clone();
            drop(entry);
            self.traces.remove(&key);
        }
        Ok(())
    }

    fn matches_query(&self, trace: &FlowTrace, query: &TraceQuery) -> bool {
        // Check flow_id filter
        if let Some(ref flow_id) = query.flow_id {
            if trace.flow_id != *flow_id {
                return false;
            }
        }

        // Check execution_id filter
        if let Some(ref execution_id) = query.execution_id {
            if trace.execution_id != *execution_id {
                return false;
            }
        }

        // Check time range filter
        if let Some((start, end)) = query.time_range {
            if trace.start_time < start || trace.start_time > end {
                return false;
            }
        }

        // Check status filter
        if let Some(ref status) = query.status {
            if trace.status != *status {
                return false;
            }
        }

        // Check actor filter (simple string contains for now)
        if let Some(ref actor_filter) = query.actor_filter {
            if !trace.events.iter().any(|e| e.actor_id.contains(actor_filter)) {
                return false;
            }
        }

        true
    }
}

#[async_trait]
impl TraceStorage for MemoryStorage {
    async fn store_trace(&self, trace: FlowTrace) -> Result<TraceId> {
        self.check_capacity()?;
        
        let trace_id = trace.trace_id.clone();
        self.traces.insert(trace_id.clone(), trace);
        
        Ok(trace_id)
    }

    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        Ok(self.traces.get(&trace_id).map(|entry| entry.value().clone()))
    }

    async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        let mut results: Vec<FlowTrace> = self
            .traces
            .iter()
            .filter(|entry| self.matches_query(entry.value(), &query))
            .map(|entry| entry.value().clone())
            .collect();

        // Sort by start_time (newest first)
        results.sort_by(|a, b| b.start_time.cmp(&a.start_time));

        // Apply pagination
        if let Some(offset) = query.offset {
            if offset >= results.len() {
                return Ok(Vec::new());
            }
            results = results.into_iter().skip(offset).collect();
        }

        if let Some(limit) = query.limit {
            results.truncate(limit);
        }

        Ok(results)
    }

    async fn delete_trace(&self, trace_id: TraceId) -> Result<bool> {
        Ok(self.traces.remove(&trace_id).is_some())
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        let traces: Vec<FlowTrace> = self.traces.iter().map(|entry| entry.value().clone()).collect();
        
        let total_traces = traces.len();
        let total_events: usize = traces.iter().map(|t| t.events.len()).sum();
        
        let mut oldest_trace_timestamp = None;
        let mut newest_trace_timestamp = None;
        
        for trace in &traces {
            if oldest_trace_timestamp.is_none() || trace.start_time < oldest_trace_timestamp.unwrap() {
                oldest_trace_timestamp = Some(trace.start_time);
            }
            if newest_trace_timestamp.is_none() || trace.start_time > newest_trace_timestamp.unwrap() {
                newest_trace_timestamp = Some(trace.start_time);
            }
        }

        // Rough estimate of memory usage
        let storage_size_bytes = traces.len() * std::mem::size_of::<FlowTrace>()
            + traces.iter().map(|t| t.events.len() * std::mem::size_of::<reflow_tracing_protocol::TraceEvent>()).sum::<usize>();

        Ok(StorageStats {
            total_traces,
            total_events,
            storage_size_bytes,
            oldest_trace_timestamp,
            newest_trace_timestamp,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reflow_tracing_protocol::*;
    use uuid::Uuid;

    fn create_test_trace() -> FlowTrace {
        FlowTrace {
            trace_id: TraceId::new(),
            flow_id: FlowId::new("test_flow"),
            execution_id: ExecutionId::new(),
            version: FlowVersion {
                major: 1,
                minor: 0,
                patch: 0,
                git_hash: None,
                timestamp: Utc::now(),
            },
            start_time: Utc::now(),
            end_time: None,
            status: ExecutionStatus::Running,
            events: Vec::new(),
            metadata: TraceMetadata {
                user_id: None,
                session_id: None,
                environment: "test".to_string(),
                hostname: "localhost".to_string(),
                process_id: 1234,
                thread_id: "main".to_string(),
                tags: std::collections::HashMap::new(),
            },
        }
    }

    #[tokio::test]
    async fn test_store_and_get_trace() {
        let config = MemoryConfig {
            max_traces: 100,
            eviction_policy: "lru".to_string(),
        };
        let storage = MemoryStorage::new(config);
        
        let trace = create_test_trace();
        let trace_id = trace.trace_id.clone();
        
        // Store trace
        storage.store_trace(trace.clone()).await.unwrap();
        
        // Retrieve trace
        let retrieved = storage.get_trace(trace_id).await.unwrap();
        assert!(retrieved.is_some());
        
        let retrieved_trace = retrieved.unwrap();
        assert_eq!(retrieved_trace.trace_id, trace.trace_id);
        assert_eq!(retrieved_trace.flow_id, trace.flow_id);
    }

    #[tokio::test]
    async fn test_eviction() {
        let config = MemoryConfig {
            max_traces: 2,
            eviction_policy: "lru".to_string(),
        };
        let storage = MemoryStorage::new(config);
        
        // Store 3 traces (should trigger eviction)
        let trace1 = create_test_trace();
        let trace2 = create_test_trace();
        let trace3 = create_test_trace();
        
        storage.store_trace(trace1.clone()).await.unwrap();
        storage.store_trace(trace2.clone()).await.unwrap();
        storage.store_trace(trace3.clone()).await.unwrap();
        
        // Should only have 2 traces now
        assert_eq!(storage.traces.len(), 2);
        
        // The first trace should be evicted
        assert!(storage.get_trace(trace1.trace_id).await.unwrap().is_none());
        assert!(storage.get_trace(trace2.trace_id).await.unwrap().is_some());
        assert!(storage.get_trace(trace3.trace_id).await.unwrap().is_some());
    }
}
