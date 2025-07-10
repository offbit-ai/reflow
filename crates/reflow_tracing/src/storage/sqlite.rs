use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params, ToSql};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::SqliteConfig;
use reflow_tracing_protocol::{FlowTrace, TraceId, TraceQuery};
use super::{StorageStats, TraceStorage};

pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    pub async fn new(config: SqliteConfig) -> Result<Self> {
        let conn = if config.database_path == ":memory:" {
            Connection::open_in_memory()?
        } else {
            // Ensure parent directory exists
            if let Some(parent) = Path::new(&config.database_path).parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            Connection::open(&config.database_path)?
        };

        let storage = Self {
            conn: Arc::new(Mutex::new(conn)),
        };

        storage.initialize_schema().await?;
        Ok(storage)
    }

    async fn initialize_schema(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        
        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS traces (
                trace_id TEXT PRIMARY KEY,
                flow_id TEXT NOT NULL,
                execution_id TEXT NOT NULL,
                version_major INTEGER NOT NULL,
                version_minor INTEGER NOT NULL,
                version_patch INTEGER NOT NULL,
                version_git_hash TEXT,
                version_timestamp INTEGER NOT NULL,
                start_time INTEGER NOT NULL,
                end_time INTEGER,
                status TEXT NOT NULL,
                metadata TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
            "#,
            [],
        )?;

        conn.execute(
            r#"
            CREATE TABLE IF NOT EXISTS trace_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trace_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                data TEXT NOT NULL,
                causality TEXT NOT NULL,
                FOREIGN KEY (trace_id) REFERENCES traces (trace_id) ON DELETE CASCADE
            )
            "#,
            [],
        )?;

        // Create indices for better query performance
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_traces_flow_id ON traces (flow_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_traces_execution_id ON traces (execution_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_traces_start_time ON traces (start_time)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trace_events_trace_id ON trace_events (trace_id)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_trace_events_timestamp ON trace_events (timestamp)",
            [],
        )?;

        Ok(())
    }
}

#[async_trait]
impl TraceStorage for SqliteStorage {
    async fn store_trace(&self, trace: FlowTrace) -> Result<TraceId> {
        let conn = self.conn.lock().await;
        
        let trace_id = trace.trace_id.clone();
        let trace_id_str = trace_id.0.to_string();
        
        // Insert trace record
        conn.execute(
            r#"
            INSERT OR REPLACE INTO traces 
            (trace_id, flow_id, execution_id, version_major, version_minor, version_patch, 
             version_git_hash, version_timestamp, start_time, end_time, status, metadata)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            params![
                trace_id_str,
                trace.flow_id.0,
                trace.execution_id.0.to_string(),
                trace.version.major,
                trace.version.minor,
                trace.version.patch,
                trace.version.git_hash,
                trace.version.timestamp.timestamp(),
                trace.start_time.timestamp(),
                trace.end_time.map(|dt| dt.timestamp()),
                serde_json::to_string(&trace.status)?,
                serde_json::to_string(&trace.metadata)?
            ],
        )?;

        // Insert trace events
        for event in &trace.events {
            conn.execute(
                r#"
                INSERT INTO trace_events 
                (trace_id, event_id, timestamp, event_type, actor_id, data, causality)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
                params![
                    trace_id_str,
                    event.event_id.0.to_string(),
                    event.timestamp.timestamp(),
                    serde_json::to_string(&event.event_type)?,
                    event.actor_id,
                    serde_json::to_string(&event.data)?,
                    serde_json::to_string(&event.causality)?
                ],
            )?;
        }

        Ok(trace_id)
    }

    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        let conn = self.conn.lock().await;
        let trace_id_str = trace_id.0.to_string();

        // Get trace record
        let mut stmt = conn.prepare(
            r#"
            SELECT trace_id, flow_id, execution_id, version_major, version_minor, version_patch,
                   version_git_hash, version_timestamp, start_time, end_time, status, metadata 
            FROM traces WHERE trace_id = ?1
            "#
        )?;

        let trace_row = stmt.query_row(params![trace_id_str], |row| {
            Ok((
                row.get::<_, String>(1)?, // flow_id
                row.get::<_, String>(2)?, // execution_id
                row.get::<_, u32>(3)?, // version_major
                row.get::<_, u32>(4)?, // version_minor
                row.get::<_, u32>(5)?, // version_patch
                row.get::<_, Option<String>>(6)?, // version_git_hash
                row.get::<_, i64>(7)?, // version_timestamp
                row.get::<_, i64>(8)?, // start_time
                row.get::<_, Option<i64>>(9)?, // end_time
                row.get::<_, String>(10)?, // status
                row.get::<_, String>(11)?, // metadata
            ))
        });

