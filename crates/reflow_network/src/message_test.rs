use std::{collections::HashMap, io::{Read, Write}, iter, sync::Arc};

use crate::message::{
    CompressionConfig, CompressionStats, CompressionStrategy, EncodableValue, EncodedMessage,
    Message, MessageError, COMPRESSION_THRESHOLD,
};
use bitcode::{Decode, Encode};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[test]
fn test_encode_integer_message() {
    let msg = Message::Integer(42);
    let encoded = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_encode_string_message() {
    let msg = Message::String("test string".to_string().into());
    let encoded = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_encode_float_message() {
    let msg = Message::Float(3.14159);
    let encoded = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_encode_boolean_message() {
    let msg = Message::Boolean(true);
    let encoded = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_encode_array_message() {
    let msg = Message::Array(vec![
        json!(1).into(),
        json!("test".to_string()).into(),
        json!(false).into(),
    ].into());
    let encoded = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_encode_any_message() {
    let msg = Message::any(
        json!({
            "key1": "value1",
            "key2": 42,
            "key3": true,
            "key4": [1, 2, 3]
        })
        .into(),
    );
    let encoded = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_encode_null_message() {
    let msg = Message::any(json!(null).into());
    let encoded = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_encode_encoded_message() {
    let original = Message::String("test".to_string().into());
    let encoded1 = EncodedMessage::new(&original);
    let msg = Message::Encoded(encoded1.0.clone().into());
    let encoded2 = EncodedMessage::new(&msg);
    let decoded: Message = bitcode::decode(&encoded2.0).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn test_decode_integer() {
    let msg = Message::Integer(42);
    let encoded = EncodedMessage(bitcode::encode(&msg));
    assert_eq!(encoded.decode(), Some(msg));
}

#[test]
fn test_decode_string() {
    let msg = Message::String("test".to_string().into());
    let encoded = EncodedMessage(bitcode::encode(&msg));
    assert_eq!(encoded.decode(), Some(msg));
}

#[test]
fn test_decode_json() {
    let msg = Message::any(
        json!({
            "key": "value",
            "number": 123,
            "array": [1, 2, 3]
        })
        .into(),
    );
    let encoded = EncodedMessage(bitcode::encode(&msg));
    assert_eq!(encoded.decode(), Some(msg));
}

#[test]
fn test_decode_invalid_data() {
    let invalid_encoded = EncodedMessage(vec![0, 1, 2, 3]);
    assert_eq!(invalid_encoded.decode(), None);
}

#[test]
fn test_decode_empty() {
    let empty_encoded = EncodedMessage(vec![]);
    assert_eq!(empty_encoded.decode(), None);
}

#[test]
fn test_compression_config_serialization() {
    // Setup basic config
    let mut config = CompressionConfig {
        size_threshold: 1024,
        streaming_threshold: 2048,
        enabled: true,
        level: 6,
        type_strategies: HashMap::new(),
    };

    // Add some type strategies
    config
        .type_strategies
        .insert("image/jpeg".to_string(), CompressionStrategy::Always);
    config
        .type_strategies
        .insert("text/plain".to_string(), CompressionStrategy::Never);

    // Serialize
    let serialized = serde_json::to_value(&config).unwrap();

    // Verify structure
    assert!(serialized.is_object());
    let obj = serialized.as_object().unwrap();

    // Verify basic fields
    assert_eq!(obj.get("size_threshold").unwrap(), &json!(1024));
    assert_eq!(obj.get("streaming_threshold").unwrap(), &json!(2048));
    assert_eq!(obj.get("enabled").unwrap(), &json!(true));
    assert_eq!(obj.get("level").unwrap(), &json!(6));

    // Verify type strategies
    let strategies = obj.get("type_strategies").unwrap().as_object().unwrap();
    assert_eq!(
        strategies.get("image/jpeg").unwrap(),
        &json!(CompressionStrategy::Always)
    );
    assert_eq!(
        strategies.get("text/plain").unwrap(),
        &json!(CompressionStrategy::Never)
    );
}

#[test]
fn test_compression_config_empty_strategies() {
    // Test with empty strategies
    let config = CompressionConfig {
        size_threshold: 1024,
        streaming_threshold: 2048,
        enabled: false,
        level: 3,
        type_strategies: HashMap::new(),
    };

    let serialized = serde_json::to_value(&config).unwrap();
    let obj = serialized.as_object().unwrap();

    assert_eq!(
        obj.get("type_strategies")
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn test_compression_config_max_values() {
    // Test with maximum values
    let config = CompressionConfig {
        size_threshold: usize::MAX,
        streaming_threshold: usize::MAX,
        enabled: true,
        level: 9,
        type_strategies: HashMap::new(),
    };

    let serialized = serde_json::to_value(&config).unwrap();
    let obj = serialized.as_object().unwrap();

    assert_eq!(obj.get("size_threshold").unwrap(), &json!(usize::MAX));
    assert_eq!(obj.get("streaming_threshold").unwrap(), &json!(usize::MAX));
}

#[test]
fn test_compression_config_strategy_updates() {
    let mut config = CompressionConfig {
        size_threshold: 1024,
        streaming_threshold: 2048,
        enabled: true,
        level: 6,
        type_strategies: HashMap::new(),
    };

    // Add first strategy

    config
        .type_strategies
        .insert("type1".to_string(), CompressionStrategy::Always);

    // Verify first state
    let serialized = serde_json::to_value(config.clone()).unwrap();
    let obj = serialized.as_object().unwrap();
    let strategies_obj = obj.get("type_strategies").unwrap().as_object().unwrap();
    assert_eq!(strategies_obj.len(), 1);

    // Add second strategy

    config
        .type_strategies
        .insert("type2".to_string(), CompressionStrategy::Never);

    // Verify final state
    let serialized = serde_json::to_value(&config).unwrap();
    let obj = serialized.as_object().unwrap();
    let strategies = obj.get("type_strategies").unwrap().as_object().unwrap();
    assert_eq!(strategies.len(), 2);
}

#[test]
fn test_compression_config_deserialize_defaults() {
    let json = json!({});
    let config: CompressionConfig = serde_json::from_value(json).unwrap();

    assert_eq!(config.size_threshold, 1024);
    assert_eq!(config.streaming_threshold, 1024 * 1024);
    assert_eq!(config.enabled, true);
    assert_eq!(config.level, 6);
    let is_empty = { config.type_strategies.is_empty() };
    assert!(is_empty);
}

#[test]
fn test_compression_config_deserialize_partial_values() {
    let json = json!({
        "size_threshold": 4096,
        "type_strategies": {
            "text": "Always"
        }
    });

    let config: CompressionConfig = serde_json::from_value(json).unwrap();
    assert_eq!(config.size_threshold, 4096);
    assert_eq!(config.streaming_threshold, 1048576);
    assert_eq!(config.enabled, true);
    assert_eq!(config.level, 6);

    assert_eq!(config.type_strategies.len(), 1);
    assert!(matches!(
        config.type_strategies.get("text"),
        Some(&CompressionStrategy::Always)
    ));
}

#[test]
fn test_compression_config_deserialize_custom_values() {
    let json = json!({
        "size_threshold": 2048,
        "streaming_threshold": 2097152,
        "enabled": false,
        "level": 9,
        "type_strategies": {
            "text": "Always",
            "binary": "Never",
            "image": "SizeThreshold",
            "video": "Adaptive"
        }
    });

    let config: CompressionConfig = serde_json::from_value(json).unwrap();

    assert_eq!(config.size_threshold, 2048);
    assert_eq!(config.streaming_threshold, 2097152);
    assert_eq!(config.enabled, false);
    assert_eq!(config.level, 9);

    assert_eq!(config.type_strategies.len(), 4);

    assert!(matches!(
        config.type_strategies.get("text"),
        Some(&CompressionStrategy::Always)
    ));
    assert!(matches!(
        config.type_strategies.get("binary"),
        Some(&CompressionStrategy::Never)
    ));
    assert!(matches!(
        config.type_strategies.get("image"),
        Some(&CompressionStrategy::SizeThreshold)
    ));
    assert!(matches!(
        config.type_strategies.get("video"),
        Some(&CompressionStrategy::Adaptive)
    ));
}

#[test]
fn test_compression_config_deserialize_invalid_strategy() {
    let json = json!({
        "type_strategies": {
            "text": "InvalidStrategy"
        }
    });

    let result: Result<CompressionConfig, _> = serde_json::from_value(json);
    assert!(result.is_err());

    let err = result.unwrap_err().to_string();
    assert!(err.contains("Invalid compression strategy"));
}

#[test]
fn test_compression_config_deserialize_invalid_types() {
    let json = json!({
        "size_threshold": "invalid",
        "streaming_threshold": true,
        "enabled": 42,
        "level": "high",
        "type_strategies": "not_an_object"
    });

    let config: CompressionConfig = serde_json::from_value(json).unwrap();

    // Should fall back to defaults for invalid types
    assert_eq!(config.size_threshold, 1024);
    assert_eq!(config.streaming_threshold, 1024 * 1024);
    assert_eq!(config.enabled, true);
    assert_eq!(config.level, 6);
    let is_empty = { config.type_strategies.is_empty() };
    assert!(is_empty);
}

#[test]
fn test_deserialize_string_values() {
    // Test valid string values
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!("Never")).unwrap(),
        CompressionStrategy::Never
    ));
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!("Always")).unwrap(),
        CompressionStrategy::Always
    ));
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!("SizeThreshold")).unwrap(),
        CompressionStrategy::SizeThreshold
    ));
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!("Adaptive")).unwrap(),
        CompressionStrategy::Adaptive
    ));

    // Test invalid string value
    let result = serde_json::from_value::<CompressionStrategy>(json!("Invalid"));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy: Invalid"));
}

