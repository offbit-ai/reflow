//! MongoDB trace storage (feature `mongodb`).
//!
//! Each `FlowTrace` is stored as one document keyed by its `trace_id` (`_id`),
//! with denormalized top-level fields (`flow_id`, `execution_id`, `status`,
//! `start_time`, `end_time`) for indexed querying and the full trace nested
//! under `trace`. Document storage maps naturally onto the JSON-shaped trace.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::DateTime;

use super::{StorageStats, TraceStorage};
use crate::config::MongoDbConfig;
use reflow_tracing_protocol::{FlowTrace, TraceId, TraceQuery};

#[cfg(feature = "mongodb")]
use {
    bson::{doc, Document},
    futures::TryStreamExt,
    mongodb::{Client, Collection},
    serde::{Deserialize, Serialize},
};

#[cfg(feature = "mongodb")]
#[derive(Serialize, Deserialize)]
struct TraceDoc {
    #[serde(rename = "_id")]
    id: String,
    flow_id: String,
    execution_id: String,
    status: String,
    start_time: i64,
    end_time: Option<i64>,
    event_count: i64,
    trace: FlowTrace,
}

#[cfg(feature = "mongodb")]
impl TraceDoc {
    fn from_trace(trace: FlowTrace) -> Self {
        Self {
            id: trace.trace_id.0.to_string(),
            flow_id: trace.flow_id.0.clone(),
            execution_id: trace.execution_id.0.to_string(),
            status: serde_json::to_string(&trace.status).unwrap_or_default(),
            start_time: trace.start_time.timestamp(),
            end_time: trace.end_time.map(|t| t.timestamp()),
            event_count: trace.events.len() as i64,
            trace,
        }
    }
}

#[cfg(feature = "mongodb")]
pub struct MongoStorage {
    collection: Collection<TraceDoc>,
}

#[cfg(feature = "mongodb")]
impl MongoStorage {
    pub async fn new(config: MongoDbConfig) -> Result<Self> {
        let client = Client::with_uri_str(&config.connection_url)
            .await
            .map_err(|e| anyhow!("Failed to connect to MongoDB: {e}"))?;
        let collection = client
            .database(&config.database_name)
            .collection::<TraceDoc>(&config.collection_name);
        // Best-effort indexes for the query columns.
        let db = client.database(&config.database_name);
        for key in ["flow_id", "execution_id", "status", "start_time"] {
            let _ = db
                .run_command(doc! {
                    "createIndexes": &config.collection_name,
                    "indexes": [ { "key": { key: 1 }, "name": format!("idx_{key}") } ],
                })
                .await;
        }
        Ok(Self { collection })
    }
}

#[cfg(feature = "mongodb")]
#[async_trait]
impl TraceStorage for MongoStorage {
    async fn store_trace(&self, trace: FlowTrace) -> Result<TraceId> {
        let trace_id = trace.trace_id.clone();
        let doc = TraceDoc::from_trace(trace);
        self.collection
            .replace_one(doc! { "_id": &doc.id }, &doc)
            .upsert(true)
            .await
            .map_err(|e| anyhow!("store trace: {e}"))?;
        Ok(trace_id)
    }

    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        let found = self
            .collection
            .find_one(doc! { "_id": trace_id.0.to_string() })
            .await
            .map_err(|e| anyhow!("get trace: {e}"))?;
        Ok(found.map(|d| d.trace))
    }

    async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        let mut filter = Document::new();
        if let Some(ref f) = query.flow_id {
            filter.insert("flow_id", &f.0);
        }
        if let Some(ref e) = query.execution_id {
            filter.insert("execution_id", e.0.to_string());
        }
        if let Some(ref s) = query.status {
            filter.insert("status", serde_json::to_string(s).unwrap_or_default());
        }
        if let Some((start, end)) = &query.time_range {
            filter.insert(
                "start_time",
                doc! { "$gte": start.timestamp(), "$lte": end.timestamp() },
            );
        }

        let mut find = self.collection.find(filter).sort(doc! { "start_time": -1 });
        if let Some(limit) = query.limit {
            find = find.limit(limit as i64);
        }
        if let Some(offset) = query.offset {
            find = find.skip(offset as u64);
        }
        let cursor = find.await.map_err(|e| anyhow!("query traces: {e}"))?;
        let docs: Vec<TraceDoc> = cursor
            .try_collect()
            .await
            .map_err(|e| anyhow!("collect traces: {e}"))?;
        Ok(docs.into_iter().map(|d| d.trace).collect())
    }

    async fn delete_trace(&self, trace_id: TraceId) -> Result<bool> {
        let res = self
            .collection
            .delete_one(doc! { "_id": trace_id.0.to_string() })
            .await
            .map_err(|e| anyhow!("delete trace: {e}"))?;
        Ok(res.deleted_count > 0)
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        let total_traces = self
            .collection
            .count_documents(doc! {})
            .await
            .map_err(|e| anyhow!("count traces: {e}"))? as usize;

        // Aggregate event totals and the time window.
        let mut total_events = 0usize;
        let mut oldest: Option<i64> = None;
        let mut newest: Option<i64> = None;
        let mut cursor = self
            .collection
            .find(doc! {})
            .await
            .map_err(|e| anyhow!("scan traces: {e}"))?;
        while let Some(d) = cursor.try_next().await.map_err(|e| anyhow!("scan: {e}"))? {
            total_events += d.event_count.max(0) as usize;
            oldest = Some(oldest.map_or(d.start_time, |o| o.min(d.start_time)));
            newest = Some(newest.map_or(d.start_time, |n| n.max(d.start_time)));
        }

        Ok(StorageStats {
            total_traces,
            total_events,
            storage_size_bytes: 0, // not cheaply available per-collection
            oldest_trace_timestamp: oldest.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            newest_trace_timestamp: newest.and_then(|ts| DateTime::from_timestamp(ts, 0)),
        })
    }
}

// Fallback stub when the feature is disabled.
#[cfg(not(feature = "mongodb"))]
pub struct MongoStorage;

#[cfg(not(feature = "mongodb"))]
impl MongoStorage {
    pub async fn new(_config: MongoDbConfig) -> Result<Self> {
        Err(anyhow!(
            "MongoDB backend not compiled in — build reflow_tracing with --features mongodb"
        ))
    }
}

#[cfg(not(feature = "mongodb"))]
#[async_trait]
impl TraceStorage for MongoStorage {
    async fn store_trace(&self, _trace: FlowTrace) -> Result<TraceId> {
        Err(anyhow!("MongoDB backend not compiled in"))
    }
    async fn get_trace(&self, _trace_id: TraceId) -> Result<Option<FlowTrace>> {
        Err(anyhow!("MongoDB backend not compiled in"))
    }
    async fn query_traces(&self, _query: TraceQuery) -> Result<Vec<FlowTrace>> {
        Err(anyhow!("MongoDB backend not compiled in"))
    }
    async fn delete_trace(&self, _trace_id: TraceId) -> Result<bool> {
        Err(anyhow!("MongoDB backend not compiled in"))
    }
    async fn get_stats(&self) -> Result<StorageStats> {
        Err(anyhow!("MongoDB backend not compiled in"))
    }
}
