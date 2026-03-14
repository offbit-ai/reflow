//! # Image Pipeline Streaming Example
//!
//! Demonstrates actor-to-actor binary data streaming through a Reflow network.
//!
//! ## Pipeline
//!
//! ```text
//! ImageSource ──StreamHandle──> GrayscaleFilter ──StreamHandle──> ImageSink
//!                                                                    │
//!                                                              Logger (stats)
//! ```
//!
//! Each actor uses `ActorContext::create_stream()` to open a bounded
//! side-channel and passes the lightweight `StreamHandle` through the
//! normal connector path. The downstream actor calls
//! `ActorContext::take_stream_receiver()` to obtain the `flume::Receiver`
//! and reads `StreamFrame`s asynchronously with backpressure.
//!
//! No real image codec is used — raw pixel bytes are synthesized to keep
//! dependencies minimal. The pattern applies equally to JPEG, PNG, video
//! frames, audio buffers, or any large binary payload.

use std::collections::HashMap;
use std::sync::Arc;

use actor_macro::actor;
use reflow_network::{
    actor::{Actor, ActorBehavior, ActorContext, ActorLoad, MemoryState, Port},
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};

// ── Actors ──────────────────────────────────────────────────────────

/// Generates a synthetic RGBA image as a stream of chunked pixel rows.
///
/// Creates a StreamHandle and returns it immediately through the outport.
/// The actual pixel data is pushed in a background task so the connector
/// can deliver the handle to the consumer before frames start flowing.
#[actor(
    ImageSourceActor,
    inports::<10>(Trigger),
    outports::<10>(ImageOut)
)]
async fn image_source_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    use reflow_actor::stream::StreamFrame;

    let width: usize = 256;
    let height: usize = 256;
    let channels: usize = 4; // RGBA
    let row_bytes = width * channels;
    let total_bytes = row_bytes * height;

    println!("[ImageSource] Generating {}x{} RGBA image ({} bytes)", width, height, total_bytes);

    let (tx, handle) = context.create_stream(
        "ImageOut",
        Some("image/raw-rgba".into()),
        Some(total_bytes as u64),
        Some(16), // buffer 16 rows before backpressure
    );

    // Spawn frame-pushing in the background so the handle can be
    // delivered through the connector before we fill the buffer.
    tokio::spawn(async move {
        let _ = tx
            .send_async(StreamFrame::Begin {
                content_type: Some("image/raw-rgba".into()),
                size_hint: Some(total_bytes as u64),
                metadata: Some(serde_json::json!({
                    "width": width,
                    "height": height,
                    "channels": channels,
                    "format": "RGBA8"
                })),
            })
            .await;

        for y in 0..height {
            let mut row = Vec::with_capacity(row_bytes);
            for x in 0..width {
                let r = (x % 256) as u8;
                let g = (y % 256) as u8;
                let b = ((x + y) % 256) as u8;
                let a = 255u8;
                row.extend_from_slice(&[r, g, b, a]);
            }
            if tx.send_async(StreamFrame::Data(Arc::new(row))).await.is_err() {
                return;
            }
        }

        let _ = tx.send_async(StreamFrame::End).await;
        println!("[ImageSource] Finished streaming {} rows", height);
    });

    Ok([("ImageOut".to_owned(), Message::stream_handle(handle))].into())
}

/// Reads an incoming image stream, converts each RGBA row to grayscale
/// (single-channel luminance), and re-streams the result downstream.
///
/// Demonstrates a streaming transform: the actor consumes one stream
/// and produces another without ever holding the full image in memory.
#[actor(
    GrayscaleFilterActor,
    inports::<10>(ImageIn),
    outports::<10>(ImageOut)
)]
async fn grayscale_filter_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    use reflow_actor::stream::StreamFrame;

    let in_rx = context
        .take_stream_receiver("ImageIn")
        .expect("[GrayscaleFilter] expected stream on ImageIn");

    // Parse metadata from Begin frame
    let (width, height) = match in_rx.recv_async().await? {
        StreamFrame::Begin { metadata, .. } => {
            let meta = metadata.unwrap_or_default();
            let w = meta.get("width").and_then(|v| v.as_u64()).unwrap_or(256) as usize;
            let h = meta.get("height").and_then(|v| v.as_u64()).unwrap_or(256) as usize;
            (w, h)
        }
        _ => (256, 256),
    };

    let out_total = width * height;

    println!(
        "[GrayscaleFilter] Converting {}x{} RGBA -> Grayscale ({} bytes)",
        width, height, out_total
    );

    let (out_tx, out_handle) = context.create_stream(
        "ImageOut",
        Some("image/raw-gray".into()),
        Some(out_total as u64),
        Some(16),
    );

    // Spawn the transform so the handle can be delivered immediately
    tokio::spawn(async move {
        let _ = out_tx
            .send_async(StreamFrame::Begin {
                content_type: Some("image/raw-gray".into()),
                size_hint: Some(out_total as u64),
                metadata: Some(serde_json::json!({
                    "width": width,
                    "height": height,
                    "channels": 1,
                    "format": "Gray8"
                })),
            })
            .await;

        loop {
            match in_rx.recv_async().await {
                Ok(StreamFrame::Data(rgba_row)) => {
                    let mut gray_row = Vec::with_capacity(rgba_row.len() / 4);
                    for pixel in rgba_row.chunks_exact(4) {
                        let lum = (0.299 * pixel[0] as f64
                            + 0.587 * pixel[1] as f64
                            + 0.114 * pixel[2] as f64) as u8;
                        gray_row.push(lum);
                    }
                    if out_tx
                        .send_async(StreamFrame::Data(Arc::new(gray_row)))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                Ok(StreamFrame::End) => break,
                Ok(StreamFrame::Error(e)) => {
                    let _ = out_tx.send_async(StreamFrame::Error(e)).await;
                    return;
                }
                _ => {}
            }
        }

        let _ = out_tx.send_async(StreamFrame::End).await;
        println!("[GrayscaleFilter] Transform complete");
    });

    Ok([("ImageOut".to_owned(), Message::stream_handle(out_handle))].into())
}

