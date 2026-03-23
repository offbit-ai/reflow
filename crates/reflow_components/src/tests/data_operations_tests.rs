//! Unit tests for Data Operations actors

#[cfg(test)]
mod tests {
    use crate::transform::{DataOperationsActor, DataTransformActor};
    use crate::Actor;
    use parking_lot::Mutex;
    use reflow_actor::message::{EncodableValue, Message};
    use reflow_actor::{ActorConfig, ActorContext, ActorLoad, MemoryState};
    use reflow_graph::types::GraphNode;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_test_context(
        property_values: HashMap<String, Value>,
        payload: HashMap<String, Message>,
    ) -> ActorContext {
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
            inport_connection_counts: HashMap::new(),
        };

        let outports = flume::unbounded();
        let state = Arc::new(Mutex::new(MemoryState::default()));
        let load = Arc::new(ActorLoad::new(0));

        ActorContext::new(payload, outports, state, config, load)
    }

    #[tokio::test]
    async fn test_data_transform_uppercase() {
        let mut property_values = HashMap::new();
        property_values.insert("transform_type".to_string(), json!("to_uppercase"));

        let mut payload = HashMap::new();
        payload.insert(
            "input".to_string(),
            Message::string("hello world".to_string()),
        );

        let context = create_test_context(property_values, payload);
        let actor = DataTransformActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("output"));
    }

    #[tokio::test]
    async fn test_data_transform_passthrough() {
        let property_values = HashMap::new();

        let mut payload = HashMap::new();
        payload.insert("input".to_string(), Message::string("keep me".to_string()));

        let context = create_test_context(property_values, payload);
        let actor = DataTransformActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("output"));
    }

    #[tokio::test]
    async fn test_data_transform_extract_field() {
        let mut property_values = HashMap::new();
        property_values.insert("transform_type".to_string(), json!("extract_field"));
        property_values.insert("field_name".to_string(), json!("name"));

        let mut payload = HashMap::new();
        payload.insert(
            "input".to_string(),
            Message::object(EncodableValue::from(json!({"name": "Alice", "age": 30}))),
        );

        let context = create_test_context(property_values, payload);
        let actor = DataTransformActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("output"));
    }

    #[tokio::test]
    async fn test_data_transform_missing_input() {
        let property_values = HashMap::new();
        let payload = HashMap::new();

        let context = create_test_context(property_values, payload);
        let actor = DataTransformActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_data_operations_passthrough_no_ops() {
        let property_values = HashMap::new();

        let mut payload = HashMap::new();
        payload.insert(
            "data".to_string(),
            Message::string("pass through".to_string()),
        );

        let context = create_test_context(property_values, payload);
        let actor = DataOperationsActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("output"));
    }

    #[tokio::test]
    async fn test_data_operations_filter() {
        let mut property_values = HashMap::new();
        property_values.insert(
            "dataOperations".to_string(),
            json!([{
                "operations": [{
                    "type": "filter",
                    "filterExpression": "item.price > 1",
                    "enabled": true
                }]
            }]),
        );

        let mut payload = HashMap::new();
        payload.insert(
            "data".to_string(),
            Message::object(EncodableValue::from(json!([
                {"name": "apple", "price": 1.20},
                {"name": "banana", "price": 0.80},
                {"name": "cherry", "price": 2.50}
            ]))),
        );

        let context = create_test_context(property_values, payload);
        let actor = DataOperationsActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_operations_aggregate_sum() {
        let mut property_values = HashMap::new();
        property_values.insert(
            "dataOperations".to_string(),
            json!([{
                "operations": [{
                    "type": "aggregate",
                    "aggregateField": "data.price",
                    "aggregateFunction": "sum",
                    "enabled": true
                }]
            }]),
        );

        let mut payload = HashMap::new();
        payload.insert(
            "data".to_string(),
            Message::object(EncodableValue::from(json!([
                {"name": "apple", "price": 1.20},
                {"name": "banana", "price": 0.80},
            ]))),
        );

        let context = create_test_context(property_values, payload);
        let actor = DataOperationsActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
    }
}
