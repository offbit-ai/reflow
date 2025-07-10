use anyhow::{Result, anyhow};
use parking_lot::RwLock;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use crate::db_pool::{ConnectionStatus, DatabaseConnection};

#[cfg(feature = "sqlite")]
use rusqlite::{Connection, params, Row};

/// SQLite database connection using rusqlite
pub struct SQLiteConnection {
    /// Unique identifier for this connection
    id: String,
    /// Connection string (file path for SQLite)
    connection_string: String,
    /// Connection status
    status: RwLock<ConnectionStatus>,
    /// SQLite connection
    #[cfg(feature = "sqlite")]
    connection: RwLock<Option<Connection>>,
    #[cfg(not(feature = "sqlite"))]
    connection: RwLock<Option<()>>,
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
            connection: RwLock::new(None),
            last_activity: RwLock::new(Instant::now()),
            initialized: AtomicBool::new(false),
        }
    }

    /// Update the last activity timestamp
    fn update_activity(&self) {
        *self.last_activity.write() = Instant::now();
    }

    /// Convert a rusqlite row to a JSON object
    #[cfg(feature = "sqlite")]
    fn row_to_json(row: &Row) -> Result<serde_json::Map<String, Value>> {
        let mut map = serde_json::Map::new();
        let column_count = row.as_ref().column_count();
        
        for i in 0..column_count {
            let column_name = row.as_ref().column_name(i)?;
            
            let value = match row.get_ref(i)? {
                rusqlite::types::ValueRef::Null => Value::Null,
                rusqlite::types::ValueRef::Integer(i) => Value::Number(i.into()),
                rusqlite::types::ValueRef::Real(f) => {
                    if let Some(num) = serde_json::Number::from_f64(f) {
                        Value::Number(num)
                    } else {
                        Value::Null
                    }
                }
                rusqlite::types::ValueRef::Text(s) => {
                    let text = std::str::from_utf8(s)
                        .map_err(|e| anyhow!("Invalid UTF-8 in text column: {}", e))?;
                    Value::String(text.to_string())
                }
                rusqlite::types::ValueRef::Blob(b) => {
                    // Convert blob to base64 string
                    Value::String(base64::encode(b))
                }
            };

            map.insert(column_name.to_string(), value);
        }

        Ok(map)
    }

    /// Convert a JSON value to a rusqlite parameter
    #[cfg(feature = "sqlite")]
    fn json_to_param(value: &Value) -> rusqlite::types::ToSqlOutput<'_> {
        match value {
            Value::Null => rusqlite::types::ToSqlOutput::Borrowed(rusqlite::types::ValueRef::Null),
            Value::Bool(b) => rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Integer(if *b { 1 } else { 0 })),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Integer(i))
                } else if let Some(f) = n.as_f64() {
                    rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Real(f))
                } else {
                    rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Text(n.to_string()))
                }
            }
            Value::String(s) => rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Text(s.clone())),
            _ => {
                let json_str = serde_json::to_string(value).unwrap_or_default();
                rusqlite::types::ToSqlOutput::Owned(rusqlite::types::Value::Text(json_str))
            }
        }
    }

    pub fn destroy(&mut self) -> Result<()> {
        *self.status.write() = ConnectionStatus::Disconnected;
        self.initialized.store(false, Ordering::SeqCst);
        
        #[cfg(feature = "sqlite")]
        {
            let mut conn = self.connection.write();
            if let Some(c) = conn.take() {
                // Close the connection
                drop(c);
            }
        }
        
        // Remove the database file if it's not in-memory
        if self.connection_string != ":memory:" {
            if let Err(e) = std::fs::remove_file(&self.connection_string) {
                // Only log as warning since file might not exist
                tracing::warn!("Could not remove database file {}: {}", self.connection_string, e);
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
            let connection_guard = self.connection.read();
            if let Some(conn) = connection_guard.as_ref() {
                // Execute a simple query to check if the connection is healthy
                match conn.execute("SELECT 1", []) {
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
            let connection_string = self.connection_string.clone();
            
            // Create connection in a blocking task
            let conn = tokio::task::spawn_blocking(move || {
                // Ensure the directory exists for file databases
                if connection_string != ":memory:" {
                    let path = Path::new(&connection_string);
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                }

                // Create the connection
                let conn = Connection::open(&connection_string)?;
                
                // Enable foreign keys
                conn.execute("PRAGMA foreign_keys = ON", [])?;
                
                Result::<Connection>::Ok(conn)
            }).await??;

            // Store the connection
            *self.connection.write() = Some(conn);
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
        // Close the existing connection if any
        self.close().await?;

        // Connect again
        self.connect().await
    }

    async fn execute(&self, query: &str, params: Vec<Value>) -> Result<u64> {
        self.update_activity();

        #[cfg(feature = "sqlite")]
        {
            let connection_guard = self.connection.read();
            let conn = connection_guard
                .as_ref()
                .ok_or_else(|| anyhow!("SQLite connection not initialized"))?;

            let query = query.to_string();
            let params_owned = params;
            
            // Execute in a blocking task since rusqlite is synchronous
            let result = tokio::task::spawn_blocking(move || {
                // For now, implement without parameters - TODO: add parameter support
                match conn.execute(&query, []) {
                    Ok(affected) => Ok(affected as u64),
                    Err(e) => Err(anyhow!("SQLite execute error: {}", e)),
                }
            }).await?;

            result
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
            let connection_guard = self.connection.read();
            let conn = connection_guard
                .as_ref()
                .ok_or_else(|| anyhow!("SQLite connection not initialized"))?;

            let query = query.to_string();
            
            // Execute in a blocking task since rusqlite is synchronous
            let result = tokio::task::spawn_blocking(move || {
                let mut stmt = conn.prepare(&query)?;
                let rows = stmt.query_map([], |row| {
                    Self::row_to_json(row).map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))
                })?;

                let mut results = Vec::new();
                for row_result in rows {
                    results.push(row_result?);
                }
                
                Result::<Vec<serde_json::Map<String, Value>>>::Ok(results)
            }).await??;

            Ok(result)
        }

        #[cfg(not(feature = "sqlite"))]
        {
            Err(anyhow!("SQLite support is not enabled"))
        }
    }

    async fn close(&mut self) -> Result<()> {
        #[cfg(feature = "sqlite")]
        {
            let mut conn = self.connection.write();
            if let Some(c) = conn.take() {
                // rusqlite connections are automatically closed when dropped
                drop(c);
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
        let insert = "INSERT INTO test (id, name) VALUES (1, 'test')";
        assert!(conn.execute(insert, vec![]).await.is_ok());

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
