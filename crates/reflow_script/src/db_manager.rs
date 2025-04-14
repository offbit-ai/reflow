use anyhow::{anyhow, Result};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Supported database types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DatabaseType {
    SQLite,
    PostgreSQL,
    MySQL,
    // Add more as needed
}

/// Configuration for a database connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    /// Unique identifier for this database connection
    pub id: String,
    /// Type of database
    pub db_type: DatabaseType,
    /// Connection string (format depends on the database type)
    pub connection_string: String,
    /// Maximum number of connections in the pool
    pub max_connections: Option<u32>,
    /// Connection timeout in seconds
    pub connection_timeout: Option<u64>,
    /// Maximum lifetime of a connection in seconds
    pub max_lifetime: Option<u64>,
    /// Idle timeout in seconds
    pub idle_timeout: Option<u64>,
}

/// Status of a database connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Connection is active and healthy
    Connected,
    /// Connection is being established
    Connecting,
    /// Connection is disconnected
    Disconnected,
    /// Connection failed
    Failed,
}

/// Trait for database connections
#[async_trait::async_trait]
pub trait DatabaseConnection: Send + Sync {
    /// Get the connection ID
    fn get_id(&self) -> &str;
    
    /// Get the connection status
    fn get_status(&self) -> ConnectionStatus;
    
    /// Check if the connection is healthy
    async fn is_healthy(&self) -> bool;
    
    /// Connect to the database
    async fn connect(&mut self) -> Result<()>;
    
    /// Reconnect to the database if the connection is lost
    async fn reconnect(&mut self) -> Result<()>;
    
    /// Execute a query that returns no results
    async fn execute(&self, query: &str, params: Vec<serde_json::Value>) -> Result<u64>;
    
    /// Execute a query that returns results
    async fn query(
        &self,
        query: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<serde_json::Map<String, serde_json::Value>>>;
    
    /// Close the connection
    async fn close(&mut self) -> Result<()>;

    /// Destroy the connection (useful for sqlite)
    fn destroy(&mut self) -> Result<()>;
}

/// Database connection pool manager
pub struct DbPoolManager {
    /// Map of connection ID to database connection
    connections: DashMap<String, Arc<Mutex<Box<dyn DatabaseConnection>>>>,
    /// Map of connection ID to last health check time
    health_checks: DashMap<String, Instant>,
    /// Health check interval
    health_check_interval: Duration,
}

impl DbPoolManager {
    /// Create a new database connection pool manager
    pub fn new(health_check_interval: Duration) -> Self {
        Self {
            connections: DashMap::new(),
            health_checks: DashMap::new(),
            health_check_interval,
        }
    }

    /// Create a new database connection pool manager with default settings
    pub fn default() -> Self {
        Self::new(Duration::from_secs(60)) // Default health check every 60 seconds
    }

    /// Register a new database connection
    pub async fn register_connection<T>(&self, mut connection: T) -> Result<()>
    where
        T: DatabaseConnection + 'static,
    {
        let id = connection.get_id().to_string();
        
        // Check if a connection with this ID already exists
        if self.connections.contains_key(&id) {
            return Err(anyhow!("Connection with ID '{}' already exists", id));
        }
        connection.connect().await?;
        // Store the connection
        self.connections.insert(id.clone(), Arc::new(Mutex::new(Box::new(connection))));
        self.health_checks.insert(id, Instant::now());
        
        Ok(())
    }

    /// Get a database connection by ID
    pub fn get_connection(&self, id: &str) -> Option<Arc<Mutex<Box<dyn DatabaseConnection>>>> {
        self.connections.get(id).map(|conn| conn.clone())
    }

    /// Check if a connection exists
    pub fn has_connection(&self, id: &str) -> bool {
        self.connections.contains_key(id)
    }

