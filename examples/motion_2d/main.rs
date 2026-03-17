//! # 2D Motion Graphics → Video (Pure Reflow DAG)
//!
//! Minimal pipeline: Background → FrameCollector → VideoEncoder → FileSave

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
    println!("=== 2D Motion Graphics → Video ===\n");

    let width = 640u32;
    let height = 360u32;
    let fps = 30u32;
    let duration = 2.0f64;
    let total_frames = (duration * fps as f64) as usize;
    let interval_ms = 1000 / fps as u64;

    println!("{}×{}, {}fps, {:.0}s = {} frames\n", width, height, fps, duration, total_frames);

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger",
        "tpl_background",
        "tpl_animation_time",
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // Tick
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": interval_ms,
        "maxExecutions": total_frames,
        "startImmediately": true,
    })))?;

    net.add_node("anim_time", "tpl_animation_time", config(json!({
        "fps": fps, "speed": 1.0,
    })))?;

    // Background: generates RGBA every tick
    net.add_node("bg", "tpl_background", config(json!({
        "type": "radialGradient",
        "from": [30, 15, 80, 255],
        "to": [8, 4, 25, 255],
        "center": [0.5, 0.4],
        "radius": 1.0,
        "width": width,
        "height": height,
    })))?;

    // Video pipeline
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": total_frames,
        "width": width,
        "height": height,
        "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({
        "fps": fps, "bitrate": 2000,
    })))?;
    net.add_node("save", "tpl_file_save", config(json!({
        "path": "motion_2d.mp4",
    })))?;

    // Connections
    net.add_connection(wire("tick", "trigger", "anim_time", "trigger"));
    net.add_connection(wire("anim_time", "time", "bg", "tick"));
    net.add_connection(wire("bg", "image", "collector", "frame"));
    net.add_connection(wire("anim_time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    println!("DAG: tick → Background → FrameCollector → VideoEncoder → FileSave");
    println!("Running...\n");

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("motion_2d.mp4");
    let timeout = std::time::Duration::from_secs(60);
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
        println!("\nSaved: motion_2d.mp4 ({} bytes)", size);
        println!("Total: {:.1}s ({:.1} effective fps)",
            total_time.as_secs_f64(), total_frames as f64 / total_time.as_secs_f64());
    }
    println!("Done!");
    Ok(())
}
