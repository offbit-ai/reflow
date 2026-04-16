//! Shared media and ML packet contracts for Reflow graphs.
//!
//! These types are deliberately runtime-agnostic. They describe data moving
//! through DAGs, while `reflow_media_codec` decides how those values are
//! transported over existing Reflow `Message` variants.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timestamp {
    pub micros: i64,
}

impl Timestamp {
    pub fn from_micros(micros: i64) -> Self {
        Self { micros }
    }

    pub fn from_millis(millis: i64) -> Self {
        Self {
            micros: millis.saturating_mul(1_000),
        }
    }

    pub fn from_seconds(seconds: f64) -> Self {
        Self {
            micros: (seconds * 1_000_000.0).round() as i64,
        }
    }

    pub fn as_seconds(self) -> f64 {
        self.micros as f64 / 1_000_000.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<Timestamp>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fields: HashMap<String, Value>,
}

impl PacketMetadata {
    pub fn with_timestamp(timestamp: Timestamp) -> Self {
        Self {
            timestamp: Some(timestamp),
            ..Self::default()
        }
    }

    pub fn merge_missing_from(&mut self, other: &Self) {
        if self.timestamp.is_none() {
            self.timestamp = other.timestamp;
        }
        if self.sequence.is_none() {
            self.sequence = other.sequence;
        }
        if self.stream_id.is_none() {
            self.stream_id = other.stream_id.clone();
        }
        if self.source.is_none() {
            self.source = other.source.clone();
        }
        for (key, value) in &other.fields {
            self.fields
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Packet<T> {
    pub data: T,
    #[serde(default)]
    pub metadata: PacketMetadata,
}

impl<T> Packet<T> {
    pub fn new(data: T) -> Self {
        Self {
            data,
            metadata: PacketMetadata::default(),
        }
    }

    pub fn with_metadata(data: T, metadata: PacketMetadata) -> Self {
        Self { data, metadata }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageFormat {
    #[default]
    Rgba8,
    Rgb8,
    Bgra8,
    Gray8,
    F32Rgb,
    F32Rgba,
    Unknown(String),
}

impl ImageFormat {
    pub fn channels(&self) -> usize {
        match self {
            Self::Rgba8 | Self::Bgra8 | Self::F32Rgba => 4,
            Self::Rgb8 | Self::F32Rgb => 3,
            Self::Gray8 => 1,
            Self::Unknown(_) => 4,
        }
    }

    pub fn bytes_per_channel(&self) -> usize {
        match self {
            Self::F32Rgb | Self::F32Rgba => 4,
            _ => 1,
        }
    }

    pub fn from_label(label: &str) -> Self {
        match label.to_ascii_lowercase().as_str() {
            "rgba8" | "rgba" | "image/raw-rgba" | "video/raw-rgba" => Self::Rgba8,
            "rgb8" | "rgb" | "image/raw-rgb" | "video/raw-rgb" => Self::Rgb8,
            "bgra8" | "bgra" | "image/raw-bgra" | "video/raw-bgra" => Self::Bgra8,
            "gray8" | "grey8" | "luma8" | "image/raw-gray" | "video/raw-gray" => Self::Gray8,
            "f32-rgb" | "float32-rgb" => Self::F32Rgb,
            "f32-rgba" | "float32-rgba" => Self::F32Rgba,
            other => Self::Unknown(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub format: ImageFormat,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stride: Option<usize>,
    pub data: Vec<u8>,
    #[serde(default)]
    pub metadata: PacketMetadata,
}

impl VideoFrame {
    pub fn new(width: u32, height: u32, format: ImageFormat, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format,
            stride: None,
            data,
            metadata: PacketMetadata::default(),
        }
    }

    pub fn channels(&self) -> usize {
        self.format.channels()
    }

    pub fn row_bytes(&self) -> usize {
        self.stride.unwrap_or_else(|| {
            self.width as usize * self.format.channels() * self.format.bytes_per_channel()
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TensorDType {
    F32,
    F16,
    I32,
    I64,
    U8,
    I8,
    Bool,
}

impl TensorDType {
    pub fn bytes_per_element(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::F16 => 2,
            Self::I64 => 8,
            Self::U8 | Self::I8 | Self::Bool => 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TensorShape {
    pub dims: Vec<usize>,
}

impl TensorShape {
    pub fn new(dims: impl Into<Vec<usize>>) -> Self {
        Self { dims: dims.into() }
    }

    pub fn element_count(&self) -> usize {
        if self.dims.is_empty() {
            0
        } else {
            self.dims.iter().product()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TensorPacket {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub dtype: TensorDType,
    pub shape: TensorShape,
    pub data: Vec<u8>,
    #[serde(default)]
    pub metadata: PacketMetadata,
}

impl TensorPacket {
    pub fn new(
        name: impl Into<Option<String>>,
        dtype: TensorDType,
        shape: TensorShape,
        data: Vec<u8>,
    ) -> Self {
        Self {
            name: name.into(),
            dtype,
            shape,
            data,
            metadata: PacketMetadata::default(),
        }
    }

    pub fn from_f32(name: impl Into<Option<String>>, shape: TensorShape, values: &[f32]) -> Self {
        let mut data = Vec::with_capacity(values.len() * 4);
        for value in values {
            data.extend_from_slice(&value.to_le_bytes());
        }
        Self::new(name, TensorDType::F32, shape, data)
    }

    pub fn expected_byte_len(&self) -> usize {
        self.shape.element_count() * self.dtype.bytes_per_element()
    }

    pub fn as_f32_vec(&self) -> Option<Vec<f32>> {
        if self.dtype != TensorDType::F32 || !self.data.len().is_multiple_of(4) {
            return None;
        }
        Some(
            self.data
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Landmark {
    pub x: f32,
    pub y: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub z: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, Value>,
}

impl Landmark {
    pub fn new(x: f32, y: f32, z: Option<f32>) -> Self {
        Self {
            x,
            y,
            z,
            visibility: None,
            presence: None,
            name: None,
            metadata: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    /// Normalized `[x, y, width, height]`.
    pub bbox: [f32; 4],
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keypoints: Vec<Landmark>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectionSet {
    #[serde(default)]
    pub detections: Vec<Detection>,
    #[serde(default)]
    pub metadata: PacketMetadata,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LandmarkSet {
    #[serde(default)]
    pub landmarks: Vec<Landmark>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub world_landmarks: Option<Vec<Landmark>>,
    #[serde(default)]
    pub metadata: PacketMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedRect {
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub rotation: f32,
}

impl Default for NormalizedRect {
    fn default() -> Self {
        Self {
            center_x: 0.5,
            center_y: 0.5,
            width: 1.0,
            height: 1.0,
            rotation: 0.0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Roi {
    pub rect: NormalizedRect,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_size: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default)]
    pub metadata: PacketMetadata,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tensor_packet_roundtrips_f32_values() {
        let tensor = TensorPacket::from_f32(
            Some("scores".to_string()),
            TensorShape::new([2, 2]),
            &[0.1, 0.2, 0.3, 0.4],
        );

        assert_eq!(tensor.expected_byte_len(), 16);
        assert_eq!(tensor.as_f32_vec().unwrap(), vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn metadata_merge_preserves_existing_values() {
        let mut lhs = PacketMetadata {
            timestamp: Some(Timestamp::from_millis(7)),
            ..PacketMetadata::default()
        };
        let rhs = PacketMetadata {
            timestamp: Some(Timestamp::from_millis(9)),
            sequence: Some(3),
            ..PacketMetadata::default()
        };

        lhs.merge_missing_from(&rhs);

        assert_eq!(lhs.timestamp.unwrap().micros, 7_000);
        assert_eq!(lhs.sequence, Some(3));
    }

    #[test]
    fn image_format_accepts_video_raw_content_types() {
        assert_eq!(
            ImageFormat::from_label("video/raw-rgba"),
            ImageFormat::Rgba8
        );
        assert_eq!(ImageFormat::from_label("video/raw-rgb"), ImageFormat::Rgb8);
        assert_eq!(
            ImageFormat::from_label("video/raw-gray"),
            ImageFormat::Gray8
        );
    }
}
