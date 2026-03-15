//! # SDF Pipeline Example
//!
//! Two parallel pipelines from the same SDF scene:
//!
//! ```text
//! SdfSphere ──┐                    ┌── SDFRender → BytesToStream → ImageEncode → FileSave (PNG)
//!             ├── SmoothUnion ── Twist ─┤
//! SdfBox ─────┘                    └── MarchingCubes → ObjExport → FileSave (OBJ)
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
    println!("=== SDF Pipeline (Render + Mesh) ===\n");

    let mut network = Network::new(NetworkConfig::default());

    // ── Register actors ──────────────────────────────────────────

    let templates = [
        // SDF
        "tpl_sdf_sphere", "tpl_sdf_box", "tpl_sdf_translate",
        "tpl_sdf_rotate", "tpl_sdf_smooth_union", "tpl_sdf_twist",
        // Render pipeline
        "tpl_sdf_render", "tpl_bytes_to_stream", "tpl_image_encode",
        // Mesh pipeline
        "tpl_sdf_marching_cubes", "tpl_obj_export",
        // I/O
        "tpl_file_save",
    ];

    for tpl_id in &templates {
        let actor = reflow_components::get_actor_for_template(tpl_id)
            .ok_or_else(|| anyhow::anyhow!("Actor not found: {}", tpl_id))?;
        network.register_actor_arc(tpl_id, actor)?;
    }

    // FileSave is used twice — register a second instance
    let save2 = reflow_components::get_actor_for_template("tpl_file_save").unwrap();
    network.register_actor_arc("tpl_file_save_2", save2)?;

    println!("Registered {} actors", templates.len() + 1);

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

    // ── Render pipeline (SDF → PNG) ──────────────────────────────

    network.add_node("render", "tpl_sdf_render",
        config(json!({
            "width": 256, "height": 256,
            "maxSteps": 64, "fov": 45.0,
            "cameraPosX": 3.0, "cameraPosY": 2.0, "cameraPosZ": 4.0,
            "ao": true
        })))?;
    network.add_node("to_stream", "tpl_bytes_to_stream",
        config(json!({ "chunkSize": 65536, "contentType": "image/raw-rgba" })))?;
    network.add_node("encode", "tpl_image_encode",
        config(json!({ "format": "png" })))?;
    network.add_node("save_png", "tpl_file_save",
        config(json!({ "path": "sdf_output.png", "createDirs": true })))?;

    // ── Mesh pipeline (SDF → OBJ) ────────────────────────────────

    network.add_node("marching_cubes", "tpl_sdf_marching_cubes",
        config(json!({ "resolution": 64, "bound": 2.5, "isoLevel": 0.0 })))?;
    network.add_node("obj_export", "tpl_obj_export",
        config(json!({ "name": "sdf_mesh", "stride": 24 })))?;
    network.add_node("save_obj", "tpl_file_save_2",
        config(json!({ "path": "sdf_output.obj", "createDirs": true })))?;

    println!("Built graph: 14 nodes");

    // ── Wire connections ─────────────────────────────────────────

    // sphere → translate → blend.sdf_a
    network.add_connection(wire("sphere", "sdf", "translate_sphere", "sdf"));
    network.add_connection(wire("translate_sphere", "sdf", "blend", "sdf_a"));

    // box → rotate → blend.sdf_b
    network.add_connection(wire("box", "sdf", "rotate_box", "sdf"));
    network.add_connection(wire("rotate_box", "sdf", "blend", "sdf_b"));

    // blend → twist (shared SDF)
    network.add_connection(wire("blend", "sdf", "twist", "sdf"));

    // Branch 1: twist → render → stream → encode → save_png
    network.add_connection(wire("twist", "sdf", "render", "sdf"));
    network.add_connection(wire("render", "output", "to_stream", "input"));
    network.add_connection(wire("to_stream", "stream", "encode", "stream"));
    network.add_connection(wire("encode", "output", "save_png", "input"));

    // Branch 2: twist → marching_cubes → obj_export → save_obj
    network.add_connection(wire("twist", "sdf", "marching_cubes", "sdf"));
    network.add_connection(wire("marching_cubes", "mesh", "obj_export", "mesh"));
    network.add_connection(wire("obj_export", "output", "save_obj", "input"));

    // Trigger source actors
    network.add_initial(InitialPacket {
        to: ConnectionPoint::new("sphere", "trigger", Some(Message::Boolean(true))),
    });
    network.add_initial(InitialPacket {
        to: ConnectionPoint::new("box", "trigger", Some(Message::Boolean(true))),
    });

    println!("Wired 12 connections + 2 triggers\n");

    // ── Execute ──────────────────────────────────────────────────

    println!("Executing pipeline...");
    println!("  SdfSphere → Translate ─┐");
    println!("                         ├ SmoothUnion → Twist ─┬─ SDFRender → PNG");
    println!("  SdfBox → Rotate ───────┘                      └─ MarchingCubes → OBJ\n");

    let start = std::time::Instant::now();
    network.start()?;

    // Wait for both output files
    let png_path = std::path::Path::new("sdf_output.png");
    let obj_path = std::path::Path::new("sdf_output.obj");
    let timeout = std::time::Duration::from_secs(60);

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let png_done = png_path.exists();
        let obj_done = obj_path.exists();
        if png_done && obj_done {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            break;
        }
        if start.elapsed() > timeout {
            eprintln!("Timeout ({:.0}s)", start.elapsed().as_secs_f64());
            if !png_done { eprintln!("  PNG not produced"); }
            if !obj_done { eprintln!("  OBJ not produced"); }
            break;
        }
    }

    let elapsed = start.elapsed();
    println!("Pipeline completed in {:.0}ms\n", elapsed.as_secs_f64() * 1000.0);

    // Report results
    if png_path.exists() {
        let size = std::fs::metadata(png_path)?.len();
        println!("  sdf_output.png  {} bytes", size);
    }
    if obj_path.exists() {
        let size = std::fs::metadata(obj_path)?.len();
        // Count vertices/faces
        let content = std::fs::read_to_string(obj_path)?;
        let verts = content.lines().filter(|l| l.starts_with("v ")).count();
        let faces = content.lines().filter(|l| l.starts_with("f ")).count();
        println!("  sdf_output.obj  {} bytes ({} vertices, {} faces)", size, verts, faces);
    }

    // Check errors
    for node in ["render", "marching_cubes", "obj_export", "save_png", "save_obj"] {
        for (port, msg) in network.read_actor_output(node) {
            if port == "error" {
                eprintln!("  [{}] {:?}", node, msg);
            }
        }
    }

    network.shutdown();
    println!("\nDone!");

    Ok(())
}