#[test]
fn test_deserialize_object_values() {
    // Test valid object values
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!({"strategy": "Never"})).unwrap(),
        CompressionStrategy::Never
    ));
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!({"strategy": "Always"})).unwrap(),
        CompressionStrategy::Always
    ));
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!({"strategy": "SizeThreshold"}))
            .unwrap(),
        CompressionStrategy::SizeThreshold
    ));
    assert!(matches!(
        serde_json::from_value::<CompressionStrategy>(json!({"strategy": "Adaptive"})).unwrap(),
        CompressionStrategy::Adaptive
    ));

    // Test invalid object values
    let result = serde_json::from_value::<CompressionStrategy>(json!({"strategy": "Invalid"}));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy: Invalid"));

    // Test missing strategy field
    let result = serde_json::from_value::<CompressionStrategy>(json!({}));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy object"));

    // Test invalid strategy field type
    let result = serde_json::from_value::<CompressionStrategy>(json!({"strategy": 123}));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy object"));
}

#[test]
fn test_deserialize_invalid_types() {
    // Test number
    let result = serde_json::from_value::<CompressionStrategy>(json!(123));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy value"));

    // Test boolean
    let result = serde_json::from_value::<CompressionStrategy>(json!(true));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy value"));

    // Test null
    let result = serde_json::from_value::<CompressionStrategy>(json!(null));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy value"));

    // Test array
    let result = serde_json::from_value::<CompressionStrategy>(json!([]));
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid compression strategy value"));
}

