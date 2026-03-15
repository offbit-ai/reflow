//! Shape test — build the snake with SdfPath actor, render to PNG.
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
    println!("=== Snake Shape Test (SdfPath) ===\n");

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_sdf_path", "tpl_sdf_marching_cubes",
        "tpl_prefab", "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        "tpl_bytes_to_stream", "tpl_image_encode", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ══════════════════════════════════════════════════════════════
    // SNAKE via SdfPath — one actor, one config
    //
    // The path describes the snake's spine as an S-curve in the XZ plane.
    // Profile defines the cross-section radius from head to tail.
    // ══════════════════════════════════════════════════════════════
    net.add_node("snake", "tpl_sdf_path", config(json!({
        "path": "M -2.5,0 C -1.5,-1.0 -0.5,1.0 0.5,0 C 1.5,-0.8 2.5,0.6 3.5,0.2",
        "profile": [0.25, 0.23, 0.21, 0.20, 0.20, 0.18, 0.15, 0.11, 0.07, 0.03],
        "segments": 24,
        "smoothness": 0.04,
        "plane": "xz",
    })))?;

    net.add_node("mc", "tpl_sdf_marching_cubes", config(json!({
        "resolution": 384, "bound": 4.5, "isoLevel": 0.0,
    })))?;

    net.add_connection(wire("snake", "sdf", "mc", "sdf"));
    net.add_initial(iip("snake", "_trigger", Message::Flow));

    // Scene render
    net.add_node("prefab", "tpl_prefab", config(json!({ "name": "snake" })))?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "snake_0" })))?;
    net.add_node("scene", "tpl_scene_graph", config(json!({ "name": "s", "expectedObjects": 1 })))?;
    net.add_node("render", "tpl_scene_render", config(json!({
        "width": 1024, "height": 1024,
        "cameraPosX": -2.0, "cameraPosY": 6.0, "cameraPosZ": 5.0,
        "cameraTargetX": 0.0, "cameraTargetY": 0.0, "cameraTargetZ": 0.0,
        "fov": 40.0,
        "bgR": 0.92, "bgG": 0.92, "bgB": 0.90,
    })))?;

    net.add_connection(wire("mc", "mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("mc", "mesh", "render", "meshes"));

    // Encode + save
    net.add_node("to_stream", "tpl_bytes_to_stream", config(json!({
        "chunkSize": 65536, "contentType": "image/raw-rgba",
    })))?;
    net.add_node("encode", "tpl_image_encode", config(json!({ "format": "png" })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "snake_shape.png" })))?;

    net.add_connection(wire("render", "output", "to_stream", "input"));
    net.add_connection(wire("to_stream", "stream", "encode", "stream"));
    net.add_connection(wire("encode", "output", "save", "input"));

    println!("SdfPath: S-curve spine, 24 segments, tapered profile");
    println!("MC: 192³, bound=4.5");
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
