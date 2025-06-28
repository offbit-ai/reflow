use bitcode::{Decode, Encode};
// use serde_with::{serde_as, DisplayFromStr};
#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
use once_cell::sync::Lazy;
use ordered_float::OrderedFloat;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use std::hash::Hash;
use std::io::{self, Read, Write};
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
use zstd;

#[cfg(target_arch = "wasm32")]
use lz4_flex::block::{compress_prepend_size, decompress_size_prepended};

use crate::graph::types::PortType;

pub const COMPRESSION_THRESHOLD: usize = 1024; // 1KB

#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode, PartialEq, Eq)]
pub struct EncodedMessage(pub Vec<u8>);

impl EncodedMessage {
    pub fn new(msg: &Message) -> Self {
        Self(bitcode::encode(msg))
    }

    pub fn decode(&self) -> Option<Message> {
        bitcode::decode(&self.0).ok()
    }
}

// #[serde_as]
#[derive(Clone, Default, Debug, Serialize, Deserialize, Encode, Decode, PartialEq)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(tag = "type", content = "data")]
pub enum Message {
    #[default]
    Flow,
    Event(EncodableValue),
    Boolean(bool),
    Integer(i64),
    Float(f64),

    String(Arc<String>),

    Object(Arc<EncodableValue>),

    Array(Arc<Vec<EncodableValue>>),

    Stream(Arc<Vec<u8>>),

    Encoded(Arc<Vec<u8>>),

    Optional(Option<Arc<EncodableValue>>),
    // Tuple(Vec<Message>),
    // Generic(EncodableValue),
    Any(Arc<EncodableValue>),
    Error(Arc<String>),

    // Remote messaging
    RemoteReference {
        network_id: String,
        actor_id: String,
        port: String,
    },
    NetworkEvent {
        event_type: NetworkEventType,
        data: EncodableValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode)]
pub enum NetworkEventType {
    ActorRegistered,
    ActorUnregistered,
    NetworkConnected,
    NetworkDisconnected,
    HeartbeatMissed,
}

// #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Encode, Decode)]
// pub enum CloudDialect {
//     Snowflake,
//     BigQuery,
//     Databricks,
//     Redshift,
//     AzureSynapse,
//     OracleAutonomous,

//     // built-in dialects
//     PostgreSQL,
//     MySQL,
//     SQLite,
// }

/// Compression configuration
#[derive(Clone)]
pub struct CompressionConfig {
    /// Minimum size in bytes before applying compression
    pub size_threshold: usize,
    /// Maximum size in bytes for a single message before switching to streaming
    pub streaming_threshold: usize,
    /// Enable/disable compression globally
    pub enabled: bool,
    /// Compression level (0-9, where 0 is no compression and 9 is max compression)
    pub level: u32,
    /// Strategy for different message types
    pub type_strategies: HashMap<String, CompressionStrategy>,
}

impl std::fmt::Debug for CompressionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompressionConfig")
            .field("size_threshold", &self.size_threshold)
            .field("streaming_threshold", &self.streaming_threshold)
            .field("enabled", &self.enabled)
            .field("level", &self.level)
            .field(
                "type_strategies",
                &HashMap::<String, CompressionStrategy>::from_iter(
                    self.type_strategies
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect::<Vec<(String, CompressionStrategy)>>(),
                ),
            )
            .finish()
    }
}

