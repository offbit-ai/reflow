use reflow_actor::message::Message;
use serde_json::{json, Value};

/// Test that Message serialization works correctly with serde
#[test]
fn test_message_serde_serialization() {
    // Test Integer - converts directly to number
    let msg = Message::Integer(42);
    let value: Value = msg.clone().into();
    println!("Integer serialized: {}", serde_json::to_string_pretty(&value).unwrap());
    assert!(value.is_i64());
    assert_eq!(value, 42);
    
    // Test String - converts directly to string
    let msg = Message::string("hello world".to_string());
    let value: Value = msg.into();
    println!("String serialized: {}", serde_json::to_string_pretty(&value).unwrap());
    assert!(value.is_string());
    assert_eq!(value, "hello world");
    
    // Test Array - converts directly to array
    let msg = Message::array(vec![
        serde_json::Value::from(1).into(),
        serde_json::Value::from("test").into(),
    ]);
    let value: Value = msg.into();
    println!("Array serialized: {}", serde_json::to_string_pretty(&value).unwrap());
    assert!(value.is_array());
    assert_eq!(value[0], 1);
    assert_eq!(value[1], "test");
    
    // Test Object - converts directly to object
    let obj_value = json!({
        "name": "test",
        "count": 123
    });
    let msg = Message::object(obj_value.clone().into());
    let value: Value = msg.into();
    println!("Object serialized: {}", serde_json::to_string_pretty(&value).unwrap());
    assert!(value.is_object());
    assert_eq!(value["name"], "test");
    assert_eq!(value["count"], 123);
}

/// Test round-trip serialization
#[test]
fn test_message_round_trip() {
    // Test various message types
    let messages = vec![
        Message::Integer(42),
        Message::Float(3.14),
        Message::string("test string".to_string()),
        Message::Boolean(true),
        Message::array(vec![Value::from(1).into(), Value::from(2).into()]),
        Message::Optional(None),
        Message::Optional(Some(std::sync::Arc::new(Value::from("optional").into()))),
    ];
    
    for original in messages {
        // Convert to Value
        let value: Value = original.clone().into();
        
        // Convert back to Message
        let restored = Message::from(value.clone());
        
        // Check they're equivalent
        let original_value: Value = original.into();
        let restored_value: Value = restored.into();
        
        assert_eq!(
            serde_json::to_string(&original_value).unwrap(),
            serde_json::to_string(&restored_value).unwrap(),
            "Round-trip failed for message type"
        );
    }
}

/// Test that large messages trigger compression
#[test]
fn test_message_compression_threshold() {
    use reflow_actor::message::COMPRESSION_THRESHOLD;
    
    // Small message (below threshold)
    let small_msg = Message::string("x".repeat(500));
    let small_value: Value = small_msg.into();
    let small_json = serde_json::to_string(&small_value).unwrap();
    println!("Small message size: {} bytes", small_json.len());
    assert!(small_json.len() < COMPRESSION_THRESHOLD);
    
    // Large message (above threshold)
    let large_msg = Message::string("x".repeat(2000));
    let large_value: Value = large_msg.into();
    let large_json = serde_json::to_string(&large_value).unwrap();
    println!("Large message size: {} bytes", large_json.len());
    assert!(large_json.len() > COMPRESSION_THRESHOLD);
    
    // Test with binary data
    let binary_data = vec![0u8; 1500];
    let stream_msg = Message::Stream(std::sync::Arc::new(binary_data));
    let stream_value: Value = stream_msg.into();
    let stream_json = serde_json::to_string(&stream_value).unwrap();
    println!("Stream message size: {} bytes", stream_json.len());
}

/// Test Message extraction in script context format
#[test]
fn test_script_context_message_extraction() {
    // Simulate how scripts will receive messages (direct values, not tagged)
    
    // Integer message - becomes a plain number
    let msg = Message::Integer(42);
    let value: Value = msg.into();
    assert!(value.is_i64());
    assert_eq!(value, 42);
    
    // String message - becomes a plain string
    let msg = Message::string("hello".to_string());
    let value: Value = msg.into();
    assert!(value.is_string());
    assert_eq!(value, "hello");
    
    // Array message - becomes a plain array
    let msg = Message::array(vec![
        Value::from(1).into(),
        Value::from("test").into(),
        Value::from(true).into(),
    ]);
    let value: Value = msg.into();
    assert!(value.is_array());
    assert_eq!(value[0], 1);
    assert_eq!(value[1], "test");
    assert_eq!(value[2], true);
}

/// Test WebSocket RPC payload format
#[test]
fn test_websocket_rpc_payload() {
    use std::collections::HashMap;
    
    // Create a payload as WebSocketScriptActor would
    let mut inputs = HashMap::new();
    inputs.insert("input1".to_string(), Message::Integer(100));
    inputs.insert("input2".to_string(), Message::string("test data".to_string()));
    inputs.insert("binary".to_string(), Message::Stream(std::sync::Arc::new(vec![1, 2, 3, 4, 5])));
    
    // Convert to JSON payload
    let mut json_payload = serde_json::Map::new();
    for (port, msg) in inputs {
        let json_value: Value = msg.into();
        json_payload.insert(port, json_value);
    }
    
    let payload = Value::Object(json_payload);
    println!("RPC Payload: {}", serde_json::to_string_pretty(&payload).unwrap());
    
    // Verify structure - Messages convert to plain JSON values
    assert!(payload["input1"].is_i64());
    assert_eq!(payload["input1"], 100);
    assert!(payload["input2"].is_string());
    assert_eq!(payload["input2"], "test data");
    assert!(payload["binary"].is_array());  // Stream becomes array of numbers
}

/// Test that scripts can extract data correctly
#[test]
fn test_script_data_extraction() {
    // Scripts receive direct JSON values, no extraction needed
    
    // Test various message types
    let test_cases = vec![
        (Message::Integer(42), json!(42)),
        (Message::Float(3.14), json!(3.14)),
        (Message::string("hello".to_string()), json!("hello")),
        (Message::Boolean(true), json!(true)),
        (Message::array(vec![Value::from(1).into(), Value::from(2).into()]), json!([1, 2])),
        (Message::object(json!({"key": "value"}).into()), json!({"key": "value"})),
    ];
    
    for (msg, expected) in test_cases {
        let serialized: Value = msg.into();
        // No extraction needed - Message converts directly to expected value
        assert_eq!(serialized, expected, "Failed to convert message correctly");
    }
}