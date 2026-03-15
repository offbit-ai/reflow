//! # Skeleton Animation → Video Example
//!
//! Single-network pipeline driven by IntervalTrigger at 30fps:
//!
//! ```text
//! SdfSphere → MarchingCubes ──mesh──→ Skinning ←─ AnimSampler ← AnimTime ← IntervalTrigger
//!             (once)           ↓                        ↑
//!                        SkinBind ─weights─→ Skinning   │
//!                        Skeleton ─skel────→ AnimSampler│
//!                        AnimClip ─clip────→ AnimSampler│
//!                                                       │
//! Skinning → Prefab → Instance → SceneGraph → SceneRender → FrameCollector → VideoEncoder → FileSave
//! ```
//!
//! Usage:
//!   cd examples/skeleton_animation && cargo run

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

/// Build bounce + wobble animation clip data.
fn bounce_clip(duration: f64, fps: u32) -> Value {
    let n = (fps as f64 * duration) as usize;
    let mut times = Vec::new();
    let mut positions = Vec::new();
    let mut rotations = Vec::new();
    for i in 0..=n {
        let t = i as f64 / fps as f64;
        times.push(t);
        positions.push(json!([0.0, (t * std::f64::consts::PI * 2.0).sin().abs() * 0.5, 0.0]));
        let a = (t * std::f64::consts::PI * 3.0).sin() * 0.2;
        let h = a / 2.0;
        rotations.push(json!([0.0, 0.0, h.sin(), h.cos()]));
    }
    json!({
        "name": "bounce", "duration": duration, "channelCount": 2,
        "channels": [
            { "boneIndex": 0, "property": "position", "interpolation": "linear", "times": &times, "values": &positions },
            { "boneIndex": 1, "property": "rotation", "interpolation": "linear", "times": &times, "values": &rotations },
        ]
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Skeleton Animation → Video ===\n");

    let duration = 5.0f64;
    let fps = 30u32;
    let total_frames = (duration * fps as f64) as usize;
    let img_size = 256u32;
    let interval_ms = 1000 / fps as u64; // ~33ms

    println!("Target: {:.0}s @ {}fps = {} frames ({}x{})\n", duration, fps, total_frames, img_size, img_size);

    // ── Build the complete pipeline as a single network ──────────

    let mut net = Network::new(NetworkConfig::default());

    // Register all actors
    let templates = [
        // Mesh generation (fires once from IIP)
        "tpl_sdf_sphere", "tpl_sdf_marching_cubes",
        // Skeleton + skin (fires once from IIP)
        "tpl_skeleton", "tpl_animation_clip", "tpl_skin_bind",
        // Animation loop (driven by interval trigger)
        "tpl_interval_trigger", "tpl_animation_time",
        "tpl_animation_sampler", "tpl_skinning",
        // Scene composition
        "tpl_prefab", "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        // Video output
        "tpl_render_frame_collector", "tpl_video_encoder", "tpl_file_save",
    ];

    for tpl in &templates {
        let actor = reflow_components::get_actor_for_template(tpl)
            .ok_or_else(|| anyhow::anyhow!("Actor not found: {}", tpl))?;
        net.register_actor_arc(tpl, actor)?;
    }

    println!("Registered {} actors", templates.len());

    // ── Nodes ────────────────────────────────────────────────────

    // Mesh generation (one-shot)
    net.add_node("sphere", "tpl_sdf_sphere", config(json!({ "radius": 0.6 })))?;
    net.add_node("mc", "tpl_sdf_marching_cubes", config(json!({
        "resolution": 16, "bound": 1.0, "isoLevel": 0.0
    })))?;

    // Skeleton + skin bind (one-shot)
    net.add_node("skeleton", "tpl_skeleton", config(json!({
        "name": "bouncer",
        "bones": [
            { "name": "root",  "parent": -1, "bindPosition": [0,0,0], "bindRotation": [0,0,0,1], "bindScale": [1,1,1] },
            { "name": "spine", "parent": 0,  "bindPosition": [0,0.5,0], "bindRotation": [0,0,0,1], "bindScale": [1,1,1] },
            { "name": "head",  "parent": 1,  "bindPosition": [0,0.5,0], "bindRotation": [0,0,0,1], "bindScale": [1,1,1] },
        ]
    })))?;
    net.add_node("clip", "tpl_animation_clip", config(json!({
        "name": "bounce", "duration": duration,
        "channels": bounce_clip(duration, fps).get("channels").unwrap().clone(),
    })))?;
    net.add_node("bind", "tpl_skin_bind", config(json!({
        "maxInfluences": 4, "stride": 24,
    })))?;

    // Animation loop
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": interval_ms, "maxExecutions": total_frames, "startImmediately": true,
    })))?;
    net.add_node("anim_time", "tpl_animation_time", config(json!({
        "fps": fps, "speed": 1.0,
    })))?;
    net.add_node("sampler", "tpl_animation_sampler", config(json!({ "loop": true })))?;
    net.add_node("skin", "tpl_skinning", config(json!({ "stride": 24 })))?;

    // Scene
    net.add_node("prefab", "tpl_prefab", config(json!({ "name": "ball" })))?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "ball_0" })))?;
    net.add_node("scene", "tpl_scene_graph", config(json!({
        "name": "anim_scene", "expectedObjects": 1,
    })))?;
    net.add_node("render", "tpl_scene_render", config(json!({
        "width": img_size, "height": img_size,
        "cameraPosX": 2.0, "cameraPosY": 1.5, "cameraPosZ": 2.5,
        "bgR": 0.12, "bgG": 0.12, "bgB": 0.18,
    })))?;

    // Video output
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": total_frames, "width": img_size, "height": img_size, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({
        "fps": fps, "bitrate": 2000,
    })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "animation.mp4" })))?;

    // ── Connections ──────────────────────────────────────────────

    // Mesh: sphere → marching cubes
    net.add_connection(wire("sphere", "sdf", "mc", "sdf"));

    // Skin bind: mesh + skeleton → bind
    net.add_connection(wire("mc", "mesh", "bind", "mesh"));
    net.add_connection(wire("skeleton", "skeleton", "bind", "skeleton"));

    // Animation sampler: clip + skeleton + ibm + time → sampler
    net.add_connection(wire("clip", "clip", "sampler", "clip"));
    net.add_connection(wire("skeleton", "skeleton", "sampler", "skeleton"));
    net.add_connection(wire("skeleton", "inverse_bind_matrices", "sampler", "inverse_bind_matrices"));

    // Interval → time → sampler
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "sampler", "time"));

    // Skinning: mesh + weights + bone_transforms → deformed mesh
    net.add_connection(wire("mc", "mesh", "skin", "mesh"));
    net.add_connection(wire("bind", "skinned_mesh", "skin", "skinned_mesh"));
    net.add_connection(wire("bind", "skin", "skin", "skin"));
    net.add_connection(wire("sampler", "bone_transforms", "skin", "bone_transforms"));

    // Scene: deformed mesh → prefab → instance → scene graph → render
    net.add_connection(wire("skin", "deformed_mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("skin", "deformed_mesh", "render", "meshes"));

    // Video: render → collector → encoder → file
    net.add_connection(wire("render", "output", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Sequence: bind completion starts the interval timer
    net.add_connection(wire("bind", "metadata", "tick", "start"));

    // ── IIPs (trigger sources) ───────────────────────────────────

    net.add_initial(iip("sphere", "_trigger", Message::Flow));
    net.add_initial(iip("skeleton", "_trigger", Message::Flow));
    net.add_initial(iip("clip", "_trigger", Message::Flow));

    let connection_count = 22;
    println!("Built graph: {} nodes, {} connections, 3 triggers\n", templates.len(), connection_count);

    // ── Execute ──────────────────────────────────────────────────

    println!("Running pipeline...");
    println!("  IntervalTrigger({}ms) → AnimTime → AnimSampler → Skinning", interval_ms);
    println!("    → Prefab → Instance → SceneGraph → SceneRender");
    println!("      → FrameCollector({}) → VideoEncoder → animation.mp4\n", total_frames);

    let start = std::time::Instant::now();
    net.start()?;

    // Wait for the MP4 file
    let mp4_path = std::path::Path::new("animation.mp4");
    let timeout = std::time::Duration::from_secs(600);

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let elapsed = start.elapsed();

        if mp4_path.exists() && mp4_path.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }

        if elapsed.as_secs() % 10 == 0 && elapsed.as_secs() > 0 {
            println!("  {:.0}s elapsed...", elapsed.as_secs());
        }

        if elapsed > timeout {
            eprintln!("Pipeline timed out after {}s", timeout.as_secs());
            break;
        }
    }

    let total_time = start.elapsed();
    net.shutdown();

    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!("\nSaved: animation.mp4 ({} bytes)", size);
        println!("Total: {:.1}s ({:.1} effective fps)",
            total_time.as_secs_f64(),
            total_frames as f64 / total_time.as_secs_f64());
    } else {
        println!("\nNo output produced. Checking errors:");
        for node in &["mc", "skeleton", "bind", "sampler", "skin", "render", "collector", "encoder", "save"] {
            for (port, msg) in net.read_actor_output(node) {
                if port == "error" { eprintln!("  [{}] {:?}", node, msg); }
            }
        }
    }

    println!("Done!");
    Ok(())
}
