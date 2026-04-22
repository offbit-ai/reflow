//! Graph-friendly computer-vision actors for Reflow ML pipelines.

use anyhow::{anyhow, bail, Error, Result};
use reflow_actor::{
    message::{EncodableValue, Message},
    stream::{spawn_stream_task, stream_collect, StreamFrame, StreamHandle},
    Actor, ActorBehavior, ActorContext, MemoryState, Port,
};
use reflow_actor_macro::actor;
use reflow_media_codec::{
    frame_to_message, message_to_frame, message_to_tensor, tensor_to_message,
    value_from_message_or_packet, value_to_object_message,
};
use reflow_media_types::{
    DetectionSet, ImageFormat, LandmarkSet, PacketMetadata, Roi, TensorDType, TensorPacket,
    TensorShape, Timestamp, VideoFrame,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    ImageToTensorActor,
    inports::<100>(frame),
    outports::<50>(tensor, error),
    state(MemoryState)
)]
pub async fn image_to_tensor_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let frame = match frame_from_context(&context, "frame").await {
        Ok(frame) => frame,
        Err(err) => return Ok(error_output(&err.to_string())),
    };
    let config = context.get_config_hashmap();
    let dtype = dtype_config(&config, "dtype", TensorDType::F32);
    let layout = string_config(&config, "layout", "nhwc").to_ascii_lowercase();
    let channels = usize_config(&config, "channels", frame.channels().clamp(1, 3));
    let name = string_config(&config, "name", "image");
    let scale = f32_config(
        &config,
        "scale",
        if dtype == TensorDType::F32 {
            1.0 / 255.0
        } else {
            1.0
        },
    );
    let offset = f32_config(&config, "offset", 0.0);

    let tensor = match image_to_tensor(&frame, &name, dtype, &layout, channels, scale, offset) {
        Ok(tensor) => tensor,
        Err(err) => return Ok(error_output(&err.to_string())),
    };

    Ok([("tensor".to_string(), tensor_to_message(&tensor)?)].into())
}

#[actor(
    ResizeLetterboxActor,
    inports::<100>(frame),
    outports::<50>(frame, transform, error),
    state(MemoryState)
)]
pub async fn resize_letterbox_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let frame = match frame_from_context(&context, "frame").await {
        Ok(frame) => frame,
        Err(err) => return Ok(error_output(&err.to_string())),
    };
    let config = context.get_config_hashmap();
    let width = u32_config(&config, "width", frame.width);
    let height = u32_config(&config, "height", frame.height);
    let fill = u8_config(&config, "fill", 0);

    match resize_letterbox(&frame, width, height, fill) {
        Ok((resized, transform)) => Ok([
            ("frame".to_string(), frame_to_message(&resized)?),
            (
                "transform".to_string(),
                Message::object(EncodableValue::from(transform)),
            ),
        ]
        .into()),
        Err(err) => Ok(error_output(&err.to_string())),
    }
}

#[actor(
    VideoStreamToFramesActor,
    inports::<100>(stream),
    outports::<100>(frame, metadata, done, error),
    state(MemoryState)
)]
pub async fn video_stream_to_frames_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let handle = match payload.get("stream") {
        Some(Message::StreamHandle(handle)) => handle.clone(),
        _ => return Ok(error_output("Expected StreamHandle message on stream port")),
    };
    let rx = match context.take_stream_receiver("stream") {
        Some(rx) => rx,
        None => return Ok(error_output("No StreamHandle receiver on stream port")),
    };

    let options = VideoStreamFrameOptions::from_config(&context.get_config_hashmap(), &handle);
    let out_sender = context.get_outports().0;
    let metadata = json!({
        "attached": true,
        "source": "videoStreamToFrames",
        "streamId": handle.stream_id,
        "originActor": handle.origin_actor,
        "originPort": handle.origin_port,
        "contentType": handle.content_type,
        "dropPolicy": options.drop_policy,
    });

    spawn_stream_task(async move {
        bridge_video_stream_to_frames(rx, out_sender, handle, options).await;
    });

    Ok([("metadata".to_string(), value_to_object_message(&metadata)?)].into())
}

