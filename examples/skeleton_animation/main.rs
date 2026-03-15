//! # Skeleton Animation → Video Example
//!
//! Demonstrates the full animation-to-video pipeline:
//!
//! 1. SDF sphere → MarchingCubes → mesh (once)
//! 2. 3-bone skeleton + skin bind (once)
//! 3. Per-frame: AnimSampler → Skinning → Scene → Render (600 frames)
//! 4. Encode all frames → H.264 MP4 (20 seconds at 30fps)
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

/// Build a bounce animation clip with rotation wobble.
fn bounce_clip_json(duration: f64, fps: u32) -> Value {
    let frames = (fps as f64 * duration) as usize;
    let mut times = Vec::new();
    let mut positions = Vec::new();
    let mut rotations = Vec::new();

    for i in 0..=frames {
        let t = i as f64 / fps as f64;
        times.push(t);

        // Root: bounce on Y via sine
        let y = (t * std::f64::consts::PI * 2.0).sin().abs() * 0.5;
        positions.push(json!([0.0, y, 0.0]));

        // Spine: rotation wobble around Z
        let angle = (t * std::f64::consts::PI * 3.0).sin() * 0.2;
        let half = angle / 2.0;
        rotations.push(json!([0.0, 0.0, half.sin(), half.cos()]));
    }

    json!({
        "name": "bounce",
        "duration": duration,
        "channelCount": 2,
        "channels": [
            {
                "boneIndex": 0,
                "property": "position",
                "interpolation": "linear",
                "times": times,
                "values": positions,
            },
            {
                "boneIndex": 1,
                "property": "rotation",
                "interpolation": "linear",
                "times": times,
                "values": rotations,
            },
        ]
    })
}

/// Build skeleton JSON and compute inverse bind matrices.
fn build_skeleton() -> (Message, Message) {
    use reflow_components::animation::math_helpers::*;

    let bones = vec![
        ("root",  -1i32, [0.0f32, 0.0, 0.0]),
        ("spine",  0,    [0.0, 0.5, 0.0]),
        ("head",   1,    [0.0, 0.5, 0.0]),
    ];

    let identity_quat = [0.0f32, 0.0, 0.0, 1.0];
    let identity_scale = [1.0f32; 3];

    // Compute local bind and world bind transforms
    let mut local_bind = Vec::new();
    let mut world_bind = Vec::new();
    let mut bones_json = Vec::new();

    for (i, (name, parent, pos)) in bones.iter().enumerate() {
        let local = trs_to_mat4(*pos, identity_quat, identity_scale);
        local_bind.push(local);

        let world = if *parent >= 0 {
            mat4_mul(&world_bind[*parent as usize], &local)
        } else {
            local
        };
        world_bind.push(world);

        bones_json.push(json!({
            "index": i,
            "name": name,
            "parent": parent,
            "localBindTransform": local.to_vec(),
        }));
    }

    // Inverse bind matrices as packed bytes
    let mut ibm_bytes: Vec<u8> = Vec::with_capacity(bones.len() * 64);
    for w in &world_bind {
        let inv = mat4_inverse(w);
        for f in &inv {
            ibm_bytes.extend_from_slice(&f.to_le_bytes());
        }
    }

    let skeleton = json!({
        "name": "bouncer",
        "boneCount": bones.len(),
        "bones": bones_json,
    });

    (
        Message::object(reflow_actor::message::EncodableValue::from(skeleton)),
        Message::bytes(ibm_bytes),
    )
}

