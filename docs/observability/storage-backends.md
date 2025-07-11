# Storage Backends

Reflow's observability framework supports multiple storage backends to accommodate different operational requirements, from development and testing to large-scale production deployments.

## Overview

The tracing system provides a pluggable storage architecture that allows you to choose the most appropriate backend for your needs:

- **Memory Storage**: Fast, ephemeral storage for development and testing
- **SQLite Storage**: Lightweight, embedded database for small to medium deployments
- **PostgreSQL Storage**: Robust, scalable database for production environments
- **ClickHouse Storage**: High-performance analytical database for massive scale
- **Custom Storage**: Implement your own storage adapter

## Memory Storage

### When to Use

- Development and testing environments
- Temporary trace analysis
- Systems with limited persistence requirements
- Quick prototyping and debugging

### Configuration

```rust
use reflow_tracing::storage::MemoryStorage;

let storage = MemoryStorage::new();
```

### Features

- **Ultra-fast**: No disk I/O overhead
- **Zero configuration**: Works out of the box
- **Bounded capacity**: Configurable memory limits
- **Automatic cleanup**: LRU eviction when capacity is reached

```rust
use reflow_tracing::storage::{MemoryStorage, MemoryConfig};

let config = MemoryConfig {
    max_traces: 10_000,
    max_events_per_trace: 1_000,
    max_memory_mb: 256,
    eviction_policy: EvictionPolicy::LRU,
};

let storage = MemoryStorage::with_config(config);
```

### Limitations

- **No persistence**: Data lost on restart
- **Memory bound**: Limited by available RAM
- **Single process**: No sharing between instances
- **No complex queries**: Basic filtering only

## SQLite Storage

### When to Use

- Small to medium production deployments
- Single-node applications
- Applications requiring persistence without database administration
- Development environments with persistence needs

### Configuration

```rust
use reflow_tracing::storage::SqliteStorage;

let storage = SqliteStorage::new("traces.db").await?;
```

### Features

- **Persistent**: Data survives restarts
- **ACID transactions**: Data integrity guarantees
- **Full SQL support**: Complex queries and analysis
- **Embedded**: No separate database server required
- **Backup friendly**: Single file for easy backups

```rust
use reflow_tracing::storage::{SqliteStorage, SqliteConfig};

let config = SqliteConfig {
    database_path: "traces.db".to_string(),
    journal_mode: JournalMode::WAL,
    synchronous: SynchronousMode::Normal,
    cache_size_mb: 64,
    busy_timeout_ms: 5000,
    max_connections: 10,
};

let storage = SqliteStorage::with_config(config).await?;
```

### Performance Tuning

```rust
// Optimize for write performance
let fast_config = SqliteConfig {
    journal_mode: JournalMode::WAL,      // Write-Ahead Logging
    synchronous: SynchronousMode::Normal, // Balanced durability/speed
    cache_size_mb: 128,                  // Larger cache
    busy_timeout_ms: 10000,              // Handle contention
    ..Default::default()
};

// Optimize for read performance
let read_config = SqliteConfig {
    cache_size_mb: 256,                  // Very large cache
    temp_store: TempStore::Memory,       // In-memory temp tables
    mmap_size_mb: 512,                   // Memory-mapped I/O
    ..Default::default()
};
```

### Limitations

- **Single writer**: Write concurrency limited
- **File size**: Large databases can become unwieldy
- **Network access**: No remote access without additional tools

## PostgreSQL Storage

### When to Use

- Production environments with multiple instances
- High-concurrency applications
- Applications requiring advanced SQL features
- Distributed systems
- Long-term data retention requirements

### Configuration

```rust
use reflow_tracing::storage::PostgresStorage;

let storage = PostgresStorage::new("postgresql://user:pass@localhost/traces").await?;
```

### Features

- **High concurrency**: Excellent multi-client performance
- **ACID compliance**: Strong consistency guarantees
- **Advanced SQL**: Window functions, CTEs, advanced analytics
- **JSON support**: Native support for trace event JSON
- **Partitioning**: Time-based table partitioning
- **Replication**: Built-in streaming replication

```rust
use reflow_tracing::storage::{PostgresStorage, PostgresConfig};

let config = PostgresConfig {
    connection_url: "postgresql://user:pass@localhost/traces".to_string(),
    max_connections: 20,
    min_connections: 5,
    connection_timeout_ms: 5000,
    idle_timeout_ms: 600000,
    max_lifetime_ms: 1800000,
    schema_name: "tracing".to_string(),
    enable_partitioning: true,
    partition_interval: PartitionInterval::Daily,
};

let storage = PostgresStorage::with_config(config).await?;
```

### Schema Setup