#[actor(
    NormalizeTensorActor,
    inports::<100>(tensor),
    outports::<50>(tensor, error),
    state(MemoryState)
)]
pub async fn normalize_tensor_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let tensor = match payload.get("tensor").map(message_to_tensor) {
        Some(Ok(tensor)) => tensor,
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => return Ok(error_output("Expected tensor input")),
    };
    let config = context.get_config_hashmap();
    let scale = f32_config(&config, "scale", 1.0);
    let offset = f32_config(&config, "offset", 0.0);
    let channels = usize_config(
        &config,
        "channels",
        tensor.shape.dims.last().copied().unwrap_or(1).max(1),
    );
    let mean = f32_vec_config(&config, "mean");
    let std = f32_vec_config(&config, "std");

    match normalize_tensor(&tensor, scale, offset, channels, &mean, &std) {
        Ok(tensor) => Ok([("tensor".to_string(), tensor_to_message(&tensor)?)].into()),
        Err(err) => Ok(error_output(&err.to_string())),
    }
}

#[actor(
    TensorCropRoiActor,
    inports::<100>(frame, roi),
    outports::<50>(frame, error),
    state(MemoryState),
    await_inports(frame, roi)
)]
pub async fn tensor_crop_roi_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let frame = match frame_from_context(&context, "frame").await {
        Ok(frame) => frame,
        Err(err) => return Ok(error_output(&err.to_string())),
    };
    let roi: Roi = match context
        .get_payload()
        .get("roi")
        .map(value_from_message_or_packet)
    {
        Some(Ok(roi)) => roi,
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => return Ok(error_output("Expected roi input")),
    };

    match crop_frame_to_roi(&frame, &roi) {
        Ok(cropped) => Ok([("frame".to_string(), frame_to_message(&cropped)?)].into()),
        Err(err) => Ok(error_output(&err.to_string())),
    }
}

#[actor(
    DetectionToRoiActor,
    inports::<100>(detections),
    outports::<50>(roi, error),
    state(MemoryState),
    await_inports(detections)
)]
pub async fn detection_to_roi_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let detections: DetectionSet = match context
        .get_payload()
        .get("detections")
        .map(value_from_message_or_packet)
    {
        Some(Ok(detections)) => detections,
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => return Ok(error_output("Expected detections input")),
    };
    let config = context.get_config_hashmap();
    let scale = f32_config(&config, "scale", 1.8);
    let square = bool_config(&config, "square", true);
    let fallback_center = bool_config(&config, "fallback_center", true);

    match detection_to_roi(&detections, scale, square, fallback_center) {
        Ok(roi) => Ok([("roi".to_string(), value_to_object_message(&roi)?)].into()),
        Err(err) => Ok(error_output(&err.to_string())),
    }
}

#[actor(
    TemporalSmootherActor,
    inports::<100>(landmarks),
    outports::<50>(landmarks, error),
    state(MemoryState),
    await_inports(landmarks)
)]
pub async fn temporal_smoother_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let landmarks: LandmarkSet = match context
        .get_payload()
        .get("landmarks")
        .map(value_from_message_or_packet)
    {
        Some(Ok(landmarks)) => landmarks,
        Some(Err(err)) => return Ok(error_output(&err.to_string())),
        None => return Ok(error_output("Expected landmarks input")),
    };
    let alpha = f32_config(&context.get_config_hashmap(), "alpha", 0.55).clamp(0.0, 1.0);
    let smoothed = smooth_landmarks(&context, landmarks, alpha);
    Ok([("landmarks".to_string(), value_to_object_message(&smoothed)?)].into())
}

#[derive(Clone)]
struct VideoStreamFrameOptions {
    width: Option<u32>,
    height: Option<u32>,
    format: Option<ImageFormat>,
    fps: f64,
    base_timestamp_micros: i64,
    max_frames: u64,
    strict_frame_size: bool,
    emit_done: bool,
    drop_policy: String,
    source: String,
}

impl VideoStreamFrameOptions {
    fn from_config(config: &HashMap<String, Value>, handle: &StreamHandle) -> Self {
        Self {
            width: optional_u32_config(config, "width"),
            height: optional_u32_config(config, "height"),
            format: config
                .get("format")
                .and_then(Value::as_str)
                .or(handle.content_type.as_deref())
                .map(ImageFormat::from_label),
            fps: f64_config(config, "fps", 30.0).max(0.001),
            base_timestamp_micros: i64_config(config, "timestampMicros", 0).max(i64_config(
                config,
                "startTimestampMicros",
                0,
            )),
            max_frames: u64_config(config, "maxFrames", 0),
            strict_frame_size: bool_config(config, "strictFrameSize", true),
            emit_done: bool_config(config, "emitDone", true),
            drop_policy: string_config(config, "dropPolicy", "block"),
            source: string_config(config, "source", "video-stream"),
        }
    }

