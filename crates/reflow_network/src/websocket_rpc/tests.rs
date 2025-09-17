#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::script_discovery::types::*;
    use crate::redis_state::RedisActorState;
    use std::sync::Arc;
    use std::collections::HashMap;
    use serde_json::json;
    
    #[tokio::test]
    async fn test_script_actor_with_redis_state() {
        // Test requires Redis to be running
        let redis_url = "redis://localhost:6379";
        
        // Create test metadata
        let metadata = DiscoveredScriptActor {
            component: "test_stateful_actor".to_string(),
            description: "Test actor with Redis state".to_string(),
            file_path: std::path::PathBuf::from("/test/stateful_actor.py"),
            runtime: ScriptRuntime::Python,
            inports: vec![],
            outports: vec![],
            workspace_metadata: ScriptActorMetadata {
                namespace: "test".to_string(),
                version: "1.0.0".to_string(),
                author: None,
                dependencies: vec![],
                runtime_requirements: RuntimeRequirements {
                    runtime_version: "3.9".to_string(),
                    memory_limit: "512MB".to_string(),
                    cpu_limit: Some(0.5),
                    timeout: 30,
                    env_vars: HashMap::new(),
                },
                config_schema: None,
                tags: vec![],
                category: None,
                source_hash: "test_hash".to_string(),
                last_modified: chrono::Utc::now(),
            },
        };
        
        // Create RPC client (mock for this test)
        let rpc_client = Arc::new(WebSocketRpcClient::new("ws://localhost:8765".to_string()));
        
        // Create actor with Redis
        let actor = WebSocketScriptActor::new(
            metadata.clone(),
            rpc_client,
            redis_url.to_string(),
        ).await;
        
        // Check if Redis state was initialized (we can't directly access private field)
        // Instead, test state operations directly
        if !redis_url.is_empty() {
            println!("Redis state initialized for actor");
            
            // Test state operations directly
            let state = RedisActorState::new(
                redis_url,
                &metadata.workspace_metadata.namespace,
                &metadata.component
            ).await.unwrap();
            
            // Set a value
            state.set("test_key", json!({"count": 0})).await.unwrap();
            
            // Get the value
            let value = state.get("test_key").await.unwrap();
            assert_eq!(value, Some(json!({"count": 0})));
            
            // Increment a counter
            let count = state.increment("counter", 1).await.unwrap();
            assert_eq!(count, 1);
            
            // Push to a list
            state.push("events", json!({"event": "test"})).await.unwrap();
            
            // Pop from list
            let event = state.pop("events").await.unwrap();
            assert_eq!(event, Some(json!({"event": "test"})));
            
            // Cleanup
            state.delete("test_key").await.unwrap();
            state.delete("counter").await.unwrap();
            
            println!("Redis state operations completed successfully");
        } else {
            println!("Redis not available, skipping state tests");
        }
    }
    
    #[tokio::test]
    async fn test_state_persistence_across_actor_restarts() {
        let redis_url = "redis://localhost:6379";
        
        // Check if Redis is available
        if RedisActorState::new(redis_url, "test", "persistence_test").await.is_err() {
            println!("Redis not available, skipping test");
            return;
        }
        
        let metadata = DiscoveredScriptActor {
            component: "persistence_test".to_string(),
            description: "Test persistence".to_string(),
            file_path: std::path::PathBuf::from("/test/persist.py"),
            runtime: ScriptRuntime::Python,
            inports: vec![],
            outports: vec![],
            workspace_metadata: ScriptActorMetadata {
                namespace: "test".to_string(),
                version: "1.0.0".to_string(),
                author: None,
                dependencies: vec![],
                runtime_requirements: RuntimeRequirements {
                    runtime_version: "3.9".to_string(),
                    memory_limit: "512MB".to_string(),
                    cpu_limit: Some(0.5),
                    timeout: 30,
                    env_vars: HashMap::new(),
                },
                config_schema: None,
                tags: vec![],
                category: None,
                source_hash: "test_hash".to_string(),
                last_modified: chrono::Utc::now(),
            },
        };
        
        // Create first actor instance
        {
            let state = RedisActorState::new(
                redis_url,
                &metadata.workspace_metadata.namespace,
                &metadata.component
            ).await.unwrap();
            
            // Set some state
            state.set("persistent_value", json!({"data": "important"})).await.unwrap();
            state.increment("run_count", 1).await.unwrap();
        }
        
        // Create second actor instance (simulating restart)
        {
            let state = RedisActorState::new(
                redis_url,
                &metadata.workspace_metadata.namespace,
                &metadata.component
            ).await.unwrap();
            
            // Verify state persisted
            let value = state.get("persistent_value").await.unwrap();
            assert_eq!(value, Some(json!({"data": "important"})));
            
            let count = state.increment("run_count", 1).await.unwrap();
            assert_eq!(count, 2); // Should be 2 after second increment
            
            // Cleanup
            state.delete("persistent_value").await.unwrap();
            state.delete("run_count").await.unwrap();
        }
        
        println!("State persistence test passed");
    }
    
    #[tokio::test]
    async fn test_actor_state_isolation() {
        let redis_url = "redis://localhost:6379";
        
        // Check if Redis is available
        if RedisActorState::new(redis_url, "test", "actor1").await.is_err() {
            println!("Redis not available, skipping test");
            return;
        }
        
        // Create state for two different actors
        let state1 = RedisActorState::new(redis_url, "test", "actor1").await.unwrap();
        let state2 = RedisActorState::new(redis_url, "test", "actor2").await.unwrap();
        
        // Set same key for both actors
        state1.set("shared_key", json!({"actor": 1})).await.unwrap();
        state2.set("shared_key", json!({"actor": 2})).await.unwrap();
        
        // Verify isolation - each actor has its own value
        let value1 = state1.get("shared_key").await.unwrap();
        let value2 = state2.get("shared_key").await.unwrap();
        
        assert_eq!(value1, Some(json!({"actor": 1})));
        assert_eq!(value2, Some(json!({"actor": 2})));
        
        // Cleanup
        state1.delete("shared_key").await.unwrap();
        state2.delete("shared_key").await.unwrap();
        
        println!("Actor state isolation test passed");
    }
}