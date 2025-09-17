//! Unit tests for Flow Control actors (Conditional, Switch, Loop)

#[cfg(test)]
mod tests {
    use super::super::flow_control::{ConditionalBranchActor, SwitchCaseActor, LoopActor};
    use super::super::Actor;
    use reflow_actor::{ActorContext, Message};
    use serde_json::{json, Value};
    use std::collections::HashMap;

    fn create_actor_context(property_values: HashMap<String, Value>) -> ActorContext {
        let mut config = HashMap::new();
        config.insert("propertyValues".to_string(), json!(property_values));
        
        ActorContext::new(
            "test_actor".to_string(),
            config,
            None
        )
    }

    #[tokio::test]
    async fn test_conditional_branch_actor_creation() {
        let actor = ConditionalBranchActor::new();
        assert_eq!(actor.actor_type(), "ConditionalBranchActor");
    }

    #[tokio::test]
    async fn test_conditional_branch_actor_simple_condition() {
        let mut property_values = HashMap::new();
        
        // Create decision rules as Zeal would provide them
        let decision_rules = vec![
            json!({
                "condition": "input.value > 10",
                "action": "continue"
            })
        ];
        property_values.insert("decisionRules".to_string(), json!(decision_rules));

        let context = create_actor_context(property_values);
        let actor = ConditionalBranchActor::new();

        // Test with a value that should pass the condition
        let input_message = Message::Object(json!({"value": 15}));
        let result = actor.process(context, input_message).await;
        
        // Should succeed in processing
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_conditional_branch_actor_no_decision_rules() {
        let property_values = HashMap::new(); // No decision rules

        let context = create_actor_context(property_values);
        let actor = ConditionalBranchActor::new();

        let result = actor.process(context, Message::Object(json!({"value": 5}))).await;
        
        // Should handle missing decision rules gracefully
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_conditional_branch_actor_multiple_conditions() {
        let mut property_values = HashMap::new();
        
        let decision_rules = vec![
            json!({
                "condition": "input.value > 20",
                "action": "high_value"
            }),
            json!({
                "condition": "input.value > 10",
                "action": "medium_value"
            }),
            json!({
                "condition": "input.value > 0",
                "action": "low_value"
            })
        ];
        property_values.insert("decisionRules".to_string(), json!(decision_rules));

        let context = create_actor_context(property_values);
        let actor = ConditionalBranchActor::new();

        let result = actor.process(context, Message::Object(json!({"value": 15}))).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_switch_case_actor_creation() {
        let actor = SwitchCaseActor::new();
        assert_eq!(actor.actor_type(), "SwitchCaseActor");
    }

    #[tokio::test]
    async fn test_switch_case_actor_with_cases() {
        let mut property_values = HashMap::new();
        
        let switch_cases = json!({
            "cases": [
                {"value": "apple", "action": "fruit"},
                {"value": "carrot", "action": "vegetable"},
                {"value": "beef", "action": "meat"}
            ],
            "default": "unknown"
        });
        property_values.insert("switchConfig".to_string(), switch_cases);
        property_values.insert("switchField".to_string(), json!("type"));

        let context = create_actor_context(property_values);
        let actor = SwitchCaseActor::new();

        let result = actor.process(context, Message::Object(json!({"type": "apple"}))).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_switch_case_actor_default_case() {
        let mut property_values = HashMap::new();
        
        let switch_cases = json!({
            "cases": [
                {"value": "known", "action": "handle_known"}
            ],
            "default": "handle_unknown"
        });
        property_values.insert("switchConfig".to_string(), switch_cases);
        property_values.insert("switchField".to_string(), json!("category"));

        let context = create_actor_context(property_values);
        let actor = SwitchCaseActor::new();

        // Test with unknown value that should hit default case
        let result = actor.process(context, Message::Object(json!({"category": "unknown_value"}))).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_switch_case_actor_missing_config() {
        let property_values = HashMap::new(); // No switch config

        let context = create_actor_context(property_values);
        let actor = SwitchCaseActor::new();

        let result = actor.process(context, Message::Object(json!({"value": "test"}))).await;
        
        // Should handle missing configuration gracefully
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_loop_actor_creation() {
        let actor = LoopActor::new();
        assert_eq!(actor.actor_type(), "LoopActor");
    }

    #[tokio::test]
    async fn test_loop_actor_simple_loop() {
        let mut property_values = HashMap::new();
        
        property_values.insert("loopType".to_string(), json!("for"));
        property_values.insert("iterations".to_string(), json!(3));
        property_values.insert("iteratorVariable".to_string(), json!("i"));

        let context = create_actor_context(property_values);
        let actor = LoopActor::new();

        let result = actor.process(context, Message::Object(json!({"data": [1, 2, 3]}))).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_loop_actor_while_loop() {
        let mut property_values = HashMap::new();
        
        property_values.insert("loopType".to_string(), json!("while"));
        property_values.insert("condition".to_string(), json!("counter < 5"));
        property_values.insert("maxIterations".to_string(), json!(10)); // Safety limit

        let context = create_actor_context(property_values);
        let actor = LoopActor::new();

        let result = actor.process(context, Message::Object(json!({"counter": 1}))).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_loop_actor_foreach_loop() {
        let mut property_values = HashMap::new();
        
        property_values.insert("loopType".to_string(), json!("forEach"));
        property_values.insert("arrayField".to_string(), json!("items"));
        property_values.insert("itemVariable".to_string(), json!("item"));

        let context = create_actor_context(property_values);
        let actor = LoopActor::new();

        let result = actor.process(context, Message::Object(json!({
            "items": ["apple", "banana", "cherry"]
        }))).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_loop_actor_missing_config() {
        let property_values = HashMap::new(); // No loop config

        let context = create_actor_context(property_values);
        let actor = LoopActor::new();

        let result = actor.process(context, Message::Object(json!({"data": "test"}))).await;
        
        // Should handle missing configuration gracefully
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_loop_actor_invalid_loop_type() {
        let mut property_values = HashMap::new();
        property_values.insert("loopType".to_string(), json!("invalid_type"));

        let context = create_actor_context(property_values);
        let actor = LoopActor::new();

        let result = actor.process(context, Message::Object(json!({"data": "test"}))).await;
        
        // Should handle invalid loop type gracefully
        assert!(result.is_err());
    }
}