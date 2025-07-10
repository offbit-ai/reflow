use anyhow::{Result, anyhow};
use parking_lot::{Mutex, RwLock};
use reflow_actor::message::Message;
use serde_json::Value;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio_postgres::types::ToSql;

use crate::db_pool::{ConnectionStatus, DatabaseConnection};

/// A simple enum to represent database parameter values
/// This avoids the need for trait objects that don't implement Send
#[cfg(feature = "postgres")]
#[derive(Debug)]
enum SimpleDbValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

#[cfg(feature = "postgres")]
impl tokio_postgres::types::ToSql for SimpleDbValue {
    fn to_sql(
        &self,
        ty: &tokio_postgres::types::Type,
        out: &mut tokio_postgres::types::private::BytesMut,
    ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        match self {
            SimpleDbValue::Null => Ok(tokio_postgres::types::IsNull::Yes),
            SimpleDbValue::Bool(b) => b.to_sql(ty, out),
            SimpleDbValue::Int(i) => i.to_sql(ty, out),
            SimpleDbValue::Float(f) => f.to_sql(ty, out),
            SimpleDbValue::Text(s) => s.to_sql(ty, out),
        }
    }

    fn accepts(ty: &tokio_postgres::types::Type) -> bool {
        <bool as tokio_postgres::types::ToSql>::accepts(ty)
            || <i64 as tokio_postgres::types::ToSql>::accepts(ty)
            || <f64 as tokio_postgres::types::ToSql>::accepts(ty)
            || <String as tokio_postgres::types::ToSql>::accepts(ty)
    }

    tokio_postgres::types::to_sql_checked!();
}

/// PostgreSQL database connection
pub struct PostgresConnection {
    /// Unique identifier for this connection
    id: String,
    /// Connection string for PostgreSQL
    connection_string: String,
    /// Connection status
    status: Arc<Mutex<ConnectionStatus>>,
    /// Connection pool
    pool: Arc<Mutex<Option<deadpool_postgres::Pool>>>,
    /// Last activity timestamp
    last_activity: Arc<Mutex<Instant>>,
    /// Whether the connection is initialized
    initialized: AtomicBool,
}

unsafe impl Sync for PostgresConnection {}
unsafe impl Send for PostgresConnection {}

impl PostgresConnection {
    /// Create a new PostgreSQL connection
    pub fn new(id: &str, connection_string: &str) -> PostgresConnection {
        Self {
            id: id.to_string(),
            connection_string: connection_string.to_string(),
            status: Arc::new(Mutex::new(ConnectionStatus::Disconnected)),
            pool: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            initialized: AtomicBool::new(false),
        }
    }

    /// Update the last activity timestamp
    fn update_activity(&self) {
        *self.last_activity.lock() = Instant::now();
    }

    /// Convert a Rust value to a PostgreSQL parameter
    #[cfg(feature = "postgres")]
    fn value_to_param(value: &Value) -> Result<Box<dyn tokio_postgres::types::ToSql + Sync>> {
        match value {
            Value::Null => Ok(Box::new(Option::<String>::None)),
            Value::Bool(b) => Ok(Box::new(*b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(Box::new(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(Box::new(f))
                } else {
                    Err(anyhow!("Unsupported number type"))
                }
            }
            Value::String(s) => Ok(Box::new(s.clone())),
            _ => Err(anyhow!("Unsupported parameter type: {:?}", value)),
        }
    }

    /// Convert a Rust value to a SimpleDbValue
    #[cfg(feature = "postgres")]
    fn value_to_simple_type(value: &Value) -> Result<SimpleDbValue> {
        match value {
            Value::Null => Ok(SimpleDbValue::Null),
            Value::Bool(b) => Ok(SimpleDbValue::Bool(*b)),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(SimpleDbValue::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(SimpleDbValue::Float(f))
                } else {
                    Err(anyhow!("Unsupported number type"))
                }
            }
            Value::String(s) => Ok(SimpleDbValue::Text(s.clone())),
            _ => Err(anyhow!("Unsupported parameter type: {:?}", value)),
        }
    }

    /// Convert a PostgreSQL row to a JSON object
    #[cfg(feature = "postgres")]
    async fn row_to_json(
        row: &tokio_postgres::Row,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut map = serde_json::Map::new();
        let columns = row.columns();

        for (i, column) in columns.iter().enumerate() {
            let column_name = column.name().to_string();
            let column_type = column.type_();

            // Handle different PostgreSQL types
            let value = if column_type == &tokio_postgres::types::Type::BOOL {
                Message::Boolean(row.get(i))
            } else if column_type == &tokio_postgres::types::Type::INT4 {
                let val: i32 = row.get(i);
                Message::Integer(val as i64)
            } else if column_type == &tokio_postgres::types::Type::INT8 {
                let val: i64 = row.get(i);
                Message::Integer(val)
            } else if column_type == &tokio_postgres::types::Type::FLOAT4
                || column_type == &tokio_postgres::types::Type::FLOAT8
            {
                let val: f64 = row.get(i);
                Message::Float(val)
            } else if column_type == &tokio_postgres::types::Type::TEXT
                || column_type == &tokio_postgres::types::Type::VARCHAR
            {
                let val: String = row.get(i);
                Message::string(val)
            } else if column_type == &tokio_postgres::types::Type::BYTEA {
                let val: Vec<u8> = row.get(i);
                Message::encoded(val)
            } else {
                // For other types, try to get as string
                match row.try_get::<_, String>(i) {
                    Ok(val) => Message::string(val),
                    Err(_) => Message::Flow,
                }
            };

            map.insert(column_name, serde_json::to_value(value)?);
        }

        Ok(map)
    }
}

#[async_trait::async_trait]
impl DatabaseConnection for PostgresConnection {
    fn get_id(&self) -> &str {
        &self.id
    }

