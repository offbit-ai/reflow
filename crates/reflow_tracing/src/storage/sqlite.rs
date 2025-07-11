use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::{HashMap, VecDeque, BTreeMap};
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{Duration, Instant};
use serde::{Serialize, Deserialize};

use crate::config::SqliteConfig;
use reflow_tracing_protocol::{FlowTrace, TraceId, TraceQuery, FlowId, FlowVersion, TraceEvent, ExecutionStatus};
use super::{StorageStats, TraceStorage};

#[derive(Debug, thiserror::Error)]
pub enum TraceStorageError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("Not found")]
    NotFound,
}

impl From<TraceStorageError> for anyhow::Error {
    fn from(err: TraceStorageError) -> Self {
        anyhow::anyhow!(err.to_string())
    }
}

#[cfg(feature = "storage")]
use sqlx::{SqlitePool, Row};

/// High-performance SQLite storage implementation with optimizations
#[cfg(feature = "storage")]
pub struct SqliteStorage {
    pool: SqlitePool,
    write_buffer: Arc<RwLock<WriteBuffer>>,
    compression_engine: CompressionEngine,
    indexing_engine: IndexingEngine,
    cache: Arc<RwLock<LruCache>>,
    metrics: StorageMetrics,
    config: SqliteStorageConfig,
}

#[derive(Debug, Clone)]
pub struct SqliteStorageConfig {
    pub connection_pool_size: u32,
    pub write_buffer_size: usize,
    pub flush_interval_ms: u64,
    pub compression_threshold_bytes: usize,
    pub cache_size_mb: usize,
    pub enable_wal: bool,
    pub enable_foreign_keys: bool,
    pub vacuum_interval_hours: u64,
}

impl Default for SqliteStorageConfig {
    fn default() -> Self {
        Self {
            connection_pool_size: num_cpus::get() as u32,
            write_buffer_size: 1000,
            flush_interval_ms: 1000,
            compression_threshold_bytes: 1024,
            cache_size_mb: 256,
            enable_wal: true,
            enable_foreign_keys: true,
            vacuum_interval_hours: 24,
        }
    }
}

impl From<SqliteConfig> for SqliteStorageConfig {
    fn from(config: SqliteConfig) -> Self {
        Self {
            connection_pool_size: 4,
            write_buffer_size: 1000,
            flush_interval_ms: 1000,
            compression_threshold_bytes: 1024,
            cache_size_mb: config.cache_size.abs() as usize / 1024, // Convert from KB
            enable_wal: config.wal_mode,
            enable_foreign_keys: true,
            vacuum_interval_hours: 24,
        }
    }
}

/// Write buffer for batch operations
struct WriteBuffer {
    traces: VecDeque<FlowTrace>,
    events: VecDeque<(TraceId, TraceEvent)>,
    last_flush: Instant,
    pending_size: usize,
}