    fn merge_begin_metadata(&mut self, metadata: Option<&Value>, content_type: Option<&str>) {
        if let Some(metadata) = metadata {
            self.width = metadata
                .get("width")
                .and_then(Value::as_u64)
                .map(|value| value as u32)
                .or(self.width);
            self.height = metadata
                .get("height")
                .and_then(Value::as_u64)
                .map(|value| value as u32)
                .or(self.height);
            self.format = metadata
                .get("format")
                .and_then(Value::as_str)
                .map(ImageFormat::from_label)
                .or_else(|| content_type.map(ImageFormat::from_label))
                .or_else(|| self.format.clone());
            self.fps = metadata
                .get("fps")
                .and_then(Value::as_f64)
                .unwrap_or(self.fps)
                .max(0.001);
            self.base_timestamp_micros = metadata
                .get("timestampMicros")
                .or_else(|| metadata.get("startTimestampMicros"))
                .and_then(Value::as_i64)
                .unwrap_or(self.base_timestamp_micros);
        } else if self.format.is_none() {
            self.format = content_type.map(ImageFormat::from_label);
        }
    }
}

async fn bridge_video_stream_to_frames(
    rx: flume::Receiver<StreamFrame>,
    out_sender: flume::Sender<HashMap<String, Message>>,
    handle: Arc<StreamHandle>,
    mut options: VideoStreamFrameOptions,
) {
    let mut frame_index = 0u64;
    let mut dropped_frames = 0u64;
    while let Ok(stream_frame) = rx.recv_async().await {
        match stream_frame {
            StreamFrame::Begin {
                content_type,
                metadata,
                ..
            } => {
                options.merge_begin_metadata(metadata.as_ref(), content_type.as_deref());
                let begin_metadata = json!({
                    "source": "videoStreamToFrames",
                    "streamId": handle.stream_id,
                    "width": options.width,
                    "height": options.height,
                    "format": options.format,
                    "fps": options.fps,
                    "contentType": content_type,
                    "streamMetadata": metadata,
                });
                if !send_bridge_packet(
                    &out_sender,
                    HashMap::from([(
                        "metadata".to_string(),
                        value_to_object_message(&begin_metadata).unwrap_or_else(error_message),
                    )]),
                    &options.drop_policy,
                )
                .await
                {
                    break;
                }
            }
            StreamFrame::Data(bytes) => {
                let Some(width) = options.width else {
                    let _ = send_stream_error(&out_sender, "video stream is missing width").await;
                    if options.strict_frame_size {
                        break;
                    }
                    continue;
                };
                let Some(height) = options.height else {
                    let _ = send_stream_error(&out_sender, "video stream is missing height").await;
                    if options.strict_frame_size {
                        break;
                    }
                    continue;
                };
                let format = options.format.clone().unwrap_or(ImageFormat::Rgba8);
                let bytes_per_frame = width as usize
                    * height as usize
                    * format.channels()
                    * format.bytes_per_channel();
                if bytes_per_frame == 0 {
                    let _ =
                        send_stream_error(&out_sender, "video frame dimensions are invalid").await;
                    break;
                }
                if bytes.len() % bytes_per_frame != 0 {
                    let _ = send_stream_error(
                        &out_sender,
                        &format!(
                            "video chunk has {} bytes, not a multiple of expected frame size {}",
                            bytes.len(),
                            bytes_per_frame
                        ),
                    )
                    .await;
                    if options.strict_frame_size {
                        break;
                    }
                    continue;
                }

                for frame_bytes in bytes.chunks(bytes_per_frame) {
                    if options.max_frames > 0 && frame_index >= options.max_frames {
                        break;
                    }
                    let timestamp = timestamp_for_frame(
                        options.base_timestamp_micros,
                        frame_index,
                        options.fps,
                    );
                    let mut frame =
                        VideoFrame::new(width, height, format.clone(), frame_bytes.to_vec());
                    frame.metadata =
                        frame_packet_metadata(&handle, &options, frame_index, timestamp);

                    let frame_message = match frame_to_message(&frame) {
                        Ok(message) => message,
                        Err(err) => {
                            let _ = send_stream_error(&out_sender, &err.to_string()).await;
                            continue;
                        }
                    };
                    let frame_meta = json!({
                        "frameIndex": frame_index,
                        "sequence": frame_index,
                        "timestampMicros": timestamp,
                        "streamId": handle.stream_id,
                        "width": width,
                        "height": height,
                        "format": format,
                    });
                    let packet = HashMap::from([
                        ("frame".to_string(), frame_message),
                        (
                            "metadata".to_string(),
                            value_to_object_message(&frame_meta).unwrap_or_else(error_message),
                        ),
                    ]);
                    if !send_bridge_packet(&out_sender, packet, &options.drop_policy).await {
                        dropped_frames += 1;
                    }
                    frame_index += 1;
                }
            }
            StreamFrame::End => {
                if options.emit_done {
                    let done = json!({
                        "done": true,
                        "streamId": handle.stream_id,
                        "frames": frame_index,
                        "droppedFrames": dropped_frames,
                    });
                    let _ = send_bridge_packet(
                        &out_sender,
                        HashMap::from([(
                            "done".to_string(),
                            value_to_object_message(&done).unwrap_or_else(error_message),
                        )]),
                        &options.drop_policy,
                    )
                    .await;
                }
                break;
            }
            StreamFrame::Error(err) => {
                let _ = send_stream_error(&out_sender, &err).await;
                break;
            }
        }
    }
}

