//! PostgreSQL trace storage (feature `postgres`).
//!
//! Mirrors the SQLite backend's model: each `FlowTrace` is stored as a single
//! (optionally zstd-compressed) JSON blob in a `traces` table, alongside
//! denormalized columns (`flow_id`, `execution_id`, `status`, `start_time`,
//! `end_time`, `event_count`) for indexed querying. This keeps the storage
//! schema-agnostic to the evolving `FlowTrace` shape while staying queryable.

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::DateTime;

use super::{StorageStats, TraceStorage};
use crate::config::PostgresConfig;
use reflow_tracing_protocol::{FlowTrace, TraceId, TraceQuery};

#[cfg(feature = "postgres")]
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
#[cfg(feature = "postgres")]
use std::time::Duration;

/// Compress with zstd above a threshold; returns (bytes, compressed?).
#[cfg(feature = "postgres")]
fn maybe_compress(serialized: Vec<u8>, threshold: usize) -> Result<(Vec<u8>, bool)> {
    if serialized.len() > threshold {
        let c = zstd::bulk::compress(&serialized, 3)
            .map_err(|e| anyhow!("zstd compress failed: {e}"))?;
        Ok((c, true))
    } else {
        Ok((serialized, false))
    }
}

#[cfg(feature = "postgres")]
fn maybe_decompress(data: Vec<u8>, compressed: bool) -> Result<Vec<u8>> {
    if compressed {
        zstd::bulk::decompress(&data, 64 * 1024 * 1024)
            .map_err(|e| anyhow!("zstd decompress failed: {e}"))
    } else {
        Ok(data)
    }
}

#[cfg(feature = "postgres")]
pub struct PostgresStorage {
    pool: PgPool,
    compress_threshold: usize,
}