/// Advanced compression engine with multiple algorithms
pub struct CompressionEngine {
    algorithm: CompressionAlgorithm,
    level: CompressionLevel,
    dictionary: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub enum CompressionAlgorithm {
    Zstd,
    Lz4,
    Brotli,
    Gzip,
}

#[derive(Debug, Clone)]
pub enum CompressionLevel {
    Fast,
    Balanced,
    BestCompression,
}

/// Indexing engine for fast queries
pub struct IndexingEngine {
    bloom_filters: HashMap<String, BloomFilter>,
    time_index: BTreeMap<DateTime<Utc>, Vec<TraceId>>,
    actor_index: HashMap<String, Vec<TraceId>>,
    status_index: HashMap<ExecutionStatus, Vec<TraceId>>,
}

/// Simple Bloom filter implementation
pub struct BloomFilter {
    bits: Vec<bool>,
    hash_functions: usize,
    size: usize,
}

/// LRU cache for frequently accessed traces
pub struct LruCache {
    capacity: usize,
    cache: HashMap<TraceId, CacheEntry>,
    usage_order: VecDeque<TraceId>,
    size_bytes: usize,
}

#[derive(Clone)]
struct CacheEntry {
    trace: FlowTrace,
    last_accessed: Instant,
    size_bytes: usize,
}

/// Storage performance metrics
#[derive(Debug, Clone)]
pub struct StorageMetrics {
    pub reads_total: u64,
    pub writes_total: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub compression_ratio: f64,
    pub avg_read_time_ms: f64,
    pub avg_write_time_ms: f64,
    pub storage_size_bytes: u64,
}

#[cfg(feature = "storage")]
impl SqliteStorage {
    pub async fn new(config: SqliteConfig) -> Result<Self> {
        let database_url = if config.database_path == ":memory:" {
            "sqlite::memory:".to_string()
        } else {
            format!("sqlite:{}", config.database_path)
        };
        
        let storage_config = SqliteStorageConfig::from(config);
        
        // Create connection pool
        let pool = SqlitePool::connect(&database_url).await
            .map_err(|e| anyhow::anyhow!("Failed to connect to SQLite: {}", e))?;

        // Configure SQLite for performance
        if storage_config.enable_wal {
            sqlx::query("PRAGMA journal_mode = WAL").execute(&pool).await
                .map_err(|e| anyhow::anyhow!("Failed to set WAL mode: {}", e))?;
        }

        if storage_config.enable_foreign_keys {
            sqlx::query("PRAGMA foreign_keys = ON").execute(&pool).await
                .map_err(|e| anyhow::anyhow!("Failed to enable foreign keys: {}", e))?;
        }

        // Set cache size
        let cache_size_kb = storage_config.cache_size_mb * 1024;
        sqlx::query(&format!("PRAGMA cache_size = -{}", cache_size_kb)).execute(&pool).await
            .map_err(|e| anyhow::anyhow!("Failed to set cache size: {}", e))?;

        // Create tables if they don't exist
        Self::create_tables(&pool).await?;

        let storage = Self {
            pool,
            write_buffer: Arc::new(RwLock::new(WriteBuffer::new())),
            compression_engine: CompressionEngine::new(CompressionAlgorithm::Zstd, CompressionLevel::Balanced),
            indexing_engine: IndexingEngine::new(),
            cache: Arc::new(RwLock::new(LruCache::new(storage_config.cache_size_mb * 1024 * 1024))),
            metrics: StorageMetrics::default(),
            config: storage_config,
        };

        // Start background tasks
        storage.start_background_tasks();

        Ok(storage)
    }

    async fn create_tables(pool: &SqlitePool) -> Result<()> {
        // Create optimized schema with proper indexing
        sqlx::query(r#"
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
                data BLOB NOT NULL,
                compressed BOOLEAN NOT NULL DEFAULT FALSE,
                size_bytes INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
        "#).execute(pool).await
            .map_err(|e| anyhow::anyhow!("Failed to create traces table: {}", e))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS trace_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                trace_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                event_type TEXT NOT NULL,
                actor_id TEXT NOT NULL,
                data TEXT NOT NULL,
                causality TEXT NOT NULL,
                compressed BOOLEAN NOT NULL DEFAULT FALSE,
                FOREIGN KEY (trace_id) REFERENCES traces (trace_id) ON DELETE CASCADE
            )
        "#).execute(pool).await
            .map_err(|e| anyhow::anyhow!("Failed to create trace_events table: {}", e))?;

        // Create performance indexes
        let indexes = vec![
            "CREATE INDEX IF NOT EXISTS idx_traces_flow_id ON traces(flow_id)",
            "CREATE INDEX IF NOT EXISTS idx_traces_execution_id ON traces(execution_id)",
            "CREATE INDEX IF NOT EXISTS idx_traces_status ON traces(status)",
            "CREATE INDEX IF NOT EXISTS idx_traces_start_time ON traces(start_time)",
            "CREATE INDEX IF NOT EXISTS idx_trace_events_trace_id ON trace_events(trace_id)",
            "CREATE INDEX IF NOT EXISTS idx_trace_events_actor_id ON trace_events(actor_id)",
            "CREATE INDEX IF NOT EXISTS idx_trace_events_timestamp ON trace_events(timestamp)",
            "CREATE INDEX IF NOT EXISTS idx_trace_events_event_type ON trace_events(event_type)",
        ];

        for index_sql in indexes {
            sqlx::query(index_sql).execute(pool).await
                .map_err(|e| anyhow::anyhow!("Failed to create index: {}", e))?;
        }

        Ok(())
    }