fn frame_packet_metadata(
    handle: &StreamHandle,
    options: &VideoStreamFrameOptions,
    frame_index: u64,
    timestamp_micros: i64,
) -> PacketMetadata {
    let mut metadata = PacketMetadata {
        timestamp: Some(Timestamp::from_micros(timestamp_micros)),
        sequence: Some(frame_index),
        stream_id: Some(handle.stream_id.to_string()),
        source: Some(options.source.clone()),
        fields: HashMap::new(),
    };
    metadata
        .fields
        .insert("sourceStreamId".to_string(), json!(handle.stream_id));
    metadata
        .fields
        .insert("originActor".to_string(), json!(handle.origin_actor));
    metadata
        .fields
        .insert("originPort".to_string(), json!(handle.origin_port));
    metadata
        .fields
        .insert("frameIndex".to_string(), json!(frame_index));
    metadata
}

fn timestamp_for_frame(base_timestamp_micros: i64, frame_index: u64, fps: f64) -> i64 {
    base_timestamp_micros + ((frame_index as f64 * 1_000_000.0) / fps).round() as i64
}

async fn send_bridge_packet(
    out_sender: &flume::Sender<HashMap<String, Message>>,
    packet: HashMap<String, Message>,
    drop_policy: &str,
) -> bool {
    if drop_policy == "drop" || drop_policy == "latest" {
        out_sender.try_send(packet).is_ok()
    } else {
        out_sender.send_async(packet).await.is_ok()
    }
}

async fn send_stream_error(
    out_sender: &flume::Sender<HashMap<String, Message>>,
    message: &str,
) -> bool {
    out_sender.send_async(error_output(message)).await.is_ok()
}

fn error_message(err: anyhow::Error) -> Message {
    Message::Error(err.to_string().into())
}

async fn frame_from_context(context: &ActorContext, port: &str) -> Result<VideoFrame> {
    let payload = context.get_payload();
    match payload.get(port) {
        Some(Message::StreamHandle(_)) => {
            let rx = context
                .take_stream_receiver(port)
                .ok_or_else(|| anyhow!("No StreamHandle receiver on {port} port"))?;
            let (content_type, metadata, data) =
                stream_collect(rx).await.map_err(anyhow::Error::msg)?;
            frame_from_stream_parts(content_type, metadata, data)
        }
        Some(message) => message_to_frame(message),
        None => bail!("Expected frame input on {port}"),
    }
}

fn frame_from_stream_parts(
    content_type: Option<String>,
    metadata: Option<Value>,
    data: Vec<u8>,
) -> Result<VideoFrame> {
    let metadata_value = metadata.unwrap_or_else(|| json!({}));
    let width = metadata_value
        .get("width")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("stream frame metadata is missing width"))? as u32;
    let height = metadata_value
        .get("height")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("stream frame metadata is missing height"))? as u32;
    let format_label = metadata_value
        .get("format")
        .and_then(Value::as_str)
        .or(content_type.as_deref())
        .unwrap_or("rgba8");
    let mut frame = VideoFrame::new(width, height, ImageFormat::from_label(format_label), data);
    frame
        .metadata
        .fields
        .insert("streamMetadata".to_string(), metadata_value);
    Ok(frame)
}