    fn get_status(&self) -> ConnectionStatus {
        *self.status.lock()
    }

    async fn is_healthy(&self) -> bool {
        // Clone the Arc but don't lock it yet
        let pool_arc = self.pool.clone();

        // Get the pool reference and release the lock immediately
        let pool_option = {
            let guard = pool_arc.lock();
            guard.clone()
        };

        // Now we can safely await without holding the lock
        if let Some(pool) = pool_option.as_ref() {
            // Try to get a client from the pool
            match pool.get().await {
                Ok(client) => {
                    // Execute a simple query to check if the connection is healthy
                    match client.query_one("SELECT 1", &[]).await {
                        Ok(_) => return true,
                        Err(e) => {
                            tracing::warn!("PostgreSQL connection health check failed: {}", e);
                            return false;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to get PostgreSQL client from pool: {}", e);
                    return false;
                }
            }
        }
        false
    }

    async fn connect(&mut self) -> Result<()> {
        if self.initialized.load(Ordering::SeqCst) {
            return Ok(());
        }

        *self.status.lock() = ConnectionStatus::Connecting;

        #[cfg(feature = "postgres")]
        {
            // Parse the connection string into a config
            let config = match tokio_postgres::Config::from_str(&self.connection_string) {
                Ok(config) => config,
                Err(e) => {
                    *self.status.lock() = ConnectionStatus::Failed;
                    return Err(anyhow!("Invalid PostgreSQL connection string: {}", e));
                }
            };

            // Create a connection pool
            let mgr_config = deadpool_postgres::ManagerConfig {
                recycling_method: deadpool_postgres::RecyclingMethod::Fast,
            };
            let mgr =
                deadpool_postgres::Manager::from_config(config, tokio_postgres::NoTls, mgr_config);
            let pool = match deadpool_postgres::Pool::builder(mgr)
                .max_size(20) // Default max connections
                .build()
            {
                Ok(pool) => pool,
                Err(e) => {
                    *self.status.lock() = ConnectionStatus::Failed;
                    return Err(anyhow!(
                        "Failed to create PostgreSQL connection pool: {}",
                        e
                    ));
                }
            };

            // Test the connection
            match pool.get().await {
                Ok(client) => {
                    // Execute a simple query to verify the connection
                    if let Err(e) = client.query_one("SELECT 1", &[]).await {
                        *self.status.lock() = ConnectionStatus::Failed;
                        return Err(anyhow!("Failed to connect to PostgreSQL: {}", e));
                    }
                }
                Err(e) => {
                    *self.status.lock() = ConnectionStatus::Failed;
                    return Err(anyhow!("Failed to connect to PostgreSQL: {}", e));
                }
            }

            // Store the pool
            *self.pool.lock() = Some(pool);
            *self.status.lock() = ConnectionStatus::Connected;
            self.initialized.store(true, Ordering::SeqCst);
            self.update_activity();

            tracing::info!("PostgreSQL connection '{}' established", self.id);
            Ok(())
        }

        #[cfg(not(feature = "postgres"))]
        {
            *self.status.write() = ConnectionStatus::Failed;
            Err(anyhow!("PostgreSQL support is not enabled"))
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

        #[cfg(feature = "postgres")]
        {
            // Clone the Arc but don't lock it yet
            let pool_arc = self.pool.clone();

            // Get the pool reference and release the lock immediately
            let pool_option = {
                let guard = pool_arc.lock();
                guard.clone()
            };

            let pool = pool_option
                .as_ref()
                .ok_or_else(|| anyhow!("PostgreSQL connection not initialized"))?;

            // Get a client from the pool
            let client = pool
                .get()
                .await
                .map_err(|e| anyhow!("Failed to get PostgreSQL client: {}", e))?;

            // Take a static approach by binding the parameters individually
            let query_str = query.to_string();

            match params.len() {
                0 => {
                    // No parameters, execute the raw query
                    client
                        .execute(&query_str, &[])
                        .await
                        .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                1 => {
                    // Convert single parameter
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    client
                        .execute(&query_str, &[&p1])
                        .await
                        .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                2 => {
                    // Convert two parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    client
                        .execute(&query_str, &[&p1, &p2])
                        .await
                        .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                3 => {
                    // Convert three parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    client
                        .execute(&query_str, &[&p1, &p2, &p3])
                        .await
                        .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                4 => {
                    // Convert four parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    client
                        .execute(&query_str, &[&p1, &p2, &p3, &p4])
                        .await
                        .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                5 => {
                    // Convert five parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    client
                        .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5])
                        .await
                        .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                6 => {
                    // Convert six parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    client
                       .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6])
                       .await
                       .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                7 => {
                    // Convert seven parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    client
                      .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7])
                      .await
                      .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                8 => {
                    // Convert eight parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    client
                      .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8])
                      .await
                      .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                9 => {
                    // Convert nine parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    client
                     .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9])
                    .await
                    .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                10 => {
                    // Convert ten parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    let p10 = Self::value_to_simple_type(&params[9])?;
                    client
                     .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10])
                     .await
                     .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                11 => {
                    // Convert eleven parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    let p10 = Self::value_to_simple_type(&params[9])?;
                    let p11 = Self::value_to_simple_type(&params[10])?;
                    client
                    .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10, &p11])
                    .await
                    .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                12 => {
                    // Convert twelve parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    let p10 = Self::value_to_simple_type(&params[9])?;
                    let p11 = Self::value_to_simple_type(&params[10])?;
                    let p12 = Self::value_to_simple_type(&params[11])?;
                    client
                   .execute(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10, &p11, &p12])
                   .await
                   .map_err(|e| anyhow!("PostgreSQL execute error: {}", e))
                }
                _ => Err(anyhow!("Too many parameters (max 12 supported)")),
            }
        }

        #[cfg(not(feature = "postgres"))]
        {
            Err(anyhow!("PostgreSQL support is not enabled"))
        }
    }