```sql
-- Create dedicated schema
CREATE SCHEMA IF NOT EXISTS tracing;

-- Create partitioned tables
CREATE TABLE tracing.traces (
    trace_id UUID PRIMARY KEY,
    flow_id VARCHAR(255) NOT NULL,
    execution_id UUID NOT NULL,
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    status VARCHAR(50) NOT NULL,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
) PARTITION BY RANGE (start_time);

CREATE TABLE tracing.events (
    event_id UUID PRIMARY KEY,
    trace_id UUID NOT NULL REFERENCES tracing.traces(trace_id),
    timestamp TIMESTAMPTZ NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    actor_id VARCHAR(255) NOT NULL,
    data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
) PARTITION BY RANGE (timestamp);

-- Create indexes for performance
CREATE INDEX idx_traces_flow_id ON tracing.traces(flow_id);
CREATE INDEX idx_traces_start_time ON tracing.traces(start_time);
CREATE INDEX idx_events_trace_id ON tracing.events(trace_id);
CREATE INDEX idx_events_timestamp ON tracing.events(timestamp);
CREATE INDEX idx_events_actor_id ON tracing.events(actor_id);
CREATE INDEX idx_events_type ON tracing.events(event_type);

-- GIN index for JSON queries
CREATE INDEX idx_events_data_gin ON tracing.events USING GIN(data);
```

### Partitioning Management

```rust
// Automatic partition management
let config = PostgresConfig {
    enable_partitioning: true,
    partition_interval: PartitionInterval::Daily,
    partition_retention_days: 30,
    auto_create_partitions: true,
    ..Default::default()
};
```

### Performance Optimization

```sql
-- Optimize PostgreSQL configuration
ALTER SYSTEM SET shared_buffers = '256MB';
ALTER SYSTEM SET effective_cache_size = '1GB';
ALTER SYSTEM SET maintenance_work_mem = '64MB';
ALTER SYSTEM SET checkpoint_completion_target = 0.9;
ALTER SYSTEM SET wal_buffers = '16MB';
ALTER SYSTEM SET default_statistics_target = 100;
SELECT pg_reload_conf();
```

## ClickHouse Storage

### When to Use

- Very high-volume trace data (millions of events per second)
- Analytical workloads and reporting
- Time-series analysis
- Long-term data retention with compression
- Real-time dashboards and monitoring

### Configuration

```rust
use reflow_tracing::storage::ClickHouseStorage;

let storage = ClickHouseStorage::new("http://localhost:8123").await?;
```

### Features

- **Columnar storage**: Excellent compression and analytical performance
- **Distributed architecture**: Horizontal scaling
- **Real-time ingestion**: Handle massive write loads
- **Advanced analytics**: Built-in analytical functions
- **Time-series optimized**: Purpose-built for time-ordered data

```rust
use reflow_tracing::storage::{ClickHouseStorage, ClickHouseConfig};

let config = ClickHouseConfig {
    url: "http://clickhouse:8123".to_string(),
    database: "tracing".to_string(),
    cluster: Some("cluster".to_string()),
    username: Some("default".to_string()),
    password: None,
    compression: CompressionMethod::LZ4,
    batch_size: 10000,
    flush_interval_ms: 5000,
    max_memory_usage: 1_000_000_000, // 1GB
    max_execution_time_ms: 300_000,   // 5 minutes
};

let storage = ClickHouseStorage::with_config(config).await?;
```

### Schema Design

```sql
-- Optimized ClickHouse schema
CREATE TABLE tracing.events_local ON CLUSTER cluster (
    timestamp DateTime64(3),
    trace_id UUID,
    event_id UUID,
    flow_id String,
    execution_id UUID,
    event_type LowCardinality(String),
    actor_id String,
    port String,
    message_type String,
    message_size UInt32,
    execution_time_ns UInt64,
    memory_usage UInt64,
    cpu_usage Float32,
    data String -- JSON as string for flexibility
) ENGINE = ReplicatedMergeTree('/clickhouse/tables/{cluster}/{shard}/events', '{replica}')
PARTITION BY toYYYYMM(timestamp)
ORDER BY (timestamp, trace_id, event_id)
SETTINGS index_granularity = 8192;

-- Distributed table
CREATE TABLE tracing.events ON CLUSTER cluster AS tracing.events_local
ENGINE = Distributed(cluster, tracing, events_local, rand());

-- Materialized views for aggregations
CREATE MATERIALIZED VIEW tracing.event_metrics
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (timestamp, actor_id, event_type)
AS SELECT
    toStartOfMinute(timestamp) as timestamp,
    actor_id,
    event_type,
    count() as event_count,
    avg(execution_time_ns) as avg_execution_time,
    max(execution_time_ns) as max_execution_time,
    sum(message_size) as total_bytes
FROM tracing.events_local
GROUP BY timestamp, actor_id, event_type;
```

### Performance Tuning

```xml
<!-- ClickHouse configuration -->
<yandex>
    <profiles>
        <default>
            <max_memory_usage>10000000000</max_memory_usage>
            <use_uncompressed_cache>1</use_uncompressed_cache>
            <load_balancing>random</load_balancing>
        </default>
    </profiles>
    
    <users>
        <default>
            <profile>default</profile>
            <networks incl="networks" replace="replace">
                <ip>::/0</ip>
            </networks>
        </default>
    </users>
</yandex>
```