fn image_to_tensor(
    frame: &VideoFrame,
    name: &str,
    dtype: TensorDType,
    layout: &str,
    out_channels: usize,
    scale: f32,
    offset: f32,
) -> Result<TensorPacket> {
    if dtype != TensorDType::F32 && dtype != TensorDType::U8 {
        bail!("ImageToTensor supports f32 and u8 outputs in V1");
    }
    let in_channels = frame.channels();
    let bpc = frame.format.bytes_per_channel();
    if bpc != 1 {
        bail!("ImageToTensor currently expects 8-bit image frames");
    }
    let out_channels = out_channels.clamp(1, 4);
    let pixel_count = frame.width as usize * frame.height as usize;
    let expected = pixel_count * in_channels;
    if frame.data.len() < expected {
        bail!(
            "frame has {} bytes, expected at least {} for {}x{} {:?}",
            frame.data.len(),
            expected,
            frame.width,
            frame.height,
            frame.format
        );
    }

    let shape = if layout == "nchw" {
        TensorShape::new([1, out_channels, frame.height as usize, frame.width as usize])
    } else {
        TensorShape::new([1, frame.height as usize, frame.width as usize, out_channels])
    };
    let mut metadata = frame.metadata.clone();
    metadata.fields.insert("layout".to_string(), json!(layout));
    metadata
        .fields
        .insert("sourceWidth".to_string(), json!(frame.width));
    metadata
        .fields
        .insert("sourceHeight".to_string(), json!(frame.height));

    match dtype {
        TensorDType::U8 => {
            let mut data = vec![0u8; pixel_count * out_channels];
            for y in 0..frame.height as usize {
                for x in 0..frame.width as usize {
                    let src = (y * frame.width as usize + x) * in_channels;
                    for c in 0..out_channels {
                        let value = read_u8_channel(&frame.data, src, in_channels, c);
                        let dst = tensor_offset(
                            layout,
                            x,
                            y,
                            c,
                            frame.width as usize,
                            frame.height as usize,
                            out_channels,
                        );
                        data[dst] = value;
                    }
                }
            }
            let mut tensor =
                TensorPacket::new(Some(name.to_string()), TensorDType::U8, shape, data);
            tensor.metadata = metadata;
            Ok(tensor)
        }
        TensorDType::F32 => {
            let mut values = vec![0.0f32; pixel_count * out_channels];
            for y in 0..frame.height as usize {
                for x in 0..frame.width as usize {
                    let src = (y * frame.width as usize + x) * in_channels;
                    for c in 0..out_channels {
                        let value = read_u8_channel(&frame.data, src, in_channels, c) as f32
                            * scale
                            + offset;
                        let dst = tensor_offset(
                            layout,
                            x,
                            y,
                            c,
                            frame.width as usize,
                            frame.height as usize,
                            out_channels,
                        );
                        values[dst] = value;
                    }
                }
            }
            let mut tensor = TensorPacket::from_f32(Some(name.to_string()), shape, &values);
            tensor.metadata = metadata;
            Ok(tensor)
        }
        _ => unreachable!(),
    }
}

fn resize_letterbox(
    frame: &VideoFrame,
    out_w: u32,
    out_h: u32,
    fill: u8,
) -> Result<(VideoFrame, Value)> {
    if out_w == 0 || out_h == 0 {
        bail!("letterbox dimensions must be non-zero");
    }
    let channels = frame.channels();
    let bpc = frame.format.bytes_per_channel();
    let pixel_bytes = channels * bpc;
    let src_row = frame.row_bytes();
    let scale = (out_w as f32 / frame.width as f32).min(out_h as f32 / frame.height as f32);
    let resized_w = ((frame.width as f32 * scale).round() as u32)
        .max(1)
        .min(out_w);
    let resized_h = ((frame.height as f32 * scale).round() as u32)
        .max(1)
        .min(out_h);
    let pad_x = ((out_w - resized_w) / 2) as usize;
    let pad_y = ((out_h - resized_h) / 2) as usize;
    let mut out = vec![fill; out_w as usize * out_h as usize * pixel_bytes];

    for y in 0..resized_h as usize {
        let sy = ((y as f32 / scale).floor() as usize).min(frame.height as usize - 1);
        for x in 0..resized_w as usize {
            let sx = ((x as f32 / scale).floor() as usize).min(frame.width as usize - 1);
            let src = sy * src_row + sx * pixel_bytes;
            let dst = ((y + pad_y) * out_w as usize + (x + pad_x)) * pixel_bytes;
            out[dst..dst + pixel_bytes].copy_from_slice(&frame.data[src..src + pixel_bytes]);
        }
    }

    let mut resized = VideoFrame::new(out_w, out_h, frame.format.clone(), out);
    resized.metadata = frame.metadata.clone();
    resized.metadata.fields.insert(
        "letterbox".to_string(),
        json!({
            "scale": scale,
            "resizedWidth": resized_w,
            "resizedHeight": resized_h,
            "padX": pad_x,
            "padY": pad_y,
        }),
    );
    let transform = resized.metadata.fields["letterbox"].clone();
    Ok((resized, transform))
}

