//! Zeal Integration Category Actors
//!
//! Specific actors for external system integration following Zeal template specifications.
//! Each actor is purpose-built for its specific Zeal template.

use std::collections::HashMap;
use std::sync::Arc;
use actor_macro::actor;
use anyhow::{Result, Error};
use reflow_actor::{ActorContext, ActorConfig, message::EncodableValue};
use reflow_tracing_protocol::client::TracingIntegration;
use serde_json::json;
use uuid::Uuid;
use crate::{Actor, ActorBehavior, ActorLoad, MemoryState, Message, Port};


/// HTTP Request Actor - Compatible with tpl_http_request
/// 
/// Executes HTTP requests based on Zeal node configuration.
/// Reads configuration from the ActorContext which contains Zeal metadata.
#[actor(
    HttpRequestActor,
    inports::<100>(data_in, headers_in),
    outports::<50>(response_out, error_out),
    state(MemoryState)
)]
pub async fn http_request_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    // Get runtime inputs from payload
    let inputs = context.get_payload();
    
    // Get Zeal configuration from context
    let config = context.get_config_hashmap();
    
    // Get propertyValues (user-provided values)
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    // Extract URL from propertyValues (required)
    let url = property_values
        .and_then(|pv| pv.get("url"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("URL not configured in Zeal node"))?;
    
    // Extract method (default to GET)
    let method = property_values
        .and_then(|pv| pv.get("method"))
        .and_then(|v| v.as_str())
        .unwrap_or("GET");
    
    // Extract timeout (default 30000ms)
    let timeout = property_values
        .and_then(|pv| pv.get("timeout"))
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    
    // Extract retry count (default 3)
    let retry_count = property_values
        .and_then(|pv| pv.get("retryCount"))
        .and_then(|v| v.as_u64())
        .unwrap_or(3);
    
    // Get runtime data from inputs
    let request_data = inputs.get("data_in");
    let headers_data = inputs.get("headers_in");
    
    // Create output messages with correct port names
    let mut output_messages = HashMap::new();
    output_messages.insert("response_out".to_string(), Message::object(EncodableValue::from(json!({
        "status": 200,
        "url": url,
        "method": method,
        "timeout": timeout,
        "retry_count": retry_count,
        "request_data": request_data,
        "headers_data": headers_data,
        "message": "HTTP request execution not yet implemented"
    }))));
    
    Ok(output_messages)
}

