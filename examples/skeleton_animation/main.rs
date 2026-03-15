//! # Skeleton Animation — Snake
//!
//! A single elongated SDF capsule with 8 bones along its length.
//! Serpentine wave animation bends the body into an S-curve.
//!
//! ```text
//! SdfCapsule → MarchingCubes → SkinBind ← Skeleton(8 bones)
//!   → Skinning ← AnimSampler ← AnimTime ← IntervalTrigger
//!     → Prefab → Instance → SceneGraph → SceneRender
//!       → FrameCollector → VideoEncoder → FileSave
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

/// Serpentine wave: each bone rotates around Y with phase offset.
fn snake_clip(duration: f64, fps: u32, bone_count: usize) -> Value {
    let n = (fps as f64 * duration) as usize;
    let mut channels = Vec::new();

    for bone_idx in 0..bone_count {
        let mut times = Vec::new();
        let mut rotations = Vec::new();
        // Wave travels from head to tail with wider spacing for clear S-shape
        let phase = bone_idx as f64 * std::f64::consts::PI * 0.35;
        // Per-bone rotation — cumulative through hierarchy creates S-curve
        let amplitude = 0.22;

        for i in 0..=n {
            let t = i as f64 / fps as f64;
            times.push(t);
            let angle = (t * std::f64::consts::PI * 2.0 - phase).sin() * amplitude;
            let half = angle / 2.0;
            // Z-axis rotation bends the Y-oriented body left/right
            rotations.push(json!([0.0, 0.0, half.sin(), half.cos()]));
        }

        channels.push(json!({
            "boneIndex": bone_idx,
            "property": "rotation",
            "interpolation": "linear",
            "times": times,
            "values": rotations,
        }));
    }

    json!({
        "name": "serpentine", "duration": duration,
        "channelCount": channels.len(), "channels": channels,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Skeleton Animation — Snake ===\n");

    let bone_count = 10;
    let body_length = 5.0f32;
    let body_radius = 0.3f32;
    let duration = 4.0f64;
    let fps = 30u32;
    let total_frames = (duration * fps as f64) as usize;
    let img_size = 512u32;
    let interval_ms = 1000 / fps as u64;

    println!("Target: {:.0}s @ {}fps = {} frames, {} bones\n",
        duration, fps, total_frames, bone_count);

    let mut net = Network::new(NetworkConfig::default());

    // Register actors
    for tpl in [
        "tpl_sdf_capsule", "tpl_sdf_marching_cubes",
        "tpl_skeleton", "tpl_animation_clip", "tpl_skin_bind",
        "tpl_interval_trigger", "tpl_animation_time", "tpl_animation_sampler", "tpl_skinning",
        "tpl_prefab", "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        "tpl_render_frame_collector", "tpl_video_encoder", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ── Mesh: single elongated capsule ───────────────────────────
    // Capsule is along Y by default. We keep it along Y for the mesh,
    // the bones are also along Y, and the animation rotates around Y
    // for horizontal slithering (the camera looks from above).
    net.add_node("capsule", "tpl_sdf_capsule", config(json!({
        "radius": body_radius, "height": body_length,
    })))?;
    net.add_node("mc", "tpl_sdf_marching_cubes", config(json!({
        "resolution": 80, "bound": 3.0, "isoLevel": 0.0,
    })))?;

    // ── Skeleton: 8 bones along Y ───────────────────────────────
    let bone_spacing = body_length as f64 / (bone_count - 1) as f64;
    let start_y = -(body_length as f64) / 2.0;
    let mut bones = Vec::new();
    for i in 0..bone_count {
        let y = start_y + i as f64 * bone_spacing;
        bones.push(json!({
            "name": format!("bone_{}", i),
            "parent": if i == 0 { -1 } else { i as i64 - 1 },
            "bindPosition": [0.0, if i == 0 { y } else { bone_spacing }, 0.0],
            "bindRotation": [0, 0, 0, 1],
            "bindScale": [1, 1, 1],
        }));
    }

    net.add_node("skeleton", "tpl_skeleton", config(json!({
        "name": "snake_spine", "bones": bones,
    })))?;

    let clip_data = snake_clip(duration, fps, bone_count);
    net.add_node("clip", "tpl_animation_clip", config(json!({
        "name": "serpentine", "duration": duration,
        "channels": clip_data.get("channels").unwrap().clone(),
    })))?;

    net.add_node("bind", "tpl_skin_bind", config(json!({ "maxInfluences": 3, "stride": 24 })))?;

    // ── Animation loop ──────────────────────────────────────────
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": interval_ms, "maxExecutions": total_frames, "startImmediately": true,
    })))?;
    net.add_node("anim_time", "tpl_animation_time", config(json!({ "fps": fps, "speed": 1.0 })))?;
    net.add_node("sampler", "tpl_animation_sampler", config(json!({ "loop": true })))?;
    net.add_node("skin", "tpl_skinning", config(json!({ "stride": 24 })))?;

    // ── Scene ───────────────────────────────────────────────────
    net.add_node("prefab", "tpl_prefab", config(json!({ "name": "snake" })))?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "snake_0" })))?;
    net.add_node("scene", "tpl_scene_graph", config(json!({ "name": "snake_scene", "expectedObjects": 1 })))?;
    // Camera: elevated view looking down to see the S-curve slithering
    net.add_node("render", "tpl_scene_render", config(json!({
        "width": img_size, "height": img_size,
        "cameraPosX": 0.0, "cameraPosY": 1.0, "cameraPosZ": 8.0,
        "cameraTargetX": 0.0, "cameraTargetY": 0.0, "cameraTargetZ": 0.0,
        "bgR": 0.05, "bgG": 0.07, "bgB": 0.09,
    })))?;

    // ── Video ───────────────────────────────────────────────────
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": total_frames, "width": img_size, "height": img_size, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 2000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "snake_animation.mp4" })))?;

    // ── Connections ──────────────────────────────────────────────
    // Mesh
    net.add_connection(wire("capsule", "sdf", "mc", "sdf"));
    // Skin bind
    net.add_connection(wire("mc", "mesh", "bind", "mesh"));
    net.add_connection(wire("skeleton", "skeleton", "bind", "skeleton"));
    // Animation
    net.add_connection(wire("clip", "clip", "sampler", "clip"));
    net.add_connection(wire("skeleton", "skeleton", "sampler", "skeleton"));
    net.add_connection(wire("skeleton", "inverse_bind_matrices", "sampler", "inverse_bind_matrices"));
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "sampler", "time"));
    // Skinning
    net.add_connection(wire("mc", "mesh", "skin", "mesh"));
    net.add_connection(wire("bind", "skinned_mesh", "skin", "skinned_mesh"));
    net.add_connection(wire("bind", "skin", "skin", "skin"));
    net.add_connection(wire("sampler", "bone_transforms", "skin", "bone_transforms"));
    // Scene
    net.add_connection(wire("skin", "deformed_mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("skin", "deformed_mesh", "render", "meshes"));
    // Video
    net.add_connection(wire("render", "output", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));
    // Sequencing
    net.add_connection(wire("bind", "metadata", "tick", "start"));

    // IIPs
    net.add_initial(iip("capsule", "_trigger", Message::Flow));
    net.add_initial(iip("skeleton", "_trigger", Message::Flow));
    net.add_initial(iip("clip", "_trigger", Message::Flow));

    println!("Pipeline: SdfCapsule(r={}, h={}) → MC → SkinBind({} bones) → Skinning → Render → MP4\n",
        body_radius, body_length, bone_count);
    println!("Running...");

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("snake_animation.mp4");
    let timeout = std::time::Duration::from_secs(300);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if mp4_path.exists() && mp4_path.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        let e = start.elapsed();
        if e.as_secs() % 10 == 0 && e.as_secs() > 0 { println!("  {:.0}s...", e.as_secs()); }
        if e > timeout { eprintln!("Timed out"); break; }
    }

    net.shutdown();
    let total_time = start.elapsed();

    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!("\nSaved: snake_animation.mp4 ({} bytes)", size);
        println!("Total: {:.1}s ({:.1} effective fps)",
            total_time.as_secs_f64(), total_frames as f64 / total_time.as_secs_f64());
    }
    println!("Done!");
    Ok(())
}
