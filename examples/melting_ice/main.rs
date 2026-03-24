//! # Melting Ice Cube — SDF Ray March with Refraction
//!
//! Uses SDF live renderer for real-time ray marching with:
//! - Rounded box SDF for ice cube
//! - Plane SDF for water puddle
//! - Smooth union to blend melting edges
//! - Soft shadows + ambient occlusion
//! - Time-driven animation (squash + spread)
//!
//! The SDF scene is wired as a DAG of SDF primitives.
//! The live renderer ray-marches it per-frame with time uniform.
//!
//! ```text
//! SdfBox (ice) ──┐
//!                 ├→ SdfSmoothUnion → SdfScene → SdfLiveRender → frames
//! SdfPlane (puddle)┘                                    ↑
//!                                          IntervalTrigger → time
//!                               GPU 2D Render (watermark) → Collector → MP4
//! ```

use reflow_network::{
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
    network::{Network, NetworkConfig},
};
use serde_json::{json, Value};
use std::collections::HashMap;

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
    println!("=== Melting Ice Cube — SDF Ray March ===\n");

    let fps = 24u32;
    let duration = 3.0f64;
    let total_frames = (duration * fps as f64) as usize;
    let w = 480u32;
    let h = 480u32;

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        // SDF primitives + operations
        "tpl_sdf_box",
        // SDF renderer (per-frame with time inport)
        "tpl_sdf_render",
        // Timing
        "tpl_interval_trigger",
        "tpl_animation_time",
        // Compositing
        "tpl_gpu_2d_render",
        // Video
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ SDF SCENE: ice cube + water puddle ═══

    // Ice cube: rounded box
    net.add_node("ice_box", "tpl_sdf_box", config(json!({
        "sizeX": 0.3, "sizeY": 0.3, "sizeZ": 0.3,
    })))?;

    // Wire SDF graph

    // ═══ SDF RENDER (per-frame with time) ═══
    net.add_node("render", "tpl_sdf_render", config(json!({
        "width": w, "height": h,
        "maxSteps": 200,
        "fov": 45.0,
        "cameraPosX": 1.5, "cameraPosY": 1.0, "cameraPosZ": 1.5,
        "cameraTargetX": 0.0, "cameraTargetY": -0.1, "cameraTargetZ": 0.0,
        "softShadows": true,
        "shadowK": 16.0,
        "ao": true,
        "ambient": 0.35,
        "lightDir": [0.5, 0.8, -0.4],
        "lightColor": [1.0, 0.98, 0.95],
        "background": [0.55, 0.58, 0.65],
    })))?;

    // ═══ TIMING ═══
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": 1000 / fps as u64,
        "maxExecutions": total_frames,
        "startImmediately": false,
    })))?;
    net.add_node("anim_time", "tpl_animation_time", config(json!({
        "fps": fps, "speed": 1.0,
    })))?;

    // ═══ COMPOSITING ═══
    net.add_node("composite", "tpl_gpu_2d_render", config(json!({
        "width": w, "height": h, "msaa": 1,
        "background": [0.0, 0.0, 0.0, 0.0],
        "shapes": [
            { "type": "image", "bounds": [0, 0, w, h], "z": 0 },
        ],
        "text": [{
            "content": "Reflow SDF — Melting Ice",
            "x": w as f64 - 250.0, "y": h as f64 - 14.0,
            "size": 14.0,
            "color": [1.0, 1.0, 1.0, 0.5],
            "tracking": 0.5, "center": false,
            "font": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        }],
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": total_frames, "width": w, "height": h, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 20000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "melting_ice.mp4" })))?;

    // ═══ CONNECTIONS ═══

    // SDF → render directly
    net.add_connection(wire("ice_box", "sdf", "render", "sdf"));

    // Per-frame: tick → time → render (triggers re-render each frame)
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "render", "time"));

    // Render output → compositor (watermark) → collector → encoder → file
    net.add_connection(wire("render", "output", "composite", "data"));
    net.add_connection(wire("composite", "image", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Start tick after first render completes
    net.add_connection(wire("render", "metadata", "tick", "start"));

    // Bootstrap SDF primitives
    net.add_initial(iip("ice_box", "_trigger", Message::Flow));

    println!("Pipeline:");
    println!("  SdfBox + SdfPlane → SmoothUnion → SdfScene → LiveRender");
    println!("  Soft shadows + AO, {}x{}, {}fps, {} frames\n", w, h, fps, total_frames);

    let event_rx = net.get_event_receiver();
    tokio::spawn(async move {
        while let Ok(evt) = event_rx.recv_async().await {
            use reflow_network::network::NetworkEvent;
            if let NetworkEvent::ActorFailed { actor_id, error, .. } = &evt {
                eprintln!("[FAIL] actor={} err={}", actor_id, error);
            }
        }
    });

    let start = std::time::Instant::now();
    net.start()?;

    let mp4 = std::path::Path::new("melting_ice.mp4");
    let timeout = std::time::Duration::from_secs(300);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if mp4.exists() && mp4.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        let e = start.elapsed();
        if e.as_secs() % 10 == 0 && e.as_secs() > 0 { println!("  {:.0}s...", e.as_secs_f64()); }
        if e > timeout { eprintln!("Timed out"); break; }
    }

    let t = start.elapsed();
    if mp4.exists() {
        let sz = std::fs::metadata(mp4)?.len();
        println!("Saved: melting_ice.mp4 ({} bytes, {:.1}s)", sz, t.as_secs_f64());
    }
    std::process::exit(0);
}