fn normalize_tensor(
    tensor: &TensorPacket,
    scale: f32,
    offset: f32,
    channels: usize,
    mean: &[f32],
    std: &[f32],
) -> Result<TensorPacket> {
    let mut values = match tensor.dtype {
        TensorDType::F32 => tensor
            .as_f32_vec()
            .ok_or_else(|| anyhow!("invalid f32 tensor bytes"))?,
        TensorDType::U8 => tensor.data.iter().map(|v| *v as f32).collect(),
        other => bail!(
            "NormalizeTensor supports f32 and u8 tensors, got {:?}",
            other
        ),
    };

    for (idx, value) in values.iter_mut().enumerate() {
        let channel = idx % channels.max(1);
        let mean = mean.get(channel).copied().unwrap_or(0.0);
        let std = std.get(channel).copied().unwrap_or(1.0).max(1e-6);
        *value = ((*value * scale + offset) - mean) / std;
    }

    let mut out = TensorPacket::from_f32(tensor.name.clone(), tensor.shape.clone(), &values);
    out.metadata = tensor.metadata.clone();
    out.metadata
        .fields
        .insert("normalized".to_string(), json!(true));
    Ok(out)
}

fn crop_frame_to_roi(frame: &VideoFrame, roi: &Roi) -> Result<VideoFrame> {
    let channels = frame.channels();
    let bpc = frame.format.bytes_per_channel();
    let pixel_bytes = channels * bpc;
    let src_row = frame.row_bytes();
    let rect = roi.rect;
    let x0 = ((rect.center_x - rect.width * 0.5) * frame.width as f32)
        .floor()
        .max(0.0) as usize;
    let y0 = ((rect.center_y - rect.height * 0.5) * frame.height as f32)
        .floor()
        .max(0.0) as usize;
    let x1 = ((rect.center_x + rect.width * 0.5) * frame.width as f32)
        .ceil()
        .min(frame.width as f32) as usize;
    let y1 = ((rect.center_y + rect.height * 0.5) * frame.height as f32)
        .ceil()
        .min(frame.height as f32) as usize;
    if x1 <= x0 || y1 <= y0 {
        bail!("ROI does not overlap frame");
    }
    let width = x1 - x0;
    let height = y1 - y0;
    let mut out = vec![0u8; width * height * pixel_bytes];
    for y in 0..height {
        let src = (y0 + y) * src_row + x0 * pixel_bytes;
        let dst = y * width * pixel_bytes;
        out[dst..dst + width * pixel_bytes]
            .copy_from_slice(&frame.data[src..src + width * pixel_bytes]);
    }
    let mut cropped = VideoFrame::new(width as u32, height as u32, frame.format.clone(), out);
    cropped.metadata = frame.metadata.clone();
    cropped.metadata.merge_missing_from(&roi.metadata);
    cropped.metadata.fields.insert(
        "cropRoi".to_string(),
        json!({"x": x0, "y": y0, "width": width, "height": height}),
    );
    Ok(cropped)
}

fn detection_to_roi(
    detections: &DetectionSet,
    scale: f32,
    square: bool,
    fallback_center: bool,
) -> Result<Roi> {
    let detection = detections
        .detections
        .iter()
        .max_by(|a, b| a.score.total_cmp(&b.score));
    let Some(detection) = detection else {
        if fallback_center {
            let mut roi = Roi::default();
            roi.metadata.merge_missing_from(&detections.metadata);
            return Ok(roi);
        }
        bail!("no detections available for ROI");
    };

    let [x, y, mut width, mut height] = detection.bbox;
    width *= scale;
    height *= scale;
    if square {
        let side = width.max(height);
        width = side;
        height = side;
    }
    let mut roi = Roi {
        rect: reflow_media_types::NormalizedRect {
            center_x: (x + detection.bbox[2] * 0.5).clamp(0.0, 1.0),
            center_y: (y + detection.bbox[3] * 0.5).clamp(0.0, 1.0),
            width: width.clamp(0.01, 1.0),
            height: height.clamp(0.01, 1.0),
            rotation: 0.0,
        },
        source_size: None,
        score: Some(detection.score),
        metadata: detections.metadata.clone(),
    };
    roi.metadata
        .fields
        .insert("sourceDetection".to_string(), json!(detection));
    Ok(roi)
}

