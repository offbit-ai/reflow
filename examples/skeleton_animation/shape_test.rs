//! Shape test — build the snake body from SDF actors, render to PNG.
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
    println!("=== Snake Shape Test ===\n");

    let mut net = Network::new(NetworkConfig::default());

    // Register base actors
    for tpl in [
        "tpl_sdf_capsule", "tpl_sdf_sphere",
        "tpl_sdf_translate", "tpl_sdf_rotate", "tpl_sdf_scale",
        "tpl_sdf_smooth_union", "tpl_sdf_smooth_difference",
        "tpl_sdf_marching_cubes",
        "tpl_prefab", "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        "tpl_bytes_to_stream", "tpl_image_encode", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // Extra instances for the construction
    for name in ["tpl_head_sphere", "tpl_head_tr", "tpl_head_union",
                  "tpl_tail_cone", "tpl_tail_tr", "tpl_tail_scale",
                  "tpl_lay_flat"] {
        let base = match name {
            n if n.contains("sphere") => "tpl_sdf_sphere",
            n if n.contains("cone") => "tpl_sdf_capsule", // use thin capsule as tail
            n if n.contains("tr") => "tpl_sdf_translate",
            n if n.contains("union") => "tpl_sdf_smooth_union",
            n if n.contains("scale") => "tpl_sdf_scale",
            n if n.contains("flat") => "tpl_sdf_rotate",
            _ => "tpl_sdf_sphere",
        };
        net.register_actor_arc(name, reflow_components::get_actor_for_template(base).unwrap())?;
    }

    // ══════════════════════════════════════════════════════════════
    // SNAKE BODY: One long capsule (along Y) — smooth, no segments.
    // The capsule gives a uniform cylinder with hemispherical caps.
    // This is the simplest shape that looks like a snake body.
    // ══════════════════════════════════════════════════════════════

    // Main body: long and thin
    let body_radius = 0.12;
    let body_length = 7.0;

    net.add_node("body", "tpl_sdf_capsule", config(json!({
        "radius": body_radius, "height": body_length,
    })))?;

    // Head: slightly wider sphere at the front, blended smoothly
    net.add_node("head", "tpl_head_sphere", config(json!({ "radius": 0.16 })))?;
    net.add_node("head_tr", "tpl_head_tr", config(json!({
        "x": 0.0, "y": body_length / 2.0 + 0.05, "z": 0.0,
    })))?;
    net.add_connection(wire("head", "sdf", "head_tr", "sdf"));
    net.add_initial(iip("head", "_trigger", Message::Flow));

    // Join head to body
    net.add_node("head_union", "tpl_head_union", config(json!({ "smoothness": 0.12 })))?;
    net.add_connection(wire("body", "sdf", "head_union", "sdf_a"));
    net.add_connection(wire("head_tr", "sdf", "head_union", "sdf_b"));
    net.add_initial(iip("body", "_trigger", Message::Flow));

    // Rotate to lay flat in XZ plane (capsule Y → Z)
    net.add_node("lay_flat", "tpl_lay_flat", config(json!({
        "x": 1.5708, "y": 0.0, "z": 0.7854,
    })))?;
    net.add_connection(wire("head_union", "sdf", "lay_flat", "sdf"));

    // ══════════════════════════════════════════════════════════════
    // MARCHING CUBES — high resolution for smooth surface
    // ══════════════════════════════════════════════════════════════
    net.add_node("mc", "tpl_sdf_marching_cubes", config(json!({
        "resolution": 128, "bound": 5.5, "isoLevel": 0.0,
    })))?;
    net.add_connection(wire("lay_flat", "sdf", "mc", "sdf"));

    // ══════════════════════════════════════════════════════════════
    // SCENE RENDER
    // ══════════════════════════════════════════════════════════════
    net.add_node("prefab", "tpl_prefab", config(json!({ "name": "snake" })))?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "snake_0" })))?;
    net.add_node("scene", "tpl_scene_graph", config(json!({ "name": "s", "expectedObjects": 1 })))?;

    // Camera: 3/4 view from above, framing the body diagonally
    // Body is now along Z after rotation, ~6 units long
    net.add_node("render", "tpl_scene_render", config(json!({
        "width": 1024, "height": 1024,
        "cameraPosX": 1.0, "cameraPosY": 8.0, "cameraPosZ": 2.5,
        "cameraTargetX": 0.0, "cameraTargetY": 0.0, "cameraTargetZ": 0.0,
        "fov": 35.0,
        "bgR": 0.92, "bgG": 0.92, "bgB": 0.90,
    })))?;

    net.add_connection(wire("mc", "mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("mc", "mesh", "render", "meshes"));

    // ══════════════════════════════════════════════════════════════
    // ENCODE + SAVE PNG
    // ══════════════════════════════════════════════════════════════
    net.add_node("to_stream", "tpl_bytes_to_stream", config(json!({
        "chunkSize": 65536, "contentType": "image/raw-rgba",
    })))?;
    net.add_node("encode", "tpl_image_encode", config(json!({ "format": "png" })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "snake_shape.png" })))?;

    net.add_connection(wire("render", "output", "to_stream", "input"));
    net.add_connection(wire("to_stream", "stream", "encode", "stream"));
    net.add_connection(wire("encode", "output", "save", "input"));

    println!("Body: capsule r={}, h={}", body_radius, body_length);
    println!("Head: sphere r=0.16");
    println!("MC: 128³, bound=4.0");
    println!("Render: 1024x1024\n");
    println!("Running...");

    net.start()?;

    let png_path = std::path::Path::new("snake_shape.png");
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_secs(60);
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