#[test]
fn test_compression_config_default() {
    let config = CompressionConfig::default();

    // Test basic configuration values
    assert!(config.enabled);
    assert_eq!(config.size_threshold, 1024);
    assert_eq!(config.streaming_threshold, 1024 * 1024);
    assert_eq!(config.level, 6);

    // Test type strategies
    let type_strategies = config.type_strategies;

    // Verify Stream type strategy
    assert!(matches!(
        type_strategies.get("Stream").unwrap(),
        &CompressionStrategy::Always
    ));

    // Verify Array type strategy
    assert!(matches!(
        type_strategies.get("Array").unwrap(),
        &CompressionStrategy::Adaptive
    ));

    // Verify String type strategy
    assert!(matches!(
        type_strategies.get("String").unwrap(),
        &CompressionStrategy::SizeThreshold
    ));

    // Verify map size
    assert_eq!(type_strategies.len(), 3);
}

#[test]
fn test_compression_config_strategy_access() {
    let mut config = CompressionConfig::default();

    // Test read access

    let strategies = &mut config.type_strategies;
    assert!(strategies.contains_key("Stream"));
    assert!(strategies.contains_key("Array"));
    assert!(strategies.contains_key("String"));

    // Test write access
    strategies.insert("Custom".to_string(), CompressionStrategy::Never);
    assert_eq!(strategies.len(), 4);
    assert!(matches!(
        strategies.get("Custom").unwrap(),
        &CompressionStrategy::Never
    ));
}

#[test]
fn test_compression_config_thread_safety() {
    let config = CompressionConfig::default();
    let config_arc = Arc::new(config);
    let config_clone = config_arc.clone();

    // Spawn thread to verify concurrent access
    let handle = std::thread::spawn(move || {
        assert!(matches!(
            config_clone.type_strategies.get("Stream").unwrap(),
            &CompressionStrategy::Always
        ));
    });

    // Access from main thread
    {
        assert!(matches!(
            config_arc.type_strategies.get("Array").unwrap(),
            &CompressionStrategy::Adaptive
        ));
    }

    handle.join().unwrap();
}

#[test]
fn test_compression_config_default_strategies() {
    let mut config = CompressionConfig::default();
    {
        let strategies = &mut config.type_strategies;
        strategies.clear();
        assert_eq!(strategies.len(), 0);
    }

    // Verify empty strategies
    assert!(config.type_strategies.is_empty());
}

#[test]
fn test_serialize_never() {
    let strategy = CompressionStrategy::Never;
    let serialized = serde_json::to_string(&strategy).unwrap();
    assert_eq!(serialized, "\"Never\"");
}