    /// Remove a database connection
    pub async fn remove_connection(&self, id: &str) -> Result<()> {
        if let Some((_, connection)) = self.connections.remove(id) {
            let mut conn = connection.lock().await;
            conn.close().await?;
            self.health_checks.remove(id);
            conn.destroy()?;
            Ok(())
        } else {
            Err(anyhow!("Connection with ID '{}' not found", id))
        }
    }

    /// Check the health of all connections and reconnect if necessary
    pub async fn check_connections_health(&self) -> Result<()> {
        println!("Checking connections health...");
        println!("Connections: {:?}", self.connections.iter().map(|r| r.key().clone()).collect::<Vec<_>>());
        println!("Health checks: {:?}", self.health_checks);
        println!("Health check interval: {:?}", self.health_check_interval);
        for item in self.connections.iter() {
            let id = item.key().clone();
            let connection = item.value().clone();
            
            // Check if it's time to perform a health check
            if let Some(last_check) = self.health_checks.get(&id) {
                if last_check.elapsed() < self.health_check_interval {
                    println!("Skipping health check for database '{}'", id);
                    continue; // Skip this connection if it was checked recently
                }
            }
            
            // Perform health check
            let mut conn = connection.lock().await;
            if !conn.is_healthy().await {
                println!("Database connection '{}' is not healthy.. Let's reconnect", id);
                // Attempt to reconnect
                if let Err(err) = conn.reconnect().await {
                    tracing::error!("Failed to reconnect to database '{}': {}", id, err);
                } else {
                    tracing::info!("Successfully reconnected to database '{}'", id);
                }
            }
            
            // Update the last health check time
            self.health_checks.insert(id, Instant::now());
        }
        
        Ok(())
    }

    /// Execute a query on a specific database connection
    pub async fn execute_query(
        &self,
        connection_id: &str,
        query: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
        if let Some(connection) = self.get_connection(connection_id) {
            let conn = connection.lock().await;
            
            // Check connection health before executing query
            if !conn.is_healthy().await {
                return Err(anyhow!("Database connection '{}' is not healthy", connection_id));
            }
            
            conn.query(query, params).await
        } else {
            Err(anyhow!("Connection with ID '{}' not found", connection_id))
        }
    }

    /// Execute a non-query command on a specific database connection
    pub async fn execute_command(
        &self,
        connection_id: &str,
        command: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<u64> {
        if let Some(connection) = self.get_connection(connection_id) {
            let conn = connection.lock().await;
            
            // Check connection health before executing command
            if !conn.is_healthy().await {
                return Err(anyhow!("Database connection '{}' is not healthy", connection_id));
            }
            
            conn.execute(command, params).await
        } else {
            Err(anyhow!("Connection with ID '{}' not found", connection_id))
        }
    }

    /// Get all connection IDs
    pub fn get_connection_ids(&self) -> Vec<String> {
        self.connections.iter().map(|item| item.key().clone()).collect()
    }

    /// Get the status of a specific connection
    pub async fn get_connection_status(&self, id: &str) -> Result<ConnectionStatus> {
        if let Some(connection) = self.get_connection(id) {
            let conn = connection.lock().await;
            Ok(conn.get_status())
        } else {
            Err(anyhow!("Connection with ID '{}' not found", id))
        }
    }
}

// Singleton instance of the DB pool manager
lazy_static::lazy_static! {
    static ref DB_POOL_MANAGER: DbPoolManager = DbPoolManager::default();
}

/// Get the global DB pool manager instance
pub fn get_db_pool_manager() -> &'static DbPoolManager {
    &DB_POOL_MANAGER
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Mock database connection for testing
    struct MockDatabaseConnection {
        id: String,
        status: RwLock<ConnectionStatus>,
        is_healthy: Arc<AtomicBool>,
    }

    impl MockDatabaseConnection {
        fn new(id: &str, is_healthy: bool) -> Self {
            Self {
                id: id.to_string(),
                status: RwLock::new(ConnectionStatus::Disconnected),
                is_healthy: Arc::new(AtomicBool::new(is_healthy)),
            }
        }
    }