    fn start_background_tasks(&self) {
        let write_buffer = self.write_buffer.clone();
        let pool = self.pool.clone();
        let config = self.config.clone();
        let compression_engine = self.compression_engine.clone();

        // Periodic flush task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(config.flush_interval_ms));
            loop {
                interval.tick().await;
                if let Err(e) = Self::flush_write_buffer(&write_buffer, &pool, &compression_engine).await {
                    eprintln!("Failed to flush write buffer: {}", e);
                }
            }
        });

        // Vacuum task
        let pool_vacuum = self.pool.clone();
        let vacuum_interval = self.config.vacuum_interval_hours;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(vacuum_interval * 3600));
            loop {
                interval.tick().await;
                if let Err(e) = sqlx::query("VACUUM").execute(&pool_vacuum).await {
                    eprintln!("Failed to vacuum database: {}", e);
                }
            }
        });
    }

    async fn flush_write_buffer(
        write_buffer: &Arc<RwLock<WriteBuffer>>,
        pool: &SqlitePool,
        compression_engine: &CompressionEngine,
    ) -> Result<()> {
        let (traces, events) = {
            let mut buffer = write_buffer.write().await;
            if buffer.traces.is_empty() && buffer.events.is_empty() {
                return Ok(());
            }

            let traces: Vec<_> = buffer.traces.drain(..).collect();
            let events: Vec<_> = buffer.events.drain(..).collect();
            buffer.pending_size = 0;
            buffer.last_flush = Instant::now();
            (traces, events)
        };

        if !traces.is_empty() {
            Self::batch_store_traces(pool, &traces, compression_engine).await?;
        }

        if !events.is_empty() {
            Self::batch_store_events(pool, &events, compression_engine).await?;
        }

        Ok(())
    }

    async fn batch_store_traces(
        pool: &SqlitePool,
        traces: &[FlowTrace],
        compression_engine: &CompressionEngine,
    ) -> Result<()> {
        let mut tx = pool.begin().await
            .map_err(|e| anyhow::anyhow!("Failed to start transaction: {}", e))?;

        for trace in traces {
            let serialized = serde_json::to_vec(trace)
                .map_err(|e| anyhow::anyhow!("Failed to serialize trace: {}", e))?;

            let (data, compressed) = if serialized.len() > 1024 {
                (compression_engine.compress(&serialized)?, true)
            } else {
                (serialized, false)
            };

            let start_time = trace.start_time.timestamp();
            let end_time = trace.end_time.map(|t| t.timestamp());

            sqlx::query(r#"
                INSERT OR REPLACE INTO traces 
                (trace_id, flow_id, execution_id, version_major, version_minor, version_patch,
                 version_git_hash, version_timestamp, start_time, end_time, status, metadata, data, compressed, size_bytes)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#)
            .bind(&trace.trace_id.0.to_string())
            .bind(&trace.flow_id.0)
            .bind(&trace.execution_id.0.to_string())
            .bind(trace.version.major as i64)
            .bind(trace.version.minor as i64)
            .bind(trace.version.patch as i64)
            .bind(&trace.version.git_hash)
            .bind(trace.version.timestamp.timestamp())
            .bind(start_time)
            .bind(end_time)
            .bind(serde_json::to_string(&trace.status).unwrap())
            .bind(serde_json::to_string(&trace.metadata).unwrap())
            .bind(&data)
            .bind(compressed)
            .bind(data.len() as i64)
            .execute(&mut *tx).await
            .map_err(|e| anyhow::anyhow!("Failed to insert trace: {}", e))?;
        }

        tx.commit().await
            .map_err(|e| anyhow::anyhow!("Failed to commit transaction: {}", e))?;

        Ok(())
    }

    async fn batch_store_events(
        pool: &SqlitePool,
        events: &[(TraceId, TraceEvent)],
        compression_engine: &CompressionEngine,
    ) -> Result<()> {
        let mut tx = pool.begin().await
            .map_err(|e| anyhow::anyhow!("Failed to start transaction: {}", e))?;

        for (trace_id, event) in events {
            let serialized = serde_json::to_vec(event)
                .map_err(|e| anyhow::anyhow!("Failed to serialize event: {}", e))?;

            let (data, compressed) = if serialized.len() > 512 {
                (compression_engine.compress(&serialized)?, true)
            } else {
                (serialized, false)
            };

            sqlx::query(r#"
                INSERT OR REPLACE INTO trace_events 
                (event_id, trace_id, actor_id, event_type, timestamp, data, causality, compressed)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#)
            .bind(&event.event_id.0.to_string())
            .bind(&trace_id.0.to_string())
            .bind(&event.actor_id)
            .bind(serde_json::to_string(&event.event_type).unwrap())
            .bind(event.timestamp.timestamp())
            .bind(serde_json::to_string(&event.data).unwrap())
            .bind(serde_json::to_string(&event.causality).unwrap())
            .bind(compressed)
            .execute(&mut *tx).await
            .map_err(|e| anyhow::anyhow!("Failed to insert event: {}", e))?;
        }

        tx.commit().await
            .map_err(|e| anyhow::anyhow!("Failed to commit transaction: {}", e))?;

        Ok(())
    }
}

#[cfg(feature = "storage")]
#[async_trait]
impl TraceStorage for SqliteStorage {
    async fn store_trace(&self, trace: FlowTrace) -> Result<TraceId> {
        let trace_id = trace.trace_id.clone();
        
        // Add to write buffer for batch processing
        let mut buffer = self.write_buffer.write().await;
        buffer.traces.push_back(trace);
        buffer.pending_size += 1;

        // Force flush if buffer is full
        if buffer.pending_size >= self.config.write_buffer_size {
            drop(buffer); // Release lock before flush
            Self::flush_write_buffer(&self.write_buffer, &self.pool, &self.compression_engine).await?;
        }

        Ok(trace_id)
    }

    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        // Check cache first
        let cache_result = {
            let mut cache = self.cache.write().await;
            cache.get(&trace_id)
        };

        if let Some(trace) = cache_result {
            return Ok(Some(trace));
        }

        // Load from database
        let row = sqlx::query(
            "SELECT data, compressed FROM traces WHERE trace_id = ?"
        )
        .bind(&trace_id.0.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to query trace: {}", e))?;

        if let Some(row) = row {
            let data: Vec<u8> = row.get("data");
            let compressed: bool = row.get("compressed");

            let decompressed_data = if compressed {
                self.compression_engine.decompress(&data)?
            } else {
                data
            };

            let trace: FlowTrace = serde_json::from_slice(&decompressed_data)
                .map_err(|e| anyhow::anyhow!("Failed to deserialize trace: {}", e))?;

            // Add to cache
            {
                let mut cache = self.cache.write().await;
                cache.insert(trace_id, trace.clone());
            }

            Ok(Some(trace))
        } else {
            Ok(None)
        }
    }

    async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        // Build optimized SQL query with proper indexing
        let mut sql = String::from("SELECT trace_id FROM traces WHERE 1=1");
        let mut params = Vec::new();

        if let Some(ref flow_id) = query.flow_id {
            sql.push_str(" AND flow_id = ?");
            params.push(flow_id.0.clone());
        }

        if let Some(ref execution_id) = query.execution_id {
            sql.push_str(" AND execution_id = ?");
            params.push(execution_id.0.to_string());
        }

        if let Some(ref status) = query.status {
            sql.push_str(" AND status = ?");
            params.push(serde_json::to_string(status).unwrap());
        }

        if let Some((start, end)) = &query.time_range {
            sql.push_str(" AND start_time BETWEEN ? AND ?");
            params.push(start.timestamp().to_string());
            params.push(end.timestamp().to_string());
        }

        sql.push_str(" ORDER BY start_time DESC");

        if let Some(limit) = query.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = query.offset {
            sql.push_str(&format!(" OFFSET {}", offset));
        }

        // Execute query and load traces
        let mut query_builder = sqlx::query(&sql);
        for param in &params {
            query_builder = query_builder.bind(param);
        }

        let rows = query_builder.fetch_all(&self.pool).await
            .map_err(|e| anyhow::anyhow!("Failed to query traces: {}", e))?;

        let trace_ids: Result<Vec<TraceId>, _> = rows.iter()
            .map(|row| {
                let id: String = row.get("trace_id");
                uuid::Uuid::parse_str(&id)
                    .map(TraceId)
                    .map_err(|e| anyhow::anyhow!("Invalid trace ID: {}", e))
            })
            .collect();

        let trace_ids = trace_ids?;

        // Load traces in parallel
        let semaphore = Arc::new(Semaphore::new(10)); // Limit concurrent loads
        let futures: Vec<_> = trace_ids.into_iter().map(|trace_id| {
            let semaphore = semaphore.clone();
            let storage = self;
            async move {
                let _permit = semaphore.acquire().await.unwrap();
                storage.get_trace(trace_id).await
            }
        }).collect();

        let results = futures::future::join_all(futures).await;
        let traces: Result<Vec<_>, _> = results.into_iter()
            .map(|result| result?.ok_or_else(|| anyhow::anyhow!("Trace not found")))
            .collect();

        Ok(traces?)
    }

    async fn delete_trace(&self, trace_id: TraceId) -> Result<bool> {
        let rows_affected = sqlx::query("DELETE FROM traces WHERE trace_id = ?")
            .bind(&trace_id.0.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to delete trace: {}", e))?
            .rows_affected();

        Ok(rows_affected > 0)
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        let total_traces: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM traces")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to count traces: {}", e))?;

        let total_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM trace_events")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to count events: {}", e))?;

        let storage_size_bytes: i64 = sqlx::query_scalar(
            "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()"
        )
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);

        let oldest_trace_timestamp: Option<i64> = sqlx::query_scalar("SELECT MIN(start_time) FROM traces")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get oldest trace: {}", e))?
            .flatten();

        let newest_trace_timestamp: Option<i64> = sqlx::query_scalar("SELECT MAX(start_time) FROM traces")
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get newest trace: {}", e))?
            .flatten();

        Ok(StorageStats {
            total_traces: total_traces as usize,
            total_events: total_events as usize,
            storage_size_bytes: storage_size_bytes as usize,
            oldest_trace_timestamp: oldest_trace_timestamp.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            newest_trace_timestamp: newest_trace_timestamp.and_then(|ts| DateTime::from_timestamp(ts, 0)),
        })
    }
}