fn smooth_landmarks(context: &ActorContext, mut current: LandmarkSet, alpha: f32) -> LandmarkSet {
    let previous = {
        let state = context.get_state();
        let state = state.lock();
        state
            .as_any()
            .downcast_ref::<MemoryState>()
            .and_then(|memory| memory.get("previous_landmarks"))
            .and_then(|value| serde_json::from_value::<LandmarkSet>(value.clone()).ok())
    };

    if let Some(previous) = previous {
        for (cur, prev) in current.landmarks.iter_mut().zip(previous.landmarks.iter()) {
            cur.x = prev.x * (1.0 - alpha) + cur.x * alpha;
            cur.y = prev.y * (1.0 - alpha) + cur.y * alpha;
            cur.z = match (cur.z, prev.z) {
                (Some(c), Some(p)) => Some(p * (1.0 - alpha) + c * alpha),
                (current, _) => current,
            };
        }
    }

    {
        let state = context.get_state();
        let mut state = state.lock();
        if let Some(memory) = state.as_mut_any().downcast_mut::<MemoryState>() {
            memory.insert(
                "previous_landmarks",
                serde_json::to_value(&current).unwrap_or(Value::Null),
            );
        }
    }
    current
}

fn read_u8_channel(data: &[u8], src: usize, in_channels: usize, channel: usize) -> u8 {
    if in_channels == 1 {
        data[src]
    } else {
        data[src + channel.min(in_channels - 1)]
    }
}

fn tensor_offset(
    layout: &str,
    x: usize,
    y: usize,
    c: usize,
    width: usize,
    height: usize,
    channels: usize,
) -> usize {
    if layout == "nchw" {
        c * width * height + y * width + x
    } else {
        (y * width + x) * channels + c
    }
}

fn dtype_config(config: &HashMap<String, Value>, key: &str, default: TensorDType) -> TensorDType {
    match config
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase)
    {
        Some(value) if value == "u8" => TensorDType::U8,
        Some(value) if value == "f32" || value == "float32" => TensorDType::F32,
        _ => default,
    }
}