#[test]
fn test_serialize_always() {
    let strategy = CompressionStrategy::Always;
    let serialized = serde_json::to_string(&strategy).unwrap();
    assert_eq!(serialized, "\"Always\"");
}

#[test]
fn test_serialize_size_threshold() {
    let strategy = CompressionStrategy::SizeThreshold;
    let serialized = serde_json::to_string(&strategy).unwrap();
    assert_eq!(serialized, "\"SizeThreshold\"");
}

#[test]
fn test_serialize_adaptive() {
    let strategy = CompressionStrategy::Adaptive;
    let serialized = serde_json::to_string(&strategy).unwrap();
    assert_eq!(serialized, "\"Adaptive\"");
}

#[test]
fn test_serialize_custom() {
    let strategy = CompressionStrategy::Custom(Arc::new(|message| {
        println!("{:?}", message);
        true
    }));
    let serialized = serde_json::to_string(&strategy).unwrap();
    assert_eq!(serialized, "\"Custom\"");
}

#[test]
fn test_serialize_all_variants() {
    let strategies = vec![
        CompressionStrategy::Never,
        CompressionStrategy::Always,
        CompressionStrategy::SizeThreshold,
        CompressionStrategy::Adaptive,
        CompressionStrategy::Custom(Arc::new(|message| {
            println!("{:?}", message);
            true
        })),
    ];

    let serialized = serde_json::to_string(&strategies).unwrap();
    assert_eq!(
        serialized,
        "[\"Never\",\"Always\",\"SizeThreshold\",\"Adaptive\",\"Custom\"]"
    );
}

#[test]
fn test_serialize_error_handling() {
    // Test that serialization never returns an error for valid variants
    let strategies = vec![
        CompressionStrategy::Never,
        CompressionStrategy::Always,
        CompressionStrategy::SizeThreshold,
        CompressionStrategy::Adaptive,
        CompressionStrategy::Custom(Arc::new(|message| {
            println!("{:?}", message);
            true
        })),
    ];

    for strategy in strategies {
        assert!(serde_json::to_string(&strategy).is_ok());
    }
}

#[test]
fn test_compression_strategy_display() {
    // Test Never variant
    let never = CompressionStrategy::Never;
    assert_eq!(format!("{:?}", never), "Never");

    // Test Always variant
    let always = CompressionStrategy::Always;
    assert_eq!(format!("{:?}", always), "Always");

    // Test SizeThreshold variant
    let size_threshold = CompressionStrategy::SizeThreshold;
    assert_eq!(format!("{:?}", size_threshold), "SizeThreshold");

    // Test Adaptive variant
    let adaptive = CompressionStrategy::Adaptive;
    assert_eq!(format!("{:?}", adaptive), "Adaptive");

    // Test Custom variant
    let custom = CompressionStrategy::Custom(Arc::new(|_| true));
    assert_eq!(format!("{:?}", custom), "Custom(_)");
}

#[test]
fn test_compression_strategy_to_string() {
    let strategies = vec![
        CompressionStrategy::Never,
        CompressionStrategy::Always,
        CompressionStrategy::SizeThreshold,
        CompressionStrategy::Adaptive,
        CompressionStrategy::Custom(Arc::new(|_| true)),
    ];

    let expected = vec!["Never", "Always", "SizeThreshold", "Adaptive", "Custom(_)"];

    for (strategy, expected) in strategies.iter().zip(expected.iter()) {
        assert_eq!(format!("{:?}", strategy), *expected);
    }
}

#[test]
fn test_update_with_threshold_small_data() {
    let mut stats = CompressionStats::default();
    let small_data = vec![1u8; 512]; // Data smaller than SAMPLE_SIZE

    let result = stats.update_with_threshold(&small_data, 1.0);

    assert_eq!(stats.samples, 1);
    assert_eq!(stats.total_original, 512);
    assert!(stats.total_compressed > 0);
    assert!(stats.average_ratio > 0.0);
}

#[test]
fn test_update_with_threshold_large_data() {
    let mut stats = CompressionStats::default();
    let large_data = vec![1u8; 2048]; // Data larger than SAMPLE_SIZE

    let result = stats.update_with_threshold(&large_data, 1.0);

    assert_eq!(stats.samples, 1);
    assert_eq!(stats.total_original, 1024); // Should use SAMPLE_SIZE
    assert!(stats.total_compressed > 0);
    assert!(stats.average_ratio > 0.0);
}

