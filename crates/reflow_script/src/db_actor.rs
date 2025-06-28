//! Database actor implementation
//!
//! This module provides an actor implementation for database operations
//! that uses the database connection pool.

use anyhow::{Result, anyhow};
use parking_lot::Mutex;
use reflow_network::{
    actor::{
        Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, ActorPayload, ActorState, MemoryState, Port,
    },
    message::Message,
};
use serde_json::Value;
use std::{collections::HashMap, sync::Arc};


use crate::db_manager::{ConnectionStatus, get_db_pool_manager};

/// Database actor for executing queries and commands
pub struct DatabaseActor {
    /// Actor ID
    id: String,
    /// Database connection ID
    connection_id: String,
}

impl DatabaseActor {
    /// Create a new database actor
    pub fn new(id: &str, connection_id: &str) -> Self {
        Self {
            id: id.to_string(),
            connection_id: connection_id.to_string(),
        }
    }

    /// Convert a Message to a JSON Value
    fn message_to_value(message: &Message) -> Result<Value> {
        serde_json::to_value(message)
            .map_err(|e| anyhow!("Could not convert message to JSON: {}", e))
    }

    /// Convert a JSON Value to a Message
    fn value_to_message(value: &Value) -> Message {
        value.clone().into()
    }

    /// Execute a query and return the results
    async fn execute_query(&self, query: &str, params: Vec<Value>) -> Result<Message> {
        let pool = get_db_pool_manager();

        // Check if the connection exists
        if !pool.has_connection(&self.connection_id) {
            return Err(anyhow!(
                "Database connection '{}' not found",
                self.connection_id
            ));
        }

        // Check connection status
        let status = pool.get_connection_status(&self.connection_id).await?;
        if status != ConnectionStatus::Connected {
            return Err(anyhow!(
                "Database connection '{}' is not connected (status: {:?})",
                self.connection_id,
                status
            ));
        }

        // Execute the query
        let results = pool
            .execute_query(&self.connection_id, query, params)
            .await?;

        // Convert results to Message format
        let mut rows = Vec::new();
        for row in results {
            rows.push(Value::Object(row).into());
        }

        Ok(Message::array(rows))
    }

    /// Execute a command and return the number of affected rows
    async fn execute_command(&self, command: &str, params: Vec<Value>) -> Result<Message> {
        let pool = get_db_pool_manager();

        // Check if the connection exists
        if !pool.has_connection(&self.connection_id) {
            return Err(anyhow!(
                "Database connection '{}' not found",
                self.connection_id
            ));
        }

        // Check connection status
        let status = pool.get_connection_status(&self.connection_id).await?;
        if status != ConnectionStatus::Connected {
            return Err(anyhow!(
                "Database connection '{}' is not connected (status: {:?})",
                self.connection_id,
                status
            ));
        }

        // Execute the command
        let affected_rows = pool
            .execute_command(&self.connection_id, command, params)
            .await?;

        Ok(Message::Integer(affected_rows as i64))
    }
}

impl Actor for DatabaseActor {
    fn get_behavior(&self) -> ActorBehavior {
        let connection_id = self.connection_id.clone();

        Box::new(move |context: ActorContext| {
            let payload: &ActorPayload = context.get_payload();
            let _state: Arc<Mutex<dyn ActorState>> = context.get_state();
            let _outports = context.get_outports();
            let connection_id = connection_id.clone();
            let payload = payload.clone();

            Box::pin(async move {
                let mut results = HashMap::new();

                // Get the query or command from the payload
                let query = match payload.get("query") {
                    Some(Message::String(q)) => q.clone(),
                    _ => match payload.get("command") {
                        Some(Message::String(c)) => c.clone(),
                        _ => {
                            results.insert(
                                "error".to_string(),
                                Message::error("Missing query or command".to_string()),
                            );
                            return Ok(results);
                        }
                    },
                };

                // Get the parameters
                let params = match payload.get("params") {
                    Some(Message::Array(p)) => {
                        let mut param_values = Vec::new();
                        for param in p.as_ref() {
                            let param_value: serde_json::Value = param.clone().into();
                            param_values.push(param_value);
                        }
                        param_values
                    }
                    Some(Message::Object(obj)) => {
                        let mut param_values = Vec::new();
                        let obj_value: serde_json::Value = obj.as_ref().clone().into();
                        param_values.push(obj_value);

                        param_values
                    }
                    _ => Vec::new(),
                };

                // Determine if this is a query or a command
                let is_query = payload.contains_key("query");

                // Create a database actor instance
                let db_actor = DatabaseActor::new("temp", &connection_id);

                // Execute the query or command
                let result = if is_query {
                    db_actor.execute_query(&query, params).await
                } else {
                    db_actor.execute_command(&query, params).await
                };

                // Process the result
                match result {
                    Ok(message) => {
                        if is_query {
                            results.insert("rows".to_string(), message);
                            results.insert("success".to_string(), Message::Boolean(true));
                        } else {
                            results.insert("affectedRows".to_string(), message);
                            results.insert("success".to_string(), Message::Boolean(true));
                        }
                    }
                    Err(e) => {
                        results.insert(
                            "error".to_string(),
                            Message::error(format!("Database error: {}", e)),
                        );
                        results.insert("success".to_string(), Message::Boolean(false));
                    }
                }

                Ok(results)
            })
        })
    }