impl Serialize for CompressionConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serde_json::Map::new();
        let strategy = self
            .type_strategies
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect::<HashMap<String, CompressionStrategy>>();
        map.insert("size_threshold".to_string(), json!(self.size_threshold));
        map.insert(
            "streaming_threshold".to_string(),
            json!(self.streaming_threshold),
        );
        map.insert("enabled".to_string(), json!(self.enabled));
        map.insert("level".to_string(), json!(self.level));
        map.insert("type_strategies".to_string(), json!(strategy));
        serde_json::Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CompressionConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = Deserialize::deserialize(deserializer)?;
        let size_threshold = value
            .get("size_threshold")
            .and_then(|t| t.as_u64())
            .map(|t| t as usize)
            .unwrap_or(1024);
        let streaming_threshold = value
            .get("streaming_threshold")
            .and_then(|t| t.as_u64())
            .map(|t| t as usize)
            .unwrap_or(1024 * 1024); // 1MB
        let enabled = value
            .get("enabled")
            .and_then(|e| e.as_bool())
            .unwrap_or(true);
        let level = value
            .get("level")
            .and_then(|l| l.as_u64())
            .map(|l| l as u32)
            .unwrap_or(6); // Default compression level
        let mut type_strategies = HashMap::new();
        if let Some(strategies) = value.get("type_strategies") {
            if let Some(map) = strategies.as_object() {
                for (type_name, strategy) in map {
                    let strategy = match strategy.as_str() {
                        Some("Never") => CompressionStrategy::Never,
                        Some("Always") => CompressionStrategy::Always,
                        Some("SizeThreshold") => CompressionStrategy::SizeThreshold,
                        Some("Adaptive") => CompressionStrategy::Adaptive,
                        _ => {
                            return Err(serde::de::Error::custom(format!(
                                "Invalid compression strategy: {}",
                                strategy
                            )));
                        }
                    };
                    type_strategies.insert(type_name.to_string(), strategy);
                }
            }
        }

        Ok(Self {
            size_threshold,
            streaming_threshold,
            enabled,
            level,
            type_strategies,
        })
    }
}

impl<'de> Deserialize<'de> for CompressionStrategy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = Deserialize::deserialize(deserializer)?;
        match value {
            Value::String(s) => match s.as_str() {
                "Never" => Ok(Self::Never),
                "Always" => Ok(Self::Always),
                "SizeThreshold" => Ok(Self::SizeThreshold),
                "Adaptive" => Ok(Self::Adaptive),
                _ => Err(serde::de::Error::custom(format!(
                    "Invalid compression strategy: {}",
                    s
                ))),
            },
            Value::Object(map) => {
                let strategy = map
                    .get("strategy")
                    .and_then(|s| s.as_str())
                    .ok_or_else(|| {
                        serde::de::Error::custom("Invalid compression strategy object")
                    })?;
                match strategy {
                    "Never" => Ok(Self::Never),
                    "Always" => Ok(Self::Always),
                    "SizeThreshold" => Ok(Self::SizeThreshold),
                    "Adaptive" => Ok(Self::Adaptive),
                    _ => Err(serde::de::Error::custom(format!(
                        "Invalid compression strategy: {}",
                        strategy
                    ))),
                }
            }
            _ => Err(serde::de::Error::custom(
                "Invalid compression strategy value",
            )),
        }
    }
}

impl Default for CompressionConfig {
    fn default() -> Self {
        let mut type_strategies = HashMap::new();
        // Default strategies for different message types
        type_strategies.insert("Stream".to_string(), CompressionStrategy::Always);
        type_strategies.insert("Array".to_string(), CompressionStrategy::Adaptive);
        type_strategies.insert("String".to_string(), CompressionStrategy::SizeThreshold);

        Self {
            size_threshold: 1024,             // 1KB
            streaming_threshold: 1024 * 1024, // 1MB
            enabled: true,
            level: 6, // Default compression level
            type_strategies,
        }
    }
}

/// Compression strategies for different message types
#[derive(Clone)]
pub enum CompressionStrategy {
    /// Never compress
    Never,
    /// Always compress
    Always,
    /// Compress based on size threshold
    SizeThreshold,
    /// Adapt based on compression ratio history
    Adaptive,
    /// Custom strategy with closure
    Custom(Arc<dyn Fn(&Message) -> bool + Send + Sync>),
}

impl Serialize for CompressionStrategy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Never => serializer.serialize_str("Never"),
            Self::Always => serializer.serialize_str("Always"),
            Self::SizeThreshold => serializer.serialize_str("SizeThreshold"),
            Self::Adaptive => serializer.serialize_str("Adaptive"),
            Self::Custom(_) => serializer.serialize_str("Custom"),
        }
    }
}