// Simplified fallback implementation when storage feature is disabled
#[cfg(not(feature = "storage"))]
pub struct SqliteStorage;

#[cfg(not(feature = "storage"))]
impl SqliteStorage {
    pub async fn new(_config: SqliteConfig) -> Result<Self> {
        Err(anyhow::anyhow!("Storage feature is not enabled"))
    }
}

#[cfg(not(feature = "storage"))]
#[async_trait]
impl TraceStorage for SqliteStorage {
    async fn store_trace(&self, _trace: FlowTrace) -> Result<TraceId> {
        Err(anyhow::anyhow!("Storage feature is not enabled"))
    }

    async fn get_trace(&self, _trace_id: TraceId) -> Result<Option<FlowTrace>> {
        Err(anyhow::anyhow!("Storage feature is not enabled"))
    }

    async fn query_traces(&self, _query: TraceQuery) -> Result<Vec<FlowTrace>> {
        Err(anyhow::anyhow!("Storage feature is not enabled"))
    }

    async fn delete_trace(&self, _trace_id: TraceId) -> Result<bool> {
        Err(anyhow::anyhow!("Storage feature is not enabled"))
    }

    async fn get_stats(&self) -> Result<StorageStats> {
        Err(anyhow::anyhow!("Storage feature is not enabled"))
    }
}

