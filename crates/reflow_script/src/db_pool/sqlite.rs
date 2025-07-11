use anyhow::{Result, anyhow};
use parking_lot::RwLock;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::db_pool::{ConnectionStatus, DatabaseConnection};

#[cfg(feature = "sqlite")]
use sqlx::{SqlitePool, SqliteConnection, Row};

/// SQLite database connection using SQLx
pub struct SQLiteConnection {
    /// Unique identifier for this connection
    id: String,
    /// Connection string (file path for SQLite)
    connection_string: String,
    /// Connection status
    status: RwLock<ConnectionStatus>,
    /// SQLite pool
    #[cfg(feature = "sqlite")]
    pool: RwLock<Option<SqlitePool>>,
    #[cfg(not(feature = "sqlite"))]
    pool: RwLock<Option<()>>,
    /// Last activity timestamp
    last_activity: RwLock<Instant>,
    /// Whether the connection is initialized
    initialized: AtomicBool,
}

impl SQLiteConnection {
    /// Create a new SQLite connection
    pub fn new(id: &str, connection_string: &str) -> Self {
        Self {
            id: id.to_string(),
            connection_string: connection_string.to_string(),
            status: RwLock::new(ConnectionStatus::Disconnected),
            pool: RwLock::new(None),
            last_activity: RwLock::new(Instant::now()),
            initialized: AtomicBool::new(false),
        }
    }

    /// Update the last activity timestamp
    fn update_activity(&self) {
        *self.last_activity.write() = Instant::now();
    }

    /// Convert a SQLx row to a JSON object
    #[cfg(feature = "sqlite")]
    fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Result<serde_json::Map<String, Value>> {
        let mut map = serde_json::Map::new();
        
        for column in row.columns() {
            let column_name = column.name();
            
            let value = match column.type_info().name() {
                "NULL" => Value::Null,
                "INTEGER" => {
                    match row.try_get::<Option<i64>, _>(column_name) {
                        Ok(Some(i)) => Value::Number(i.into()),
                        Ok(None) => Value::Null,
                        Err(_) => Value::Null,
                    }
                },
                "REAL" => {
                    match row.try_get::<Option<f64>, _>(column_name) {
                        Ok(Some(f)) => {
                            if let Some(num) = serde_json::Number::from_f64(f) {
                                Value::Number(num)
                            } else {
                                Value::Null
                            }
                        },
                        Ok(None) => Value::Null,
                        Err(_) => Value::Null,
                    }
                },
                "TEXT" => {
                    match row.try_get::<Option<String>, _>(column_name) {
                        Ok(Some(s)) => Value::String(s),
                        Ok(None) => Value::Null,
                        Err(_) => Value::Null,
                    }
                },
                "BLOB" => {
                    match row.try_get::<Option<Vec<u8>>, _>(column_name) {
                        Ok(Some(b)) => Value::String(base64::encode(b)),
                        Ok(None) => Value::Null,
                        Err(_) => Value::Null,
                    }
                },
                _ => {
                    // Try to get as string as fallback
                    match row.try_get::<Option<String>, _>(column_name) {
                        Ok(Some(s)) => Value::String(s),
                        Ok(None) => Value::Null,
                        Err(_) => Value::Null,
                    }
                }
            };

            map.insert(column_name.to_string(), value);
        }

        Ok(map)
    }

