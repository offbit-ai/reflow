//! # SDF Render Pipeline Example
//!
//! Full end-to-end: SDF IR → GPU render → PNG encode → file save.
//!
//! ```text
//! SdfSphere ──┐
//!             ├── SmoothUnion ── Twist ── SDFRender ── BytesToStream ── ImageEncode ── FileSave
//! SdfBox ─────┘                           (wgpu)       (chunk RGBA)     (PNG)         (disk)
//! ```
//!
//! Usage:
//!   cd examples/sdf_render && cargo run

use std::collections::HashMap;

use reflow_network::{
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value};

fn config(cfg: Value) -> Option<HashMap<String, Value>> {
    if let Value::Object(map) = cfg {
        Some(map.into_iter().collect())
    } else {
        None
    }
}

fn wire(from_actor: &str, from_port: &str, to_actor: &str, to_port: &str) -> Connector {
    Connector {
        from: ConnectionPoint {
            actor: from_actor.to_owned(),
            port: from_port.to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: to_actor.to_owned(),
            port: to_port.to_owned(),
            ..Default::default()
        },
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== SDF Render Pipeline ===\n");

    let mut network = Network::new(NetworkConfig::default());

    // ── Register actors ──────────────────────────────────────────

    let templates = [
        "tpl_sdf_sphere",
        "tpl_sdf_box",
        "tpl_sdf_translate",
        "tpl_sdf_rotate",
        "tpl_sdf_smooth_union",
        "tpl_sdf_twist",
        "tpl_sdf_render",
        "tpl_bytes_to_stream",
        "tpl_image_encode",
        "tpl_file_save",
    ];

    for tpl_id in &templates {
        let actor = reflow_components::get_actor_for_template(tpl_id)
            .ok_or_else(|| anyhow::anyhow!("Actor not found: {}", tpl_id))?;
        network.register_actor_arc(tpl_id, actor)?;
    }

    println!("Registered {} actors", templates.len());

    // ── Build graph ──────────────────────────────────────────────

    // SDF primitives
    network.add_node("sphere", "tpl_sdf_sphere",
        config(json!({ "radius": 0.8 })))?;
    network.add_node("box", "tpl_sdf_box",
        config(json!({ "sizeX": 0.6, "sizeY": 0.6, "sizeZ": 0.6 })))?;

    // SDF transforms
    network.add_node("translate_sphere", "tpl_sdf_translate",
        config(json!({ "x": 0.6, "y": 0.3, "z": 0.0 })))?;
    network.add_node("rotate_box", "tpl_sdf_rotate",
        config(json!({ "x": 0.0, "y": 35.0, "z": 15.0 })))?;

    // SDF combine + deform
    network.add_node("blend", "tpl_sdf_smooth_union",
        config(json!({ "smoothness": 0.25 })))?;
    network.add_node("twist", "tpl_sdf_twist",
        config(json!({ "strength": 0.3 })))?;

    // GPU render → raw RGBA bytes
    network.add_node("render", "tpl_sdf_render",
        config(json!({
            "width": 256, "height": 256,
            "maxSteps": 64, "fov": 45.0,
            "cameraPosX": 3.0, "cameraPosY": 2.0, "cameraPosZ": 4.0,
            "ao": true
        })))?;

    // Bytes → Stream (chunk the RGBA blob for ImageEncode)
    network.add_node("to_stream", "tpl_bytes_to_stream",
        config(json!({
            "chunkSize": 65536,
            "contentType": "image/raw-rgba"
        })))?;

    // Stream → PNG
    network.add_node("encode", "tpl_image_encode",
        config(json!({ "format": "png" })))?;

    // PNG → disk
    network.add_node("save", "tpl_file_save",
        config(json!({ "path": "sdf_output.png", "createDirs": true })))?;

    println!("Built graph: 10 nodes");

    // ── Wire connections ─────────────────────────────────────────

    // sphere → translate → blend.sdf_a
    network.add_connection(wire("sphere", "sdf", "translate_sphere", "sdf"));
    network.add_connection(wire("translate_sphere", "sdf", "blend", "sdf_a"));

    // box → rotate → blend.sdf_b
    network.add_connection(wire("box", "sdf", "rotate_box", "sdf"));
    network.add_connection(wire("rotate_box", "sdf", "blend", "sdf_b"));

    // blend → twist → render → to_stream → encode → save
    network.add_connection(wire("blend", "sdf", "twist", "sdf"));
    network.add_connection(wire("twist", "sdf", "render", "sdf"));
    network.add_connection(wire("render", "output", "to_stream", "input"));
    network.add_connection(wire("to_stream", "stream", "encode", "stream"));
    network.add_connection(wire("encode", "output", "save", "input"));

    // Trigger source actors
    network.add_initial(InitialPacket {
        to: ConnectionPoint::new("sphere", "trigger", Some(Message::Boolean(true))),
    });
    network.add_initial(InitialPacket {
        to: ConnectionPoint::new("box", "trigger", Some(Message::Boolean(true))),
    });

    println!("Wired 9 connections + 2 triggers\n");

    // ── Execute ──────────────────────────────────────────────────

    println!("Executing pipeline...");
    println!("  SdfSphere → Translate ─┐");
    println!("                         ├ SmoothUnion → Twist → SDFRender → BytesToStream → ImageEncode → FileSave");
    println!("  SdfBox → Rotate ───────┘\n");

    let start = std::time::Instant::now();
    network.start()?;

    // Wait for output file
    let output_path = std::path::Path::new("sdf_output.png");
    let timeout = std::time::Duration::from_secs(30);

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if output_path.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            break;
        }
        if start.elapsed() > timeout {
            eprintln!("Timeout waiting for pipeline to complete");
            break;
        }
    }

    let elapsed = start.elapsed();

    if output_path.exists() {
        let size = std::fs::metadata(output_path)?.len();
        println!("Pipeline completed in {:.0}ms", elapsed.as_secs_f64() * 1000.0);
        println!("Output: sdf_output.png ({} bytes)", size);
    } else {
        println!("Pipeline failed after {:.0}ms", elapsed.as_secs_f64() * 1000.0);
        for node in ["render", "to_stream", "encode", "save"] {
            for (port, msg) in network.read_actor_output(node) {
                if port == "error" {
                    eprintln!("  [{}] {:?}", node, msg);
                }
            }
        }
    }

    network.shutdown();
    println!("\nDone!");

    Ok(())
}
