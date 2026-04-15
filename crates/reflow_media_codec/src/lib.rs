//! Codecs between typed media/ML packets and existing Reflow `Message` values.

use anyhow::{anyhow, bail, Context, Result};
use reflow_actor::message::{EncodableValue, Message};
use reflow_media_types::{Packet, TensorDType, TensorPacket, TensorShape, VideoFrame};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

const ENVELOPE_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinaryEnvelopeHeader {
    kind: String,
    version: u8,
    payload: Value,
    byte_len: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TensorHeader {
    name: Option<String>,
    dtype: TensorDType,
    shape: TensorShape,
    metadata: reflow_media_types::PacketMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FrameHeader {
    width: u32,
    height: u32,
    format: reflow_media_types::ImageFormat,
    stride: Option<usize>,
    metadata: reflow_media_types::PacketMetadata,
}

pub fn packet_to_message<T: Serialize>(packet: &Packet<T>) -> Result<Message> {
    let value = serde_json::to_value(packet)?;
    Ok(Message::object(EncodableValue::from(value)))
}

pub fn message_to_packet<T: DeserializeOwned>(message: &Message) -> Result<Packet<T>> {
    let value = message_to_value(message)?;
    Ok(serde_json::from_value(value)?)
}

pub fn value_to_object_message<T: Serialize>(value: &T) -> Result<Message> {
    Ok(Message::object(EncodableValue::from(serde_json::to_value(
        value,
    )?)))
}

pub fn message_to_value(message: &Message) -> Result<Value> {
    match message {
        Message::Object(value) | Message::Any(value) => Ok(value.as_ref().clone().into()),
        Message::Event(value) => Ok(value.clone().into()),
        Message::String(value) => serde_json::from_str(value.as_str())
            .with_context(|| "string message did not contain JSON"),
        Message::Encoded(encoded) => {
            let decoded = Message::decode(encoded)
                .map_err(|err| anyhow!("failed to decode encoded message: {:?}", err))?;
            message_to_value(&decoded)
        }
        other => Ok(serde_json::Value::from(other.clone())),
    }
}

pub fn value_from_message_or_packet<T: DeserializeOwned>(message: &Message) -> Result<T> {
    let value = message_to_value(message)?;
    if let Ok(value) = serde_json::from_value::<T>(value.clone()) {
        return Ok(value);
    }
    let packet: Packet<T> = serde_json::from_value(value)?;
    Ok(packet.data)
}

pub fn tensor_to_message(tensor: &TensorPacket) -> Result<Message> {
    let header = TensorHeader {
        name: tensor.name.clone(),
        dtype: tensor.dtype,
        shape: tensor.shape.clone(),
        metadata: tensor.metadata.clone(),
    };
    encode_binary_envelope("tensor", &header, &tensor.data)
}

pub fn message_to_tensor(message: &Message) -> Result<TensorPacket> {
    match message {
        Message::Bytes(bytes) => {
            let (header, data): (TensorHeader, Vec<u8>) = decode_binary_envelope(bytes, "tensor")?;
            Ok(TensorPacket {
                name: header.name,
                dtype: header.dtype,
                shape: header.shape,
                data,
                metadata: header.metadata,
            })
        }
        Message::Encoded(encoded) => {
            let decoded = Message::decode(encoded)
                .map_err(|err| anyhow!("failed to decode encoded tensor message: {:?}", err))?;
            message_to_tensor(&decoded)
        }
        _ => value_from_message_or_packet(message),
    }
}

pub fn frame_to_message(frame: &VideoFrame) -> Result<Message> {
    let header = FrameHeader {
        width: frame.width,
        height: frame.height,
        format: frame.format.clone(),
        stride: frame.stride,
        metadata: frame.metadata.clone(),
    };
    encode_binary_envelope("video-frame", &header, &frame.data)
}

pub fn message_to_frame(message: &Message) -> Result<VideoFrame> {
    match message {
        Message::Bytes(bytes) => {
            let (header, data): (FrameHeader, Vec<u8>) =
                decode_binary_envelope(bytes, "video-frame")?;
            Ok(VideoFrame {
                width: header.width,
                height: header.height,
                format: header.format,
                stride: header.stride,
                data,
                metadata: header.metadata,
            })
        }
        Message::Encoded(encoded) => {
            let decoded = Message::decode(encoded)
                .map_err(|err| anyhow!("failed to decode encoded frame message: {:?}", err))?;
            message_to_frame(&decoded)
        }
        _ => value_from_message_or_packet(message),
    }
}

fn encode_binary_envelope<T: Serialize>(kind: &str, payload: &T, data: &[u8]) -> Result<Message> {
    let payload = serde_json::to_value(payload)?;
    let header = BinaryEnvelopeHeader {
        kind: kind.to_string(),
        version: ENVELOPE_VERSION,
        payload,
        byte_len: data.len(),
    };
    let header_bytes = serde_json::to_vec(&header)?;
    let header_len = u32::try_from(header_bytes.len())
        .map_err(|_| anyhow!("media envelope header is too large"))?;

    let mut bytes = Vec::with_capacity(4 + header_bytes.len() + data.len());
    bytes.extend_from_slice(&header_len.to_le_bytes());
    bytes.extend_from_slice(&header_bytes);
    bytes.extend_from_slice(data);
    Ok(Message::bytes(bytes))
}

fn decode_binary_envelope<T: DeserializeOwned>(
    bytes: &[u8],
    expected_kind: &str,
) -> Result<(T, Vec<u8>)> {
    if bytes.len() < 4 {
        bail!("media envelope is too small");
    }
    let header_len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let header_start = 4;
    let data_start = header_start + header_len;
    if data_start > bytes.len() {
        bail!("media envelope header exceeds payload length");
    }

    let header: BinaryEnvelopeHeader = serde_json::from_slice(&bytes[header_start..data_start])?;
    if header.version != ENVELOPE_VERSION {
        bail!(
            "unsupported media envelope version {}, expected {}",
            header.version,
            ENVELOPE_VERSION
        );
    }
    if header.kind != expected_kind {
        bail!(
            "media envelope kind mismatch: expected {}, got {}",
            expected_kind,
            header.kind
        );
    }

    let data = bytes[data_start..].to_vec();
    if data.len() != header.byte_len {
        bail!(
            "media envelope byte length mismatch: expected {}, got {}",
            header.byte_len,
            data.len()
        );
    }

    Ok((serde_json::from_value(header.payload)?, data))
}

pub fn tensor_summary(tensor: &TensorPacket) -> Value {
    json!({
        "name": tensor.name,
        "dtype": tensor.dtype,
        "shape": tensor.shape.dims,
        "bytes": tensor.data.len(),
        "expectedBytes": tensor.expected_byte_len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use reflow_media_types::{
        ImageFormat, PacketMetadata, TensorPacket, TensorShape, Timestamp, VideoFrame,
    };

    #[test]
    fn tensor_message_roundtrip_preserves_header_and_bytes() {
        let mut tensor = TensorPacket::from_f32(
            Some("landmarks".to_string()),
            TensorShape::new([1, 2]),
            &[0.25, 0.75],
        );
        tensor.metadata = PacketMetadata::with_timestamp(Timestamp::from_millis(4));

        let message = tensor_to_message(&tensor).unwrap();
        let decoded = message_to_tensor(&message).unwrap();

        assert_eq!(decoded.name.as_deref(), Some("landmarks"));
        assert_eq!(decoded.shape.dims, vec![1, 2]);
        assert_eq!(decoded.metadata.timestamp.unwrap().micros, 4_000);
        assert_eq!(decoded.as_f32_vec().unwrap(), vec![0.25, 0.75]);
    }

    #[test]
    fn frame_message_roundtrip_preserves_raw_pixels() {
        let frame = VideoFrame::new(2, 1, ImageFormat::Rgba8, vec![1, 2, 3, 4, 5, 6, 7, 8]);

        let message = frame_to_message(&frame).unwrap();
        let decoded = message_to_frame(&message).unwrap();

        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.format, ImageFormat::Rgba8);
        assert_eq!(decoded.data, frame.data);
    }
}