        let (flow_id, execution_id, version_major, version_minor, version_patch, 
             version_git_hash, version_timestamp, start_time, end_time, status, metadata) = match trace_row {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e.into()),
        };

        // Get trace events
        let mut stmt = conn.prepare(
            r#"
            SELECT event_id, timestamp, event_type, actor_id, data, causality
            FROM trace_events 
            WHERE trace_id = ?1 
            ORDER BY timestamp
            "#
        )?;

        let event_rows = stmt.query_map(params![trace_id_str], |row| {
            Ok((
                row.get::<_, String>(0)?, // event_id
                row.get::<_, i64>(1)?, // timestamp
                row.get::<_, String>(2)?, // event_type
                row.get::<_, String>(3)?, // actor_id
                row.get::<_, String>(4)?, // data
                row.get::<_, String>(5)?, // causality
            ))
        })?;

        let mut events = Vec::new();
        for event_row in event_rows {
            let (event_id_str, timestamp, event_type_str, actor_id, data_str, causality_str) = event_row?;
            
            let event_id = reflow_tracing_protocol::EventId(uuid::Uuid::parse_str(&event_id_str)?);
            let event_type = serde_json::from_str(&event_type_str)?;
            let data = serde_json::from_str(&data_str)?;
            let causality = serde_json::from_str(&causality_str)?;

            events.push(reflow_tracing_protocol::TraceEvent {
                event_id,
                timestamp: DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now),
                event_type,
                actor_id,
                data,
                causality,
            });
        }

        let execution_id = reflow_tracing_protocol::ExecutionId(uuid::Uuid::parse_str(&execution_id)?);
        let version = reflow_tracing_protocol::FlowVersion {
            major: version_major,
            minor: version_minor,
            patch: version_patch,
            git_hash: version_git_hash,
            timestamp: DateTime::from_timestamp(version_timestamp, 0).unwrap_or_else(Utc::now),
        };

        Ok(Some(FlowTrace {
            trace_id,
            flow_id: reflow_tracing_protocol::FlowId(flow_id),
            execution_id,
            version,
            start_time: DateTime::from_timestamp(start_time, 0).unwrap_or_else(Utc::now),
            end_time: end_time.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            status: serde_json::from_str(&status)?,
            events,
            metadata: serde_json::from_str(&metadata)?,
        }))
    }

    async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        // Collect trace IDs first without holding the connection across await
        let trace_id_strings = {
            let conn = self.conn.lock().await;
            
            let mut sql = "SELECT trace_id FROM traces WHERE 1=1".to_string();
            let mut params: Vec<String> = Vec::new();

            if let Some(flow_id) = &query.flow_id {
                sql.push_str(" AND flow_id = ?");
                params.push(flow_id.0.clone());
            }

            if let Some(execution_id) = &query.execution_id {
                sql.push_str(" AND execution_id = ?");
                params.push(execution_id.0.to_string());
            }

            if let Some((start_time, end_time)) = query.time_range {
                sql.push_str(" AND start_time >= ? AND start_time <= ?");
                params.push(start_time.timestamp().to_string());
                params.push(end_time.timestamp().to_string());
            }

            if let Some(status) = &query.status {
                sql.push_str(" AND status = ?");
                params.push(serde_json::to_string(status)?);
            }

            sql.push_str(" ORDER BY start_time DESC");

            if let Some(limit) = query.limit {
                sql.push_str(&format!(" LIMIT {}", limit));
            }

            if let Some(offset) = query.offset {
                sql.push_str(&format!(" OFFSET {}", offset));
            }

            let mut stmt = conn.prepare(&sql)?;
            
            let trace_ids: Result<Vec<String>, rusqlite::Error> = if params.is_empty() {
                stmt.query_map([], |row| row.get::<_, String>(0))?.collect()
            } else {
                let params_refs: Vec<&dyn ToSql> = params.iter().map(|s| s as &dyn ToSql).collect();
                stmt.query_map(params_refs.as_slice(), |row| row.get::<_, String>(0))?.collect()
            };

            trace_ids?
        };

        // Now fetch full traces using the collected IDs
        let mut traces = Vec::new();
        for trace_id_str in trace_id_strings {
            if let Ok(trace_id) = uuid::Uuid::parse_str(&trace_id_str) {
                if let Ok(Some(trace)) = self.get_trace(reflow_tracing_protocol::TraceId(trace_id)).await {
                    traces.push(trace);
                }
            }
        }

        Ok(traces)
    }

    async fn delete_trace(&self, trace_id: TraceId) -> Result<bool> {
        let conn = self.conn.lock().await;
        let trace_id_str = trace_id.0.to_string();

        let rows_affected = conn.execute(
            "DELETE FROM traces WHERE trace_id = ?1",
            params![trace_id_str],
        )?;

        Ok(rows_affected > 0)
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        let conn = self.conn.lock().await;

        let total_traces: usize = conn.query_row(
            "SELECT COUNT(*) FROM traces",
            [],
            |row| row.get(0),
        )?;

        let total_events: usize = conn.query_row(
            "SELECT COUNT(*) FROM trace_events",
            [],
            |row| row.get(0),
        )?;

        // Get storage size (rough estimate for SQLite)
        let storage_size_bytes: usize = conn.query_row(
            "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
            [],
            |row| row.get(0),
        ).unwrap_or(0);

        let oldest_trace_timestamp = conn.query_row(
            "SELECT MIN(start_time) FROM traces",
            [],
            |row| {
                let timestamp: Option<i64> = row.get(0)?;
                Ok(timestamp.and_then(|ts| DateTime::from_timestamp(ts, 0)))
            },
        ).unwrap_or(None);

        let newest_trace_timestamp = conn.query_row(
            "SELECT MAX(start_time) FROM traces",
            [],
            |row| {
                let timestamp: Option<i64> = row.get(0)?;
                Ok(timestamp.and_then(|ts| DateTime::from_timestamp(ts, 0)))
            },
        ).unwrap_or(None);

        Ok(StorageStats {
            total_traces,
            total_events,
            storage_size_bytes,
            oldest_trace_timestamp,
            newest_trace_timestamp,
        })
    }
}
