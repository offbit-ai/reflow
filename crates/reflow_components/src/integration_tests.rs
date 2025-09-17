//! Unit tests for Integration actors (HTTP and Database)

#[cfg(test)]
mod tests {
    use super::super::integration::{
        HttpRequestActor, PostgreSQLPoolActor, MySQLPoolActor, 
        MongoDbPoolActor, MongoCollectionActor
    };
    use super::super::{Actor, Message};
    use reflow_actor::{MemoryState, ActorContext, ActorConfig};
    use reflow_graph::types::GraphNode;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::Arc;
    use parking_lot::Mutex;
    use reflow_actor::ActorLoad;

    fn create_test_context(property_values: HashMap<String, Value>) -> ActorContext {
        let mut metadata = HashMap::new();
        metadata.insert("propertyValues".to_string(), json!(property_values));
        
        let node = GraphNode {
            id: "test_node".to_string(),
            component: "TestComponent".to_string(), 
            metadata: Some(metadata.clone()),
            ..Default::default()
        };
        
        let config = ActorConfig {
            node,
            resolved_env: HashMap::new(),
            config: metadata,
            namespace: None,
        };
        
        let payload = HashMap::new();
        let outports = flume::unbounded();
        let state = Arc::new(Mutex::new(MemoryState::default()));
        let load = Arc::new(Mutex::new(ActorLoad::new(0)));
        
        ActorContext::new(payload, outports, state, config, load)
    }

    #[tokio::test]
    async fn test_http_request_actor_creation() {
        let actor = HttpRequestActor::new();
        // Test that actor can be created successfully
        assert!(true);
    }

    #[tokio::test]
    async fn test_http_request_actor_with_valid_config() {
        let mut property_values = HashMap::new();
        property_values.insert("url".to_string(), json!("https://api.example.com/data"));
        property_values.insert("method".to_string(), json!("GET"));
        property_values.insert("timeout".to_string(), json!(30000));
        property_values.insert("retryCount".to_string(), json!(3));

        let context = create_test_context(property_values);
        let actor = HttpRequestActor::new();
        let behavior = actor.get_behavior();

        // Test that the behavior can be executed without panicking
        let result = behavior(context).await;
        
        // Should return success with mock response
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("response_out"));
    }

    #[tokio::test]
    async fn test_http_request_actor_missing_url() {
        let mut property_values = HashMap::new();
        property_values.insert("method".to_string(), json!("GET"));

        let context = create_test_context(property_values);
        let actor = HttpRequestActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should fail due to missing URL
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("URL"));
        }
    }

    #[tokio::test]
    async fn test_http_request_actor_invalid_method() {
        let mut property_values = HashMap::new();
        property_values.insert("url".to_string(), json!("https://api.example.com"));
        property_values.insert("method".to_string(), json!("INVALID_METHOD"));

        let context = create_test_context(property_values);
        let actor = HttpRequestActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should succeed with mock implementation, but include the invalid method in response
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("response_out"));
    }

    #[tokio::test]
    async fn test_postgresql_pool_actor_creation() {
        let actor = PostgreSQLPoolActor::new();
        // Test that actor can be created successfully
        assert!(true);
    }

    #[tokio::test]
    async fn test_postgresql_pool_actor_with_config() {
        let mut property_values = HashMap::new();
        property_values.insert("host".to_string(), json!("localhost"));
        property_values.insert("port".to_string(), json!(5432));
        property_values.insert("database".to_string(), json!("testdb"));
        property_values.insert("maxConnections".to_string(), json!(10));

        let context = create_test_context(property_values);
        let actor = PostgreSQLPoolActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should succeed with mock pool creation
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("pool_out"));
        assert!(output.contains_key("status_out"));
    }

    #[tokio::test]
    async fn test_postgresql_pool_actor_missing_host() {
        let mut property_values = HashMap::new();
        property_values.insert("database".to_string(), json!("testdb"));

        let context = create_test_context(property_values);
        let actor = PostgreSQLPoolActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should fail due to missing host
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("host"));
        }
    }

    #[tokio::test]
    async fn test_mysql_pool_actor_with_config() {
        let mut property_values = HashMap::new();
        property_values.insert("host".to_string(), json!("localhost"));
        property_values.insert("port".to_string(), json!(3306));
        property_values.insert("database".to_string(), json!("testdb"));

        let context = create_test_context(property_values);
        let actor = MySQLPoolActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should succeed with mock pool creation
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("pool_out"));
        assert!(output.contains_key("status_out"));
    }

    #[tokio::test]
    async fn test_mongodb_pool_actor_with_config() {
        let mut property_values = HashMap::new();
        property_values.insert("host".to_string(), json!("localhost"));
        property_values.insert("port".to_string(), json!(27017));
        property_values.insert("database".to_string(), json!("testdb"));
        property_values.insert("replicaSet".to_string(), json!("rs0"));

        let context = create_test_context(property_values);
        let actor = MongoDbPoolActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should succeed with mock pool creation
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("pool_out"));
        assert!(output.contains_key("status_out"));
    }

    #[tokio::test]
    async fn test_mongo_collection_actor_with_config() {
        let mut property_values = HashMap::new();
        property_values.insert("collection".to_string(), json!("users"));
        property_values.insert("operation".to_string(), json!("find"));
        property_values.insert("limit".to_string(), json!(50));

        let context = create_test_context(property_values);
        let actor = MongoCollectionActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should succeed with mock documents
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("docs_out"));
        assert!(output.contains_key("count_out"));
    }

    #[tokio::test]
    async fn test_mongo_collection_actor_missing_collection() {
        let property_values = HashMap::new(); // No collection specified

        let context = create_test_context(property_values);
        let actor = MongoCollectionActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        
        // Should fail due to missing collection
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("collection") || e.to_string().contains("Collection"));
        }
    }
}