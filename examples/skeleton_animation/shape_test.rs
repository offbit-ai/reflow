//! Shape test — snake with procedural vertex coloring
//! DAG: TubeMesh → VertexColor ← NoiseGenerator → Prefab → SceneRender
//! Run: cd examples/skeleton_animation && cargo run --bin shape_test

use std::collections::HashMap;
use reflow_network::{
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value};

fn config(cfg: Value) -> Option<HashMap<String, Value>> {
    if let Value::Object(map) = cfg { Some(map.into_iter().collect()) } else { None }
}
fn wire(fa: &str, fp: &str, ta: &str, tp: &str) -> Connector {
    Connector {
        from: ConnectionPoint { actor: fa.to_owned(), port: fp.to_owned(), ..Default::default() },
        to: ConnectionPoint { actor: ta.to_owned(), port: tp.to_owned(), ..Default::default() },
    }
}
fn iip(node: &str, port: &str, msg: Message) -> InitialPacket {
    InitialPacket { to: ConnectionPoint::new(node, port, Some(msg)) }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Snake Shape (Procedural Vertex Color) ===\n");

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_tube_mesh", "tpl_noise_generator", "tpl_vertex_color",
        "tpl_prefab", "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        "tpl_bytes_to_stream", "tpl_image_encode", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ MESH with UV ═══
    net.add_node("body", "tpl_tube_mesh", config(json!({
        "path": "M -2.5,0 C -1.5,-0.8 -0.5,0.8 0.5,0 C 1.5,-0.6 2.5,0.4 3.5,0.1",
        "profile": [0.22, 0.20, 0.18, 0.17, 0.17, 0.15, 0.12, 0.09, 0.05, 0.02],
        "segments": 64,
        "rings": 24,
        "plane": "xz",
    })))?;
    net.add_initial(iip("body", "_trigger", Message::Flow));

    // ═══ NOISE for skin pattern ═══
    net.add_node("noise", "tpl_noise_generator", config(json!({
        "noiseType": "perlin",
        "width": 64, "height": 64,
        "scale": 4.0, "octaves": 3, "persistence": 0.5,
    })))?;
    net.add_initial(iip("noise", "_trigger", Message::Flow));

    // ═══ VERTEX COLOR: mesh + uv + noise → colored mesh ═══
    net.add_node("colorize", "tpl_vertex_color", config(json!({
        "color1": [0.35, 0.42, 0.25],
        "color2": [0.15, 0.18, 0.08],
        "noiseScale": 6.0,
        "noiseWidth": 64,
    })))?;

    net.add_connection(wire("body", "mesh", "colorize", "mesh"));
    net.add_connection(wire("body", "uv", "colorize", "uv"));
    net.add_connection(wire("noise", "output", "colorize", "noise"));

    // ═══ SCENE RENDER ═══
    // The colored mesh is 36-byte stride (pos3+normal3+color3).
    // SceneRenderActor build_vertex_buffer handles this if mesh is pre-colored.
    net.add_node("prefab", "tpl_prefab", config(json!({ "name": "snake", "stride": 36 })))?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "snake_0" })))?;
    net.add_node("scene", "tpl_scene_graph", config(json!({ "name": "s", "expectedObjects": 1 })))?;
    net.add_node("render", "tpl_scene_render", config(json!({
        "width": 1024, "height": 1024,
        "cameraPosX": -1.0, "cameraPosY": 7.0, "cameraPosZ": 4.0,
        "cameraTargetX": 0.0, "cameraTargetY": 0.0, "cameraTargetZ": 0.0,
        "fov": 40.0,
        "bgR": 0.92, "bgG": 0.92, "bgB": 0.90,
    })))?;

    // Colored mesh goes directly to render (36-byte stride already has colors)
    net.add_connection(wire("colorize", "colored_mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("colorize", "colored_mesh", "render", "meshes"));

    // ═══ ENCODE + SAVE ═══
    net.add_node("to_stream", "tpl_bytes_to_stream", config(json!({
        "chunkSize": 65536, "contentType": "image/raw-rgba",
    })))?;
    net.add_node("encode", "tpl_image_encode", config(json!({ "format": "png" })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "snake_shape.png" })))?;

    net.add_connection(wire("render", "output", "to_stream", "input"));
    net.add_connection(wire("to_stream", "stream", "encode", "stream"));
    net.add_connection(wire("encode", "output", "save", "input"));

    println!("DAG: TubeMesh → VertexColor ← NoiseGenerator → SceneRender\n");
    println!("Running...");

    net.start()?;

    let png_path = std::path::Path::new("snake_shape.png");
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(30);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        if png_path.exists() && png_path.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            break;
        }
        if start.elapsed() > timeout { eprintln!("Timed out"); break; }
    }
    net.shutdown();

    if png_path.exists() {
        let size = std::fs::metadata(png_path)?.len();
        println!("Saved: snake_shape.png ({} bytes, {:.1}s)", size, start.elapsed().as_secs_f64());
    }
    println!("Done!");
    Ok(())
}
