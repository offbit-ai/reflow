use super::types::*;
use anyhow::{Result, Context};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, Client};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::debug;

/// Redis-backed actor state
pub struct RedisActorState {
    namespace: String,
    actor_id: String,
    connection: Arc<Mutex<Option<ConnectionManager>>>,
}

impl RedisActorState {
    /// Create a new Redis actor state
    pub async fn new(url: &str, namespace: &str, actor_id: &str) -> Result<Self> {
        let client = Client::open(url)
            .context("Failed to create Redis client")?;
        
        // Create connection manager
        let manager = ConnectionManager::new(client).await
            .context("Failed to create Redis connection manager")?;
        
        Ok(Self {
            namespace: namespace.to_string(),
            actor_id: actor_id.to_string(),
            connection: Arc::new(Mutex::new(Some(manager))),
        })
    }
    
    /// Get a connection manager
    async fn get_connection(&self) -> Result<ConnectionManager> {
        let conn_guard = self.connection.lock().await;
        
        conn_guard.as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Redis connection not available"))
    }
    
    /// Create a Redis key with namespace
    fn make_key(&self, key: &str) -> String {
        format!("{}:{}:{}", self.namespace, self.actor_id, key)
    }
    
    /// Get a value from state
    pub async fn get(&self, key: &str) -> Result<Option<Value>> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let value: Option<String> = conn.get(redis_key).await?;
        
