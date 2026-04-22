use actor_macro::actor;
use anyhow::{bail, Context, Result};
use reflow_asset_registry::{sha256_hex, validate_model_bytes, ModelManifest};
use reflow_litert::{InferenceOutput, TensorSpec};
use reflow_media_codec::{message_to_value, tensor_to_message, value_from_message_or_packet};
use reflow_media_types::{LandmarkSet, TensorDType, TensorPacket, TensorShape};
use reflow_ml_ops::{DecodeLandmarksActor, RunInferenceActor};
use reflow_network::{
    actor::{Actor, ActorBehavior, ActorContext, ActorLoad, MemoryState, Port},
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PALM_FILE: &str = "palm_detection_full.tflite";
const LANDMARK_FILE: &str = "hand_landmark_full.tflite";

#[actor(
    SyntheticTensorSourceActor,
    inports::<10>(Trigger),
    outports::<10>(Tensor)
)]
async fn synthetic_tensor_source_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    let config = context.get_config_hashmap();
    let name = string_config(&config, "name", "image");
    let width = usize_config(&config, "width", 224);
    let height = usize_config(&config, "height", 224);

    let tensor = synthetic_rgb_tensor(&name, width, height);
    println!(
        "[tensor:{name}] generated synthetic RGB tensor {:?}",
        tensor.shape.dims
    );

    Ok([("Tensor".to_string(), tensor_to_message(&tensor)?)].into())
}

#[actor(
    InferenceSummaryActor,
    inports::<10>(Inference),
    outports::<10>(Summary),
    await_inports(Inference)
)]
async fn inference_summary_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    let config = context.get_config_hashmap();
    let label = string_config(&config, "label", "inference");
    let message = context
        .get_payload()
        .get("Inference")
        .context("InferenceSummaryActor expected Inference input")?;
    let value = message_to_value(message)?;
    let output: InferenceOutput = serde_json::from_value(value)?;

    let mut lines = vec![format!(
        "{label}: backend={} tensors={}",
        output
            .metadata
            .get("backend")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        output.tensors.len()
    )];
    for tensor in &output.tensors {
        let preview = tensor
            .as_f32_vec()
            .unwrap_or_default()
            .into_iter()
            .take(6)
            .map(|value| format!("{value:.4}"))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "  {} {:?} {:?} [{}]",
            tensor.name.as_deref().unwrap_or("<unnamed>"),
            tensor.dtype,
            tensor.shape.dims,
            preview
        ));
    }

    let summary = lines.join("\n");
    println!("[summary]\n{summary}");
    Ok([("Summary".to_string(), Message::string(summary))].into())
}