/// Terminal actor: consumes a stream, collects stats, and forwards a
/// summary string to the Logger.
#[actor(
    ImageSinkActor,
    inports::<10>(ImageIn),
    outports::<10>(Log)
)]
async fn image_sink_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    use reflow_actor::stream::StreamFrame;

    let rx = context
        .take_stream_receiver("ImageIn")
        .expect("[ImageSink] expected stream on ImageIn");

    let mut total_bytes = 0usize;
    let mut chunks = 0usize;
    let mut format = String::from("unknown");
    let mut dimensions = String::from("?");

    loop {
        match rx.recv_async().await {
            Ok(StreamFrame::Begin { metadata, .. }) => {
                if let Some(meta) = metadata {
                    format = meta
                        .get("format")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    dimensions = meta
                        .get("width")
                        .zip(meta.get("height"))
                        .map(|(w, h)| format!("{}x{}", w, h))
                        .unwrap_or_else(|| "?".into());
                }
            }
            Ok(StreamFrame::Data(data)) => {
                total_bytes += data.len();
                chunks += 1;
            }
            Ok(StreamFrame::End) => break,
            Ok(StreamFrame::Error(e)) => {
                eprintln!("[ImageSink] Stream error: {}", e);
                break;
            }
            Err(_) => break,
        }
    }

    let summary = format!(
        "Pipeline complete: {} {} image, {} chunks, {} bytes",
        dimensions, format, chunks, total_bytes
    );
    println!("[ImageSink] {}", summary);

    Ok([("Log".to_owned(), Message::string(summary))].into())
}

/// Collects string messages and stores them in state for later retrieval.
#[actor(
    LoggerActor,
    inports::<100>(In),
    outports::<10>(Out),
    state(MemoryState)
)]
async fn logger_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    if let Some(msg) = payload.get("In") {
        let text = match msg {
            Message::String(s) => s.as_str().to_string(),
            other => format!("{:?}", other),
        };
        println!("[Logger] {}", text);
        return Ok([("Out".to_owned(), Message::string(text))].into());
    }
    Ok(HashMap::new())
}

// ── Network orchestration ───────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Image Pipeline Streaming Example ===\n");
    println!("Pipeline: ImageSource -> GrayscaleFilter -> ImageSink -> Logger");
    println!("  - 256x256 RGBA image streamed row-by-row");
    println!("  - Grayscale conversion in-flight (no full-image buffering)");
    println!("  - Bounded channels provide backpressure");
    println!("  - Press Ctrl+C to exit\n");

    let mut network = Network::new(NetworkConfig::default());

    // Register actors
    network.register_actor("image_source", ImageSourceActor::new())?;
    network.register_actor("grayscale_filter", GrayscaleFilterActor::new())?;
    network.register_actor("image_sink", ImageSinkActor::new())?;
    network.register_actor("logger", LoggerActor::new())?;

    // Add nodes
    network.add_node("source", "image_source", None)?;
    network.add_node("grayscale", "grayscale_filter", None)?;
    network.add_node("sink", "image_sink", None)?;
    network.add_node("logger", "logger", None)?;

    // Wire: source → grayscale → sink → logger
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "source".to_owned(),
            port: "ImageOut".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "grayscale".to_owned(),
            port: "ImageIn".to_owned(),
            ..Default::default()
        },
    });
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "grayscale".to_owned(),
            port: "ImageOut".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "sink".to_owned(),
            port: "ImageIn".to_owned(),
            ..Default::default()
        },
    });
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "sink".to_owned(),
            port: "Log".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "logger".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });

    // Trigger the pipeline
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "source".to_owned(),
            port: "Trigger".to_owned(),
            initial_data: Some(Message::Flow),
        },
    });

    // Start and wait for Ctrl+C
    network.start()?;

    tokio::signal::ctrl_c().await?;
    println!("\nShutting down...");

    // Read logger output via the convenience method
    let logs = network.read_actor_output("logger");
    if !logs.is_empty() {
        println!("\n=== Logger Output ===");
        for (port, msg) in &logs {
            if let Message::String(s) = msg {
                println!("  [{}] {}", port, s);
            }
        }
    }

    network.shutdown();
    println!("Done.");
    Ok(())
}