impl CompressionEngine {
    pub fn new(algorithm: CompressionAlgorithm, level: CompressionLevel) -> Self {
        Self {
            algorithm,
            level,
            dictionary: None,
        }
    }

    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.algorithm {
            CompressionAlgorithm::Zstd => {
                let level = match self.level {
                    CompressionLevel::Fast => 1,
                    CompressionLevel::Balanced => 3,
                    CompressionLevel::BestCompression => 22,
                };
                zstd::bulk::compress(data, level)
                    .map_err(|e| anyhow::anyhow!("Zstd compression failed: {}", e))
            }
            CompressionAlgorithm::Lz4 => {
                Ok(lz4_flex::compress_prepend_size(data))
            }
            CompressionAlgorithm::Gzip => {
                use flate2::write::GzEncoder;
                use flate2::Compression;
                use std::io::Write;

                let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(data)
                    .map_err(|e| anyhow::anyhow!("Gzip write failed: {}", e))?;
                encoder.finish()
                    .map_err(|e| anyhow::anyhow!("Gzip finish failed: {}", e))
            }
            CompressionAlgorithm::Brotli => {
                use brotli::enc::BrotliEncoderParams;
                let params = BrotliEncoderParams::default();
                let mut output = Vec::new();
                brotli::BrotliCompress(&mut data.as_ref(), &mut output, &params)
                    .map_err(|e| anyhow::anyhow!("Brotli compression failed: {}", e))?;
                Ok(output)
            }
        }
    }

    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.algorithm {
            CompressionAlgorithm::Zstd => {
                zstd::bulk::decompress(data, 10 * 1024 * 1024) // 10MB limit
                    .map_err(|e| anyhow::anyhow!("Zstd decompression failed: {}", e))
            }
            CompressionAlgorithm::Lz4 => {
                lz4_flex::decompress_size_prepended(data)
                    .map_err(|e| anyhow::anyhow!("Lz4 decompression failed: {}", e))
            }
            CompressionAlgorithm::Gzip => {
                use flate2::read::GzDecoder;
                use std::io::Read;

                let mut decoder = GzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)
                    .map_err(|e| anyhow::anyhow!("Gzip decompression failed: {}", e))?;
                Ok(decompressed)
            }
            CompressionAlgorithm::Brotli => {
                let mut output = Vec::new();
                brotli::BrotliDecompress(&mut data.as_ref(), &mut output)
                    .map_err(|e| anyhow::anyhow!("Brotli decompression failed: {}", e))?;
                Ok(output)
            }
        }
    }
}