#[actor(
    LandmarkPreviewActor,
    inports::<10>(Landmarks),
    outports::<10>(Summary),
    await_inports(Landmarks)
)]
async fn landmark_preview_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    let message = context
        .get_payload()
        .get("Landmarks")
        .context("LandmarkPreviewActor expected Landmarks input")?;
    let landmarks: LandmarkSet = value_from_message_or_packet(message)?;

    let mut lines = vec![format!("decoded landmarks: {}", landmarks.landmarks.len())];
    for (index, landmark) in landmarks.landmarks.iter().take(5).enumerate() {
        lines.push(format!(
            "  {index:02}: x={:.3}, y={:.3}, z={:.3}",
            landmark.x,
            landmark.y,
            landmark.z.unwrap_or(0.0)
        ));
    }

    let summary = lines.join("\n");
    println!("[landmarks]\n{summary}");
    Ok([("Summary".to_string(), Message::string(summary))].into())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = litert::set_global_log_severity(litert::LogSeverity::Error);

    let options = Options::parse()?;
    let models_dir = options.models_dir();

    println!("=== Reflow LiteRT Hand Landmark Network Demo ===");
    println!("models: {}", models_dir.display());
    println!(
        "Pipeline: IIP model bytes + SyntheticTensorSourceActor -> RunInferenceActor -> DecodeLandmarksActor -> summary actors\n"
    );

    let palm_bytes = read_model(&models_dir, PALM_FILE)?;
    let landmark_bytes = read_model(&models_dir, LANDMARK_FILE)?;
    let palm_manifest = palm_manifest();
    let landmark_manifest = landmark_manifest();

    validate_model("palm detector", &palm_manifest, &palm_bytes)?;
    validate_model("hand landmark", &landmark_manifest, &landmark_bytes)?;

    let mut network = Network::new(NetworkConfig::default());
    network.register_actor("synthetic_tensor_source", SyntheticTensorSourceActor::new())?;
    network.register_actor("run_inference", RunInferenceActor::new())?;
    network.register_actor("decode_landmarks", DecodeLandmarksActor::new())?;
    network.register_actor("inference_summary", InferenceSummaryActor::new())?;
    network.register_actor("landmark_preview", LandmarkPreviewActor::new())?;

    network.add_node(
        "palm_tensor",
        "synthetic_tensor_source",
        config(json!({
            "name": "image",
            "width": 192,
            "height": 192
        })),
    )?;
    network.add_node(
        "palm_infer",
        "run_inference",
        config(json!({
            "model": palm_manifest
        })),
    )?;
    network.add_node(
        "palm_summary",
        "inference_summary",
        config(json!({ "label": "palm detector" })),
    )?;

    network.add_node(
        "landmark_tensor",
        "synthetic_tensor_source",
        config(json!({
            "name": "roi_image",
            "width": 224,
            "height": 224
        })),
    )?;
    network.add_node(
        "landmark_infer",
        "run_inference",
        config(json!({
            "model": landmark_manifest
        })),
    )?;
    network.add_node(
        "landmark_summary",
        "inference_summary",
        config(json!({ "label": "hand landmark" })),
    )?;
    network.add_node(
        "landmark_decode",
        "decode_landmarks",
        config(json!({
            "values_per_landmark": 3,
            "max_landmarks": 21
        })),
    )?;
    network.add_node("landmark_preview", "landmark_preview", None)?;

    network.add_connection(wire("palm_tensor", "Tensor", "palm_infer", "tensor"));
    network.add_connection(wire("palm_infer", "inference", "palm_summary", "Inference"));

    network.add_connection(wire(
        "landmark_tensor",
        "Tensor",
        "landmark_infer",
        "tensor",
    ));
    network.add_connection(wire(
        "landmark_infer",
        "inference",
        "landmark_summary",
        "Inference",
    ));
    network.add_connection(wire(
        "landmark_infer",
        "tensor",
        "landmark_decode",
        "tensor",
    ));
    network.add_connection(wire(
        "landmark_decode",
        "landmarks",
        "landmark_preview",
        "Landmarks",
    ));

    // Send model bytes first. RunInferenceActor caches them until the tensor
    // arrives because it selectively awaits the `tensor` inport.
    network.add_initial(iip("palm_infer", "model_data", Message::bytes(palm_bytes)));
    network.add_initial(iip(
        "landmark_infer",
        "model_data",
        Message::bytes(landmark_bytes),
    ));
    network.add_initial(iip("palm_tensor", "Trigger", Message::Flow));
    network.add_initial(iip("landmark_tensor", "Trigger", Message::Flow));

    network.start()?;
    let summaries = wait_for_summaries(
        &network,
        &["palm_summary", "landmark_summary", "landmark_preview"],
        Duration::from_secs(20),
    )
    .await?;

    println!("\n=== Terminal Actor Outputs ===");
    for (actor, summary) in summaries {
        println!("[{actor}]\n{summary}\n");
    }

    network.shutdown();
    Ok(())
}

#[derive(Debug, Clone)]
struct Options {
    models_dir: Option<PathBuf>,
}

