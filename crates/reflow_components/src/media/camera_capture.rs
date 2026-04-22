//! Camera capture source actor.
//!
//! V1 establishes the graph-facing contract for camera frames without binding
//! Reflow's default build to a native camera SDK. The default backend is a
//! deterministic test-pattern source; enabling `camera-native` adds a Nokhwa
//! native backend that feeds the same `video/raw-rgba` stream contract.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{
    message::EncodableValue,
    stream::{spawn_stream_task, StreamFrame, STREAM_REGISTRY},
    ActorContext,
};
use reflow_actor_macro::actor;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
use nokhwa::{
    pixel_format::RgbFormat,
    utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType},
    Camera,
};

static CAPTURE_CANCEL: OnceLock<parking_lot::Mutex<HashMap<u64, Arc<AtomicBool>>>> =
    OnceLock::new();

#[actor(
    CameraCaptureActor,
    inports::<10>(start, stop),
    outports::<50>(stream, metadata, error),
    state(MemoryState)
)]
pub async fn camera_capture_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    if payload.get("stop").is_some_and(message_truthy) {
        return Ok(stop_active_capture(&context));
    }

    let config = context.get_config_hashmap();
    let backend = string_config(&config, "backend", "mock").to_ascii_lowercase();
    if is_native_backend(&backend) {
        #[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
        {
            return start_native_capture(context, &config, backend);
        }

        #[cfg(not(all(feature = "camera-native", not(target_arch = "wasm32"))))]
        {
            return Ok(error_output(format!(
                "Camera backend '{}' is not available in this build. Use backend='mock' or provide a native capture adapter.",
                backend
            )));
        }
    }
    if !is_mock_backend(&backend) {
        return Ok(error_output(format!(
            "Camera backend '{}' is not available in this build. Use backend='mock'{}.",
            backend,
            native_backend_hint()
        )));
    }

    let width = u32_config(&config, "width", 640).max(1);
    let height = u32_config(&config, "height", 480).max(1);
    let fps = u32_config(&config, "fps", 30).max(1);
    let frame_count = u64_config(&config, "frameCount", 1);
    let buffer_size = usize_config(&config, "bufferSize", 4).max(1);
    let device_id = string_config(&config, "deviceId", "mock-camera");
    let bytes_per_frame = width as u64 * height as u64 * 4;
    let size_hint = (frame_count > 0).then_some(bytes_per_frame.saturating_mul(frame_count));
    let (tx, handle) = context.create_stream(
        "stream",
        Some("video/raw-rgba".to_string()),
        size_hint,
        Some(buffer_size),
    );
    let stream_id = handle.stream_id;
    let cancel = Arc::new(AtomicBool::new(false));
    capture_cancel_registry()
        .lock()
        .insert(stream_id, cancel.clone());
    context.pool_upsert("_camera_capture", "stream_id", json!(stream_id));

    let metadata = camera_metadata(&backend, &device_id, width, height, fps, frame_count);
    let stream_metadata = metadata.clone();

    spawn_stream_task(async move {
        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type: Some("video/raw-rgba".to_string()),
                size_hint,
                metadata: Some(stream_metadata),
            })
            .await;

        let frame_delay = Duration::from_secs_f64(1.0 / fps as f64);
        let mut frame_index = 0;
        while !cancel.load(Ordering::Relaxed) && (frame_count == 0 || frame_index < frame_count) {
            let frame = mock_camera_frame(width, height, frame_index);
            if tx
                .send_async(StreamFrame::Data(Arc::new(frame)))
                .await
                .is_err()
            {
                break;
            }
            frame_index += 1;
            if frame_count == 0 || frame_index < frame_count {
                tokio::time::sleep(frame_delay).await;
            }
        }

        let _ = tx.send_async(StreamFrame::End).await;
        capture_cancel_registry().lock().remove(&stream_id);
    });

    Ok([
        ("stream".to_string(), Message::stream_handle(handle)),
        (
            "metadata".to_string(),
            Message::object(EncodableValue::from(metadata)),
        ),
    ]
    .into())
}

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
fn start_native_capture(
    context: ActorContext,
    config: &HashMap<String, Value>,
    backend: String,
) -> Result<HashMap<String, Message>, Error> {
    let width = u32_config(config, "width", 640).max(1);
    let height = u32_config(config, "height", 480).max(1);
    let fps = u32_config(config, "fps", 30).max(1);
    let frame_count = u64_config(config, "frameCount", 0);
    let buffer_size = usize_config(config, "bufferSize", 4).max(1);
    let device_id = string_config(config, "deviceId", "0");
    let bytes_per_frame = width as u64 * height as u64 * 4;
    let size_hint = (frame_count > 0).then_some(bytes_per_frame.saturating_mul(frame_count));
    let (tx, handle) = context.create_stream(
        "stream",
        Some("video/raw-rgba".to_string()),
        size_hint,
        Some(buffer_size),
    );
    let stream_id = handle.stream_id;
    let cancel = Arc::new(AtomicBool::new(false));
    capture_cancel_registry()
        .lock()
        .insert(stream_id, cancel.clone());
    context.pool_upsert("_camera_capture", "stream_id", json!(stream_id));

    let metadata = camera_metadata(&backend, &device_id, width, height, fps, frame_count);
    let options = NativeCaptureOptions {
        backend,
        device_id,
        width,
        height,
        fps,
        frame_count,
        size_hint,
    };
    spawn_native_capture_loop(tx, stream_id, cancel, options);

    Ok([
        ("stream".to_string(), Message::stream_handle(handle)),
        (
            "metadata".to_string(),
            Message::object(EncodableValue::from(metadata)),
        ),
    ]
    .into())
}

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
#[derive(Debug, Clone)]
struct NativeCaptureOptions {
    backend: String,
    device_id: String,
    width: u32,
    height: u32,
    fps: u32,
    frame_count: u64,
    size_hint: Option<u64>,
}

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
fn spawn_native_capture_loop(
    tx: flume::Sender<StreamFrame>,
    stream_id: u64,
    cancel: Arc<AtomicBool>,
    options: NativeCaptureOptions,
) {
    std::thread::spawn(move || {
        let result = run_native_capture_loop(&tx, &cancel, &options);
        if let Err(err) = result {
            let _ = tx.send(StreamFrame::Error(err.to_string()));
        } else {
            let _ = tx.send(StreamFrame::End);
        }
        capture_cancel_registry().lock().remove(&stream_id);
    });
}

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
fn run_native_capture_loop(
    tx: &flume::Sender<StreamFrame>,
    cancel: &AtomicBool,
    options: &NativeCaptureOptions,
) -> Result<()> {
    let mut camera = Camera::new(
        native_camera_index(&options.device_id),
        requested_format(options),
    )
    .map_err(|err| anyhow::anyhow!("open native camera: {err}"))?;
    camera
        .open_stream()
        .map_err(|err| anyhow::anyhow!("start native camera stream: {err}"))?;
    let actual = camera.camera_format();
    let resolution = actual.resolution();
    let width = resolution.width();
    let height = resolution.height();
    let fps = actual.frame_rate();
    let begin_metadata = json!({
        "source": "camera",
        "backend": options.backend,
        "deviceId": options.device_id,
        "width": width,
        "height": height,
        "fps": fps,
        "format": "rgba8",
        "contentType": "video/raw-rgba",
        "frameCount": options.frame_count,
        "nativeFrameFormat": format!("{:?}", actual.format()),
        "timestampMicros": now_micros(),
    });
    tx.send(StreamFrame::Begin {
        content_type: Some("video/raw-rgba".to_string()),
        size_hint: options.size_hint,
        metadata: Some(begin_metadata),
    })
    .map_err(|err| anyhow::anyhow!("send native camera stream begin: {err}"))?;

    let frame_delay = Duration::from_secs_f64(1.0 / fps.max(1) as f64);
    let mut frame_index = 0;
    while !cancel.load(Ordering::Relaxed)
        && (options.frame_count == 0 || frame_index < options.frame_count)
    {
        let frame_start = std::time::Instant::now();
        let frame = camera
            .frame()
            .map_err(|err| anyhow::anyhow!("capture native camera frame: {err}"))?;
        let rgb = frame
            .decode_image::<RgbFormat>()
            .map_err(|err| anyhow::anyhow!("decode native camera frame: {err}"))?
            .into_raw();
        let rgba = rgb_to_rgba(&rgb);
        tx.send(StreamFrame::Data(Arc::new(rgba)))
            .map_err(|err| anyhow::anyhow!("send native camera frame: {err}"))?;
        frame_index += 1;

        if options.frame_count == 0 || frame_index < options.frame_count {
            let elapsed = frame_start.elapsed();
            if elapsed < frame_delay {
                std::thread::sleep(frame_delay - elapsed);
            }
        }
    }

    let _ = camera.stop_stream();
    Ok(())
}

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
fn native_camera_index(device_id: &str) -> CameraIndex {
    device_id
        .parse::<u32>()
        .map(CameraIndex::Index)
        .unwrap_or_else(|_| CameraIndex::String(device_id.to_string()))
}

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
fn requested_format(options: &NativeCaptureOptions) -> RequestedFormat<'static> {
    let format = CameraFormat::new_from(
        options.width,
        options.height,
        FrameFormat::MJPEG,
        options.fps,
    );
    RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(format))
}