#[test]
fn test_update_with_threshold_highly_compressible() {
    let mut stats = CompressionStats::default();
    // Create highly compressible data (repeated pattern)
    let data: Vec<u8> = iter::repeat(&[0u8; 64][..])
        .take(16)
        .flatten()
        .copied()
        .collect();

    let result = stats.update_with_threshold(&data, 1.0);

    assert!(result); // Should return true for highly compressible data
    assert!(stats.average_ratio < 0.8);
}

#[test]
fn test_update_with_threshold_multiple_updates() {
    let mut stats = CompressionStats::default();
    let data = vec![1u8; 512];

    // Multiple updates to test average ratio calculation
    for _ in 0..5 {
        stats.update_with_threshold(&data, 1.0);
    }

    assert_eq!(stats.samples, 5);
    assert_eq!(stats.total_original, 512 * 5);
    assert!(stats.total_compressed > 0);
}

#[test]
fn test_update_with_threshold_different_multipliers() {
    let mut stats = CompressionStats::default();
    let data = vec![1u8; 512];

    let strict_result = stats.update_with_threshold(&data, 0.5); // More strict threshold
    let stats_strict = stats.average_ratio;

    stats = CompressionStats::default(); // Reset stats

    let lenient_result = stats.update_with_threshold(&data, 2.0); // More lenient threshold
    let stats_lenient = stats.average_ratio;

    assert_eq!(stats_strict, stats_lenient); // Same data should give same ratio
    assert!(lenient_result || !strict_result); // Lenient should pass if strict passes
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Decode, Encode)]
struct TestMessage {
    id: u32,
    content: String,
    data: Vec<u8>,
}

#[test]
fn test_encode_small_message() {
    let msg = TestMessage {
        id: 1,
        content: "small message".to_string(),
        data: vec![1, 2, 3],
    };

    let encoded = bitcode::encode(&msg);

    assert!(encoded.len() < COMPRESSION_THRESHOLD);

    // Verify we can decode it back
    let decoded: TestMessage = bitcode::decode(&encoded).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn test_encode_large_message() {
    let msg = TestMessage {
        id: 2,
        content: "large message".to_string(),
        data: vec![1; COMPRESSION_THRESHOLD * 1000], // Ensure larger than threshold
    };

    let encoded = bitcode::encode(&msg);
    assert!(encoded.len() > COMPRESSION_THRESHOLD);

    // Verify we can decode it back
    let decoded: TestMessage = bitcode::decode(&encoded).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn test_encode_empty_message() {
    let msg = TestMessage {
        id: 0,
        content: String::new(),
        data: vec![],
    };

    let encoded = bitcode::encode(&msg);

    // Verify we can decode it back
    let decoded: TestMessage = bitcode::decode(&encoded).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn test_encode_special_characters() {
    let msg = TestMessage {
        id: 3,
        content: "🦀 Rust 测试".to_string(),
        data: vec![255, 0, 255],
    };

    let encoded = bitcode::encode(&msg);

    // Verify we can decode it back
    let decoded: TestMessage = bitcode::decode(&encoded).unwrap();
    assert_eq!(decoded, msg);
}

#[test]
fn test_decode_valid_message() {
    let test_msg = TestMessage {
        id: 1,
        content: "test message".to_string(),
        data: vec![],
    };

    // Encode the test message
    let encoded = bitcode::encode(&Message::any(json!(test_msg).into()));

    // Test decoding
    let decoded: Message = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Message::any(json!(test_msg).into()));
}

#[test]
fn test_decode_empty_message() {
    let empty_bytes: &[u8] = &[];
    let result = Message::decode(empty_bytes);
    assert!(result.is_err());

    match result {
        Err(MessageError::Decoding(_)) => (),
        _ => panic!("Expected MessageError::Decoding"),
    }
}

#[test]
fn test_decode_corrupted_message() {
    let corrupted_bytes = &[0xFF, 0xFF, 0xFF];
    let result = Message::decode(corrupted_bytes);
    assert!(result.is_err());

    match result {
        Err(MessageError::Decoding(_)) => (),
        _ => panic!("Expected MessageError::Decoding"),
    }
}

#[test]
fn test_decode_large_message() {
    let large_msg = TestMessage {
        id: 999999,
        content: "a".repeat(1000000),
        data: vec![],
    };

    let encoded = bitcode::encode(&Message::any(json!(large_msg).into()));
    let decoded: Message = Message::decode(&encoded).unwrap();
    assert_eq!(decoded, Message::any(json!(large_msg).into()));
}

#[test]
fn test_decode_with_config_no_compression() {
    let config = CompressionConfig {
        enabled: false,
        level: 0,
        ..Default::default()
    };

    let test_msg = Message::Stream(b"Hello, World!".to_vec().into());
    let test_data = test_msg.encode().unwrap();
    let result = Message::decode_with_config(&test_data, config);
    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), test_msg));
}

