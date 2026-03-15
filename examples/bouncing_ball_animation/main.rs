//! # Bouncing Ball Animation → Video
//!
//! Clip-driven animation: a sphere mesh bounces via position keyframes.
//! No skeleton or skinning needed — the AnimSampler drives bone 0's
//! position which the InstanceActor picks up as its transform.
//!
//! ```text
//! SdfSphere → MarchingCubes → Prefab → Instance → SceneGraph → SceneRender
//!                                                    ↑
//! IntervalTrigger → AnimTime → AnimSampler (bounce clip)
//!   → FrameCollector → VideoEncoder → FileSave
//! ```
//!
//! Usage:
//!   cd examples/bouncing_ball_animation && cargo run

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

/// Bounce animation: root bone Y position oscillates via abs(sin).
fn bounce_clip(duration: f64, fps: u32) -> Value {
    let n = (fps as f64 * duration) as usize;
    let mut times = Vec::new();
    let mut positions = Vec::new();
    for i in 0..=n {
        let t = i as f64 / fps as f64;
        times.push(t);
        let y = (t * std::f64::consts::PI * 2.0).sin().abs() * 1.5;
        positions.push(json!([0.0, y, 0.0]));
    }
    json!({
        "name": "bounce", "duration": duration, "channelCount": 1,
        "channels": [{
            "boneIndex": 0, "property": "position", "interpolation": "linear",
            "times": &times, "values": &positions,
        }]
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Bouncing Ball → Video ===\n");

    let duration = 3.0f64;
    let fps = 30u32;
    let total_frames = (duration * fps as f64) as usize;
    let img_size = 256u32;
    let interval_ms = 1000 / fps as u64;

    println!("Target: {:.0}s @ {}fps = {} frames\n", duration, fps, total_frames);

    let mut net = Network::new(NetworkConfig::default());

    let templates = [
        "tpl_sdf_sphere", "tpl_sdf_marching_cubes",
        "tpl_skeleton", "tpl_animation_clip", "tpl_skin_bind",
        "tpl_interval_trigger", "tpl_animation_time", "tpl_animation_sampler", "tpl_skinning",
        "tpl_prefab", "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        "tpl_render_frame_collector", "tpl_video_encoder", "tpl_file_save",
    ];
    for tpl in &templates {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ── Nodes ────────────────────────────────────────────────────

    // Mesh (one-shot)
    net.add_node("sphere", "tpl_sdf_sphere", config(json!({ "radius": 0.4 })))?;
    net.add_node("mc", "tpl_sdf_marching_cubes", config(json!({
        "resolution": 24, "bound": 0.6, "isoLevel": 0.0
    })))?;

    // Skeleton (single root bone — just to satisfy sampler/skinning pipeline)
    net.add_node("skeleton", "tpl_skeleton", config(json!({
        "name": "ball_rig",
        "bones": [
            { "name": "root", "parent": -1, "bindPosition": [0,0,0], "bindRotation": [0,0,0,1], "bindScale": [1,1,1] },
        ]
    })))?;
    net.add_node("clip", "tpl_animation_clip", config(json!({
        "name": "bounce", "duration": duration,
        "channels": bounce_clip(duration, fps).get("channels").unwrap().clone(),
    })))?;
    net.add_node("bind", "tpl_skin_bind", config(json!({ "maxInfluences": 4, "stride": 24 })))?;

    // Animation loop
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": interval_ms, "maxExecutions": total_frames, "startImmediately": true,
    })))?;
    net.add_node("anim_time", "tpl_animation_time", config(json!({ "fps": fps, "speed": 1.0 })))?;
    net.add_node("sampler", "tpl_animation_sampler", config(json!({ "loop": true })))?;
    net.add_node("skin", "tpl_skinning", config(json!({ "stride": 24 })))?;

    // Scene
    net.add_node("prefab", "tpl_prefab", config(json!({ "name": "ball" })))?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "ball_0" })))?;
    net.add_node("scene", "tpl_scene_graph", config(json!({ "name": "bounce_scene", "expectedObjects": 1 })))?;
    net.add_node("render", "tpl_scene_render", config(json!({
        "width": img_size, "height": img_size,
        "cameraPosX": 3.0, "cameraPosY": 2.0, "cameraPosZ": 3.0,
        "bgR": 0.1, "bgG": 0.1, "bgB": 0.15,
    })))?;

    // Video
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": total_frames, "width": img_size, "height": img_size, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 1500 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "bouncing_ball.mp4" })))?;

    // ── Connections ──────────────────────────────────────────────

    net.add_connection(wire("sphere", "sdf", "mc", "sdf"));
    net.add_connection(wire("mc", "mesh", "bind", "mesh"));
    net.add_connection(wire("skeleton", "skeleton", "bind", "skeleton"));
    net.add_connection(wire("clip", "clip", "sampler", "clip"));
    net.add_connection(wire("skeleton", "skeleton", "sampler", "skeleton"));
    net.add_connection(wire("skeleton", "inverse_bind_matrices", "sampler", "inverse_bind_matrices"));
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "sampler", "time"));
    net.add_connection(wire("mc", "mesh", "skin", "mesh"));
    net.add_connection(wire("bind", "skinned_mesh", "skin", "skinned_mesh"));
    net.add_connection(wire("bind", "skin", "skin", "skin"));
    net.add_connection(wire("sampler", "bone_transforms", "skin", "bone_transforms"));
    net.add_connection(wire("skin", "deformed_mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("skin", "deformed_mesh", "render", "meshes"));
    net.add_connection(wire("render", "output", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));
    net.add_connection(wire("bind", "metadata", "tick", "start"));

    // IIPs
    net.add_initial(iip("sphere", "_trigger", Message::Flow));
    net.add_initial(iip("skeleton", "_trigger", Message::Flow));
    net.add_initial(iip("clip", "_trigger", Message::Flow));

    println!("Built graph: {} actors, 22 connections\n", templates.len());
    println!("Running...");

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("bouncing_ball.mp4");
    let timeout = std::time::Duration::from_secs(120);
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
    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!("\nSaved: bouncing_ball.mp4 ({} bytes, {:.1}s)", size, start.elapsed().as_secs_f64());
    }
    println!("Done!");
    Ok(())
}