/// Auto-assign skin weights: nearest bone per vertex.
fn build_skin_weights(mesh_bytes: &[u8], bone_positions: &[[f32; 3]], max_influences: usize) -> (Message, Message) {
    let stride = 24usize;
    let vertex_count = mesh_bytes.len() / stride;
    let entry_size = 2 + 4; // u16 + f32
    let mut weights = Vec::with_capacity(vertex_count * max_influences * entry_size);

    for i in 0..vertex_count {
        let off = i * stride;
        let vx = f32::from_le_bytes(mesh_bytes[off..off+4].try_into().unwrap());
        let vy = f32::from_le_bytes(mesh_bytes[off+4..off+8].try_into().unwrap());
        let vz = f32::from_le_bytes(mesh_bytes[off+8..off+12].try_into().unwrap());

        let mut dists: Vec<(usize, f32)> = bone_positions.iter().enumerate()
            .map(|(bi, bp)| {
                let dx = vx - bp[0]; let dy = vy - bp[1]; let dz = vz - bp[2];
                (bi, dx*dx + dy*dy + dz*dz)
            })
            .collect();
        dists.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        let top: Vec<(usize, f32)> = dists.iter().take(max_influences)
            .map(|(bi, d2)| (*bi, 1.0 / (d2.sqrt() + 0.001)))
            .collect();
        let total: f32 = top.iter().map(|(_, w)| w).sum();

        for j in 0..max_influences {
            if j < top.len() {
                weights.extend_from_slice(&(top[j].0 as u16).to_le_bytes());
                weights.extend_from_slice(&(top[j].1 / total).to_le_bytes());
            } else {
                weights.extend_from_slice(&0u16.to_le_bytes());
                weights.extend_from_slice(&0.0f32.to_le_bytes());
            }
        }
    }

    let skin = json!({
        "vertexCount": vertex_count,
        "maxInfluences": max_influences,
        "inputStride": 24,
        "boneCount": bone_positions.len(),
    });

    (
        Message::object(reflow_actor::message::EncodableValue::from(skin)),
        Message::bytes(weights),
    )
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Skeleton Animation → Video Pipeline ===\n");

    let duration_secs = 5.0; // 5 second animation
    let fps = 30u32;
    let total_frames = (duration_secs * fps as f64) as usize;
    let img_size = 256u32;

    println!("Target: {:.0}s at {}fps = {} frames ({}x{})\n", duration_secs, fps, total_frames, img_size, img_size);

    // ── Step 1: Generate mesh (once) ──────────────────────────────
    print!("Generating mesh... ");
    let mut net = Network::new(NetworkConfig::default());
    for tpl in ["tpl_sdf_sphere", "tpl_sdf_marching_cubes"] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }
    net.add_node("sphere", "tpl_sdf_sphere", config(json!({ "radius": 0.6 })))?;
    net.add_node("mc", "tpl_sdf_marching_cubes",
        config(json!({ "resolution": 48, "bound": 1.0, "isoLevel": 0.0 })))?;
    net.add_connection(wire("sphere", "sdf", "mc", "sdf"));
    net.add_initial(InitialPacket {
        to: ConnectionPoint::new("sphere", "_trigger", Some(Message::Flow)),
    });
    net.start()?;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let mesh_bytes: Vec<u8> = net.read_actor_output("mc")
        .into_iter()
        .find(|(p, _)| p == "mesh")
        .map(|(_, m)| match m { Message::Bytes(b) => b.to_vec(), _ => vec![] })
        .unwrap_or_default();
    net.shutdown();

    if mesh_bytes.is_empty() {
        eprintln!("ERROR: No mesh generated");
        return Ok(());
    }
    println!("{} verts", mesh_bytes.len() / 24);

    // ── Step 2: Build skeleton + skin weights (in code) ──────────
    let (skeleton_msg, ibm_msg) = build_skeleton();

    // Bone world positions for auto-weighting (root=0,0,0 spine=0,0.5,0 head=0,1,0)
    let bone_world_positions = [[0.0f32, 0.0, 0.0], [0.0, 0.5, 0.0], [0.0, 1.0, 0.0]];
    let (skin_msg, skin_weights_msg) = build_skin_weights(&mesh_bytes, &bone_world_positions, 4);
    println!("Skeleton: 3 bones, skin weights computed");

    // ── Step 3: Build animation clip ─────────────────────────────
    let clip_json = bounce_clip_json(duration_secs, fps);
    let clip_msg = Message::object(reflow_actor::message::EncodableValue::from(clip_json));
    println!("Animation: {:.0}s bounce + wobble\n", duration_secs);

    // ── Step 4: Render all frames ────────────────────────────────
    println!("Rendering {} frames...", total_frames);
    let start = std::time::Instant::now();
    let mut rgba_frames: Vec<Vec<u8>> = Vec::with_capacity(total_frames);

    for frame_idx in 0..total_frames {
        let time = frame_idx as f64 / fps as f64;

        let mut net_f = Network::new(NetworkConfig::default());
        for tpl in [
            "tpl_animation_sampler", "tpl_skinning", "tpl_prefab",
            "tpl_instance", "tpl_scene_graph", "tpl_scene_render",
        ] {
            net_f.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
        }

        net_f.add_node("sampler", "tpl_animation_sampler", config(json!({ "loop": true })))?;
        net_f.add_node("skin", "tpl_skinning", config(json!({ "stride": 24 })))?;
        net_f.add_node("prefab", "tpl_prefab", config(json!({ "name": "ball" })))?;
        net_f.add_node("inst", "tpl_instance", config(json!({ "id": "ball_0" })))?;
        net_f.add_node("scene", "tpl_scene_graph", config(json!({ "name": "s", "expectedObjects": 1 })))?;
        net_f.add_node("render", "tpl_scene_render", config(json!({
            "width": img_size, "height": img_size,
            "cameraPosX": 2.0, "cameraPosY": 1.5, "cameraPosZ": 2.5,
            "bgR": 0.12, "bgG": 0.12, "bgB": 0.18,
        })))?;

        net_f.add_connection(wire("sampler", "bone_transforms", "skin", "bone_transforms"));
        net_f.add_connection(wire("skin", "deformed_mesh", "prefab", "mesh"));
        net_f.add_connection(wire("prefab", "prefab", "inst", "prefab"));
        net_f.add_connection(wire("inst", "object", "scene", "object"));
        net_f.add_connection(wire("scene", "scene", "render", "scene"));
        net_f.add_connection(wire("skin", "deformed_mesh", "render", "meshes"));

        // Feed all pre-computed data as IIPs
        for (node, port, msg) in [
            ("sampler", "clip", clip_msg.clone()),
            ("sampler", "time", Message::Float(time)),
            ("sampler", "skeleton", skeleton_msg.clone()),
            ("sampler", "inverse_bind_matrices", ibm_msg.clone()),
            ("skin", "mesh", Message::bytes(mesh_bytes.clone())),
            ("skin", "skin", skin_msg.clone()),
            ("skin", "skinned_mesh", skin_weights_msg.clone()),
        ] {
            net_f.add_initial(InitialPacket {
                to: ConnectionPoint::new(node, port, Some(msg)),
            });
        }

        net_f.start()?;

        let timeout = std::time::Duration::from_secs(10);
        let t0 = std::time::Instant::now();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let outputs = net_f.read_actor_output("render");
            if let Some((_, Message::Bytes(b))) = outputs.into_iter().find(|(p, _)| p == "output") {
                rgba_frames.push(b.to_vec());
                break;
            }
            if t0.elapsed() > timeout {
                eprintln!("  Frame {} timed out", frame_idx);
                break;
            }
        }
        net_f.shutdown();

        if (frame_idx + 1) % 30 == 0 || frame_idx + 1 == total_frames {
            let elapsed = start.elapsed().as_secs_f64();
            let render_fps = (frame_idx + 1) as f64 / elapsed;
            println!("  Frame {}/{} ({:.1} fps, {:.1}s elapsed)",
                frame_idx + 1, total_frames, render_fps, elapsed);
        }
    }

    let render_time = start.elapsed();
    println!("\nRendered {} frames in {:.1}s ({:.1} fps)\n",
        rgba_frames.len(), render_time.as_secs_f64(),
        rgba_frames.len() as f64 / render_time.as_secs_f64());

    if rgba_frames.is_empty() {
        eprintln!("ERROR: No frames rendered");
        return Ok(());
    }

    // Save a preview frame
    if let Some(frame) = rgba_frames.first() {
        if let Some(img) = image::RgbaImage::from_raw(img_size, img_size, frame.clone()) {
            img.save("frame_preview.png")?;
            println!("Saved: frame_preview.png");
        }
    }

    // ── Step 5: Encode to H.264 MP4 ─────────────────────────────
    println!("Encoding H.264 video ({} frames)...", rgba_frames.len());
    let enc_start = std::time::Instant::now();

    // Feed frames through FrameCollector → VideoEncoder → FileSave
    let mut net_enc = Network::new(NetworkConfig::default());
    for tpl in ["tpl_render_frame_collector", "tpl_video_encoder", "tpl_file_save"] {
        net_enc.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    net_enc.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": rgba_frames.len(),
        "width": img_size,
        "height": img_size,
        "fps": fps,
    })))?;
    net_enc.add_node("encoder", "tpl_video_encoder", config(json!({
        "fps": fps,
        "bitrate": 2000,
    })))?;
    net_enc.add_node("save", "tpl_file_save", config(json!({
        "path": "animation.mp4",
    })))?;

    net_enc.add_connection(wire("collector", "stream", "encoder", "stream"));
    net_enc.add_connection(wire("encoder", "output", "save", "input"));

    // Feed all frames as IIPs to the collector
    for (i, frame) in rgba_frames.iter().enumerate() {
        net_enc.add_initial(InitialPacket {
            to: ConnectionPoint::new("collector", "frame", Some(Message::bytes(frame.clone()))),
        });
        net_enc.add_initial(InitialPacket {
            to: ConnectionPoint::new("collector", "frame_number", Some(Message::Integer(i as i64 + 1))),
        });
    }

    net_enc.start()?;

    let mp4_path = std::path::Path::new("animation.mp4");
    let timeout = std::time::Duration::from_secs(120);
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if mp4_path.exists() && mp4_path.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        if enc_start.elapsed() > timeout {
            eprintln!("Encoding timed out");
            break;
        }
    }
    net_enc.shutdown();

    if mp4_path.exists() {
        let mp4_size = std::fs::metadata(mp4_path)?.len();
        println!("Saved: animation.mp4 ({} bytes, {:.1}s encode time)",
            mp4_size, enc_start.elapsed().as_secs_f64());
    }

    println!("\nDone!");
    Ok(())
}