#[test]
fn test_decode_with_config_zstd() {
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        ..Default::default()
    };

    let test_msg = Message::Stream(b"Hello, World!".to_vec().into());
    let test_data = test_msg.encode().unwrap();
    let mut compressed = Vec::new();
    let mut encoder = zstd::Encoder::new(&mut compressed, 3).unwrap();
    encoder.write_all(&test_data).unwrap();
    encoder.finish().unwrap();

    let result = Message::decode_with_config(&compressed, config);

    assert!(result.is_ok());
    assert!(matches!(result.unwrap(), test_msg));
}

#[test]
fn test_decode_with_config_invalid_data() {
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        ..Default::default()
    };

    let invalid_data = b"Invalid compressed data";
    let result = Message::decode_with_config(invalid_data, config);

    assert!(result.is_err());
    assert!(matches!(result, Err(MessageError::Decoding(_))));
}

#[test]
fn test_decode_with_config_empty_data() {
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        ..Default::default()
    };

    let empty_data = Message::String(String::from("").into()).encode().unwrap();
    let result = Message::decode_with_config(&empty_data, config);

    assert!(result.is_ok());
    if let Ok(Message::String(decoded)) = result {
        assert!(decoded.is_empty());
    } else {
        panic!("Expected empty decoded bytes");
    }
}