## Custom Storage Implementation

### Storage Trait

```rust
use async_trait::async_trait;
use reflow_tracing::storage::{StorageBackend, StorageError};

#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn store_trace(&self, trace: FlowTrace) -> Result<(), StorageError>;
    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>, StorageError>;
    async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>, StorageError>;
    async fn store_event(&self, trace_id: TraceId, event: TraceEvent) -> Result<(), StorageError>;
    async fn get_events(&self, trace_id: TraceId) -> Result<Vec<TraceEvent>, StorageError>;
    async fn health_check(&self) -> Result<(), StorageError>;
}
```

### Example: Redis Storage

```rust
use redis::{Client, Connection};
use reflow_tracing::storage::{StorageBackend, StorageError};

pub struct RedisStorage {
    client: Client,
}

impl RedisStorage {
    pub fn new(url: &str) -> Result<Self, StorageError> {
        let client = Client::open(url)?;
        Ok(Self { client })
    }
}

#[async_trait]
impl StorageBackend for RedisStorage {
    async fn store_trace(&self, trace: FlowTrace) -> Result<(), StorageError> {
        let mut conn = self.client.get_connection()?;
        let key = format!("trace:{}", trace.trace_id);
        let value = serde_json::to_string(&trace)?;
        
        redis::cmd("SET")
            .arg(&key)
            .arg(&value)
            .arg("EX")
            .arg(3600) // 1 hour TTL
            .query(&mut conn)?;
            
        Ok(())
    }
    
    async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>, StorageError> {
        let mut conn = self.client.get_connection()?;
        let key = format!("trace:{}", trace_id);
        
        let value: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query(&mut conn)?;
            
        match value {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }
    
    // Implement other methods...
}
```

## Storage Selection Guide

### Decision Matrix

| Feature | Memory | SQLite | PostgreSQL | ClickHouse | Custom |
|---------|--------|---------|------------|------------|---------|
| **Persistence** | ❌ | ✅ | ✅ | ✅ | Depends |
| **Concurrency** | Medium | Low | High | Very High | Depends |
| **Scale** | Small | Medium | Large | Massive | Depends |
| **Setup Complexity** | None | Low | Medium | High | Varies |
| **Query Flexibility** | Limited | High | Very High | High | Depends |
| **Analytics** | Basic | Good | Excellent | Outstanding | Depends |
| **Operational Overhead** | None | Low | Medium | High | Varies |

### Recommendations

**Development/Testing**:
```rust
// Quick start with memory storage
let storage = MemoryStorage::new();
```

**Small Production**:
```rust
// SQLite for simple deployments
let storage = SqliteStorage::new("traces.db").await?;
```

**Medium Production**:
```rust
// PostgreSQL for robust applications
let storage = PostgresStorage::new("postgresql://...").await?;
```

**Large Scale/Analytics**:
```rust
// ClickHouse for high-volume scenarios
let storage = ClickHouseStorage::new("http://clickhouse:8123").await?;
```

## Migration Between Backends

### Export/Import Tool

```rust
use reflow_tracing::migration::StorageMigrator;

// Migrate from SQLite to PostgreSQL
let migrator = StorageMigrator::new(
    SqliteStorage::new("traces.db").await?,
    PostgresStorage::new("postgresql://...").await?
);

migrator.migrate_all_traces().await?;
```

### Backup and Restore

```rust
// Backup to file
let backup_path = "traces_backup.json";
storage.export_to_file(backup_path).await?;

// Restore from file
storage.import_from_file(backup_path).await?;
```

## Monitoring Storage Performance

### Metrics Collection

```rust
use reflow_tracing::storage::StorageMetrics;

let metrics = storage.get_metrics().await?;
println!("Storage performance:");
println!("  Write latency: {}ms", metrics.avg_write_latency_ms);
println!("  Read latency: {}ms", metrics.avg_read_latency_ms);
println!("  Storage size: {}MB", metrics.storage_size_mb);
println!("  Query performance: {}ms", metrics.avg_query_latency_ms);
```

### Health Monitoring

```rust
// Regular health checks
tokio::spawn(async move {
    loop {
        match storage.health_check().await {
            Ok(_) => println!("Storage healthy"),
            Err(e) => eprintln!("Storage unhealthy: {}", e),
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
});
```

## Best Practices

1. **Choose Appropriate Backend**: Match storage backend to your scale and requirements
2. **Plan for Growth**: Start simple but design for scale
3. **Monitor Performance**: Track storage metrics and query performance
4. **Regular Backups**: Implement automated backup strategies
5. **Partition Large Tables**: Use time-based partitioning for better performance
6. **Index Strategically**: Create indexes for common query patterns
7. **Manage Retention**: Implement data retention policies to control growth
8. **Test Disaster Recovery**: Regularly test backup and restore procedures
9. **Optimize Queries**: Use EXPLAIN to understand and optimize query performance
10. **Monitor Resources**: Keep an eye on disk space, memory, and CPU usage