impl Clone for CompressionEngine {
    fn clone(&self) -> Self {
        Self {
            algorithm: self.algorithm.clone(),
            level: self.level.clone(),
            dictionary: self.dictionary.clone(),
        }
    }
}

impl WriteBuffer {
    fn new() -> Self {
        Self {
            traces: VecDeque::new(),
            events: VecDeque::new(),
            last_flush: Instant::now(),
            pending_size: 0,
        }
    }
}

impl LruCache {
    fn new(capacity_bytes: usize) -> Self {
        Self {
            capacity: capacity_bytes,
            cache: HashMap::new(),
            usage_order: VecDeque::new(),
            size_bytes: 0,
        }
    }

    fn get(&mut self, trace_id: &TraceId) -> Option<FlowTrace> {
        if let Some(entry) = self.cache.get_mut(trace_id) {
            entry.last_accessed = Instant::now();
            
            // Move to front of usage order
            self.usage_order.retain(|id| id != trace_id);
            self.usage_order.push_front(trace_id.clone());
            
            Some(entry.trace.clone())
        } else {
            None
        }
    }

    fn insert(&mut self, trace_id: TraceId, trace: FlowTrace) {
        let entry_size = estimate_trace_size(&trace);
        
        // Remove old entry if exists
        if let Some(old_entry) = self.cache.remove(&trace_id) {
            self.size_bytes -= old_entry.size_bytes;
            self.usage_order.retain(|id| id != &trace_id);
        }

        // Evict entries if necessary
        while self.size_bytes + entry_size > self.capacity && !self.usage_order.is_empty() {
            if let Some(lru_id) = self.usage_order.pop_back() {
                if let Some(entry) = self.cache.remove(&lru_id) {
                    self.size_bytes -= entry.size_bytes;
                }
            }
        }

        // Insert new entry
        let entry = CacheEntry {
            trace,
            last_accessed: Instant::now(),
            size_bytes: entry_size,
        };

        self.cache.insert(trace_id.clone(), entry);
        self.usage_order.push_front(trace_id);
        self.size_bytes += entry_size;
    }
}

