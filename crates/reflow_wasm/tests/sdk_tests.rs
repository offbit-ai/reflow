//! Unit tests for the WASM plugin SDK

use reflow_wasm::*;
use std::collections::HashMap;

#[test]
fn test_message_conversions() {
    // Test that messages convert to/from JSON values correctly
    // Note: The Into<Value> implementation converts to the actual value, not the tagged format
    
    // Test integer conversion
    let msg = Message::Integer(42);
    let json: serde_json::Value = msg.clone().into();
    assert_eq!(json, serde_json::json!(42));
    
    // Test string conversion
    let msg = Message::String("hello".to_string());
    let json: serde_json::Value = msg.into();
    assert_eq!(json, serde_json::json!("hello"));
    
    // Test array conversion
    let msg = Message::Array(vec![
        serde_json::json!(1),
        serde_json::json!(2),
        serde_json::json!(3),
    ]);
    let json: serde_json::Value = msg.into();
    assert_eq!(json, serde_json::json!([1, 2, 3]));
    
    // Test from JSON conversion
    let json = serde_json::json!(42);
    let msg = Message::from(json);
    assert_eq!(msg, Message::Integer(42));
    
    let json = serde_json::json!("test");
    let msg = Message::from(json);
    assert_eq!(msg, Message::String("test".to_string()));
    
    // Test serialization with tagged format
    let msg = Message::Integer(42);
    let serialized = serde_json::to_value(&msg).unwrap();
    assert_eq!(serialized, serde_json::json!({"type": "Integer", "data": 42}));
}

#[test]
fn test_actor_config_helpers() {
    let mut config_map = HashMap::new();
    config_map.insert("string_val".to_string(), serde_json::json!("hello"));
    config_map.insert("number_val".to_string(), serde_json::json!(3.14));
    config_map.insert("bool_val".to_string(), serde_json::json!(true));
    config_map.insert("int_val".to_string(), serde_json::json!(42));
    
    let mut env_map = HashMap::new();
    env_map.insert("API_KEY".to_string(), "secret123".to_string());
    
    let config = ActorConfig {
        node_id: "test_node".to_string(),
        component: "TestComponent".to_string(),
        resolved_env: env_map,
        config: config_map,
        namespace: Some("test_namespace".to_string()),
    };
    
    // Test string access
    assert_eq!(config.get_string("string_val"), Some("hello".to_string()));
    assert_eq!(config.get_string("missing"), None);
    
    // Test number access
    assert_eq!(config.get_number("number_val"), Some(3.14));
    
    // Test boolean access
    assert_eq!(config.get_bool("bool_val"), Some(true));
    
    // Test integer access
    assert_eq!(config.get_integer("int_val"), Some(42));
    
    // Test env access
    assert_eq!(config.get_env("API_KEY"), Some(&"secret123".to_string()));
    assert_eq!(config.get_env("MISSING"), None);
}

#[test]
fn test_port_definition_macro() {
    // Test required port
    let port = port_def!("input", "Input data", "Integer", required);
    assert_eq!(port.name, "input");
    assert_eq!(port.description, "Input data");
    assert_eq!(port.port_type, "Integer");
    assert!(port.required);
    
    // Test optional port
    let port = port_def!("output", "Output data", "String");
    assert_eq!(port.name, "output");
    assert_eq!(port.description, "Output data");
    assert_eq!(port.port_type, "String");
    assert!(!port.required);
}

#[test]
fn test_state_manager() {
    let mut state = MemoryState::default();
    
    // Test set and get
    state.set("counter", serde_json::json!(1));
    assert_eq!(state.get("counter"), Some(serde_json::json!(1)));
    
    // Test get_all
    state.set("name", serde_json::json!("test"));
    let all = state.get_all();
    assert_eq!(all["counter"], serde_json::json!(1));
    assert_eq!(all["name"], serde_json::json!("test"));
    
    // Test set_all
    let new_state = serde_json::json!({
        "a": 1,
        "b": 2,
        "c": 3
    });
    state.set_all(new_state);
    assert_eq!(state.get("a"), Some(serde_json::json!(1)));
    assert_eq!(state.get("b"), Some(serde_json::json!(2)));
    assert_eq!(state.get("c"), Some(serde_json::json!(3)));
}

#[test]
fn test_helper_functions() {
    // Test error_result
    let result = error_result("Something went wrong");
    assert_eq!(result.outputs.len(), 1);
    assert!(matches!(result.outputs.get("error"), Some(Message::Error(_))));
    
    // Test success_result
    let result = success_result("output", Message::Integer(42));
    assert_eq!(result.outputs.len(), 1);
    assert_eq!(result.outputs.get("output"), Some(&Message::Integer(42)));
    
    // Test create_output
    let (port, msg) = create_output("result", Message::String("done".to_string()));
    assert_eq!(port, "result");
    assert_eq!(msg, Message::String("done".to_string()));
}

#[test]
fn test_create_transform_metadata() {
    let metadata = create_transform_metadata(
        "MyTransform",
        "Transforms input data",
        "String",
        "Integer"
    );
    
    assert_eq!(metadata.component, "MyTransform");
    assert_eq!(metadata.description, "Transforms input data");
    assert_eq!(metadata.inports.len(), 1);
    assert_eq!(metadata.outports.len(), 1);
    
    let inport = &metadata.inports[0];
    assert_eq!(inport.name, "input");
    assert_eq!(inport.port_type, "String");
    assert!(inport.required);
    
    let outport = &metadata.outports[0];
    assert_eq!(outport.name, "output");
    assert_eq!(outport.port_type, "Integer");
    assert!(!outport.required);
}

#[test]
fn test_actor_context_serialization() {
    let mut payload = HashMap::new();
    payload.insert("input".to_string(), Message::Integer(42));
    
    let config = ActorConfig {
        node_id: "test".to_string(),
        component: "Test".to_string(),
        resolved_env: HashMap::new(),
        config: HashMap::new(),
        namespace: None,
    };
    
    let context = ActorContext {
        payload,
        config,
        state: serde_json::json!({"count": 0}),
    };
    
    // Test serialization
    let json = serde_json::to_value(&context).unwrap();
    assert!(json.is_object());
    assert!(json["payload"].is_object());
    assert!(json["config"].is_object());
    assert!(json["state"].is_object());
    
    // Test deserialization
    let context2: ActorContext = serde_json::from_value(json).unwrap();
    assert_eq!(context2.payload.len(), 1);
    assert_eq!(context2.config.node_id, "test");
}

#[test]
fn test_actor_result_serialization() {
    let mut outputs = HashMap::new();
    outputs.insert("result".to_string(), Message::Integer(84));
    
    let result = ActorResult {
        outputs,
        state: Some(serde_json::json!({"count": 1})),
    };
    
    // Test serialization
    let json = serde_json::to_value(&result).unwrap();
    assert!(json["outputs"].is_object());
    assert!(json["state"].is_object());
    
    // Test deserialization
    let result2: ActorResult = serde_json::from_value(json).unwrap();
    assert_eq!(result2.outputs.len(), 1);
    assert!(result2.state.is_some());
}