impl std::fmt::Debug for CompressionStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Never => write!(f, "Never"),
            Self::Always => write!(f, "Always"),
            Self::SizeThreshold => write!(f, "SizeThreshold"),
            Self::Adaptive => write!(f, "Adaptive"),
            Self::Custom(_) => write!(f, "Custom(_)"),
        }
    }
}

/// Stats for adaptive compression decisions
#[derive(Default)]
pub struct CompressionStats {
    pub total_original: usize,
    pub total_compressed: usize,
    pub samples: usize,
    pub average_ratio: f64,
}

impl CompressionStats {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn update(&mut self, data: &[u8]) -> bool {
        const SAMPLE_SIZE: usize = 1024;
        const MIN_RATIO: f64 = 0.8;

        let sample = if data.len() > SAMPLE_SIZE {
            &data[..SAMPLE_SIZE]
        } else {
            data
        };

        let compressed = zstd::bulk::compress(sample, 1).unwrap_or_else(|_| sample.to_vec());
        let ratio = compressed.len() as f64 / sample.len() as f64;

        self.samples += 1;
        self.total_original += sample.len();
        self.total_compressed += compressed.len();
        self.average_ratio =
            (self.average_ratio * (self.samples - 1) as f64 + ratio) / self.samples as f64;

        self.average_ratio < MIN_RATIO
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn update_with_threshold(&mut self, data: &[u8], threshold_multiplier: f64) -> bool {
        const SAMPLE_SIZE: usize = 1024;
        const BASE_MIN_RATIO: f64 = 0.85;

        let sample = if data.len() > SAMPLE_SIZE {
            &data[..SAMPLE_SIZE]
        } else {
            data
        };

        let compressed = zstd::bulk::compress(sample, 3).unwrap_or_else(|_| sample.to_vec());
        let ratio = compressed.len() as f64 / sample.len() as f64;

        const ALPHA: f64 = 0.5;
        self.samples += 1;
        self.total_original += sample.len();
        self.total_compressed += compressed.len();
        self.average_ratio = (1.0 - ALPHA) * self.average_ratio + ALPHA * ratio;

        let adjusted_threshold = BASE_MIN_RATIO * threshold_multiplier;
        self.average_ratio < adjusted_threshold
    }

    #[cfg(target_arch = "wasm32")]
    pub fn update(&mut self, data: &[u8]) -> bool {
        const SAMPLE_SIZE: usize = 1024;
        const MIN_RATIO: f64 = 0.8;

        let sample = if data.len() > SAMPLE_SIZE {
            &data[..SAMPLE_SIZE]
        } else {
            data
        };

        let compressed = compress_prepend_size(sample);
        let ratio = compressed.len() as f64 / sample.len() as f64;

        self.samples += 1;
        self.total_original += sample.len();
        self.total_compressed += compressed.len();
        self.average_ratio =
            (self.average_ratio * (self.samples - 1) as f64 + ratio) / self.samples as f64;

        self.average_ratio < MIN_RATIO
    }

    #[cfg(target_arch = "wasm32")]
    pub fn update_with_threshold(&mut self, data: &[u8], threshold_multiplier: f64) -> bool {
        const SAMPLE_SIZE: usize = 1024;
        const BASE_MIN_RATIO: f64 = 0.8;

        let sample = if data.len() > SAMPLE_SIZE {
            &data[..SAMPLE_SIZE]
        } else {
            data
        };

        let compressed = compress_prepend_size(sample);
        let ratio = compressed.len() as f64 / sample.len() as f64;

        const ALPHA: f64 = 0.1;
        self.samples += 1;
        self.total_original += sample.len();
        self.total_compressed += compressed.len();
        self.average_ratio = (1.0 - ALPHA) * self.average_ratio + ALPHA * ratio;

        let adjusted_threshold = BASE_MIN_RATIO * threshold_multiplier;
        self.average_ratio < adjusted_threshold
    }
}

impl Message {
    /// Encode message with optional compression
    pub fn encode(&self) -> Result<Vec<u8>, MessageError> {
        let encoded = bitcode::encode(self);

        if encoded.len() < COMPRESSION_THRESHOLD {
            Ok(encoded)
        } else {
            Ok(encoded) // For now returning uncompressed, will add compression in next implementation
        }
    }