    fn get_outports(&self) -> Port {
        let (sender, receiver) = flume::unbounded();
        (sender, receiver)
    }

    fn get_inports(&self) -> Port {
        let (sender, receiver) = flume::unbounded();
        (sender, receiver)
    }

    fn create_process(
        &self,
        actor_config: ActorConfig,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = self.get_outports();

        Box::pin(async move {
            let (_, receiver) = inports;
            let load = Arc::new(parking_lot::Mutex::new(ActorLoad::new(0)));
            
            while let Ok(payload) = receiver.recv_async().await {
                let context = ActorContext::new(
                    payload,
                    outports.clone(),
                    state.clone(),
                    actor_config.clone(),
                    load.clone(),
                );
                let result = behavior(context).await;

                if let Err(e) = result {
                    outports
                        .0
                        .send_async(HashMap::from_iter([(
                            "error".to_string(),
                            Message::error(e.to_string()),
                        )]))
                        .await
                        .unwrap();
                } else if let Ok(result) = result {
                    outports.0.send_async(result).await.unwrap();
                }
            }
        })
    }
}

#[cfg(test)]
#[cfg(feature = "db")]
mod tests {
    use serde_json::json;

    use crate::db_pool::SQLiteConnection;

    use super::*;

    #[tokio::test]
    async fn test_database_actor() -> Result<()> {
        // Initialize the database connection
        let pool = get_db_pool_manager();
        let connection_id = "test_actor_db";
        let sqlite_conn = SQLiteConnection::new(connection_id, ":memory:");
        pool.register_connection(sqlite_conn).await?;

        // Create a database actor
        let actor = DatabaseActor::new("test_actor", connection_id);

        // Get behavior function
        let behavior = actor.get_behavior();

        // Create state and ports
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = actor.get_outports();

        // Create a table
        let mut create_payload = HashMap::new();
        create_payload.insert(
            "command".to_string(),
            Message::string("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)".to_string()),
        );

        // Create ActorConfig for test
        let node = reflow_network::graph::types::GraphNode {
            id: "test_actor".to_string(),
            component: "DatabaseActor".to_string(),
            metadata: Some(HashMap::new()),
        };

        let actor_config = reflow_network::actor::ActorConfig {
            node,
            resolved_env: HashMap::new(),
            config: HashMap::new(),
            namespace: None,
        };

        let context = ActorContext::new(
            create_payload,
            outports.clone(),
            state.clone(),
            actor_config.clone(),
            Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
        );
        let result = behavior(context).await?;
        assert!(result.contains_key("affectedRows"));
        assert_eq!(result["success"], Message::Boolean(true));

        // Insert data
        let mut insert_payload = HashMap::new();
        insert_payload.insert(
            "command".to_string(),
            Message::string("INSERT INTO test (id, name) VALUES (?, ?)".to_string()),
        );
        insert_payload.insert(
            "params".to_string(),
            Message::array(vec![json!(1).into(), json!("test").into()]),
        );

        let context = ActorContext::new(
            insert_payload,
            outports.clone(),
            state.clone(),
            actor_config.clone(),
            Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
        );
        let result = behavior(context).await?;
        assert!(result.contains_key("affectedRows"));
        assert_eq!(result["success"], Message::Boolean(true));

        // Query data
        let mut query_payload = HashMap::new();
        query_payload.insert(
            "query".to_string(),
            Message::string("SELECT * FROM test".to_string()),
        );
        let context = ActorContext::new(
            query_payload,
            outports.clone(),
            state.clone(),
            actor_config.clone(),
            Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
        );
        let result = behavior(context).await?;
        assert!(result.contains_key("rows"));
        assert_eq!(result["success"], Message::Boolean(true));

        if let Message::Array(rows) = &result["rows"] {
            assert_eq!(rows.len(), 1);
            if let serde_json::Value::Object(row) = &rows[0].clone().into() {
                assert_eq!(row["id"], json!(1));
                assert_eq!(row["name"], json!("test"));
            } else {
                panic!("Expected row to be an object");
            }
        } else {
            panic!("Expected rows to be an array");
        }

        // Clean up
        pool.remove_connection(connection_id).await?;

        Ok(())
    }
}
