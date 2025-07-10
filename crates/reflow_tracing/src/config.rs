use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub server: ServerConfig,
    pub storage: StorageConfig,
    pub compression: CompressionConfig,
    pub metrics: MetricsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_connections: usize,
    pub websocket_buffer_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub backend: String,
    pub sqlite: Option<SqliteConfig>,
    pub postgres: Option<PostgresConfig>,
    pub mongodb: Option<MongoDbConfig>,
    pub memory: Option<MemoryConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqliteConfig {
    pub database_path: String,
    pub wal_mode: bool,
    pub journal_mode: String,
    pub synchronous: String,
    pub cache_size: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostgresConfig {
    pub connection_url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MongoDbConfig {
    pub connection_url: String,
    pub database_name: String,
    pub collection_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub max_traces: usize,
    pub eviction_policy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompressionConfig {
    pub enabled: bool,
    pub algorithm: String, // "zstd", "lz4", "brotli", "gzip"
    pub level: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub port: u16,
    pub endpoint: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 8080,
                max_connections: 1000,
                websocket_buffer_size: 1024 * 1024, // 1MB
            },
            storage: StorageConfig {
                backend: "memory".to_string(),
                sqlite: Some(SqliteConfig {
                    database_path: "traces.db".to_string(),
                    wal_mode: true,
                    journal_mode: "WAL".to_string(),
                    synchronous: "NORMAL".to_string(),
                    cache_size: -2000, // 2MB
                }),
                postgres: None,
                mongodb: None,
                memory: Some(MemoryConfig {
                    max_traces: 10_000,
                    eviction_policy: "lru".to_string(),
                }),
            },
            compression: CompressionConfig {
                enabled: true,
                algorithm: "zstd".to_string(),
                level: 3,
            },
            metrics: MetricsConfig {
                enabled: true,
                port: 9090,
                endpoint: "/metrics".to_string(),
            },
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        // Try environment variables first
        if let Ok(config_str) = env::var("REFLOW_TRACING_CONFIG") {
            return Ok(serde_json::from_str(&config_str)?);
        }

        // Try config file
        let config_paths = [
            "reflow_tracing.toml",
            "config/reflow_tracing.toml",
            "/etc/reflow/tracing.toml",
        ];

        for path in &config_paths {
            if Path::new(path).exists() {
                let content = fs::read_to_string(path)?;
                return Ok(toml::from_str(&content)?);
            }
        }

        // Load from individual environment variables
        let mut config = Config::default();
        
        if let Ok(host) = env::var("REFLOW_TRACING_HOST") {
            config.server.host = host;
        }
        
        if let Ok(port) = env::var("REFLOW_TRACING_PORT") {
            config.server.port = port.parse()?;
        }
        
        if let Ok(backend) = env::var("REFLOW_TRACING_STORAGE_BACKEND") {
            config.storage.backend = backend;
        }
        
        if let Ok(db_path) = env::var("REFLOW_TRACING_SQLITE_PATH") {
            if let Some(ref mut sqlite) = config.storage.sqlite {
                sqlite.database_path = db_path;
            }
        }
        
        if let Ok(pg_url) = env::var("REFLOW_TRACING_POSTGRES_URL") {
            config.storage.postgres = Some(PostgresConfig {
                connection_url: pg_url,
                max_connections: 10,
                min_connections: 1,
                acquire_timeout_secs: 30,
            });
        }

        Ok(config)
    }
    
    pub fn validate(&self) -> Result<()> {
        if self.server.port == 0 {
            return Err(anyhow::anyhow!("Server port cannot be 0"));
        }
        
        if self.server.max_connections == 0 {
            return Err(anyhow::anyhow!("Max connections cannot be 0"));
        }
        
        match self.storage.backend.as_str() {
            "sqlite" => {
                if self.storage.sqlite.is_none() {
                    return Err(anyhow::anyhow!("SQLite config required when backend is sqlite"));
                }
            }
            "postgres" => {
                if self.storage.postgres.is_none() {
                    return Err(anyhow::anyhow!("PostgreSQL config required when backend is postgres"));
                }
            }
            "mongodb" => {
                if self.storage.mongodb.is_none() {
                    return Err(anyhow::anyhow!("MongoDB config required when backend is mongodb"));
                }
            }
            "memory" => {
                if self.storage.memory.is_none() {
                    return Err(anyhow::anyhow!("Memory config required when backend is memory"));
                }
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported storage backend: {}", self.storage.backend));
            }
        }
        
        Ok(())
    }
}