fn string_config(config: &HashMap<String, Value>, key: &str, default: &str) -> String {
    config
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn u32_config(config: &HashMap<String, Value>, key: &str, default: u32) -> u32 {
    config
        .get(key)
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(default)
}

fn optional_u32_config(config: &HashMap<String, Value>, key: &str) -> Option<u32> {
    config
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
}

fn u64_config(config: &HashMap<String, Value>, key: &str, default: u64) -> u64 {
    config.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn i64_config(config: &HashMap<String, Value>, key: &str, default: i64) -> i64 {
    config.get(key).and_then(Value::as_i64).unwrap_or(default)
}

fn usize_config(config: &HashMap<String, Value>, key: &str, default: usize) -> usize {
    config
        .get(key)
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(default)
}

fn u8_config(config: &HashMap<String, Value>, key: &str, default: u8) -> u8 {
    config
        .get(key)
        .and_then(Value::as_u64)
        .map(|v| v.min(u8::MAX as u64) as u8)
        .unwrap_or(default)
}

fn f32_config(config: &HashMap<String, Value>, key: &str, default: f32) -> f32 {
    config
        .get(key)
        .and_then(Value::as_f64)
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn f64_config(config: &HashMap<String, Value>, key: &str, default: f64) -> f64 {
    config.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn bool_config(config: &HashMap<String, Value>, key: &str, default: bool) -> bool {
    config.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn f32_vec_config(config: &HashMap<String, Value>, key: &str) -> Vec<f32> {
    config
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_f64)
                .map(|value| value as f32)
                .collect()
        })
        .unwrap_or_default()
}

fn error_output(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("error".to_string(), Message::Error(msg.to_string().into()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use reflow_actor::{
        stream::{StreamFrame, StreamHandle, STREAM_REGISTRY},
        ActorConfig,
    };
    use reflow_media_codec::{message_to_frame, message_to_tensor};
    use std::{sync::Arc, time::Duration};

    #[test]
    fn image_to_tensor_uses_nhwc_layout() {
        let frame = VideoFrame::new(1, 1, ImageFormat::Rgba8, vec![255, 128, 0, 255]);
        let tensor = image_to_tensor(
            &frame,
            "image",
            TensorDType::F32,
            "nhwc",
            3,
            1.0 / 255.0,
            0.0,
        )
        .unwrap();

        assert_eq!(tensor.shape.dims, vec![1, 1, 1, 3]);
        assert_eq!(tensor.as_f32_vec().unwrap(), vec![1.0, 128.0 / 255.0, 0.0]);
    }

    #[test]
    fn letterbox_preserves_aspect_ratio() {
        let frame = VideoFrame::new(2, 1, ImageFormat::Gray8, vec![10, 20]);
        let (resized, transform) = resize_letterbox(&frame, 4, 4, 0).unwrap();

        assert_eq!(resized.width, 4);
        assert_eq!(resized.height, 4);
        assert_eq!(transform["resizedWidth"], json!(4));
        assert_eq!(transform["resizedHeight"], json!(2));
    }

    #[test]
    fn normalize_converts_u8_to_f32() {
        let tensor = TensorPacket::new(
            Some("image".to_string()),
            TensorDType::U8,
            TensorShape::new([1, 2]),
            vec![0, 255],
        );

        let normalized = normalize_tensor(&tensor, 1.0 / 255.0, 0.0, 1, &[], &[]).unwrap();

        assert_eq!(normalized.dtype, TensorDType::F32);
        assert_eq!(normalized.as_f32_vec().unwrap(), vec![0.0, 1.0]);
    }

    #[test]
    fn tensor_message_output_is_decodable() {
        let frame = VideoFrame::new(1, 1, ImageFormat::Rgb8, vec![1, 2, 3]);
        let tensor =
            image_to_tensor(&frame, "image", TensorDType::U8, "nhwc", 3, 1.0, 0.0).unwrap();
        let message = tensor_to_message(&tensor).unwrap();

        assert_eq!(message_to_tensor(&message).unwrap().data, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn video_stream_to_frames_emits_timestamped_frame_packets() {
        let actor = VideoStreamToFramesActor::new();
        let inport = actor.get_inports().0;
        let outport = actor.get_outports().1;
        let process = tokio::spawn(actor.create_process(ActorConfig::default(), None));

        let (stream_id, tx) = STREAM_REGISTRY.create_stream(None);
        let stream_handle = StreamHandle {
            stream_id,
            origin_actor: "camera".to_string(),
            origin_port: "stream".to_string(),
            content_type: Some("video/raw-rgba".to_string()),
            size_hint: None,
        };

        inport
            .send_async(HashMap::from([(
                "stream".to_string(),
                Message::stream_handle(stream_handle),
            )]))
            .await
            .unwrap();
        tx.send_async(StreamFrame::Begin {
            content_type: Some("video/raw-rgba".to_string()),
            size_hint: None,
            metadata: Some(json!({
                "width": 2,
                "height": 1,
                "fps": 25.0,
                "timestampMicros": 1_000,
            })),
        })
        .await
        .unwrap();
        tx.send_async(StreamFrame::Data(Arc::new(vec![
            1, 2, 3, 255, 4, 5, 6, 255,
        ])))
        .await
        .unwrap();
        tx.send_async(StreamFrame::Data(Arc::new(vec![
            7, 8, 9, 255, 10, 11, 12, 255,
        ])))
        .await
        .unwrap();
        tx.send_async(StreamFrame::End).await.unwrap();

        let first_packet = recv_packet_with(&outport, "frame").await;
        let first = message_to_frame(first_packet.get("frame").unwrap()).unwrap();
        assert_eq!(first.width, 2);
        assert_eq!(first.height, 1);
        assert_eq!(first.metadata.sequence, Some(0));
        assert_eq!(
            first.metadata.timestamp,
            Some(Timestamp::from_micros(1_000))
        );
        assert_eq!(first.metadata.stream_id, Some(stream_id.to_string()));

        let second_packet = recv_packet_with(&outport, "frame").await;
        let second = message_to_frame(second_packet.get("frame").unwrap()).unwrap();
        assert_eq!(second.metadata.sequence, Some(1));
        assert_eq!(
            second.metadata.timestamp,
            Some(Timestamp::from_micros(41_000))
        );

        let done_packet = recv_packet_with(&outport, "done").await;
        assert!(done_packet.contains_key("done"));
        process.abort();
    }

    async fn recv_packet_with(
        outport: &flume::Receiver<HashMap<String, Message>>,
        port: &str,
    ) -> HashMap<String, Message> {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let packet = outport.recv_async().await.unwrap();
                if packet.contains_key(port) {
                    break packet;
                }
            }
        })
        .await
        .expect("timed out waiting for frame bridge output")
    }
}