    #[async_trait::async_trait]
    impl DatabaseConnection for MockDatabaseConnection {
        fn get_id(&self) -> &str {
            &self.id
        }

        fn get_status(&self) -> ConnectionStatus {
            *self.status.read()
        }

        async fn is_healthy(&self) -> bool {
            self.is_healthy.load(Ordering::SeqCst)
        }

        async fn connect(&mut self) -> Result<()> {
            *self.status.write() = ConnectionStatus::Connected;
            Ok(())
        }

        async fn reconnect(&mut self) -> Result<()> {
            *self.status.write() = ConnectionStatus::Connected;
            self.is_healthy.store(true, Ordering::SeqCst);
            Ok(())
        }

        async fn execute(&self, _query: &str, _params: Vec<serde_json::Value>) -> Result<u64> {
            if self.is_healthy.load(Ordering::SeqCst) {
                Ok(1)
            } else {
                Err(anyhow!("Connection is not healthy"))
            }
        }

        async fn query(
            &self,
            _query: &str,
            _params: Vec<serde_json::Value>,
        ) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
            if self.is_healthy.load(Ordering::SeqCst) {
                let mut map = serde_json::Map::new();
                map.insert("result".to_string(), serde_json::Value::String("test".to_string()));
                Ok(vec![map])
            } else {
                Err(anyhow!("Connection is not healthy"))
            }
        }

        async fn close(&mut self) -> Result<()> {
            *self.status.write() = ConnectionStatus::Disconnected;
            Ok(())
        }

        fn destroy(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_register_and_get_connection() {
        let pool = DbPoolManager::default();
        let conn = MockDatabaseConnection::new("test_db", true);
        
        // Register the connection
        assert!(pool.register_connection(conn).await.is_ok());
        
        // Check if the connection exists
        assert!(pool.has_connection("test_db"));
        
        // Get the connection
        let conn = pool.get_connection("test_db");
        assert!(conn.is_some());
        
        // Try to register a connection with the same ID
        let conn2 = MockDatabaseConnection::new("test_db", true);
        assert!(pool.register_connection(conn2).await.is_err());
    }

    #[tokio::test]
    async fn test_execute_query() {
        let pool = DbPoolManager::default();
        let conn = MockDatabaseConnection::new("query_db", true);
        
        // Register the connection
        assert!(pool.register_connection(conn).await.is_ok());
        
        // Execute a query
        let result = pool.execute_query("query_db", "SELECT * FROM test", vec![]).await;
        assert!(result.is_ok());
        
        let rows = result.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["result"], serde_json::Value::String("test".to_string()));
    }

    #[tokio::test]
    async fn test_health_check_and_reconnect() {
        let pool = DbPoolManager::new(Duration::from_millis(10)); // Short interval for testing
        let conn = MockDatabaseConnection::new("health_db", false); // Start unhealthy
        
        // Register the connection
        assert!(pool.register_connection(conn).await.is_ok());
        
        // Get the connection and verify it's unhealthy
        let conn_ref = pool.get_connection("health_db").unwrap();
        assert!(!conn_ref.lock().await.is_healthy().await);
        
        // Run health check which should attempt to reconnect
        tokio::time::sleep(Duration::from_millis(20)).await; // Wait for health check interval
        assert!(pool.check_connections_health().await.is_ok());
        
        // Verify the connection is now healthy
        assert!(conn_ref.lock().await.is_healthy().await);
    }

    #[tokio::test]
    async fn test_remove_connection() {
        let pool = DbPoolManager::default();
        let conn = MockDatabaseConnection::new("remove_db", true);
        
        // Register the connection
        assert!(pool.register_connection(conn).await.is_ok());
        
        // Remove the connection
        assert!(pool.remove_connection("remove_db").await.is_ok());
        
        // Verify the connection is gone
        assert!(!pool.has_connection("remove_db"));
        
        // Try to remove a non-existent connection
        assert!(pool.remove_connection("nonexistent").await.is_err());
    }
}