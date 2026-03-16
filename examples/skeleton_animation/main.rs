//! # Skeleton Animation — Snake Slithering
//!
//! TubeMesh snake body + SDF head → MeshCombine → Skeleton → Animation → Video
//!
//! ```text
//! TubeMesh(body) + SdfSphere(head) → MeshCombine
//!   → SkinBind ← Skeleton(8 bones along body)
//!     → Skinning ← AnimSampler ← AnimTime ← IntervalTrigger
//!       → Prefab → Instance → SceneGraph → SceneRender
//!         → FrameCollector → VideoEncoder → FileSave
//! ```

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

fn snake_clip(duration: f64, fps: u32, bone_count: usize) -> Value {
    let n = (fps as f64 * duration) as usize;
    let mut channels = Vec::new();

    // Lateral undulation: traveling sine wave from head to tail.
    // Each bone rotates in Y (yaw), creating side-to-side motion.
    // Phase increases with bone index — wave propagates backward.
    // The head (bone 0) leads, tail follows with delay.
    let wave_speed = 0.6; // gentle undulation
    let wave_length = 0.5; // tight wave — 2+ S-curves across body
    let amplitude = 0.15; // moderate per-bone — 12 bones gives gentle S-curves

    for bone_idx in 0..bone_count {
        let mut times = Vec::new();
        let mut rotations = Vec::new();

        // Phase delay increases toward tail — wave travels head→tail
        let phase = bone_idx as f64 * wave_length;
        // Amplitude increases toward tail — head is steady, tail whips
        let bone_amp = amplitude * (0.5 + 0.5 * bone_idx as f64 / (bone_count - 1) as f64);

        for i in 0..=n {
            let t = i as f64 / fps as f64;
            times.push(t);

            let angle = (t * std::f64::consts::PI * 2.0 * wave_speed - phase).sin() * bone_amp;
            let half = angle / 2.0;
            // Y rotation = yaw (sideways bending in XZ plane — snake slithering)
            rotations.push(json!([0.0, half.sin(), 0.0, half.cos()]));
        }

        channels.push(json!({
            "boneIndex": bone_idx, "property": "rotation",
            "interpolation": "linear", "times": times, "values": rotations,
        }));
    }

    // Root bone (bone 0) also moves forward slowly — snake locomotion
    {
        let mut times = Vec::new();
        let mut positions = Vec::new();
        for i in 0..=n {
            let t = i as f64 / fps as f64;
            times.push(t);
            // Slow forward movement along X
            let x = -3.0 + t * 0.3; // slow forward crawl
            positions.push(json!([x, 0.0, 0.0]));
        }
        channels.push(json!({
            "boneIndex": 0, "property": "position",
            "interpolation": "linear", "times": times, "values": positions,
        }));
    }

    json!({ "name": "slither", "duration": duration, "channelCount": channels.len(), "channels": channels })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Snake Skeleton Animation → Video ===\n");

    let bone_count = 12;
    let duration = 4.0f64;
    let fps = 30u32;
    let total_frames = (duration * fps as f64) as usize;
    let img_size = 1024u32;
    let interval_ms = 1000 / fps as u64;

    println!("Target: {:.0}s @ {}fps = {} frames, {} bones\n", duration, fps, total_frames, bone_count);

    let mut net = Network::new(NetworkConfig::default());

    // Register all actors
    for tpl in [
        // Mesh generation + coloring
        "tpl_tube_mesh", "tpl_noise_generator", "tpl_vertex_color",
        // Skeleton + animation
        "tpl_skeleton", "tpl_animation_clip", "tpl_skin_bind",
        "tpl_interval_trigger", "tpl_animation_time", "tpl_animation_sampler", "tpl_skinning",
        // Scene
        "tpl_prefab", "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        // Video
        "tpl_render_frame_collector", "tpl_video_encoder", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ MESH: body with procedural coloring ═══
    net.add_node("body", "tpl_tube_mesh", config(json!({
        "path": "M -3,0 C -1.5,-0.6 0.5,0.6 2,0 C 3,-0.3 4,0.2 5,0",
        "profile": [0.22, 0.20, 0.18, 0.17, 0.17, 0.15, 0.12, 0.09, 0.05, 0.02],
        "segments": 48,
        "rings": 16,
        "plane": "xz",
    })))?;

    net.add_node("noise", "tpl_noise_generator", config(json!({
        "noiseType": "perlin", "width": 64, "height": 64,
        "scale": 4.0, "octaves": 3, "persistence": 0.5,
    })))?;

    net.add_node("colorize", "tpl_vertex_color", config(json!({
        "color1": [0.35, 0.42, 0.25],
        "color2": [0.15, 0.18, 0.08],
        "noiseScale": 6.0, "noiseWidth": 64,
    })))?;

    net.add_connection(wire("body", "mesh", "colorize", "mesh"));
    net.add_connection(wire("body", "uv", "colorize", "uv"));
    net.add_connection(wire("noise", "output", "colorize", "noise"));

    // ═══ SKELETON: 8 bones along the body path ═══
    let body_len = 8.0f64; // approximate path length
    let bone_spacing = body_len / (bone_count - 1) as f64;
    let mut bones = Vec::new();
    for i in 0..bone_count {
        let x = -3.0 + i as f64 * bone_spacing;
        bones.push(json!({
            "name": format!("bone_{}", i),
            "parent": if i == 0 { -1 } else { i as i64 - 1 },
            "bindPosition": [if i == 0 { x } else { bone_spacing }, 0.0, 0.0],
            "bindRotation": [0, 0, 0, 1],
            "bindScale": [1, 1, 1],
        }));
    }

    net.add_node("skeleton", "tpl_skeleton", config(json!({
        "name": "snake_spine", "bones": bones,
    })))?;

    let clip_data = snake_clip(duration, fps, bone_count);
    net.add_node("clip", "tpl_animation_clip", config(json!({
        "name": "slither", "duration": duration,
        "channels": clip_data.get("channels").unwrap().clone(),
    })))?;

    net.add_node("bind", "tpl_skin_bind", config(json!({ "maxInfluences": 3, "stride": 36 })))?;

    // ═══ ANIMATION LOOP ═══
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": interval_ms, "maxExecutions": total_frames, "startImmediately": true,
    })))?;
    net.add_node("anim_time", "tpl_animation_time", config(json!({ "fps": fps, "speed": 1.0 })))?;
    net.add_node("sampler", "tpl_animation_sampler", config(json!({ "loop": true })))?;
    net.add_node("skin", "tpl_skinning", config(json!({ "stride": 36 })))?;

    // ═══ SCENE ═══
    net.add_node("prefab", "tpl_prefab", config(json!({
        "name": "snake",
        "stride": 36,
    })))?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "snake_0" })))?;
    net.add_node("scene", "tpl_scene_graph", config(json!({ "name": "snake_scene", "expectedObjects": 1 })))?;
    net.add_node("render", "tpl_scene_render", config(json!({
        "width": img_size, "height": img_size,
        "cameraPosX": 1.0, "cameraPosY": 10.0, "cameraPosZ": 2.0,
        "cameraTargetX": 1.0, "cameraTargetY": 0.0, "cameraTargetZ": 0.0,
        "fov": 45.0,
        "bgR": 0.08, "bgG": 0.08, "bgB": 0.12,
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": total_frames, "width": img_size, "height": img_size, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 2000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "snake_animation.mp4" })))?;

    // ═══ CONNECTIONS ═══

    // Mesh → skin bind
    net.add_connection(wire("colorize", "colored_mesh", "bind", "mesh"));
    net.add_connection(wire("skeleton", "skeleton", "bind", "skeleton"));

    // Animation sampler
    net.add_connection(wire("clip", "clip", "sampler", "clip"));
    net.add_connection(wire("skeleton", "skeleton", "sampler", "skeleton"));
    net.add_connection(wire("skeleton", "inverse_bind_matrices", "sampler", "inverse_bind_matrices"));
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "sampler", "time"));

    // Skinning
    net.add_connection(wire("colorize", "colored_mesh", "skin", "mesh"));
    net.add_connection(wire("bind", "skinned_mesh", "skin", "skinned_mesh"));
    net.add_connection(wire("bind", "skin", "skin", "skin"));
    net.add_connection(wire("sampler", "bone_transforms", "skin", "bone_transforms"));

    // Scene render
    net.add_connection(wire("skin", "deformed_mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("skin", "deformed_mesh", "render", "meshes"));

    // Video output
    net.add_connection(wire("render", "output", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Sequencing: bind completion starts the interval timer
    net.add_connection(wire("bind", "metadata", "tick", "start"));

    // IIPs
    net.add_initial(iip("body", "_trigger", Message::Flow));
    net.add_initial(iip("noise", "_trigger", Message::Flow));
    net.add_initial(iip("skeleton", "_trigger", Message::Flow));
    net.add_initial(iip("clip", "_trigger", Message::Flow));

    println!("Pipeline: TubeMesh + SDF head → MeshCombine → Skeleton → Animation → Video");
    println!("Running...\n");

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