impl BloomFilter {
    fn new(expected_elements: usize, false_positive_rate: f64) -> Self {
        let size = Self::optimal_size(expected_elements, false_positive_rate);
        let hash_functions = Self::optimal_hash_functions(size, expected_elements);
        
        Self {
            bits: vec![false; size],
            hash_functions,
            size,
        }
    }

    fn optimal_size(n: usize, p: f64) -> usize {
        (-(n as f64) * p.ln() / (2.0_f64.ln().powi(2))).ceil() as usize
    }

    fn optimal_hash_functions(m: usize, n: usize) -> usize {
        ((m as f64 / n as f64) * 2.0_f64.ln()).round() as usize
    }

    fn add(&mut self, item: &str) {
        for i in 0..self.hash_functions {
            let hash = self.hash(item, i);
            self.bits[hash % self.size] = true;
        }
    }

    fn contains(&self, item: &str) -> bool {
        for i in 0..self.hash_functions {
            let hash = self.hash(item, i);
            if !self.bits[hash % self.size] {
                return false;
            }
        }
        true
    }

    fn hash(&self, item: &str, seed: usize) -> usize {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        seed.hash(&mut hasher);
        hasher.finish() as usize
    }
}

impl IndexingEngine {
    fn new() -> Self {
        Self {
            bloom_filters: HashMap::new(),
            time_index: BTreeMap::new(),
            actor_index: HashMap::new(),
            status_index: HashMap::new(),
        }
    }

    fn add_trace(&mut self, trace: &FlowTrace) {
        // Add to time index
        self.time_index
            .entry(trace.start_time)
            .or_insert_with(Vec::new)
            .push(trace.trace_id.clone());

        // Add to status index
        self.status_index
            .entry(trace.status.clone())
            .or_insert_with(Vec::new)
            .push(trace.trace_id.clone());

        // Add actors to actor index and bloom filter
        let mut actor_bloom = self.bloom_filters
            .entry("actors".to_string())
            .or_insert_with(|| BloomFilter::new(10000, 0.01));

        for event in &trace.events {
            actor_bloom.add(&event.actor_id);
            
            self.actor_index
                .entry(event.actor_id.clone())
                .or_insert_with(Vec::new)
                .push(trace.trace_id.clone());
        }
    }

    fn query_by_time_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<TraceId> {
        self.time_index
            .range(start..=end)
            .flat_map(|(_, trace_ids)| trace_ids.iter().cloned())
            .collect()
    }

    fn query_by_actor(&self, actor_id: &str) -> Option<&Vec<TraceId>> {
        // Check bloom filter first
        if let Some(bloom) = self.bloom_filters.get("actors") {
            if !bloom.contains(actor_id) {
                return None;
            }
        }

        self.actor_index.get(actor_id)
    }
}

impl Default for StorageMetrics {
    fn default() -> Self {
        Self {
            reads_total: 0,
            writes_total: 0,
            cache_hits: 0,
            cache_misses: 0,
            compression_ratio: 0.0,
            avg_read_time_ms: 0.0,
            avg_write_time_ms: 0.0,
            storage_size_bytes: 0,
        }
    }
}

fn estimate_trace_size(trace: &FlowTrace) -> usize {
    // Rough estimation of trace size in memory
    let base_size = std::mem::size_of::<FlowTrace>();
    let events_size = trace.events.len() * std::mem::size_of::<TraceEvent>();
    let string_sizes = trace.flow_id.0.len() + 
                      trace.execution_id.0.to_string().len() +
                      trace.events.iter().map(|e| e.actor_id.len()).sum::<usize>();
    
    base_size + events_size + string_sizes
}
