//! Unit tests for Integration actors (HTTP)

#[cfg(test)]
mod tests {
    use crate::integration::HttpRequestActor;
    use crate::Actor;
    use parking_lot::Mutex;
    use reflow_actor::ActorLoad;
    use reflow_actor::{ActorConfig, ActorContext, MemoryState};
    use reflow_graph::types::GraphNode;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_test_context(property_values: HashMap<String, Value>) -> ActorContext {
        let mut metadata = HashMap::new();
        metadata.insert("propertyValues".to_string(), json!(property_values));

        let node = GraphNode {
            id: "test_node".to_string(),
            component: "TestComponent".to_string(),
            metadata: Some(metadata.clone()),
        };

        let config = ActorConfig {
            node,
            resolved_env: HashMap::new(),
            config: metadata,
            namespace: None,
            inport_connection_counts: HashMap::new(),
        };

        let payload = HashMap::new();
        let outports = flume::unbounded();
        let state = Arc::new(Mutex::new(MemoryState::default()));
        let load = Arc::new(ActorLoad::new(0));

        ActorContext::new(payload, outports, state, config, load)
    }

    #[tokio::test]
    async fn test_http_request_actor_creation() {
        let _actor = HttpRequestActor::new();
    }

    #[tokio::test]
    async fn test_http_request_actor_with_valid_config() {
        let mut property_values = HashMap::new();
        property_values.insert("url".to_string(), json!("https://api.example.com/data"));
        property_values.insert("method".to_string(), json!("GET"));
        property_values.insert("timeout".to_string(), json!(30000));

        let context = create_test_context(property_values);
        let actor = HttpRequestActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;

        // Should return a result (may be error_out if network fails, but shouldn't panic)
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(
            output.contains_key("response_out") || output.contains_key("error_out"),
            "Should have response_out or error_out"
        );
    }

    #[tokio::test]
    async fn test_http_request_actor_missing_url() {
        let mut property_values = HashMap::new();
        property_values.insert("method".to_string(), json!("GET"));

        let context = create_test_context(property_values);
        let actor = HttpRequestActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("URL"));
        }
    }

    #[tokio::test]
    async fn test_http_request_actor_unsupported_method() {
        let mut property_values = HashMap::new();
        property_values.insert("url".to_string(), json!("https://api.example.com"));
        property_values.insert("method".to_string(), json!("INVALID_METHOD"));

        let context = create_test_context(property_values);
        let actor = HttpRequestActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;

        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.to_string().contains("Unsupported"));
        }
    }
}