#[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
fn rgb_to_rgba(rgb: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
    for pixel in rgb.chunks_exact(3) {
        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
    }
    rgba
}

fn capture_cancel_registry() -> &'static parking_lot::Mutex<HashMap<u64, Arc<AtomicBool>>> {
    CAPTURE_CANCEL.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

fn stop_active_capture(context: &ActorContext) -> HashMap<String, Message> {
    let state: HashMap<String, Value> = context.get_pool("_camera_capture").into_iter().collect();
    let Some(stream_id) = state.get("stream_id").and_then(Value::as_u64) else {
        return [(
            "metadata".to_string(),
            Message::object(EncodableValue::from(json!({"stopped": false}))),
        )]
        .into();
    };

    if let Some(cancel) = capture_cancel_registry().lock().remove(&stream_id) {
        cancel.store(true, Ordering::Relaxed);
    }
    if let Some(tx) = STREAM_REGISTRY.clone_sender(stream_id) {
        let _ = tx.send(StreamFrame::End);
    }
    context.pool_clear("_camera_capture");

    [(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "stopped": true,
            "streamId": stream_id,
        }))),
    )]
    .into()
}

fn camera_metadata(
    backend: &str,
    device_id: &str,
    width: u32,
    height: u32,
    fps: u32,
    frame_count: u64,
) -> Value {
    json!({
        "source": "camera",
        "backend": backend,
        "deviceId": device_id,
        "width": width,
        "height": height,
        "fps": fps,
        "format": "rgba8",
        "contentType": "video/raw-rgba",
        "frameCount": frame_count,
        "timestampMicros": now_micros(),
    })
}

