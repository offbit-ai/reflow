//! Unit tests for Flow Control actors (Conditional, Switch, Loop)

#[cfg(test)]
mod tests {
    use crate::flow_control::{ConditionalBranchActor, LoopActor, SwitchCaseActor};
    use crate::Actor;
    use parking_lot::Mutex;
    use reflow_actor::message::{EncodableValue, Message};
    use reflow_actor::{ActorConfig, ActorContext, ActorLoad, MemoryState};
    use reflow_graph::types::GraphNode;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_test_context(
        component: &str,
        property_values: HashMap<String, Value>,
        payload: HashMap<String, Message>,
    ) -> ActorContext {
        let mut metadata = HashMap::new();
        metadata.insert("propertyValues".to_string(), json!(property_values));

        let node = GraphNode {
            id: "test_node".to_string(),
            component: component.to_string(),
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
    async fn test_conditional_branch_greater_than_true() {
        let mut property_values = HashMap::new();
        property_values.insert("condition_type".to_string(), json!("greater_than"));
        property_values.insert("condition_value".to_string(), json!(10));

        let mut payload = HashMap::new();
        payload.insert("data".to_string(), Message::Integer(15));

        let context = create_test_context("tpl_if_branch", property_values, payload);
        let actor = ConditionalBranchActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("true-out"));
    }

    #[tokio::test]
    async fn test_conditional_branch_greater_than_false() {
        let mut property_values = HashMap::new();
        property_values.insert("condition_type".to_string(), json!("greater_than"));
        property_values.insert("condition_value".to_string(), json!(100));

        let mut payload = HashMap::new();
        payload.insert("data".to_string(), Message::Integer(5));

        let context = create_test_context("tpl_if_branch", property_values, payload);
        let actor = ConditionalBranchActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("false-out"));
    }

    #[tokio::test]
    async fn test_switch_case_matching() {
        let mut property_values = HashMap::new();
        // No switch_field → uses data directly via serde_json::to_value
        // Message::Boolean serializes as {"type":"Boolean","data":true}
        // so match against the serialized form
        property_values.insert(
            "case1_value".to_string(),
            json!({"type": "Boolean", "data": true}),
        );
        property_values.insert(
            "case2_value".to_string(),
            json!({"type": "Boolean", "data": false}),
        );

        let mut payload = HashMap::new();
        payload.insert("data".to_string(), Message::Boolean(true));

        let context = create_test_context("tpl_switch", property_values, payload);
        let actor = SwitchCaseActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("case1"));
    }

    #[tokio::test]
    async fn test_switch_case_default() {
        let mut property_values = HashMap::new();
        property_values.insert("switch_field".to_string(), json!("type"));
        property_values.insert("case1_value".to_string(), json!("known"));

        let mut payload = HashMap::new();
        payload.insert(
            "data".to_string(),
            Message::object(EncodableValue::from(json!({"type": "unknown"}))),
        );

        let context = create_test_context("tpl_switch", property_values, payload);
        let actor = SwitchCaseActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("default"));
    }

    #[tokio::test]
    async fn test_loop_actor_iterates_collection() {
        let property_values = HashMap::new();

        let mut payload = HashMap::new();
        payload.insert(
            "collection".to_string(),
            Message::Array(
                vec![
                    EncodableValue::from(json!(1)),
                    EncodableValue::from(json!(2)),
                    EncodableValue::from(json!(3)),
                ]
                .into(),
            ),
        );

        let context = create_test_context("tpl_loop", property_values, payload);
        let actor = LoopActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("item"));
    }

    #[tokio::test]
    async fn test_loop_actor_completes_without_collection() {
        let property_values = HashMap::new();
        let payload = HashMap::new();

        let context = create_test_context("tpl_loop", property_values, payload);
        let actor = LoopActor::new();
        let behavior = actor.get_behavior();

        let result = behavior(context).await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains_key("completed"));
    }
}