#[cfg(feature = "postgres")]
impl PostgresStorage {
    pub async fn new(config: PostgresConfig) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections.max(1))
            .min_connections(config.min_connections)
            .acquire_timeout(Duration::from_secs(config.acquire_timeout_secs.max(1)))
            .connect(&config.connection_url)
            .await
            .map_err(|e| anyhow!("Failed to connect to PostgreSQL: {e}"))?;

        Self::create_tables(&pool).await?;
        Ok(Self {
            pool,
            compress_threshold: 1024,
        })
    }

    async fn create_tables(pool: &PgPool) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS traces (
                trace_id     TEXT PRIMARY KEY,
                flow_id      TEXT NOT NULL,
                execution_id TEXT NOT NULL,
                status       TEXT NOT NULL,
                start_time   BIGINT NOT NULL,
                end_time     BIGINT,
                event_count  BIGINT NOT NULL DEFAULT 0,
                data         BYTEA NOT NULL,
                compressed   BOOLEAN NOT NULL DEFAULT FALSE,
                size_bytes   BIGINT NOT NULL,
                created_at   BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT
            )
            "#,
        )
        .execute(pool)
        .await
        .map_err(|e| anyhow!("Failed to create traces table: {e}"))?;

        for idx in [
            "CREATE INDEX IF NOT EXISTS idx_traces_flow_id ON traces(flow_id)",
            "CREATE INDEX IF NOT EXISTS idx_traces_execution_id ON traces(execution_id)",
            "CREATE INDEX IF NOT EXISTS idx_traces_status ON traces(status)",
            "CREATE INDEX IF NOT EXISTS idx_traces_start_time ON traces(start_time)",
        ] {
            sqlx::query(idx)
                .execute(pool)
                .await
                .map_err(|e| anyhow!("Failed to create index: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(feature = "postgres")]
#[async_trait]
impl TraceStorage for PostgresStorage {
    async fn store_trace(&self, trace: FlowTrace) -> Result<TraceId> {
        let trace_id = trace.trace_id.clone();
        let serialized =
            serde_json::to_vec(&trace).map_err(|e| anyhow!("serialize trace: {e}"))?;
        let (data, compressed) = maybe_compress(serialized, self.compress_threshold)?;

        sqlx::query(
            r#"
            INSERT INTO traces
                (trace_id, flow_id, execution_id, status, start_time, end_time,
                 event_count, data, compressed, size_bytes)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (trace_id) DO UPDATE SET
                flow_id      = EXCLUDED.flow_id,
                execution_id = EXCLUDED.execution_id,
                status       = EXCLUDED.status,
                start_time   = EXCLUDED.start_time,
                end_time     = EXCLUDED.end_time,
                event_count  = EXCLUDED.event_count,
                data         = EXCLUDED.data,
                compressed   = EXCLUDED.compressed,
                size_bytes   = EXCLUDED.size_bytes
            "#,
        )
        .bind(trace.trace_id.0.to_string())
        .bind(&trace.flow_id.0)
        .bind(trace.execution_id.0.to_string())
        .bind(serde_json::to_string(&trace.status).unwrap_or_default())
        .bind(trace.start_time.timestamp())
        .bind(trace.end_time.map(|t| t.timestamp()))
        .bind(trace.events.len() as i64)
        .bind(&data)
        .bind(compressed)
        .bind(data.len() as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| anyhow!("insert trace: {e}"))?;

        Ok(trace_id)
    }

    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        let row = sqlx::query("SELECT data, compressed FROM traces WHERE trace_id = $1")
            .bind(trace_id.0.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow!("query trace: {e}"))?;

        match row {
            Some(row) => {
                let data: Vec<u8> = row.get("data");
                let compressed: bool = row.get("compressed");
                let bytes = maybe_decompress(data, compressed)?;
                let trace: FlowTrace =
                    serde_json::from_slice(&bytes).map_err(|e| anyhow!("deserialize trace: {e}"))?;
                Ok(Some(trace))
            }
            None => Ok(None),
        }
    }

    async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        // Build a parameterized query with $-placeholders.
        let mut sql = String::from("SELECT data, compressed FROM traces WHERE 1=1");
        let mut n = 0;
        // Collected bind values as strings/ints in order; bind by matching type below.
        let mut flow_id: Option<String> = None;
        let mut execution_id: Option<String> = None;
        let mut status: Option<String> = None;
        let mut time_range: Option<(i64, i64)> = None;

        if let Some(ref f) = query.flow_id {
            n += 1;
            sql.push_str(&format!(" AND flow_id = ${n}"));
            flow_id = Some(f.0.clone());
        }
        if let Some(ref e) = query.execution_id {
            n += 1;
            sql.push_str(&format!(" AND execution_id = ${n}"));
            execution_id = Some(e.0.to_string());
        }
        if let Some(ref s) = query.status {
            n += 1;
            sql.push_str(&format!(" AND status = ${n}"));
            status = Some(serde_json::to_string(s).unwrap_or_default());
        }
        if let Some((start, end)) = &query.time_range {
            let a = n + 1;
            let b = n + 2;
            n += 2;
            sql.push_str(&format!(" AND start_time BETWEEN ${a} AND ${b}"));
            time_range = Some((start.timestamp(), end.timestamp()));
        }
        sql.push_str(" ORDER BY start_time DESC");
        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit as i64));
        }
        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset as i64));
        }

        let mut q = sqlx::query(&sql);
        if let Some(v) = flow_id {
            q = q.bind(v);
        }
        if let Some(v) = execution_id {
            q = q.bind(v);
        }
        if let Some(v) = status {
            q = q.bind(v);
        }
        if let Some((a, b)) = time_range {
            q = q.bind(a).bind(b);
        }

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| anyhow!("query traces: {e}"))?;

        let mut traces = Vec::with_capacity(rows.len());
        for row in rows {
            let data: Vec<u8> = row.get("data");
            let compressed: bool = row.get("compressed");
            let bytes = maybe_decompress(data, compressed)?;
            let trace: FlowTrace =
                serde_json::from_slice(&bytes).map_err(|e| anyhow!("deserialize trace: {e}"))?;
            traces.push(trace);
        }
        Ok(traces)
    }

    async fn delete_trace(&self, trace_id: TraceId) -> Result<bool> {
        let affected = sqlx::query("DELETE FROM traces WHERE trace_id = $1")
            .bind(trace_id.0.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow!("delete trace: {e}"))?
            .rows_affected();
        Ok(affected > 0)
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        let total_traces: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM traces")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| anyhow!("count traces: {e}"))?;
        let total_events: Option<i64> =
            sqlx::query_scalar("SELECT COALESCE(SUM(event_count), 0) FROM traces")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| anyhow!("sum events: {e}"))?;
        let storage_size_bytes: Option<i64> =
            sqlx::query_scalar("SELECT COALESCE(SUM(size_bytes), 0) FROM traces")
                .fetch_one(&self.pool)
                .await
                .map_err(|e| anyhow!("sum size: {e}"))?;
        let oldest: Option<i64> = sqlx::query_scalar("SELECT MIN(start_time) FROM traces")
            .fetch_one(&self.pool)
            .await
            .ok()
            .flatten();
        let newest: Option<i64> = sqlx::query_scalar("SELECT MAX(start_time) FROM traces")
            .fetch_one(&self.pool)
            .await
            .ok()
            .flatten();

        Ok(StorageStats {
            total_traces: total_traces as usize,
            total_events: total_events.unwrap_or(0) as usize,
            storage_size_bytes: storage_size_bytes.unwrap_or(0) as usize,
            oldest_trace_timestamp: oldest.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            newest_trace_timestamp: newest.and_then(|ts| DateTime::from_timestamp(ts, 0)),
        })
    }
}

// Fallback stub when the feature is disabled, so `StorageBackend::create` can
// reference the type unconditionally and return a clear error.
#[cfg(not(feature = "postgres"))]
pub struct PostgresStorage;

#[cfg(not(feature = "postgres"))]
impl PostgresStorage {
    pub async fn new(_config: PostgresConfig) -> Result<Self> {
        Err(anyhow!(
            "PostgreSQL backend not compiled in — build reflow_tracing with --features postgres"
        ))
    }
}

#[cfg(not(feature = "postgres"))]
#[async_trait]
impl TraceStorage for PostgresStorage {
    async fn store_trace(&self, _trace: FlowTrace) -> Result<TraceId> {
        Err(anyhow!("PostgreSQL backend not compiled in"))
    }
    async fn get_trace(&self, _trace_id: TraceId) -> Result<Option<FlowTrace>> {
        Err(anyhow!("PostgreSQL backend not compiled in"))
    }
    async fn query_traces(&self, _query: TraceQuery) -> Result<Vec<FlowTrace>> {
        Err(anyhow!("PostgreSQL backend not compiled in"))
    }
    async fn delete_trace(&self, _trace_id: TraceId) -> Result<bool> {
        Err(anyhow!("PostgreSQL backend not compiled in"))
    }
    async fn get_stats(&self) -> Result<StorageStats> {
        Err(anyhow!("PostgreSQL backend not compiled in"))
    }
}