impl Options {
    fn parse() -> Result<Self> {
        let mut models_dir = None;
        let mut args = env::args().skip(1);
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--models-dir" => {
                    let value = args
                        .next()
                        .context("--models-dir expects a path argument")?;
                    models_dir = Some(PathBuf::from(value));
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => bail!("unknown argument '{other}'"),
            }
        }
        Ok(Self { models_dir })
    }

    fn models_dir(&self) -> PathBuf {
        self.models_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models"))
    }
}

fn print_help() {
    println!("Usage:");
    println!(
        "  cargo run --manifest-path examples/ml_hand_landmark_demo/Cargo.toml -- --models-dir examples/ml_hand_landmark_demo/models"
    );
    println!();
    println!("Before running, fetch models with:");
    println!("  examples/ml_hand_landmark_demo/scripts/fetch_models.sh");
}

fn read_model(models_dir: &Path, file_name: &str) -> Result<Vec<u8>> {
    let path = models_dir.join(file_name);
    fs::read(&path).with_context(|| {
        format!(
            "missing model '{}'. Run examples/ml_hand_landmark_demo/scripts/fetch_models.sh first",
            path.display()
        )
    })
}

fn validate_model(label: &str, manifest: &ModelManifest, bytes: &[u8]) -> Result<()> {
    validate_model_bytes(manifest, bytes)?;
    println!(
        "{label}: {} bytes, sha256 {}",
        bytes.len(),
        sha256_hex(bytes)
    );
    Ok(())
}

async fn wait_for_summaries(
    network: &Network,
    actors: &[&str],
    timeout: Duration,
) -> Result<Vec<(String, String)>> {
    let mut summaries = HashMap::<String, String>::new();
    let started = Instant::now();

    while summaries.len() < actors.len() {
        if started.elapsed() > timeout {
            bail!(
                "timed out waiting for terminal actor summaries; received {} of {}",
                summaries.len(),
                actors.len()
            );
        }

        for actor in actors {
            if summaries.contains_key(*actor) {
                continue;
            }
            for (port, message) in network.read_actor_output(actor) {
                if port != "Summary" {
                    continue;
                }
                let summary = match message {
                    Message::String(text) => text.as_str().to_string(),
                    other => format!("{other:?}"),
                };
                summaries.insert((*actor).to_string(), summary);
            }
        }

        tokio::time::sleep(Duration::from_millis(25)).await;
    }

    Ok(actors
        .iter()
        .filter_map(|actor| {
            summaries
                .remove(*actor)
                .map(|summary| ((*actor).to_string(), summary))
        })
        .collect())
}

fn synthetic_rgb_tensor(name: &str, width: usize, height: usize) -> TensorPacket {
    let mut values = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let fx = x as f32 / (width.saturating_sub(1).max(1)) as f32;
            let fy = y as f32 / (height.saturating_sub(1).max(1)) as f32;
            let dx = fx - 0.5;
            let dy = fy - 0.48;
            let palm_like_blob = (-(dx * dx * 18.0 + dy * dy * 12.0)).exp();
            let edge_falloff = (1.0 - (dx.abs() + dy.abs()).min(1.0)).max(0.0);
            values.push((0.12 + 0.70 * palm_like_blob + 0.08 * edge_falloff).clamp(0.0, 1.0));
            values.push((0.10 + 0.58 * palm_like_blob + 0.14 * fy).clamp(0.0, 1.0));
            values.push((0.08 + 0.45 * palm_like_blob + 0.18 * fx).clamp(0.0, 1.0));
        }
    }
    TensorPacket::from_f32(
        Some(name.to_string()),
        TensorShape::new([1, height, width, 3]),
        &values,
    )
}

fn config(value: Value) -> Option<HashMap<String, Value>> {
    value.as_object().map(|object| {
        object
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    })
}

