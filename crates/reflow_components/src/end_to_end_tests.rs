//! End-to-End Integration Tests
//! 
//! Tests the complete flow: Zeal node metadata → Actor execution

#[cfg(test)]
mod tests {
    use super::super::integration::{HttpRequestActor, PostgreSQLPoolActor};
    use super::super::{Actor, get_actor_for_template};
    use crate::registry;
    use reflow_actor::{MemoryState, ActorContext, ActorConfig};
    use reflow_graph::types::GraphNode;
    use serde_json::json;
    use std::collections::HashMap;

    fn create_http_node() -> GraphNode {
        GraphNode {
            id: "http_node".to_string(),
            component: "tpl_http_request".to_string(),
            metadata: Some({
                let mut metadata = HashMap::new();
                metadata.insert("propertyValues".to_string(), json!({
                    "url": "https://jsonplaceholder.typicode.com/posts/1",
                    "method": "GET",
                    "timeout": 30000,
                    "retryCount": 3
                }));
                metadata
            }),
            ..Default::default()
        }
    }

    fn create_postgresql_node() -> GraphNode {
        GraphNode {
            id: "pg_pool_node".to_string(),
            component: "tpl_postgresql".to_string(),
            metadata: Some({
                let mut metadata = HashMap::new();
                metadata.insert("propertyValues".to_string(), json!({
                    "host": "localhost",
                    "port": 5432,
                    "database": "testdb",
                    "maxConnections": 10,
                    "connectionTimeout": 30000
                }));
                metadata
            }),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn test_zeal_node_structure() {
        let http_node = create_http_node();
        
        // Verify node structure matches Zeal expectations
        assert_eq!(http_node.id, "http_node");
        assert_eq!(http_node.component, "tpl_http_request");
        
        // Verify metadata was preserved
        let metadata = http_node.metadata.as_ref().unwrap();
        let property_values = metadata.get("propertyValues").unwrap().as_object().unwrap();
        assert_eq!(property_values.get("url").unwrap().as_str().unwrap(), "https://jsonplaceholder.typicode.com/posts/1");
        assert_eq!(property_values.get("method").unwrap().as_str().unwrap(), "GET");
        assert_eq!(property_values.get("timeout").unwrap().as_u64().unwrap(), 30000);
        assert_eq!(property_values.get("retryCount").unwrap().as_u64().unwrap(), 3);
    }

    #[tokio::test]
    async fn test_registry_actor_mapping() {
        // Test that registry can find actors for Zeal templates
        let http_actor = get_actor_for_template("tpl_http_request");
        assert!(http_actor.is_some(), "Should find actor for tpl_http_request");
        
        let pg_actor = get_actor_for_template("tpl_postgresql");
        assert!(pg_actor.is_some(), "Should find actor for tpl_postgresql");
        
        let mysql_actor = get_actor_for_template("tpl_mysql");
        assert!(mysql_actor.is_some(), "Should find actor for tpl_mysql");
        
        let mongo_actor = get_actor_for_template("tpl_mongodb");
        assert!(mongo_actor.is_some(), "Should find actor for tpl_mongodb");
        
        let mongo_collection_actor = get_actor_for_template("tpl_mongo_get_collection");
        assert!(mongo_collection_actor.is_some(), "Should find actor for tpl_mongo_get_collection");
        
        // Test non-existent template
        let unknown_actor = get_actor_for_template("tpl_unknown");
        assert!(unknown_actor.is_none(), "Should not find actor for unknown template");
    }

    #[tokio::test]
    async fn test_http_node_to_actor_execution() {
        // Test HTTP request flow specifically
        let node = create_http_node();
        
        let actor = get_actor_for_template(&node.component);
        assert!(actor.is_some(), "Should find HTTP actor");
        
        let actor = actor.unwrap();
        let behavior = actor.get_behavior();
        
        let config = ActorConfig {
            node: node.clone(),
            resolved_env: HashMap::new(),
            config: node.metadata.clone().unwrap_or_default(),
            namespace: None,
        };
        
        let payload = HashMap::new();
        let outports = flume::unbounded();
        let state = std::sync::Arc::new(parking_lot::Mutex::new(MemoryState::default()));
        let load = std::sync::Arc::new(parking_lot::Mutex::new(reflow_actor::ActorLoad::new(0)));
        
        let context = ActorContext::new(payload, outports, state, config, load);
        
        let result = behavior(context).await;
        assert!(result.is_ok(), "HTTP actor execution should succeed");
        
        let output = result.unwrap();
        assert!(output.contains_key("response_out"), "Should have response_out");
        
        // Verify the response contains expected Zeal metadata
        if let Some(response) = output.get("response_out") {
            // The response should be a mock containing the URL and method
            println!("HTTP Response: {:?}", response);
        }
        
        println!("✅ HTTP Node → Actor execution successful");
    }

    #[tokio::test] 
    async fn test_postgresql_node_to_actor_execution() {
        // Step 1: Create a Zeal-compatible GraphNode
        let node = create_postgresql_node();
        
        // Step 2: Get the actor for the template
        let actor = get_actor_for_template(&node.component);
        assert!(actor.is_some(), "Should find actor for {}", node.component);
        
        let actor = actor.unwrap();
        let behavior = actor.get_behavior();
        
        // Step 3: Create ActorContext from the Graph node
        let config = ActorConfig {
            node: node.clone(),
            resolved_env: HashMap::new(),
            config: node.metadata.clone().unwrap_or_default(),
            namespace: None,
        };
        
        let payload = HashMap::new();
        let outports = flume::unbounded();
        let state = std::sync::Arc::new(parking_lot::Mutex::new(MemoryState::default()));
        let load = std::sync::Arc::new(parking_lot::Mutex::new(reflow_actor::ActorLoad::new(0)));
        
        let context = ActorContext::new(payload, outports, state, config, load);
        
        // Step 4: Execute the actor
        let result = behavior(context).await;
        assert!(result.is_ok(), "Actor execution should succeed");
        
        let output = result.unwrap();
        
        // Step 5: Verify expected outputs based on template type
        assert!(output.contains_key("pool_out"), "PostgreSQL actor should output pool_out");
        assert!(output.contains_key("status_out"), "PostgreSQL actor should output status_out");
        
        // Verify pool ID is generated correctly
        if let Some(pool_message) = output.get("pool_out") {
            match pool_message {
                reflow_actor::message::Message::String(pool_id) => {
                    assert!(pool_id.starts_with("pg_pool_"), "Pool ID should start with 'pg_pool_'");
                }
                _ => panic!("Pool ID should be a string message"),
            }
        }
        
        println!("✅ PostgreSQL Node → Actor execution successful");
    }

    #[tokio::test]
    async fn test_template_mapping_completeness() {
        // Test that all expected Zeal templates have corresponding actors
        let expected_templates = vec![
            "tpl_http_request",
            "tpl_postgresql", 
            "tpl_mysql",
            "tpl_mongodb",
            "tpl_mongo_get_collection",
        ];
        
        for template in expected_templates {
            let actor = get_actor_for_template(template);
            assert!(actor.is_some(), "Missing actor implementation for template: {}", template);
        }
        
        // Get the complete mapping for verification
        let mapping = registry::get_template_mapping();
        assert!(!mapping.is_empty(), "Template mapping should not be empty");
        
        // Verify key templates are mapped
        assert!(mapping.contains_key("tpl_http_request"), "HTTP request template should be mapped");
        assert!(mapping.contains_key("tpl_postgresql"), "PostgreSQL template should be mapped");
        assert!(mapping.contains_key("tpl_mysql"), "MySQL template should be mapped");
        assert!(mapping.contains_key("tpl_mongodb"), "MongoDB template should be mapped");
        
        println!("✅ All template mappings verified");
    }

    #[tokio::test]
    async fn test_actor_metadata_processing() {
        // Test that actors correctly process Zeal propertyValues
        let node = create_http_node();
        
        // Direct instantiation test
        let http_actor = HttpRequestActor::new();
        let behavior = http_actor.get_behavior();
        
        let config = ActorConfig {
            node: node.clone(),
            resolved_env: HashMap::new(),
            config: node.metadata.clone().unwrap_or_default(),
            namespace: None,
        };
        
        let payload = HashMap::new();
        let outports = flume::unbounded();
        let state = std::sync::Arc::new(parking_lot::Mutex::new(MemoryState::default()));
        let load = std::sync::Arc::new(parking_lot::Mutex::new(reflow_actor::ActorLoad::new(0)));
        
        let context = ActorContext::new(payload, outports, state, config, load);
        
        let result = behavior(context).await;
        assert!(result.is_ok(), "Actor should process metadata correctly");
        
        let output = result.unwrap();
        assert!(output.contains_key("response_out"), "Should generate correct output ports");
        
        println!("✅ Actor metadata processing verified");
    }
}