    pub fn destroy(&mut self) -> Result<()> {
        *self.status.write() = ConnectionStatus::Disconnected;
        self.initialized.store(false, Ordering::SeqCst);
        
        #[cfg(feature = "sqlite")]
        {
            let mut pool = self.pool.write();
            if let Some(p) = pool.take() {
                // Close the pool
                drop(p);
            }
        }
        
        // Remove the database file if it's not in-memory
        if self.connection_string != ":memory:" && !self.connection_string.contains("memory") {
            // Extract file path from connection string
            let file_path = if self.connection_string.starts_with("sqlite://") {
                &self.connection_string[9..]
            } else {
                &self.connection_string
            };
            
            if let Err(e) = std::fs::remove_file(file_path) {
                // Only log as warning since file might not exist
                tracing::warn!("Could not remove database file {}: {}", file_path, e);
            }
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl DatabaseConnection for SQLiteConnection {
    fn get_id(&self) -> &str {
        &self.id
    }

    fn get_status(&self) -> ConnectionStatus {
        *self.status.read()
    }

    async fn is_healthy(&self) -> bool {
        #[cfg(feature = "sqlite")]
        {
            let pool_guard = self.pool.read();
            if let Some(pool) = pool_guard.as_ref() {
                // Execute a simple query to check if the connection is healthy
                match sqlx::query("SELECT 1").fetch_one(pool).await {
                    Ok(_) => return true,
                    Err(e) => {
                        tracing::warn!("SQLite connection health check failed: {}", e);
                        return false;
                    }
                }
            }
            false
        }

        #[cfg(not(feature = "sqlite"))]
        {
            false
        }
    }

    async fn connect(&mut self) -> Result<()> {
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        *self.status.write() = ConnectionStatus::Connecting;

        #[cfg(feature = "sqlite")]
        {
            let connection_string = if self.connection_string == ":memory:" {
                "sqlite::memory:".to_string()
            } else {
                // Ensure the directory exists for file databases
                let path = Path::new(&self.connection_string);
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }

                format!("sqlite://{}", self.connection_string)
            };

            // Create the pool
            let pool = SqlitePool::connect(&connection_string).await
                .map_err(|e| anyhow!("Failed to create SQLite pool: {}", e))?;

            // Enable foreign keys
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&pool)
                .await
                .map_err(|e| anyhow!("Failed to enable foreign keys: {}", e))?;

            // Store the pool
            *self.pool.write() = Some(pool);
            *self.status.write() = ConnectionStatus::Connected;
            self.initialized.store(true, Ordering::SeqCst);
            self.update_activity();

            tracing::info!("SQLite connection '{}' established", self.id);
            Ok(())
        }

        #[cfg(not(feature = "sqlite"))]
        {
            *self.status.write() = ConnectionStatus::Failed;
            Err(anyhow!("SQLite support is not enabled"))
        }
    }

    async fn reconnect(&mut self) -> Result<()> {
        // Close the existing pool if any
        self.close().await?;

        // Connect again
        self.connect().await
    }

    async fn execute(&self, query: &str, params: Vec<Value>) -> Result<u64> {
        self.update_activity();

        #[cfg(feature = "sqlite")]
        {
            let pool_guard = self.pool.read();
            let pool = pool_guard
                .as_ref()
                .ok_or_else(|| anyhow!("SQLite pool not initialized"))?;

            let mut query_builder = sqlx::query(query);
            
            // Bind parameters
            for param in params {
                query_builder = match param {
                    Value::Null => query_builder.bind(None::<String>),
                    Value::Bool(b) => query_builder.bind(b),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            query_builder.bind(i)
                        } else if let Some(f) = n.as_f64() {
                            query_builder.bind(f)
                        } else {
                            query_builder.bind(n.to_string())
                        }
                    },
                    Value::String(s) => query_builder.bind(s),
                    _ => {
                        let json_str = serde_json::to_string(&param)?;
                        query_builder.bind(json_str)
                    }
                };
            }

            let result = query_builder.execute(pool).await
                .map_err(|e| anyhow!("SQLite execute error: {}", e))?;

            Ok(result.rows_affected())
        }

        #[cfg(not(feature = "sqlite"))]
        {
            Err(anyhow!("SQLite support is not enabled"))
        }
    }

    async fn query(
        &self,
        query: &str,
        params: Vec<Value>,
    ) -> Result<Vec<serde_json::Map<String, Value>>> {
        self.update_activity();

        #[cfg(feature = "sqlite")]
        {
            let pool_guard = self.pool.read();
            let pool = pool_guard
                .as_ref()
                .ok_or_else(|| anyhow!("SQLite pool not initialized"))?;

            let mut query_builder = sqlx::query(query);
            
            // Bind parameters
            for param in params {
                query_builder = match param {
                    Value::Null => query_builder.bind(None::<String>),
                    Value::Bool(b) => query_builder.bind(b),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            query_builder.bind(i)
                        } else if let Some(f) = n.as_f64() {
                            query_builder.bind(f)
                        } else {
                            query_builder.bind(n.to_string())
                        }
                    },
                    Value::String(s) => query_builder.bind(s),
                    _ => {
                        let json_str = serde_json::to_string(&param)?;
                        query_builder.bind(json_str)
                    }
                };
            }

            let rows = query_builder.fetch_all(pool).await
                .map_err(|e| anyhow!("SQLite query error: {}", e))?;

            let mut results = Vec::new();
            for row in rows {
                results.push(Self::row_to_json(&row)?);
            }

            Ok(results)
        }

        #[cfg(not(feature = "sqlite"))]
        {
            Err(anyhow!("SQLite support is not enabled"))
        }
    }

    async fn close(&mut self) -> Result<()> {
        #[cfg(feature = "sqlite")]
        {
            let mut pool = self.pool.write();
            if let Some(p) = pool.take() {
                p.close().await;
            }
        }

        *self.status.write() = ConnectionStatus::Disconnected;
        self.initialized.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn destroy(&mut self) -> Result<()> {
        self.destroy()?;
        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "sqlite")]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_connection() {
        let mut conn = SQLiteConnection::new("test_sqlite", ":memory:");

        // Connect
        assert!(conn.connect().await.is_ok());
        assert_eq!(conn.get_status(), ConnectionStatus::Connected);

        // Create a table
        let create_table = "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)";
        assert!(conn.execute(create_table, vec![]).await.is_ok());

        // Insert data
        let insert = "INSERT INTO test (id, name) VALUES (?, ?)";
        let params = vec![Value::Number(1.into()), Value::String("test".to_string())];
        assert!(conn.execute(insert, params).await.is_ok());

        // Query data
        let query = "SELECT * FROM test";
        let result = conn.query(query, vec![]).await;
        assert!(result.is_ok());

        let rows = result.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], Value::Number(1.into()));
        assert_eq!(rows[0]["name"], Value::String("test".to_string()));

        // Check health
        assert!(conn.is_healthy().await);

        // Close
        assert!(conn.close().await.is_ok());
        assert_eq!(conn.get_status(), ConnectionStatus::Disconnected);
    }
}
