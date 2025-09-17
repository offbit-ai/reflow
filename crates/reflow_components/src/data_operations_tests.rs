//! Unit tests for Data Operations actors

#[cfg(test)]
mod tests {
    use super::super::data_operations::DataTransformActor;
    use super::super::data_ops::DataOperationsActor;
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
    async fn test_data_transform_actor_creation() {
        let actor = DataTransformActor::new();
        assert_eq!(actor.actor_type(), "DataTransformActor");
    }

    #[tokio::test]
    async fn test_data_transform_actor_simple_mapping() {
        let mut property_values = HashMap::new();
        
        let mapping_rules = json!({
            "mappings": [
                {
                    "source": "firstName",
                    "target": "first_name",
                    "transform": "toLowerCase"
                },
                {
                    "source": "lastName", 
                    "target": "last_name",
                    "transform": "toUpperCase"
                }
            ]
        });
        property_values.insert("transformRules".to_string(), mapping_rules);

        let context = create_actor_context(property_values);
        let actor = DataTransformActor::new();

        let input_data = json!({
            "firstName": "John",
            "lastName": "Doe",
            "age": 30
        });

        let result = actor.process(context, Message::Object(input_data)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_transform_actor_complex_transformation() {
        let mut property_values = HashMap::new();
        
        let mapping_rules = json!({
            "mappings": [
                {
                    "source": "user.profile.name",
                    "target": "userName",
                    "transform": "trim"
                },
                {
                    "source": "user.metadata.created",
                    "target": "createdAt",
                    "transform": "formatDate"
                }
            ],
            "filters": [
                {
                    "field": "status",
                    "condition": "equals",
                    "value": "active"
                }
            ]
        });
        property_values.insert("transformRules".to_string(), mapping_rules);

        let context = create_actor_context(property_values);
        let actor = DataTransformActor::new();

        let input_data = json!({
            "user": {
                "profile": {"name": "  Alice  "},
                "metadata": {"created": "2023-01-01T00:00:00Z"}
            },
            "status": "active"
        });

        let result = actor.process(context, Message::Object(input_data)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_transform_actor_missing_rules() {
        let property_values = HashMap::new(); // No transform rules

        let context = create_actor_context(property_values);
        let actor = DataTransformActor::new();

        let result = actor.process(context, Message::Object(json!({"data": "test"}))).await;
        
        // Should handle missing rules gracefully
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_data_operations_actor_creation() {
        let actor = DataOperationsActor::new();
        assert_eq!(actor.actor_type(), "DataOperationsActor");
    }

    #[tokio::test]
    async fn test_data_operations_actor_template_literal() {
        let mut property_values = HashMap::new();
        
        let operations = json!({
            "operations": [
                {
                    "type": "template",
                    "template": "Hello ${input.get('name').data}, welcome to ${input.get('app').data}!",
                    "output": "greeting"
                }
            ]
        });
        property_values.insert("dataOperations".to_string(), operations);

        let context = create_actor_context(property_values);
        let actor = DataOperationsActor::new();

        // Create a message that simulates input.get() calls
        let input_data = json!({
            "name": {"data": "Alice"},
            "app": {"data": "MyApp"}
        });

        let result = actor.process(context, Message::Object(input_data)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_operations_actor_field_extraction() {
        let mut property_values = HashMap::new();
        
        let operations = json!({
            "operations": [
                {
                    "type": "extract",
                    "source": "user.profile.email",
                    "target": "email"
                },
                {
                    "type": "extract",
                    "source": "user.preferences.theme",
                    "target": "theme",
                    "default": "light"
                }
            ]
        });
        property_values.insert("dataOperations".to_string(), operations);

        let context = create_actor_context(property_values);
        let actor = DataOperationsActor::new();

        let input_data = json!({
            "user": {
                "profile": {"email": "alice@example.com"},
                "preferences": {"theme": "dark"}
            }
        });

        let result = actor.process(context, Message::Object(input_data)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_operations_actor_aggregation() {
        let mut property_values = HashMap::new();
        
        let operations = json!({
            "operations": [
                {
                    "type": "aggregate",
                    "source": "items",
                    "function": "sum",
                    "field": "price",
                    "target": "total"
                },
                {
                    "type": "aggregate",
                    "source": "items",
                    "function": "count",
                    "target": "itemCount"
                }
            ]
        });
        property_values.insert("dataOperations".to_string(), operations);

        let context = create_actor_context(property_values);
        let actor = DataOperationsActor::new();

        let input_data = json!({
            "items": [
                {"name": "apple", "price": 1.20},
                {"name": "banana", "price": 0.80},
                {"name": "cherry", "price": 2.50}
            ]
        });

        let result = actor.process(context, Message::Object(input_data)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_operations_actor_conditional_logic() {
        let mut property_values = HashMap::new();
        
        let operations = json!({
            "operations": [
                {
                    "type": "conditional",
                    "condition": "age >= 18",
                    "trueValue": "adult",
                    "falseValue": "minor",
                    "target": "category"
                }
            ]
        });
        property_values.insert("dataOperations".to_string(), operations);

        let context = create_actor_context(property_values);
        let actor = DataOperationsActor::new();

        let input_data = json!({"age": 25, "name": "Alice"});

        let result = actor.process(context, Message::Object(input_data)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_data_operations_actor_missing_operations() {
        let property_values = HashMap::new(); // No operations

        let context = create_actor_context(property_values);
        let actor = DataOperationsActor::new();

        let result = actor.process(context, Message::Object(json!({"data": "test"}))).await;
        
        // Should handle missing operations gracefully
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_data_operations_actor_invalid_template_literal() {
        let mut property_values = HashMap::new();
        
        let operations = json!({
            "operations": [
                {
                    "type": "template",
                    "template": "Hello ${invalid.syntax",  // Invalid template
                    "output": "greeting"
                }
            ]
        });
        property_values.insert("dataOperations".to_string(), operations);

        let context = create_actor_context(property_values);
        let actor = DataOperationsActor::new();

        let result = actor.process(context, Message::Object(json!({"name": "Alice"}))).await;
        
        // Should handle invalid template gracefully
        assert!(result.is_err());
    }
}