fn mock_camera_frame(width: u32, height: u32, frame_index: u64) -> Vec<u8> {
    let mut data = Vec::with_capacity(width as usize * height as usize * 4);
    let phase = (frame_index % 255) as u32;
    for y in 0..height {
        for x in 0..width {
            let r = ((x * 255 / width + phase) % 255) as u8;
            let g = ((y * 255 / height + phase * 2) % 255) as u8;
            let checker = ((x / 32 + y / 32 + frame_index as u32) % 2) as u8;
            let b = if checker == 0 { 210 } else { 90 };
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    data
}

fn is_mock_backend(backend: &str) -> bool {
    matches!(backend, "mock" | "test-pattern" | "test_pattern")
}

fn is_native_backend(backend: &str) -> bool {
    matches!(backend, "native" | "nokhwa" | "auto")
}

fn native_backend_hint() -> &'static str {
    #[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
    {
        " or backend='native'"
    }
    #[cfg(not(all(feature = "camera-native", not(target_arch = "wasm32"))))]
    {
        " or enable the camera-native feature for backend='native'"
    }
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

fn message_truthy(message: &Message) -> bool {
    match message {
        Message::Flow => true,
        Message::Boolean(value) => *value,
        Message::Integer(value) => *value != 0,
        Message::Float(value) => *value != 0.0,
        Message::String(value) => !value.is_empty() && value.as_str() != "false",
        _ => false,
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
        .map(|value| value as u32)
        .unwrap_or(default)
}

fn u64_config(config: &HashMap<String, Value>, key: &str, default: u64) -> u64 {
    config.get(key).and_then(Value::as_u64).unwrap_or(default)
}

fn usize_config(config: &HashMap<String, Value>, key: &str, default: usize) -> usize {
    config
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn error_output(msg: String) -> HashMap<String, Message> {
    [("error".to_string(), Message::Error(msg.into()))].into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Actor;
    use parking_lot::Mutex;
    use reflow_actor::{ActorConfig, ActorLoad, MemoryState};
    use reflow_graph::types::GraphNode;

    fn test_context(config: HashMap<String, Value>) -> ActorContext {
        let node = GraphNode {
            id: "camera".to_string(),
            component: "CameraCaptureActor".to_string(),
            metadata: Some(config.clone()),
        };
        ActorContext::new(
            HashMap::from([("start".to_string(), Message::Flow)]),
            flume::unbounded(),
            Arc::new(Mutex::new(MemoryState::default())),
            ActorConfig {
                node,
                resolved_env: HashMap::new(),
                config,
                namespace: None,
                inport_connection_counts: HashMap::new(),
            },
            Arc::new(ActorLoad::new(0)),
        )
    }

    #[tokio::test]
    async fn mock_camera_capture_emits_raw_rgba_stream() {
        let actor = CameraCaptureActor::new();
        let behavior = actor.get_behavior();
        let output = behavior(test_context(HashMap::from([
            ("width".to_string(), json!(8)),
            ("height".to_string(), json!(4)),
            ("fps".to_string(), json!(60)),
            ("frameCount".to_string(), json!(1)),
        ])))
        .await
        .unwrap();

        let Message::StreamHandle(handle) = output.get("stream").unwrap() else {
            panic!("camera capture should output a stream handle");
        };
        let rx = STREAM_REGISTRY
            .take_receiver(handle.stream_id)
            .expect("camera stream receiver should exist");

        match rx.recv_async().await.unwrap() {
            StreamFrame::Begin {
                content_type,
                metadata,
                ..
            } => {
                assert_eq!(content_type.as_deref(), Some("video/raw-rgba"));
                assert_eq!(metadata.unwrap()["width"], json!(8));
            }
            other => panic!("expected Begin frame, got {:?}", other),
        }
        match rx.recv_async().await.unwrap() {
            StreamFrame::Data(frame) => assert_eq!(frame.len(), 8 * 4 * 4),
            other => panic!("expected Data frame, got {:?}", other),
        }
        assert!(matches!(rx.recv_async().await.unwrap(), StreamFrame::End));
    }

    #[test]
    fn mock_camera_frame_changes_over_time() {
        let first = mock_camera_frame(4, 4, 0);
        let second = mock_camera_frame(4, 4, 1);
        assert_eq!(first.len(), 4 * 4 * 4);
        assert_ne!(first, second);
    }

    #[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
    #[test]
    fn native_device_id_accepts_index_or_string() {
        assert_eq!(native_camera_index("2").as_index().unwrap(), 2);
        assert!(native_camera_index("camera://demo").is_string());
    }

    #[cfg(all(feature = "camera-native", not(target_arch = "wasm32")))]
    #[test]
    fn rgb_to_rgba_adds_opaque_alpha() {
        assert_eq!(
            rgb_to_rgba(&[1, 2, 3, 4, 5, 6]),
            vec![1, 2, 3, 255, 4, 5, 6, 255]
        );
    }
}