        match value {
            Some(json_str) => {
                let parsed = serde_json::from_str(&json_str)
                    .unwrap_or_else(|_| Value::String(json_str));
                Ok(Some(parsed))
            }
            None => Ok(None)
        }
    }
    
    /// Set a value in state
    pub async fn set(&self, key: &str, value: Value) -> Result<()> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        let json_str = serde_json::to_string(&value)?;
        
        let _: () = conn.set(redis_key, json_str).await?;
        Ok(())
    }
    
    /// Delete a key from state
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let deleted: i64 = conn.del(redis_key).await?;
        Ok(deleted > 0)
    }
    
    /// Atomically increment a counter
    pub async fn increment(&self, key: &str, amount: i64) -> Result<i64> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let new_value: i64 = conn.incr(redis_key, amount).await?;
        Ok(new_value)
    }
    
    /// Atomically decrement a counter
    pub async fn decrement(&self, key: &str, amount: i64) -> Result<i64> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let new_value: i64 = conn.decr(redis_key, amount).await?;
        Ok(new_value)
    }
    
    /// Push a value to a list
    pub async fn push(&self, key: &str, value: Value) -> Result<i64> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        let json_str = serde_json::to_string(&value)?;
        
        let length: i64 = conn.rpush(redis_key, json_str).await?;
        Ok(length)
    }
    
    /// Pop a value from a list
    pub async fn pop(&self, key: &str) -> Result<Option<Value>> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let value: Option<String> = conn.lpop(redis_key, None).await?;
        
        match value {
            Some(json_str) => {
                let parsed = serde_json::from_str(&json_str)
                    .unwrap_or_else(|_| Value::String(json_str));
                Ok(Some(parsed))
            }
            None => Ok(None)
        }
    }
    
    /// Extend a list with multiple values
    pub async fn extend(&self, key: &str, values: Vec<Value>) -> Result<i64> {
        if values.is_empty() {
            return Ok(0);
        }
        
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let json_values: Vec<String> = values
            .into_iter()
            .map(|v| serde_json::to_string(&v))
            .collect::<Result<Vec<_>, _>>()?;
        
        let mut pipe = redis::pipe();
        for value in json_values {
            pipe.rpush(&redis_key, value);
        }
        
        let results: Vec<i64> = pipe.query_async(&mut conn).await?;
        Ok(results.last().copied().unwrap_or(0))
    }
    
    /// Set expiration time for a key
    pub async fn expire(&self, key: &str, seconds: i64) -> Result<bool> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let result: bool = conn.expire(redis_key, seconds).await?;
        Ok(result)
    }
    
    /// Get remaining TTL for a key
    pub async fn ttl(&self, key: &str) -> Result<i64> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let ttl: i64 = conn.ttl(redis_key).await?;
        Ok(ttl)
    }
    
    /// Check if a key exists
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let mut conn = self.get_connection().await?;
        let redis_key = self.make_key(key);
        
        let exists: bool = conn.exists(redis_key).await?;
        Ok(exists)
    }
    
    /// List keys matching a pattern
    pub async fn keys(&self, pattern: &str) -> Result<Vec<String>> {
        let mut conn = self.get_connection().await?;
        let redis_pattern = format!("{}:{}:{}", self.namespace, self.actor_id, pattern);
        
        let keys: Vec<String> = conn.keys(redis_pattern).await?;
        
        // Strip namespace prefix from keys
        let prefix = format!("{}:{}:", self.namespace, self.actor_id);
        let stripped_keys = keys
            .into_iter()
            .map(|k| k.strip_prefix(&prefix).unwrap_or(&k).to_string())
            .collect();
        
        Ok(stripped_keys)
    }
    
    /// Execute a state operation
    pub async fn execute(&self, operation: StateOperation) -> Result<StateResult> {
        match operation {
            StateOperation::Get { key } => {
                let value = self.get(&key).await?;
                Ok(StateResult::Value(value))
            }
            StateOperation::Set { key, value } => {
                self.set(&key, value).await?;
                Ok(StateResult::Value(None))
            }
            StateOperation::Delete { key } => {
                let deleted = self.delete(&key).await?;
                Ok(StateResult::Boolean(deleted))
            }
            StateOperation::Increment { key, amount } => {
                let new_value = self.increment(&key, amount).await?;
                Ok(StateResult::Number(new_value))
            }
            StateOperation::Decrement { key, amount } => {
                let new_value = self.decrement(&key, amount).await?;
                Ok(StateResult::Number(new_value))
            }
            StateOperation::Push { key, value } => {
                let length = self.push(&key, value).await?;
                Ok(StateResult::Number(length))
            }
            StateOperation::Pop { key } => {
                let value = self.pop(&key).await?;
                Ok(StateResult::Value(value))
            }
            StateOperation::Extend { key, values } => {
                let length = self.extend(&key, values).await?;
                Ok(StateResult::Number(length))
            }
            StateOperation::Expire { key, seconds } => {
                let result = self.expire(&key, seconds).await?;
                Ok(StateResult::Boolean(result))
            }
            StateOperation::Ttl { key } => {
                let ttl = self.ttl(&key).await?;
                Ok(StateResult::Number(ttl))
            }
            StateOperation::Exists { key } => {
                let exists = self.exists(&key).await?;
                Ok(StateResult::Boolean(exists))
            }
            StateOperation::Keys { pattern } => {
                let keys = self.keys(&pattern).await?;
                Ok(StateResult::List(keys))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    
    #[tokio::test]
    async fn test_redis_state_basic_operations() {
        // This test requires Redis to be running
        let state = match RedisActorState::new(
            "redis://localhost:6379",
            "test",
            "test_actor"
        ).await {
            Ok(s) => s,
            Err(_) => {
                println!("Skipping test: Redis not available");
                return;
            }
        };
        
        // Test set/get
        state.set("test_key", json!({"foo": "bar"})).await.unwrap();
        let value = state.get("test_key").await.unwrap();
        assert_eq!(value, Some(json!({"foo": "bar"})));
        
        // Test increment
        let count = state.increment("counter", 1).await.unwrap();
        assert_eq!(count, 1);
        
        let count = state.increment("counter", 5).await.unwrap();
        assert_eq!(count, 6);
        
        // Test delete
        let deleted = state.delete("test_key").await.unwrap();
        assert!(deleted);
        
        let value = state.get("test_key").await.unwrap();
        assert_eq!(value, None);
        
        // Cleanup
        state.delete("counter").await.unwrap();
    }
}