    async fn query(
        &self,
        query: &str,
        params: Vec<Value>,
    ) -> Result<Vec<serde_json::Map<String, Value>>> {
        self.update_activity();

        #[cfg(feature = "postgres")]
        {
            // Clone the Arc but don't lock it yet
            let pool_arc = self.pool.clone();

            // Get the pool reference and release the lock immediately
            let pool_option = {
                let guard = pool_arc.lock();
                guard.clone()
            };

            let pool = pool_option
                .as_ref()
                .ok_or_else(|| anyhow!("PostgreSQL connection not initialized"))?;

            // Get a client from the pool
            let client = pool
                .get()
                .await
                .map_err(|e| anyhow!("Failed to get PostgreSQL client: {}", e))?;

            // Take a static approach by binding the parameters individually
            let query_str = query.to_string();

            let rows = match params.len() {
                0 => {
                    // No parameters, execute the raw query
                    client.query(&query_str, &[]).await
                }
                1 => {
                    // Convert single parameter
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    client.query(&query_str, &[&p1]).await
                }
                2 => {
                    // Convert two parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    client.query(&query_str, &[&p1, &p2]).await
                }
                3 => {
                    // Convert three parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    client.query(&query_str, &[&p1, &p2, &p3]).await
                }
                4 => {
                    // Convert four parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4]).await
                }
                5 => {
                    // Convert five parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5]).await
                }
                6 => {
                    // Convert six parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6]).await
                }
                7 => {
                    // Convert seven parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7]).await    
                }
                8 => {
                    // Convert eight parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;   
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8]).await
                }
                9 => {
                    // Convert nine parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9]).await
                }
                10 => {
                    // Convert ten parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    let p10 = Self::value_to_simple_type(&params[9])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10]).await
                }
                11 => {
                    // Convert eleven parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    let p10 = Self::value_to_simple_type(&params[9])?;
                    let p11 = Self::value_to_simple_type(&params[10])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10, &p11]).await
                }
                12 => {
                    // Convert twelve parameters
                    let p1 = Self::value_to_simple_type(&params[0])?;
                    let p2 = Self::value_to_simple_type(&params[1])?;
                    let p3 = Self::value_to_simple_type(&params[2])?;
                    let p4 = Self::value_to_simple_type(&params[3])?;
                    let p5 = Self::value_to_simple_type(&params[4])?;
                    let p6 = Self::value_to_simple_type(&params[5])?;
                    let p7 = Self::value_to_simple_type(&params[6])?;
                    let p8 = Self::value_to_simple_type(&params[7])?;
                    let p9 = Self::value_to_simple_type(&params[8])?;
                    let p10 = Self::value_to_simple_type(&params[9])?;
                    let p11 = Self::value_to_simple_type(&params[10])?;
                    let p12 = Self::value_to_simple_type(&params[11])?;
                    client.query(&query_str, &[&p1, &p2, &p3, &p4, &p5, &p6, &p7, &p8, &p9, &p10, &p11, &p12]).await
                }
                _ => {
                    return Err(anyhow!("Too many parameters (max 12 supported)"));
                }
            }
            .map_err(|e| anyhow!("PostgreSQL query error: {}", e))?;

            // Convert rows to JSON
            let mut results = Vec::with_capacity(rows.len());
            for row in rows {
                results.push(Self::row_to_json(&row).await?);
            }

            Ok(results)
        }

        #[cfg(not(feature = "postgres"))]
        {
            Err(anyhow!("PostgreSQL support is not enabled"))
        }
    }

    async fn close(&mut self) -> Result<()> {
        #[cfg(feature = "postgres")]
        {
            // Reset the pool
            *self.pool.lock() = None;
            *self.status.lock() = ConnectionStatus::Disconnected;
            self.initialized.store(false, Ordering::SeqCst);
        }

        Ok(())
    }

    fn destroy(&mut self) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
