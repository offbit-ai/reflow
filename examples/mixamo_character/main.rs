//! # Mixamo Character Animation — FBX Import → Skinned Render → Video
//!
//! Loads a Mixamo FBX character file, extracts mesh/skeleton/animation/skin,
//! plays the animation with skeletal skinning, renders with GPU scene renderer,
//! and encodes to MP4.
//!
//! ```text
//! FileLoad → FbxImport
//!   → mesh ─────────────────────────────→ Skinning
//!   → skin ─────────────────────────────→ Skinning (weights)
//!   → skin_descriptor ──────────────────→ Skinning (metadata)
//!   → skeleton ─────────────────────────→ AnimSampler
//!   → clip ─────────────────────────────→ AnimSampler
//!   → inverse_bind_matrices ────────────→ AnimSampler
//!
//! IntervalTrigger → AnimTime → AnimSampler → bone_transforms → Skinning
//!   → deformed_mesh → Prefab → Instance → SceneGraph → SceneRender
//!   → deformed_mesh → SceneRender (mesh cache)
//!     → FrameCollector → VideoEncoder → FileSave
//! ```

use reflow_network::{
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value};
use std::collections::HashMap;

fn config(cfg: Value) -> Option<HashMap<String, Value>> {
    if let Value::Object(map) = cfg {
        Some(map.into_iter().collect())
    } else {
        None
    }
}
fn wire(fa: &str, fp: &str, ta: &str, tp: &str) -> Connector {
    Connector {
        from: ConnectionPoint {
            actor: fa.to_owned(),
            port: fp.to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: ta.to_owned(),
            port: tp.to_owned(),
            ..Default::default()
        },
    }
}
fn iip(node: &str, port: &str, msg: Message) -> InitialPacket {
    InitialPacket {
        to: ConnectionPoint::new(node, port, Some(msg)),
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let fbx_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "../../assets/leg_sweep.fbx".to_string());

    println!("=== Mixamo Character Animation → Video ===\n");

    let fps = 60u32;
    let duration = 3.0f64; // slightly longer than clip (2.867s) for full loop
    let total_frames = (duration * fps as f64) as usize;
    let w = 720u32;
    let h = 720u32;
    let interval_ms = 1000 / fps as u64;

    println!(
        "Source: {}\nTarget: {:.1}s @ {}fps = {} frames, {}x{}",
        fbx_path, duration, fps, total_frames, w, h
    );

    let mut net = Network::new(NetworkConfig::default());

    // Register all actors
    for tpl in [
        // File + import
        "tpl_file_load",
        "tpl_fbx_import",
        // Animation pipeline
        "tpl_interval_trigger",
        "tpl_animation_time",
        "tpl_animation_sampler",
        "tpl_skinning",
        // Scene
        "tpl_prefab",
        "tpl_instance",
        "tpl_scene_graph",
        "tpl_scene_render",
        // Compositing (layer + watermark)
        "tpl_gpu_2d_render",
        // Flow control
        "tpl_signal",
        "tpl_subscriber",
        // Video
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ FILE LOAD + FBX IMPORT ═══
    net.add_node("load", "tpl_file_load", config(json!({ "path": fbx_path })))?;
    net.add_node("fbx", "tpl_fbx_import", None)?;

    net.add_connection(wire("load", "output", "fbx", "file_data"));

    // ═══ ANIMATION LOOP ═══
    net.add_node(
        "tick",
        "tpl_interval_trigger",
        config(json!({
            "interval": interval_ms,
            "maxExecutions": total_frames,
            "startImmediately": false,
        })),
    )?;
    net.add_node(
        "anim_time",
        "tpl_animation_time",
        config(json!({ "fps": fps, "speed": 1.0 })),
    )?;
    net.add_node(
        "sampler",
        "tpl_animation_sampler",
        config(json!({ "loop": true })),
    )?;
    net.add_node("skin", "tpl_skinning", config(json!({ "stride": 32 })))?;

    // ═══ SCENE ═══
    net.add_node(
        "prefab",
        "tpl_prefab",
        config(json!({ "name": "character", "stride": 32 })),
    )?;
    net.add_node("inst", "tpl_instance", config(json!({ "id": "character_0" })))?;
    net.add_node(
        "scene",
        "tpl_scene_graph",
        config(json!({ "name": "character_scene", "expectedObjects": 1 })),
    )?;
    // Camera: Mixamo character ~186cm tall (cm units), center at Y≈90
    net.add_node(
        "render",
        "tpl_scene_render",
        config(json!({
            "width": w, "height": h,
            "cameraPosX": 150.0, "cameraPosY": 100.0, "cameraPosZ": 350.0,
            "cameraTargetX": 0.0, "cameraTargetY": 80.0, "cameraTargetZ": 0.0,
            "fov": 45.0, "msaa": 1,
            "near": 1.0, "far": 5000.0,
            "bgR": 0.12, "bgG": 0.12, "bgB": 0.16,
        })),
    )?;

    // ═══ COMPOSITOR — scene frame as layer + watermark text ═══
    net.add_node(
        "composite",
        "tpl_gpu_2d_render",
        config(json!({
            "width": w, "height": h, "msaa": 1,
            "background": [0.0, 0.0, 0.0, 0.0],
            "shapes": [
                { "type": "image", "bounds": [0, 0, w, h], "z": 0 },
            ],
            "text": [{
                "content": "Reflow + Mixamo",
                "x": w as f64 - 155.0, "y": h as f64 - 14.0,
                "size": 14.0,
                "color": [1.0, 1.0, 1.0, 0.5],
                "tracking": 0.5, "center": false,
                "font": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            }],
        })),
    )?;

    // ═══ VIDEO ═══
    net.add_node(
        "collector",
        "tpl_render_frame_collector",
        config(json!({
            "totalFrames": total_frames,
            "width": w, "height": h, "fps": fps,
        })),
    )?;
    net.add_node(
        "encoder",
        "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 20000 })),
    )?;
    net.add_node(
        "save",
        "tpl_file_save",
        config(json!({ "path": "mixamo_character.mp4" })),
    )?;

    // ═══ SIGNAL: prefab loaded → start animation ═══
    net.add_node("sig_ready", "tpl_signal", config(json!({ "name": "prefab_ready" })))?;
    net.add_node("sub_ready", "tpl_subscriber", config(json!({ "signal": "prefab_ready" })))?;

    // ═══ CONNECTIONS ═══

    // FBX → Animation sampler (static data, cached once)
    net.add_connection(wire("fbx", "clip", "sampler", "clip"));
    net.add_connection(wire("fbx", "skeleton", "sampler", "skeleton"));
    net.add_connection(wire(
        "fbx",
        "inverse_bind_matrices",
        "sampler",
        "inverse_bind_matrices",
    ));

    // FBX → Scene render (diffuse texture, cached once)
    net.add_connection(wire("fbx", "texture", "render", "texture"));

    // FBX → Skinning (static data, cached once)
    net.add_connection(wire("fbx", "mesh", "skin", "mesh"));
    net.add_connection(wire("fbx", "skin", "skin", "skinned_mesh"));
    net.add_connection(wire("fbx", "skin_descriptor", "skin", "skin"));

    // Animation time → sampler (per frame)
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "sampler", "time"));

    // Sampler → skinning (per frame)
    net.add_connection(wire("sampler", "bone_transforms", "skin", "bone_transforms"));

    // Skinning → scene render (per frame)
    net.add_connection(wire("skin", "deformed_mesh", "prefab", "mesh"));
    net.add_connection(wire("prefab", "prefab", "inst", "prefab"));
    net.add_connection(wire("inst", "object", "scene", "object"));
    net.add_connection(wire("scene", "scene", "render", "scene"));
    net.add_connection(wire("skin", "deformed_mesh", "render", "meshes"));

    // Scene render → compositor (layer image) → video
    net.add_connection(wire("render", "output", "composite", "data"));
    net.add_connection(wire("composite", "image", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Sequencing: FBX import done → start animation tick
    net.add_connection(wire("fbx", "metadata", "tick", "start"));
    // Signal when prefab loaded (for diagnostics/future use)
    net.add_connection(wire("prefab", "metadata", "sig_ready", "data"));

    // Bootstrap
    net.add_initial(iip("load", "_trigger", Message::Flow));

    println!("\nPipeline: FBX → Skeleton/Animation → Skinning → SceneRender → MP4");
    println!("Running...\n");

    let event_rx = net.get_event_receiver();
    tokio::spawn(async move {
        while let Ok(evt) = event_rx.recv_async().await {
            use reflow_network::network::NetworkEvent;
            match &evt {
                NetworkEvent::ActorFailed {
                    actor_id, error, ..
                } => {
                    eprintln!("[FAIL] actor={} err={}", actor_id, error);
                }
                _ => {}
            }
        }
    });

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("mixamo_character.mp4");
    let timeout = std::time::Duration::from_secs(300);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if mp4_path.exists() && mp4_path.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        let e = start.elapsed();
        if e.as_secs() % 10 == 0 && e.as_secs() > 0 {
            println!("  {:.0}s...", e.as_secs_f64());
        }
        if e > timeout {
            eprintln!("Timed out");
            break;
        }
    }

    net.shutdown();
    let total_time = start.elapsed();

    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!(
            "\nSaved: mixamo_character.mp4 ({} bytes, {:.1}s)",
            size,
            total_time.as_secs_f64()
        );
        println!(
            "Effective: {:.1} fps",
            total_frames as f64 / total_time.as_secs_f64()
        );
    }
    println!("Done!");
    std::process::exit(0);
}