    /// Decode message
    pub fn decode(bytes: &[u8]) -> Result<Self, MessageError> {
        bitcode::decode(bytes).map_err(|e| MessageError::Decoding(e.to_string()))
    }

    pub fn decode_with_config(
        bytes: &[u8],
        config: CompressionConfig,
    ) -> Result<Self, MessageError> {
        Self::decode_compressed(bytes, &config)
    }

    /// Get the port type of this message
    pub fn get_type(&self) -> PortType {
        match self {
            Message::Flow => PortType::Flow,
            Message::Event(_) => PortType::Event,
            Message::Boolean(_) => PortType::Boolean,
            Message::Integer(_) => PortType::Integer,
            Message::Float(_) => PortType::Float,
            Message::String(_) => PortType::String,
            Message::Object(v) => {
                // Expensive operation
                // let value: Value = v.as_ref().clone().into();
                // if let Some(type_name) = value.get("type").and_then(|t| t.as_str()) {
                //     PortType::Object(type_name.to_string())
                // } else {
                //     PortType::Object("Dynamic".to_string())
                // }
                PortType::Object("Dynamic".to_string())
            }
            Message::Array(arr) => PortType::Array(Box::new(PortType::Any)),
            Message::Stream(_) => PortType::Stream,
            Message::Optional(opt) => PortType::Option(Box::new(PortType::Any)),
            Message::Any(_) => PortType::Any,
            Message::Error(_) => PortType::String,
            Message::Encoded(..) => PortType::Encoded,
            Message::RemoteReference { .. } => PortType::Any,
            Message::NetworkEvent { .. } => PortType::Event,
        }
    }

    /// Validate message against a port type
    pub fn validate_type(&self, port_type: &PortType) -> Result<(), MessageError> {
        match (self, port_type) {
            // Direct type matches
            (msg, t) if msg.get_type() == *t => Ok(()),

            // Number type compatibility
            (Message::Integer(_), PortType::Float) => Ok(()),

            // Array type validation
            (Message::Array(arr), PortType::Array(elem_type)) => {
                arr.iter().try_for_each(|_elem| Ok(()))
            }

            // Optional type validation
            (Message::Optional(opt), PortType::Option(inner_type)) => match opt {
                Some(inner) => Ok(()),
                None => Ok(()),
            },

            // Tuple type validation
            // (Message::Tuple(items), PortType::Tuple(types)) => {
            //     if items.len() != types.len() {
            //         return Err(MessageError::TypeMismatch(format!(
            //             "Expected tuple of size {}, got {}",
            //             types.len(),
            //             items.len()
            //         )));
            //     }
            //     items
            //         .iter()
            //         .zip(types)
            //         .try_for_each(|(item, t)| item.validate_type(t))
            // }

            // Any type accepts everything
            (_, PortType::Any) => Ok(()),

            _ => Err(MessageError::TypeMismatch(format!(
                "Expected {:?}, got {:?}",
                port_type,
                self.get_type()
            ))),
        }
    }

    /// Get encoded size
    pub fn encoded_size(&self) -> Result<usize, MessageError> {
        self.encode().map(|bytes| bytes.len())
    }

    /// Encode message with configurable compression
    pub fn encode_with_config(
        &self,
        config: &CompressionConfig,
    ) -> Result<EncodedMessage, MessageError> {
        if !config.enabled {
            return Ok(EncodedMessage(bitcode::encode(self)));
        }

        let strategy = self.get_compression_strategy(config);
        let encoded = bitcode::encode(self);

        match strategy {
            CompressionStrategy::Never => Ok(EncodedMessage(encoded)),
            CompressionStrategy::Always => {
                Ok(EncodedMessage(self.compress_data(&encoded, config)?))
            }
            CompressionStrategy::SizeThreshold => {
                if encoded.len() >= config.size_threshold {
                    Ok(EncodedMessage(self.compress_data(&encoded, config)?))
                } else {
                    Ok(EncodedMessage(encoded))
                }
            }
            CompressionStrategy::Adaptive => {
                // Use compression history to decide
                if self.should_compress_adaptive(&encoded) {
                    Ok(EncodedMessage(self.compress_data(&encoded, config)?))
                } else {
                    Ok(EncodedMessage(encoded))
                }
            }
            CompressionStrategy::Custom(strategy_fn) => {
                if strategy_fn(self) {
                    Ok(EncodedMessage(self.compress_data(&encoded, config)?))
                } else {
                    Ok(EncodedMessage(encoded))
                }
            }
        }
    }