#[cfg(feature = "postgres")]
mod tests {
    use super::*;
    use tokio;

    #[tokio::test]
    async fn test_postgres_connection() {
        // This test requires a PostgreSQL server to be running
        // It's disabled by default and only runs when explicitly enabled
        if std::env::var("RUN_POSTGRES_TESTS").is_err() {
            return;
        }

        let conn_string = std::env::var("POSTGRES_TEST_CONN_STRING")
            .unwrap_or_else(|_| "host=localhost user=postgres password=postgres".to_string());

        let mut conn = PostgresConnection::new("test_postgres", &conn_string);

        // Connect to the database
        let result = conn.connect().await;
        assert!(result.is_ok(), "Failed to connect: {:?}", result);

        // Check health
        assert!(conn.is_healthy().await);

        // Execute a query
        let result = conn
            .execute(
                "CREATE TEMPORARY TABLE test_table (id INT, name TEXT)",
                vec![],
            )
            .await;
        assert!(result.is_ok(), "Failed to create table: {:?}", result);

        // Insert data
        let params = vec![
            serde_json::Value::Number(1.into()),
            serde_json::Value::String("Test".to_string()),
        ];
        let result = conn
            .execute("INSERT INTO test_table VALUES ($1, $2)", params)
            .await;
        assert!(result.is_ok(), "Failed to insert data: {:?}", result);

        // Query data
        let result = conn.query("SELECT * FROM test_table", vec![]).await;
        assert!(result.is_ok(), "Failed to query data: {:?}", result);

        let rows = result.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], serde_json::Value::Number(1.into()));
        assert_eq!(
            rows[0]["name"],
            serde_json::Value::String("Test".to_string())
        );

        // Close connection
        let result = conn.close().await;
        assert!(result.is_ok(), "Failed to close connection: {:?}", result);
    }
}