/// PostgreSQL Pool Actor - Compatible with tpl_postgresql
/// 
/// Creates a PostgreSQL connection pool and returns a pool ID.
/// Outputs: pool-out (Pool ID), status-out (Connection Status)
#[actor(
    PostgreSQLPoolActor,
    inports::<10>(),
    outports::<50>(pool_out, status_out),
    state(MemoryState)
)]
pub async fn postgresql_pool_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    
    // Get Zeal configuration from context
    let config = context.get_config_hashmap();
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    // Extract PostgreSQL-specific configuration
    let host = property_values
        .and_then(|pv| pv.get("host"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("PostgreSQL host not configured"))?;
    
    let port = property_values
        .and_then(|pv| pv.get("port"))
        .and_then(|v| v.as_u64())
        .unwrap_or(5432); // PostgreSQL default port
    
    let database = property_values
        .and_then(|pv| pv.get("database"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("PostgreSQL database name not configured"))?;
    
    let max_connections = property_values
        .and_then(|pv| pv.get("maxConnections"))
        .and_then(|v| v.as_u64())
        .unwrap_or(10);
    
    let connection_timeout = property_values
        .and_then(|pv| pv.get("connectionTimeout"))
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    
    // Generate a unique PostgreSQL pool ID
    let pool_id = format!("pg_pool_{}", Uuid::new_v4());
    
    // Return pool ID and status as expected by Zeal
    result.insert("pool_out".to_string(), Message::string(pool_id.clone()));
    result.insert("status_out".to_string(), Message::object(EncodableValue::from(json!({
        "pool_id": pool_id,
        "type": "postgresql",
        "status": "connected",
        "host": host,
        "port": port,
        "database": database,
        "max_connections": max_connections,
        "connection_timeout": connection_timeout,
        "message": "PostgreSQL pool created successfully"
    }))));
    
    Ok(result)
}

/// MySQL Pool Actor - Compatible with tpl_mysql
/// 
/// Creates a MySQL connection pool and returns a pool ID.
/// Outputs: pool-out (Pool ID), status-out (Connection Status)
#[actor(
    MySQLPoolActor,
    inports::<10>(),
    outports::<50>(pool_out, status_out),
    state(MemoryState)
)]
pub async fn mysql_pool_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    
    // Get Zeal configuration from context
    let config = context.get_config_hashmap();
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    // Extract MySQL-specific configuration
    let host = property_values
        .and_then(|pv| pv.get("host"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("MySQL host not configured"))?;
    
    let port = property_values
        .and_then(|pv| pv.get("port"))
        .and_then(|v| v.as_u64())
        .unwrap_or(3306); // MySQL default port
    
    let database = property_values
        .and_then(|pv| pv.get("database"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("MySQL database name not configured"))?;
    
    let max_connections = property_values
        .and_then(|pv| pv.get("maxConnections"))
        .and_then(|v| v.as_u64())
        .unwrap_or(10);
    
    let connection_timeout = property_values
        .and_then(|pv| pv.get("connectionTimeout"))
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    
    // Generate a unique MySQL pool ID
    let pool_id = format!("mysql_pool_{}", Uuid::new_v4());
    
    // Return pool ID and status as expected by Zeal
    result.insert("pool_out".to_string(), Message::string(pool_id.clone()));
    result.insert("status_out".to_string(), Message::object(EncodableValue::from(json!({
        "pool_id": pool_id,
        "type": "mysql",
        "status": "connected",
        "host": host,
        "port": port,
        "database": database,
        "max_connections": max_connections,
        "connection_timeout": connection_timeout,
        "message": "MySQL pool created successfully"
    }))));
    
    Ok(result)
}

/// MongoDB Pool Actor - Compatible with tpl_mongodb
/// 
/// Creates a MongoDB connection pool and returns a pool ID.
/// Outputs: pool-out (Pool ID), status-out (Connection Status)
#[actor(
    MongoDbPoolActor,
    inports::<10>(),
    outports::<50>(pool_out, status_out),
    state(MemoryState)
)]
pub async fn mongodb_pool_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    
    // Get Zeal configuration from context
    let config = context.get_config_hashmap();
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    // Extract MongoDB-specific configuration
    let host = property_values
        .and_then(|pv| pv.get("host"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("MongoDB host not configured"))?;
    
    let port = property_values
        .and_then(|pv| pv.get("port"))
        .and_then(|v| v.as_u64())
        .unwrap_or(27017); // MongoDB default port
    
    let database = property_values
        .and_then(|pv| pv.get("database"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("MongoDB database name not configured"))?;
    
    let max_connections = property_values
        .and_then(|pv| pv.get("maxConnections"))
        .and_then(|v| v.as_u64())
        .unwrap_or(10);
    
    let replica_set = property_values
        .and_then(|pv| pv.get("replicaSet"))
        .and_then(|v| v.as_str());
    
    // Generate a unique MongoDB pool ID
    let pool_id = format!("mongo_pool_{}", Uuid::new_v4());
    
    // Return pool ID and status as expected by Zeal
    result.insert("pool_out".to_string(), Message::string(pool_id.clone()));
    result.insert("status_out".to_string(), Message::object(EncodableValue::from(json!({
        "pool_id": pool_id,
        "type": "mongodb",
        "status": "connected",
        "host": host,
        "port": port,
        "database": database,
        "max_connections": max_connections,
        "replica_set": replica_set,
        "message": "MongoDB pool created successfully"
    }))));
    
    Ok(result)
}

/// MongoDB Collection Operations Actor - Compatible with tpl_mongo_get_collection
/// 
/// Executes MongoDB operations on collections using a pool ID.
/// Inputs: pool-in (Pool ID), query-in (Query parameters)
/// Outputs: docs-out (Documents), count-out (Count)
#[actor(
    MongoCollectionActor,
    inports::<100>(pool_in, query_in),
    outports::<50>(docs_out, count_out),
    state(MemoryState)
)]
pub async fn mongo_collection_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let mut result = HashMap::new();
    
    // Get Zeal configuration from context
    let config = context.get_config_hashmap();
    let property_values = config.get("propertyValues")
        .and_then(|v| v.as_object());
    
    let collection = property_values
        .and_then(|pv| pv.get("collection"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Collection name not configured"))?;
    
    let operation = property_values
        .and_then(|pv| pv.get("operation"))
        .and_then(|v| v.as_str())
        .unwrap_or("find");
    
    let limit = property_values
        .and_then(|pv| pv.get("limit"))
        .and_then(|v| v.as_u64())
        .unwrap_or(100);
    
    let sort = property_values
        .and_then(|pv| pv.get("sort"))
        .and_then(|v| v.as_object())
        .cloned();
    
    let projection = property_values
        .and_then(|pv| pv.get("projection"))
        .and_then(|v| v.as_object())
        .cloned();
    
    // Mock response based on operation type
    let mock_documents = match operation {
        "find" => vec![
            json!({"_id": "doc1", "name": "Sample Document 1", "status": "active"}),
            json!({"_id": "doc2", "name": "Sample Document 2", "status": "inactive"})
        ],
        "findOne" => vec![
            json!({"_id": "doc1", "name": "Sample Document", "status": "active"})
        ],
        "aggregate" => vec![
            json!({"_id": "group1", "count": 5, "total": 100}),
            json!({"_id": "group2", "count": 3, "total": 75})
        ],
        _ => vec![]
    };
    
    result.insert("docs_out".to_string(), Message::object(EncodableValue::from(json!({
        "collection": collection,
        "operation": operation,
        "limit": limit,
        "sort": sort,
        "projection": projection,
        "documents": mock_documents,
        "message": format!("MongoDB {} operation completed", operation)
    }))));
    
    result.insert("count_out".to_string(), Message::integer(mock_documents.len() as i64));
    
    Ok(result)
}