    /// Get compression strategy for this message type
    fn get_compression_strategy(&self, config: &CompressionConfig) -> CompressionStrategy {
        let type_name = self.type_name();

        config
            .type_strategies
            .get(type_name)
            .cloned()
            .unwrap_or(CompressionStrategy::SizeThreshold)
    }

    /// Compress data with configured compression level
    pub fn compress_data(
        &self,
        data: &[u8],
        config: &CompressionConfig,
    ) -> Result<Vec<u8>, MessageError> {
        if config.enabled {
            return if data.len() >= config.streaming_threshold {
                self.compress_streaming(data, config)
            } else {
                self.compress_normal(data, config)
            };
        }
        Ok(data.to_vec())
    }

    /// Normal compression for regular-sized data
    #[cfg(not(target_arch = "wasm32"))]
    fn compress_normal(
        &self,
        data: &[u8],
        config: &CompressionConfig,
    ) -> Result<Vec<u8>, MessageError> {
        let mut encoder = flate2::Compress::new(flate2::Compression::new(config.level), false);

        let mut compressed = Vec::with_capacity(data.len());
        encoder
            .compress_vec(data, &mut compressed, flate2::FlushCompress::Finish)
            .map_err(|e| MessageError::Compression(e.to_string()))?;

        Ok(compressed)
    }

    #[cfg(target_arch = "wasm32")]
    fn compress_normal(
        &self,
        data: &[u8],
        config: &CompressionConfig,
    ) -> Result<Vec<u8>, MessageError> {
        let compressed = compress_prepend_size(data);
        Ok(compressed)
    }

    /// Streaming compression for large data
    #[cfg(not(target_arch = "wasm32"))]
    pub fn compress_streaming(
        &self,
        data: &[u8],
        config: &CompressionConfig,
    ) -> Result<Vec<u8>, MessageError> {
        use std::io::Write;

        if !zstd::compression_level_range().contains(&(config.level as i32)) {
            return Err(MessageError::Compression(format!(
                "Invalid compression level {}",
                config.level
            )));
        }
        let mut encoder = zstd::Encoder::new(Vec::new(), config.level as i32)
            .map_err(|e| MessageError::Compression(e.to_string()))?;

        // Process in chunks
        for chunk in data.chunks(64 * 1024) {
            // 64KB chunks
            encoder
                .write_all(chunk)
                .map_err(|e| MessageError::Compression(e.to_string()))?;
        }

        encoder
            .finish()
            .map_err(|e| MessageError::Compression(e.to_string()))
    }

