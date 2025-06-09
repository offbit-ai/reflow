use anyhow::{Result, anyhow};
use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::db_pool::{ConnectionStatus, DatabaseConnection};

/// SQLite database connection
pub struct SQLiteConnection {
    /// Unique identifier for this connection
    id: String,
    /// Connection string (file path for SQLite)
    connection_string: String,
    /// Connection status
    status: RwLock<ConnectionStatus>,
    /// Connection handle
    #[cfg(feature = "sqlite")]
    connection: Arc<Mutex<Option<rusqlite::Connection>>>,
    #[cfg(not(feature = "sqlite"))]
    connection: Arc<Mutex<Option<()>>>,
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
            connection: Arc::new(Mutex::new(None)),
            last_activity: RwLock::new(Instant::now()),
            initialized: AtomicBool::new(false),
        }
    }

    /// Update the last activity timestamp
    fn update_activity(&self) {
        *self.last_activity.write() = Instant::now();
    }

    /// Convert a Rust value to a SQLite parameter
    #[cfg(feature = "sqlite")]
    fn value_to_param(value: &Value) -> Result<rusqlite::types::Value> {
        match value {
            Value::Null => Ok(rusqlite::types::Value::Null),
            Value::Bool(b) => Ok(rusqlite::types::Value::Integer(*b as i64)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(rusqlite::types::Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(rusqlite::types::Value::Real(f))
                } else {
                    Err(anyhow!("Unsupported number type"))
                }
            }
            Value::String(s) => Ok(rusqlite::types::Value::Text(s.clone())),
            _ => Err(anyhow!("Unsupported parameter type: {:?}", value)),
        }
    }

    /// Convert a SQLite row to a JSON object
    #[cfg(feature = "sqlite")]
    fn row_to_json(row: &rusqlite::Row) -> Result<serde_json::Map<String, Value>> {
        let mut map = serde_json::Map::new();
        let column_count = row.as_ref().column_count();

        for i in 0..column_count {
            let column_name = row.as_ref().column_name(i)?.to_string();
            let value = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(i) => Value::Number(i.into()),
                rusqlite::types::ValueRef::Real(f) => {
                    // Convert f64 to serde_json::Number
                    if let Some(num) = serde_json::Number::from_f64(f) {
                        Value::Number(num)
                    } else {
                        return Err(anyhow!("Failed to convert f64 to JSON number"));
                    }
                }
                rusqlite::types::ValueRef::Text(t) => {
                    Value::String(String::from_utf8_lossy(t).to_string())
                }
                rusqlite::types::ValueRef::Blob(b) => {
                    // Convert blob to base64 string
                    Value::String(base64::encode(b))
                }
            };

            map.insert(column_name, value);
        }

        Ok(map)
    }

    pub fn destroy(&mut self) -> Result<()> {
        *self.status.write() = ConnectionStatus::Disconnected;
        self.initialized.store(false, Ordering::SeqCst);
        #[cfg(feature = "sqlite")]
        {
            let mut conn = self.connection.lock();
            if let Some(c) = conn.take() {
                drop(c); // This will close the connection
            }
        }
        if self.connection_string != ":memory:" {
            std::fs::remove_file(&self.connection_string)?;
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
            let conn = self.connection.lock();
            if let Some(conn) = conn.as_ref() {
                // Execute a simple query to check if the connection is healthy
                match conn.query_row("SELECT 1", [], |_| Ok(())) {
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
            // Check if the database file exists or if it's an in-memory database
            let is_memory = self.connection_string == ":memory:";
            let path = Path::new(&self.connection_string);

            if !is_memory && !path.exists() {
                // Ensure the directory exists
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
            }

            // Open the connection
            match rusqlite::Connection::open(&self.connection_string) {
                Ok(conn) => {
                    // Enable foreign keys
                    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

                    // Store the connection
                    *self.connection.lock() = Some(conn);
                    *self.status.write() = ConnectionStatus::Connected;
                    self.initialized.store(true, Ordering::SeqCst);
                    self.update_activity();

                    tracing::info!("SQLite connection '{}' established", self.id);
                    Ok(())
                }
                Err(e) => {
                    *self.status.write() = ConnectionStatus::Failed;
                    Err(anyhow!("Failed to connect to SQLite database: {}", e))
                }
            }
        }

        #[cfg(not(feature = "sqlite"))]
        {
            *self.status.write() = ConnectionStatus::Failed;
            Err(anyhow!("SQLite support is not enabled"))
        }
    }

    async fn reconnect(&mut self) -> Result<()> {
        // Close the existing connection if any
        self.close().await?;

        // Connect again
        self.connect().await
    }

    async fn execute(&self, query: &str, params: Vec<Value>) -> Result<u64> {
        self.update_activity();

        #[cfg(feature = "sqlite")]
        {
            let conn_guard = self.connection.lock();
            let conn = conn_guard
                .as_ref()
                .ok_or_else(|| anyhow!("SQLite connection not initialized"))?;

            // Convert parameters
            let params: Result<Vec<rusqlite::types::Value>> =
                params.iter().map(|p| Self::value_to_param(p)).collect();
            let params = params?;

            // Execute the query
            match conn.execute(query, rusqlite::params_from_iter(params.iter())) {
                Ok(rows) => Ok(rows as u64),
                Err(e) => Err(anyhow!("SQLite execute error: {}", e)),
            }
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
            let conn_guard = self.connection.lock();
            let conn = conn_guard
                .as_ref()
                .ok_or_else(|| anyhow!("SQLite connection not initialized"))?;

            // Convert parameters
            let params: Result<Vec<rusqlite::types::Value>> =
                params.iter().map(|p| Self::value_to_param(p)).collect();
            let params = params?;

            // Execute the query
            let mut stmt = conn.prepare(query)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(params.iter()), |row| {
                Ok(Self::row_to_json(row))
            })?;

            // Collect results
            let mut results = Vec::new();
            for row_result in rows {
                match row_result {
                    Ok(row) => results.push(row?),
                    Err(e) => return Err(anyhow!("SQLite query error: {}", e)),
                }
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
            let mut conn = self.connection.lock();
            if let Some(c) = conn.take() {
                drop(c); // This will close the connection
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