fn wire(from_actor: &str, from_port: &str, to_actor: &str, to_port: &str) -> Connector {
    Connector {
        from: ConnectionPoint {
            actor: from_actor.to_string(),
            port: from_port.to_string(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: to_actor.to_string(),
            port: to_port.to_string(),
            ..Default::default()
        },
    }
}

fn iip(actor: &str, port: &str, data: Message) -> InitialPacket {
    InitialPacket {
        to: ConnectionPoint {
            actor: actor.to_string(),
            port: port.to_string(),
            initial_data: Some(data),
        },
    }
}

fn string_config(config: &HashMap<String, Value>, key: &str, default: &str) -> String {
    config
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn usize_config(config: &HashMap<String, Value>, key: &str, default: usize) -> usize {
    config
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
}

fn palm_manifest() -> ModelManifest {
    ModelManifest {
        model_id: "mediapipe-palm-detection-full".to_string(),
        task_kind: "palm_detection".to_string(),
        backend: "litert".to_string(),
        asset_id: Some("mediapipe:palm_detection_full".to_string()),
        input_specs: vec![TensorSpec {
            name: "image".to_string(),
            dtype: TensorDType::F32,
            shape: TensorShape::new([1, 192, 192, 3]),
        }],
        output_specs: vec![
            TensorSpec {
                name: "boxes".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 2016, 18]),
            },
            TensorSpec {
                name: "scores".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 2016, 1]),
            },
        ],
        license: "Apache-2.0".to_string(),
        source_url: "https://storage.googleapis.com/mediapipe-assets/palm_detection_full.tflite"
            .to_string(),
        checksum_sha256: "1b14e9422c6ad006cde6581a46c8b90dd573c07ab7f3934b5589e7cea3f89a54"
            .to_string(),
        attribution_required: true,
        tags: vec![
            "mediapipe".to_string(),
            "hand".to_string(),
            "palm".to_string(),
            "detector".to_string(),
            "litert".to_string(),
        ],
        metadata: HashMap::from([
            ("inputColorSpace".to_string(), json!("rgb")),
            ("inputValueRange".to_string(), json!([0.0, 1.0])),
            ("outputOrder".to_string(), json!(["boxes", "scores"])),
        ]),
    }
}

fn landmark_manifest() -> ModelManifest {
    ModelManifest {
        model_id: "mediapipe-hand-landmark-full".to_string(),
        task_kind: "hand_landmark".to_string(),
        backend: "litert".to_string(),
        asset_id: Some("mediapipe:hand_landmark_full".to_string()),
        input_specs: vec![TensorSpec {
            name: "roi_image".to_string(),
            dtype: TensorDType::F32,
            shape: TensorShape::new([1, 224, 224, 3]),
        }],
        output_specs: vec![
            TensorSpec {
                name: "landmarks".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 63]),
            },
            TensorSpec {
                name: "hand_presence".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 1]),
            },
            TensorSpec {
                name: "handedness".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 1]),
            },
            TensorSpec {
                name: "world_landmarks".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 63]),
            },
        ],
        license: "Apache-2.0".to_string(),
        source_url: "https://storage.googleapis.com/mediapipe-assets/hand_landmark_full.tflite"
            .to_string(),
        checksum_sha256: "11c272b891e1a99ab034208e23937a8008388cf11ed2a9d776ed3d01d0ba00e3"
            .to_string(),
        attribution_required: true,
        tags: vec![
            "mediapipe".to_string(),
            "hand".to_string(),
            "landmark".to_string(),
            "tracker".to_string(),
            "litert".to_string(),
        ],
        metadata: HashMap::from([
            ("inputColorSpace".to_string(), json!("rgb")),
            ("inputValueRange".to_string(), json!([0.0, 1.0])),
            ("landmarkCount".to_string(), json!(21)),
            (
                "landmarkCoordinateSpace".to_string(),
                json!("model_input_pixels"),
            ),
            (
                "outputOrder".to_string(),
                json!([
                    "landmarks",
                    "hand_presence",
                    "handedness",
                    "world_landmarks"
                ]),
            ),
        ]),
    }
}
