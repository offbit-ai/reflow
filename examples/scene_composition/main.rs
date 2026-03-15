//! # Scene Composition Example
//!
//! Demonstrates prefab-based scene composition with terrain:
//!
//! ```text
//! SdfSphere → SmoothUnion → MarchingCubes → Prefab("blob") ─→ Instance("blob_1", pos=[2,0,0]) ─┐
//! SdfBox ───┘                                                  Instance("blob_2", pos=[-2,0,1])─┤
//!                                                                                                ├→ SceneGraph → FileSave
//! NoiseGenerator → HeightmapToMesh → Terrain("ground") ─────────────────────────────────────────┘
//! ```
//!
//! Usage:
//!   cd examples/scene_composition && cargo run

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
    println!("=== Scene Composition Pipeline ===\n");

    let mut network = Network::new(NetworkConfig::default());

    // ── Register actors ──────────────────────────────────────────

    let templates = [
        // SDF
        "tpl_sdf_sphere", "tpl_sdf_box", "tpl_sdf_smooth_union",
        // GPU
        "tpl_sdf_marching_cubes", "tpl_scene_render",
        // Procedural
        "tpl_noise_generator", "tpl_heightmap_to_mesh",
        // Scene
        "tpl_prefab", "tpl_instance", "tpl_terrain", "tpl_scene_graph",
        // Encode + I/O
        "tpl_bytes_to_stream", "tpl_image_encode", "tpl_file_save",
    ];

    for tpl_id in &templates {
        let actor = reflow_components::get_actor_for_template(tpl_id)
            .ok_or_else(|| anyhow::anyhow!("Actor not found: {}", tpl_id))?;
        network.register_actor_arc(tpl_id, actor)?;
    }

    // Instance is used 2x — register a second copy
    let inst2 = reflow_components::get_actor_for_template("tpl_instance").unwrap();
    network.register_actor_arc("tpl_instance_2", inst2)?;

    println!("Registered {} actors", templates.len() + 1);

    // ── SDF blob ─────────────────────────────────────────────────

    network.add_node("sphere", "tpl_sdf_sphere",
        config(json!({ "radius": 0.8 })))?;
    network.add_node("box", "tpl_sdf_box",
        config(json!({ "sizeX": 0.5, "sizeY": 0.5, "sizeZ": 0.5 })))?;
    network.add_node("blend", "tpl_sdf_smooth_union",
        config(json!({ "smoothness": 0.4 })))?;

    // ── Mesh extraction ──────────────────────────────────────────

    network.add_node("mesh", "tpl_sdf_marching_cubes",
        config(json!({ "resolution": 128, "bound": 1.2, "isoLevel": 0.0 })))?;

    // ── Prefab ───────────────────────────────────────────────────

    network.add_node("prefab", "tpl_prefab",
        config(json!({ "name": "blob" })))?;

    // ── Instances ────────────────────────────────────────────────

    network.add_node("blob_1", "tpl_instance",
        config(json!({ "id": "blob_1", "posX": 3.0, "posY": 3.5, "posZ": 0.0 })))?;
    network.add_node("blob_2", "tpl_instance_2",
        config(json!({ "id": "blob_2", "posX": -3.0, "posY": 3.0, "posZ": 1.5, "scale": 1.5 })))?;

    // ── Terrain ──────────────────────────────────────────────────

    network.add_node("noise", "tpl_noise_generator",
        config(json!({
            "noiseType": "perlin", "width": 64, "height": 64,
            "scale": 3.0, "octaves": 4, "persistence": 0.5
        })))?;
    network.add_node("terrain_mesh", "tpl_heightmap_to_mesh",
        config(json!({ "heightScale": 2.0, "meshWidth": 20.0, "meshDepth": 20.0, "width": 64 })))?;
    network.add_node("terrain", "tpl_terrain",
        config(json!({ "id": "ground", "width": 20.0, "depth": 20.0, "heightScale": 2.0 })))?;

    // ── Scene Graph ──────────────────────────────────────────────

    network.add_node("scene", "tpl_scene_graph",
        config(json!({ "name": "my_scene", "expectedObjects": 3 })))?;

    // ── Scene Render ──────────────────────────────────────────────

    network.add_node("render", "tpl_scene_render",
        config(json!({
            "width": 512, "height": 512, "fov": 45.0,
            "cameraPosX": 12.0, "cameraPosY": 8.0, "cameraPosZ": 14.0,
            "msaa": 4,
            "bgR": 0.15, "bgG": 0.15, "bgB": 0.2,
        })))?;

    // ── Encode + Save ────────────────────────────────────────────

    network.add_node("to_stream", "tpl_bytes_to_stream",
        config(json!({ "chunkSize": 65536, "contentType": "image/raw-rgba" })))?;
    network.add_node("encode", "tpl_image_encode",
        config(json!({ "format": "png" })))?;
    network.add_node("save_json", "tpl_file_save",
        config(json!({ "path": "scene_output.json", "createDirs": true })))?;
    // Register a second FileSave for PNG
    let save2 = reflow_components::get_actor_for_template("tpl_file_save").unwrap();
    network.register_actor_arc("tpl_file_save_png", save2)?;
    network.add_node("save_png", "tpl_file_save_png",
        config(json!({ "path": "scene_render.png", "createDirs": true })))?;

    println!("Built graph: 17 nodes");

    // ── Wire connections ─────────────────────────────────────────

    // SDF → mesh
    network.add_connection(wire("sphere", "sdf", "blend", "sdf_a"));
    network.add_connection(wire("box", "sdf", "blend", "sdf_b"));
    network.add_connection(wire("blend", "sdf", "mesh", "sdf"));

    // Mesh → prefab
    network.add_connection(wire("mesh", "mesh", "prefab", "mesh"));

    // Prefab → instances
    network.add_connection(wire("prefab", "prefab", "blob_1", "prefab"));
    network.add_connection(wire("prefab", "prefab", "blob_2", "prefab"));

    // Noise → terrain mesh → terrain
    network.add_connection(wire("noise", "output", "terrain_mesh", "input"));
    network.add_connection(wire("terrain_mesh", "mesh", "terrain", "mesh"));
    network.add_connection(wire("noise", "output", "terrain", "heightmap"));

    // All objects → scene graph
    network.add_connection(wire("blob_1", "object", "scene", "object"));
    network.add_connection(wire("blob_2", "object", "scene", "object"));
    network.add_connection(wire("terrain", "object", "scene", "object"));

    // Scene → save JSON
    network.add_connection(wire("scene", "scene", "save_json", "input"));

    // Scene → render → encode → save PNG
    network.add_connection(wire("scene", "scene", "render", "scene"));
    network.add_connection(wire("mesh", "mesh", "render", "meshes"));
    network.add_connection(wire("terrain_mesh", "mesh", "render", "terrain_mesh"));
    network.add_connection(wire("render", "output", "to_stream", "input"));
    network.add_connection(wire("to_stream", "stream", "encode", "stream"));
    network.add_connection(wire("encode", "output", "save_png", "input"));

    // Trigger source actors
    network.add_initial(InitialPacket {
        to: ConnectionPoint::new("sphere", "_trigger", Some(Message::Flow)),
    });
    network.add_initial(InitialPacket {
        to: ConnectionPoint::new("box", "_trigger", Some(Message::Flow)),
    });
    network.add_initial(InitialPacket {
        to: ConnectionPoint::new("noise", "_trigger", Some(Message::Flow)),
    });

    println!("Wired 19 connections + 3 triggers\n");

    // ── Execute ──────────────────────────────────────────────────

    println!("Executing pipeline...");
    println!("  SdfSphere → SmoothUnion → MarchingCubes → Prefab ─→ Instance(blob_1) ─┐");
    println!("  SdfBox ───┘                                         Instance(blob_2) ─┤");
    println!("                                                                         ├→ SceneGraph");
    println!("  NoiseGenerator → HeightmapToMesh → Terrain ────────────────────────────┘\n");

    let start = std::time::Instant::now();
    network.start()?;

    // Wait for both outputs
    let json_path = std::path::Path::new("scene_output.json");
    let png_path = std::path::Path::new("scene_render.png");
    let timeout = std::time::Duration::from_secs(30);

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if json_path.exists() && png_path.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            break;
        }
        if start.elapsed() > timeout {
            if !json_path.exists() { eprintln!("Timeout: JSON not produced"); }
            if !png_path.exists() { eprintln!("Timeout: PNG not produced"); }
            break;
        }
    }

    let elapsed = start.elapsed();

    if json_path.exists() {
        let content = std::fs::read_to_string(json_path)?;
        let scene: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();

        println!("Pipeline completed in {:.0}ms\n", elapsed.as_secs_f64() * 1000.0);
        println!("Scene: {}", scene.get("name").and_then(|v| v.as_str()).unwrap_or("?"));
        println!("Objects: {}", scene.get("objectCount").and_then(|v| v.as_u64()).unwrap_or(0));

        if let Some(objects) = scene.get("objects").and_then(|v| v.as_array()) {
            for obj in objects {
                let id = obj.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                let obj_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                let pos = obj.get("transform")
                    .and_then(|t| t.get("position"))
                    .and_then(|p| p.as_array())
                    .map(|a| format!("[{:.1}, {:.1}, {:.1}]",
                        a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.0),
                        a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0),
                        a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0)))
                    .unwrap_or_else(|| "?".to_string());
                println!("  {} ({}) at {}", id, obj_type, pos);
            }
        }

        let json_size = std::fs::metadata(json_path)?.len();
        println!("\nSaved: scene_output.json ({} bytes)", json_size);

        if png_path.exists() {
            let png_size = std::fs::metadata(png_path)?.len();
            println!("Saved: scene_render.png ({} bytes)", png_size);
        }
    } else {
        println!("Pipeline failed after {:.0}ms", elapsed.as_secs_f64() * 1000.0);
        for node in ["mesh", "prefab", "blob_1", "blob_2", "terrain", "scene", "render", "save_json", "save_png"] {
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