    fn type_name(&self) -> &'static str {
        match self {
            Message::Flow => "Flow",
            Message::Event(_) => "Event",
            Message::Boolean(_) => "Boolean",
            Message::Integer(_) => "Integer",
            Message::Float(_) => "Float",
            Message::String(_) => "String",
            Message::Object(_) => "Object",
            Message::Array(_) => "Array",
            Message::Stream(_) => "Stream",
            Message::Optional(_) => "Optional",
            Message::Any(_) => "Any",
            Message::Error(_) => "Error",
            Message::Encoded(..) => "Encoded",
            Message::RemoteReference { .. } => "NetworkReference",
            Message::NetworkEvent { .. } => "NetworkEvent",
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn compress_streaming(
        &self,
        data: &[u8],
        config: &CompressionConfig,
    ) -> Result<Vec<u8>, MessageError> {
        const CHUNK_SIZE: usize = 64 * 1024; // 64KB chunks
        let mut compressed = Vec::new();

        for chunk in data.chunks(CHUNK_SIZE) {
            let chunk_compressed = compress_prepend_size(chunk);
            compressed.extend_from_slice(&(chunk_compressed.len() as u32).to_le_bytes());
            compressed.extend_from_slice(&chunk_compressed);
        }

        Ok(compressed)
    }

    /// Adaptive compression decision based on history
    pub(crate) fn should_compress_adaptive(&self, data: &[u8]) -> bool {
        const MAX_HISTORY_SIZE: usize = 1000;
        const CLEANUP_THRESHOLD: usize = 10000;

        // Get compression history for this message type
        static HISTORY: Lazy<RwLock<HashMap<String, (CompressionStats, std::time::Instant)>>> =
            Lazy::new(|| RwLock::new(HashMap::new()));

        let type_name = self.type_name();
        let mut history = HISTORY.write();

        // Cleanup old entries if the history gets too large
        if history.len() > CLEANUP_THRESHOLD {
            history.retain(|_, (_, last_access)| {
                last_access.elapsed() < std::time::Duration::from_secs(3600)
            });
        }

        // Get or create stats for this type
        let (stats, last_access) = history
            .entry(type_name.to_string())
            .or_insert_with(|| (CompressionStats::default(), std::time::Instant::now()));

        *last_access = std::time::Instant::now();

        // Apply type-specific adjustments
        let threshold_multiplier = match self {
            Message::Stream(_) => 0.7, // More aggressive compression for streams
            Message::String(_) => 1.5, // Much less aggressive for strings
            Message::Array(_) => 0.8,  // Moderate for arrays
            _ => 0.8,                  // Default threshold
        };

        // Limit samples to prevent old data from affecting decisions too much
        if stats.samples > MAX_HISTORY_SIZE {
            *stats = CompressionStats::default();
        }

        // Update stats and make decision
        stats.update_with_threshold(data, threshold_multiplier)
    }

    /// Decode message with automatic decompression
    #[cfg(not(target_arch = "wasm32"))]
    pub fn decode_compressed(
        bytes: &[u8],
        config: &CompressionConfig,
    ) -> Result<Self, MessageError> {
        if !config.enabled {
            return bitcode::decode(bytes).map_err(|e| MessageError::Decoding(e.to_string()));
        }

        // Check for streaming compression marker
        if bytes.len() > 4 && bytes[0..4] == [0x28, 0xB5, 0x2F, 0xFD] {
            // ZSTD magic number
            let decoded =
                zstd::decode_all(bytes).map_err(|e| MessageError::Compression(e.to_string()))?;
            bitcode::decode(&decoded).map_err(|e| MessageError::Decoding(e.to_string()))
        } else {
            let decoded = &mut Vec::new();
            // Try normal decompression first
            match flate2::read::GzDecoder::new(bytes).read_to_end(decoded) {
                Ok(_) => {
                    bitcode::decode(&decoded).map_err(|e| MessageError::Decoding(e.to_string()))
                }
                Err(_) => {
                    // If decompression fails, try decoding as uncompressed
                    bitcode::decode(bytes).map_err(|e| MessageError::Decoding(e.to_string()))
                }
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn decode_compressed(bytes: &[u8], config: &CompressionConfig) -> Result<Self, MessageError> {
        if !config.enabled {
            return bitcode::decode(bytes).map_err(|e| MessageError::Decoding(e.to_string()));
        }

        // Try to decompress as a single block first
        match decompress_size_prepended(bytes) {
            Ok(decompressed) => {
                bitcode::decode(&decompressed).map_err(|e| MessageError::Decoding(e.to_string()))
            }
            Err(_) => {
                // If single block decompression fails, try chunked format
                let mut position = 0;
                let mut decompressed = Vec::new();

                while position + 4 <= bytes.len() {
                    let chunk_size =
                        u32::from_le_bytes(bytes[position..position + 4].try_into().unwrap())
                            as usize;
                    position += 4;

                    if position + chunk_size > bytes.len() {
                        break;
                    }

                    let chunk = &bytes[position..position + chunk_size];
                    match decompress_size_prepended(chunk) {
                        Ok(chunk_decompressed) => {
                            decompressed.extend_from_slice(&chunk_decompressed)
                        }
                        Err(e) => return Err(MessageError::Compression(e.to_string())),
                    }
                    position += chunk_size;
                }

                if !decompressed.is_empty() {
                    bitcode::decode(&decompressed)
                        .map_err(|e| MessageError::Decoding(e.to_string()))
                } else {
                    // If all decompression attempts fail, try to decode as uncompressed
                    bitcode::decode(bytes).map_err(|e| MessageError::Decoding(e.to_string()))
                }
            }
        }
    }
}

#[derive(Debug)]
pub enum MessageError {
    TypeMismatch(String),
    Encoding(String),
    Decoding(String),
    Validation(String),
    Compression(String),
}

impl From<Value> for Message {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Message::Optional(None),
            Value::Bool(b) => Message::Boolean(b),
            Value::Number(n) => {
                if n.is_i64() {
                    Message::Integer(n.as_i64().unwrap())
                } else {
                    Message::Float(n.as_f64().unwrap())
                }
            }
            Value::String(s) => Message::String(Arc::new(s)),
            Value::Array(vec) => Message::array(vec.into_iter().map(|v| v.into()).collect()),
            Value::Object(_) => Message::Object(Arc::new(EncodableValue::from(value))),
        }
    }
}

impl Into<Value> for Message {
    fn into(self) -> Value {
        match self {
            Message::Flow => Value::String("flow".to_string()),
            Message::Event(v) => v.into(),
            Message::Boolean(b) => Value::Bool(b),
            Message::Integer(i) => Value::Number(i.into()),
            Message::Float(f) => Value::Number(serde_json::Number::from_f64(f).unwrap()),
            Message::String(s) => Value::String(s.as_str().to_string()),
            Message::Object(v) => v.as_ref().clone().into(),
            Message::Array(arr) => Value::Array(
                arr.iter()
                    // .filter_map(|m| m.decode())
                    .map(|m| m.clone().into())
                    .collect(),
            ),
            Message::Stream(bytes) => Value::Array(
                <Vec<u8> as Clone>::clone(&bytes)
                    .into_iter()
                    .map(|b| Value::Number(b.into()))
                    .collect(),
            ),
            Message::Optional(opt) => match opt {
                Some(m) => Value::from(m.as_ref().clone()),
                None => Value::Null,
            },
            Message::Any(v) => v.as_ref().clone().into(),
            Message::Error(e) => Value::String(e.as_str().to_string()),
            Message::Encoded(encoded) => bitcode::decode::<Message>(&encoded)
                .expect("Failed to decode message")
                .into(),
            Message::RemoteReference {
                network_id,
                actor_id,
                port,
            } => json!({
                "network_id": network_id,
                "actor_id": actor_id,
                "port": port
            }),
            Message::NetworkEvent { event_type, data } => json!({
                "event_type": event_type,
                "data": serde_json::Value::from(data)
            }),
        }
    }
}

// WebAssembly-specific implementations
#[cfg(target_arch = "wasm32")]
impl From<JsValue> for Message {
    fn from(value: JsValue) -> Self {
        if let Ok(val) = value.into_serde::<Value>() {
            match val {
                Value::Bool(b) => Message::Boolean(b),
                Value::Number(n) => {
                    if n.is_i64() {
                        Message::Integer(n.as_i64().unwrap())
                    } else {
                        Message::Float(n.as_f64().unwrap())
                    }
                }
                Value::String(s) => Message::String(Arc::new(s)),
                Value::Array(arr) => {
                    Message::array(arr.into_iter().map(|v| EncodableValue::from(v)).collect())
                }
                Value::Object(obj) => {
                    Message::Object(Arc::new(EncodableValue::from(Value::Object(obj))))
                }
                Value::Null => Message::Optional(None),
            }
        } else {
            Message::Error(Arc::new("Invalid JS value".to_string()))
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Into<JsValue> for Message {
    fn into(self) -> JsValue {
        match self {
            Message::Flow => JsValue::from_str("flow"),
            Message::Event(v) => JsValue::from_serde(&v).unwrap_or_default(),
            Message::Boolean(b) => JsValue::from_bool(b),
            Message::Integer(i) => JsValue::from_f64(i as f64),
            Message::Float(f) => JsValue::from_f64(f),
            Message::String(s) => JsValue::from_str(&s),
            Message::Object(v) => JsValue::from_serde(&v).unwrap_or_default(),
            Message::Array(arr) => {
                let js_arr = js_sys::Array::new();
                for msg in arr.iter() {
                    if let Ok(js_val) = JsValue::from_serde(&msg) {
                        js_arr.push(&js_val);
                    }
                }
                js_arr.into()
            }
            Message::Stream(bytes) => {
                let array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
                array.copy_from(&bytes);
                array.into()
            }
            Message::Optional(opt) => match opt {
                Some(msg) => msg
                    .decode()
                    .map(|m: Message| m.into())
                    .unwrap_or(JsValue::NULL),
                None => JsValue::NULL,
            },
            // Message::Tuple(items) => {
            //     let js_arr = js_sys::Array::new();
            //     for msg in items {
            //         js_arr.push(&msg.into());
            //     }
            //     js_arr.into()
            // }
            // Message::Generic(v) => JsValue::from_serde(&v).unwrap_or_default(),
            Message::Any(v) => JsValue::from_serde(&v).unwrap_or_default(),
            Message::Error(e) => JsValue::from_str(&e),
            Message::Encoded(encoded) => {
                let decoded = bitcode::decode::<Message>(&encoded).unwrap_or_default();
                decoded.into()
            }
        }
    }
}

/// Wrapper for any bitcode encodable value
#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode, PartialEq, Eq)]
pub struct EncodableValue {
    pub(crate) data: Vec<u8>,
}

impl EncodableValue {
    pub fn new<T: Encode>(value: &T) -> Self {
        Self {
            data: bitcode::encode(value),
        }
    }

    pub fn decode<'a, T: Decode<'a>>(&'a self) -> Option<T> {
        bitcode::decode(&self.data).ok()
    }

    pub(crate) fn len(&self) -> usize {
        self.data.len()
    }
}

// Implementation for Value (using JSON serialization)
impl From<Value> for EncodableValue {
    fn from(v: Value) -> Self {
        Self {
            data: serde_json::to_vec(&v).unwrap_or_default(),
        }
    }
}

impl From<EncodableValue> for Value {
    fn from(v: EncodableValue) -> Self {
        serde_json::from_slice(&v.data).unwrap_or(Value::Null)
    }
}

// Helper constructors
impl Message {
    pub fn object(value: EncodableValue) -> Self {
        Message::Object(Arc::new(value))
    }
    pub fn any(value: EncodableValue) -> Self {
        Message::Any(Arc::new(value))
    }
    pub fn event(value: EncodableValue) -> Self {
        Message::Event(value)
    }

    pub fn array(messages: Vec<EncodableValue>) -> Self {
        Message::Array(Arc::new(messages))
    }
    pub fn stream(bytes: Vec<u8>) -> Self {
        Message::Stream(bytes.into())
    }
    pub fn encoded(encoded: Vec<u8>) -> Self {
        Message::Encoded(Arc::new(encoded))
    }
    pub fn error(msg: String) -> Self {
        Message::Error(msg.into())
    }
    pub fn boolean(value: bool) -> Self {
        Message::Boolean(value)
    }
    pub fn integer(value: i64) -> Self {
        Message::Integer(value)
    }
    pub fn float(value: f64) -> Self {
        Message::Float(value)
    }
    pub fn string(value: String) -> Self {
        Message::String(Arc::new(value))
    }
    pub fn flow() -> Self {
        Message::Flow
    }

    pub fn optional(msg: Option<EncodableValue>) -> Self {
        Message::Optional(msg.map(|d| Arc::new(d)))
    }
}