#[test]
fn test_encoded_size_integer() {
    let msg = Message::Integer(42);
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encoded_size_float() {
    let msg = Message::Float(3.14);
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encoded_size_string() {
    let msg = Message::String("test string".to_string().into());
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encoded_size_boolean() {
    let msg = Message::Boolean(true);
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encoded_size_array() {
    let msg = Message::array(vec![
        json!(1).into(),
        json!("test".to_string()).into(),
        json!(true).into(),
    ]);
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encoded_size_object() {
    let msg = Message::any(
        json!({
            "number": 42,
            "text": "hello",
            "flag": true
        })
        .into(),
    );
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encoded_size_null() {
    let msg = Message::any(json!(null).into());
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encoded_size_encoded() {
    let encoded_data = vec![1, 2, 3, 4, 5];
    let msg = Message::Encoded(encoded_data.clone().into());
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), encoded_data.len());
}

#[test]
fn test_encoded_size_large_data() {
    let large_string = "x".repeat(1_000_000);
    let msg = Message::String(large_string.into());
    assert!(msg.encoded_size().is_ok());
    assert_eq!(msg.encoded_size().unwrap(), msg.encode().unwrap().len());
}

#[test]
fn test_encode_compression_disabled() {
    let message = Message::String("test data".to_string().into());
    let config = CompressionConfig {
        enabled: false,
        ..Default::default()
    };

    let result = message.encode_with_config(&config).unwrap();
    assert!(result.0.len() > 0);
}

#[test]
fn test_encode_never_compress() {
    let message = Message::String("test data".to_string().into());
    let config = CompressionConfig {
        enabled: true,
        type_strategies: HashMap::from_iter(vec![("never".into(), CompressionStrategy::Never)]),
        ..Default::default()
    };

    let result = message.encode_with_config(&config).unwrap();
    let uncompressed = bitcode::encode(&message);
    assert_eq!(result.0, uncompressed);
}

#[test]
fn test_encode_always_compress() {
    let message = Message::String("test data".to_string().into());
    let config = CompressionConfig {
        enabled: true,
        type_strategies: HashMap::from_iter(vec![("String".into(), CompressionStrategy::Always)]),
        level: 6,
        ..Default::default()
    };

    let result = message.encode_with_config(&config).unwrap();
    let uncompressed = bitcode::encode(&message);
    assert_ne!(result.0, uncompressed);
}

#[test]
fn test_encode_size_threshold() {
    let small_message = Message::String("small".to_string().into());
    let large_message = Message::String("x".repeat(10000).into());

    let config = CompressionConfig {
        enabled: true,
        type_strategies: HashMap::from_iter(vec![(
            "String".into(),
            CompressionStrategy::SizeThreshold,
        )]),
        size_threshold: 1000,
        ..Default::default()
    };

    // Small message shouldn't be compressed
    let small_result = small_message.encode_with_config(&config).unwrap();
    let small_uncompressed = bitcode::encode(&small_message);
    assert_eq!(small_result.0, small_uncompressed);

    // Large message should be compressed
    let large_result = large_message.encode_with_config(&config).unwrap();
    let large_uncompressed = bitcode::encode(&large_message);
    assert_ne!(large_result.0, large_uncompressed);
}

#[test]
fn test_encode_adaptive() {
    let message = Message::String("test data".to_string().into());
    let config = CompressionConfig {
        enabled: true,
        type_strategies: HashMap::from_iter(vec![("String".into(), CompressionStrategy::Adaptive)]),
        ..Default::default()
    };

    let result = message.encode_with_config(&config).unwrap();
    assert!(result.0.len() > 0);
}

#[test]
fn test_encode_custom_strategy() {
    let message = Message::Integer(42);
    let custom_strategy = |msg: &Message| matches!(msg, Message::Integer(n) if *n > 0);

    let config = CompressionConfig {
        enabled: true,
        type_strategies: HashMap::from_iter(vec![(
            "Integer".into(),
            CompressionStrategy::Custom(Arc::new(custom_strategy)),
        )]),
        ..Default::default()
    };

    // Should compress positive integers
    let pos_result = message.encode_with_config(&config).unwrap();
    let pos_uncompressed = bitcode::encode(&message);
    assert_ne!(pos_result.0, pos_uncompressed);

    // Should not compress negative integers
    let neg_message = Message::Integer(-42);
    let neg_result = neg_message.encode_with_config(&config).unwrap();
    let neg_uncompressed = bitcode::encode(&neg_message);
    assert_eq!(neg_result.0, neg_uncompressed);
}

#[test]
fn test_encode_json_values() {
    let json_message = Message::any(
        json!({
            "name": "test",
            "values": [1, 2, 3],
            "nested": {"key": "value"}
        })
        .into(),
    );

    let config = CompressionConfig {
        enabled: true,
        type_strategies: HashMap::from_iter(vec![("Any".into(), CompressionStrategy::Always)]),
        ..Default::default()
    };

    let result = json_message.encode_with_config(&config).unwrap();
    assert!(result.0.len() > 0);
}


#[test]
fn test_compress_data_normal() {
    let message = Message::default();
    let small_data: Vec<u8> = vec![1, 2, 3, 4, 5];
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        streaming_threshold: 1024,
        ..Default::default()
    };

    let result = message.compress_data(&small_data, &config);
    assert!(result.is_ok());
    let compressed = result.unwrap();
    assert!(compressed.len() > 0);
    assert!(compressed.len() <= small_data.len());
}

#[test]
fn test_compress_data_streaming() {
    let message = Message::default();
    // Create data larger than threshold
    let large_data: Vec<u8> = iter::repeat(42u8)
        .take(2048)
        .collect();
    
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        streaming_threshold: 1024,
        ..Default::default()
    };

    let result = message.compress_data(&large_data, &config);
    assert!(result.is_ok());
    let compressed = result.unwrap();
    assert!(compressed.len() > 0);
    assert!(compressed.len() < large_data.len());
}

#[test]
fn test_compress_data_disabled() {
    let message = Message::default();
    let data: Vec<u8> = vec![1, 2, 3, 4, 5];
    let config = CompressionConfig {
        enabled: false,
        level: 3,
        streaming_threshold: 1024,
        ..Default::default()
    };

    let result = message.compress_data(&data, &config);
    assert!(result.is_ok());
    let compressed = result.unwrap();
    assert_eq!(compressed, data);
}

#[test]
fn test_compress_data_empty() {
    let message = Message::default();
    let empty_data: Vec<u8> = vec![];
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        streaming_threshold: 1024,
        ..Default::default()
    };

    let result = message.compress_data(&empty_data, &config);
    assert!(result.is_ok());
    let compressed = result.unwrap();
    assert!(compressed.is_empty());
}


#[test]
fn test_compress_data_different_levels() {
    let message = Message::default();
    let data: Vec<u8> = iter::repeat(42u8)
        .take(1000)
        .collect();
    
    let mut sizes = Vec::new();
    
    for level in 1..=9 {
        let config = CompressionConfig {
            enabled: true,
            level,
            streaming_threshold: 1024,
            ..Default::default()
        };
        
        let result = message.compress_data(&data, &config).unwrap();
        sizes.push(result.len());
    }
    
    // Higher compression levels should generally produce smaller or equal output
    for i in 1..sizes.len() {
        assert!(sizes[i] <= sizes[i-1]);
    }
}


#[test]
fn test_compress_streaming_empty() {
    let message = Message::default();
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        ..Default::default()
    };
    
    let result = message.compress_streaming(&[], &config);
    assert!(result.is_ok());
    
    let compressed = result.unwrap();
    assert!(!compressed.is_empty());
}


#[test]
fn test_compress_streaming_small_data() {
    let message = Message::default();
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        ..Default::default()
    };
    
    let data = b"Hello, World!".repeat(10);
    let result = message.compress_streaming(&data, &config);
    assert!(result.is_ok());
    
    let compressed = result.unwrap();
    assert!(compressed.len() < data.len());
    
    // Verify decompression
    let mut decoder = zstd::Decoder::new(&compressed[..]).unwrap();
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn test_compress_streaming_large_data() {
    let message = Message::default();
    let config = CompressionConfig {
        enabled: true,
        level: 3,
        ..Default::default()
    };
    
    // Create data larger than chunk size (64KB)
    let data = b"Large data block for testing".repeat(4000); // ~100KB
    let result = message.compress_streaming(&data, &config);
    assert!(result.is_ok());
    
    let compressed = result.unwrap();
    assert!(compressed.len() < data.len());
    
    // Verify decompression
    let mut decoder = zstd::Decoder::new(&compressed[..]).unwrap();
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn test_compress_streaming_different_levels() {
    let message = Message::default();
    let data = b"Compression test data".repeat(1000);
    
    // Test different compression levels
    for level in 1..=9 {
        let config = CompressionConfig {
            enabled: true,
            level,
            ..Default::default()
        };
        
        let result = message.compress_streaming(&data, &config);
        assert!(result.is_ok());
        
        let compressed = result.unwrap();
        assert!(compressed.len() < data.len());
        
        // Verify decompression
        let mut decoder = zstd::Decoder::new(&compressed[..]).unwrap();
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        assert_eq!(decompressed, data);
    }
}


#[test]
fn test_compress_streaming_invalid_level() {
    let message = Message::default();
    let data = b"Test data";
    
    // Test with invalid compression level
    let config = CompressionConfig {
        enabled: true,
        level: 100, // Invalid level
        ..Default::default()
    };
    
    let result = message.compress_streaming(data, &config);
    assert!(result.is_err());
    match result {
        Err(MessageError::Compression(_)) => (),
        _ => panic!("Expected compression error"),
    }
}

#[test]
fn test_compress_streaming_chunk_boundaries() {
    let message = Message::default();
    // Create data exactly CHUNK_SIZE (64KB)
    let data: Vec<u8> = iter::repeat(1u8).take(64 * 1024).collect();
    let config = CompressionConfig::default();
    
    let result = message.compress_streaming(&data, &config).unwrap();
    
    // The compressed data should be smaller than the original
    assert!(result.len() < data.len());
    
    // Verify we can decompress it back
    let mut decoder = zstd::Decoder::new(&result[..]).unwrap();
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed).unwrap();
    assert_eq!(decompressed, data);
}

#[test]
fn test_compress_streaming_config_levels() {
    let message = Message::default();
    let data: Vec<u8> = vec![1, 2, 3, 4, 5];
    
    // Test different compression levels
    for level in 1..=9 {
        let config = CompressionConfig {
            enabled: true,
            level,
            ..Default::default()
        };
        
        let result = message.compress_streaming(&data, &config);
        assert!(result.is_ok());
    }
}

#[test]
fn test_compression_decision_stream() {
    let msg = Message::Stream(vec![0u8; 1000].into());
    let data = vec![0u8; 1000];
    
    // First call should compress due to size
    assert!(msg.should_compress_adaptive(&data));
    
    // Second call should use historical data
    assert!(msg.should_compress_adaptive(&data));
}

#[test]
fn test_compression_decision_string() {
    // Test case 1: Medium-sized repeated string (not compressible enough)
    let msg = Message::String("test".repeat(200).into());
    let data = "test".repeat(200).into_bytes();
    println!("Data size: {}", data.len());
    // assert!(!msg.should_compress_adaptive(&data));

    // Test case 2: Large repeated string (should compress)
    let large_msg = Message::String("a".repeat(10000).into());
    let large_data = "a".repeat(10000).into_bytes();
    assert!(large_msg.should_compress_adaptive(&large_data));

    // Test case 3: Small string (should not compress)
    let small_msg = Message::String("small test string".to_string().into());
    let small_data = "small test string".to_string().into_bytes();
    println!("Small Data size: {}", small_data.len());
    assert!(!small_msg.should_compress_adaptive(&small_data));

    // Test case 4: Mixed content string (should not compress)
    let mixed_msg = Message::String("Mixed123!@#".repeat(50).into());
    let mixed_data = "Mixed123!@#".repeat(50).into_bytes();
    assert!(!mixed_msg.should_compress_adaptive(&mixed_data));

    // Test case 5: Very large repeated string (should compress)
    let very_large_msg = Message::String("b".repeat(100000).into());
    let very_large_data = "b".repeat(100000).into_bytes();
    assert!(very_large_msg.should_compress_adaptive(&very_large_data));
}