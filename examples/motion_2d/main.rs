//! # Abstract Motion — GPU SDF Rendering
//!
//! All shapes + text rendered in ONE GPU draw call via SDF instancing.
//! Single timeline drives shapes (sN_*) and text characters (cN_*).
//!
//! ```text
//! tick → timeline(shapes + staggered text) → values → Gpu2DRender → video
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== Abstract Motion — GPU SDF ===\n");

    let w = 800u32;
    let h = 450u32;
    let fps = 30u32;
    let dur = 8.0f64;
    let frames = (dur * fps as f64) as usize;
    let ms = 1000 / fps as u64;
    let cx = w as f64 / 2.0;
    let cy = h as f64 / 2.0;

    println!("{}×{}, {}fps, {:.0}s = {} frames\n", w, h, fps, dur, frames);

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger", "tpl_animation_time", "tpl_animation_timeline",
        "tpl_gpu_2d_render", "tpl_font_load", "tpl_glyph_atlas",
        "tpl_render_frame_collector", "tpl_video_encoder", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ TIMING ═══
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": ms, "maxExecutions": frames, "startImmediately": true,
    })))?;
    net.add_node("time", "tpl_animation_time", config(json!({ "fps": fps, "speed": 1.0 })))?;

    // ═══ FONT PIPELINE ═══
    // FontLoad → GlyphAtlas → Renderer (atlas + metrics persist across ticks)
    net.add_node("font", "tpl_font_load", config(json!({
        "path": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    })))?;
    net.add_node("atlas", "tpl_glyph_atlas", config(json!({
        "fontSize": 64, "sdf": true,
    })))?;

    // Build tracks programmatically to avoid json! macro recursion limit
    let mut tracks = serde_json::Map::new();

    // Helper: keyframes shorthand
    let kf = |frames: Value| -> Value { json!({ "keyframes": frames }) };
    let kfd = |frames: Value, delay: f64| -> Value { json!({ "keyframes": frames, "delay": delay }) };

    // Shape 0: Orange rect — spirals, scales in/out, rotates 2 full turns
    tracks.insert("s0_x".into(), kf(json!([
        {"time": 0.0, "value": cx, "easing": "easeInOutCubic"},
        {"time": 2.0, "value": cx + 150.0, "easing": "easeInOutSine"},
        {"time": 4.0, "value": cx, "easing": "easeInOutSine"},
        {"time": 6.0, "value": cx - 150.0, "easing": "easeInOutSine"},
        {"time": 8.0, "value": cx}
    ])));
    tracks.insert("s0_y".into(), kf(json!([
        {"time": 0.0, "value": cy, "easing": "easeInOutCubic"},
        {"time": 2.0, "value": cy - 100.0, "easing": "easeInOutSine"},
        {"time": 4.0, "value": cy + 100.0, "easing": "easeInOutSine"},
        {"time": 6.0, "value": cy - 100.0, "easing": "easeInOutSine"},
        {"time": 8.0, "value": cy}
    ])));
    tracks.insert("s0_scale".into(), kf(json!([
        {"time": 0.0, "value": 0.0, "easing": "easeOutBack"},
        {"time": 1.5, "value": 1.0},
        {"time": 6.5, "value": 1.0, "easing": "easeInCubic"},
        {"time": 8.0, "value": 0.0}
    ])));
    tracks.insert("s0_rotation".into(), kf(json!([
        {"time": 0.0, "value": 0.0}, {"time": 8.0, "value": 720.0}
    ])));

    // Shape 1: Blue circle — wide orbit
    tracks.insert("s1_x".into(), kf(json!([
        {"time": 0.0, "value": cx + 250.0, "easing": "easeInOutSine"},
        {"time": 2.0, "value": cx, "easing": "easeInOutSine"},
        {"time": 4.0, "value": cx - 250.0, "easing": "easeInOutSine"},
        {"time": 6.0, "value": cx, "easing": "easeInOutSine"},
        {"time": 8.0, "value": cx + 250.0}
    ])));
    tracks.insert("s1_y".into(), kf(json!([
        {"time": 0.0, "value": cy - 120.0, "easing": "easeInOutSine"},
        {"time": 2.0, "value": cy + 120.0, "easing": "easeInOutSine"},
        {"time": 4.0, "value": cy - 120.0, "easing": "easeInOutSine"},
        {"time": 6.0, "value": cy + 120.0, "easing": "easeInOutSine"},
        {"time": 8.0, "value": cy - 120.0}
    ])));
    tracks.insert("s1_scale".into(), kf(json!([
        {"time": 0.0, "value": 0.0},
        {"time": 0.5, "value": 0.0, "easing": "easeOutBack"},
        {"time": 1.5, "value": 1.0},
        {"time": 7.0, "value": 1.0, "easing": "easeInCubic"},
        {"time": 8.0, "value": 0.0}
    ])));

    // Shape 2: Pink circle — counter-orbit
    tracks.insert("s2_x".into(), kf(json!([
        {"time": 0.0, "value": cx - 200.0, "easing": "easeInOutSine"},
        {"time": 2.0, "value": cx + 200.0, "easing": "easeInOutSine"},
        {"time": 4.0, "value": cx - 200.0, "easing": "easeInOutSine"},
        {"time": 6.0, "value": cx + 200.0, "easing": "easeInOutSine"},
        {"time": 8.0, "value": cx - 200.0}
    ])));
    tracks.insert("s2_y".into(), kf(json!([
        {"time": 0.0, "value": cy + 100.0, "easing": "easeInOutSine"},
        {"time": 2.0, "value": cy - 100.0, "easing": "easeInOutSine"},
        {"time": 4.0, "value": cy + 100.0, "easing": "easeInOutSine"},
        {"time": 6.0, "value": cy - 100.0, "easing": "easeInOutSine"},
        {"time": 8.0, "value": cy + 100.0}
    ])));
    tracks.insert("s2_scale".into(), kf(json!([
        {"time": 0.0, "value": 0.0},
        {"time": 1.0, "value": 0.0, "easing": "easeOutBack"},
        {"time": 2.0, "value": 1.0},
        {"time": 6.5, "value": 1.0, "easing": "easeInCubic"},
        {"time": 8.0, "value": 0.0}
    ])));

    // Shape 3: Purple hex border — pulses at center, slow counter-rotation
    tracks.insert("s3_x".into(), kf(json!([{"time": 0.0, "value": cx}, {"time": 8.0, "value": cx}])));
    tracks.insert("s3_y".into(), kf(json!([{"time": 0.0, "value": cy}, {"time": 8.0, "value": cy}])));
    tracks.insert("s3_scale".into(), kf(json!([
        {"time": 0.0, "value": 0.0},
        {"time": 2.0, "value": 0.0, "easing": "easeOutElastic"},
        {"time": 3.5, "value": 1.0},
        {"time": 4.5, "value": 0.85, "easing": "easeInOutSine"},
        {"time": 5.5, "value": 1.0, "easing": "easeInOutSine"},
        {"time": 6.5, "value": 0.85, "easing": "easeInCubic"},
        {"time": 8.0, "value": 0.0}
    ])));
    tracks.insert("s3_rotation".into(), kf(json!([
        {"time": 0.0, "value": 0.0}, {"time": 8.0, "value": -180.0}
    ])));

    // Text "REFLOW" — staggered reveal at t=3.0s
    // Each character: scale 0→1 (pop in), slide up 25px, fade in.
    // 0.1s stagger between characters. Fade out before end.
    let text = "REFLOW";
    for (i, _ch) in text.chars().enumerate() {
        let d = 3.0 + i as f64 * 0.1; // stagger delay
        let hold = 3.5 - i as f64 * 0.1; // hold duration (earlier chars stay longer)

        tracks.insert(format!("c{}_scale", i), kfd(json!([
            {"time": 0.0, "value": 0, "easing": "easeOutBack"},
            {"time": 0.5, "value": 1.0},
            {"time": hold, "value": 1.0, "easing": "easeInCubic"},
            {"time": hold + 0.7, "value": 0}
        ]), d));

        tracks.insert(format!("c{}_y", i), kfd(json!([
            {"time": 0.0, "value": 25, "easing": "easeOutCubic"},
            {"time": 0.4, "value": 0}
        ]), d));

        tracks.insert(format!("c{}_opacity", i), kfd(json!([
            {"time": 0.0, "value": 0},
            {"time": 0.3, "value": 1.0},
            {"time": hold, "value": 1.0},
            {"time": hold + 0.5, "value": 0}
        ]), d));
    }

    net.add_node("tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": Value::Object(tracks),
    })))?;

    // ═══ GPU 2D RENDERER ═══
    net.add_node("render", "tpl_gpu_2d_render", config(json!({
        "width": w, "height": h,
        "background": [0.02, 0.008, 0.06, 1.0],
        "shapes": [
            {
                "type": "rect", "bounds": [0, 0, 100, 100],
                "color": [1.0, 0.53, 0.13, 1.0], "cornerRadius": 10.0,
                "shadow": { "x": 0, "y": 4, "blur": 25, "color": [1.0, 0.4, 0.0, 0.5] },
            },
            {
                "type": "circle", "bounds": [0, 0, 56, 56],
                "color": [0.25, 0.75, 1.0, 0.9],
                "shadow": { "x": 0, "y": 2, "blur": 20, "color": [0.2, 0.6, 1.0, 0.4] },
            },
            {
                "type": "circle", "bounds": [0, 0, 40, 40],
                "color": [1.0, 0.25, 0.5, 0.85],
                "shadow": { "x": 0, "y": 2, "blur": 15, "color": [1.0, 0.2, 0.4, 0.3] },
            },
            {
                "type": "rect", "bounds": [0, 0, 140, 140],
                "color": [0.0, 0.0, 0.0, 0.0], "cornerRadius": 0.0,
                "border": { "width": 2.0, "color": [0.5, 0.38, 1.0, 0.7] },
            },
        ],
        "text": [
            {
                "content": "REFLOW",
                "x": cx, "y": cy + 140.0,
                "size": 52.0,
                "color": [0.9, 0.88, 1.0, 1.0],
                "tracking": 10.0,
                "center": true,
            },
        ]
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": frames, "width": w, "height": h, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 8000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "motion_2d.mp4" })))?;

    // ═══ WIRING ═══
    net.add_connection(wire("tick", "trigger", "time", "trigger"));

    // Font pipeline: font → atlas → renderer (one-shot, persists in pools)
    net.add_connection(wire("font", "font_data", "atlas", "font_data"));
    net.add_connection(wire("atlas", "atlas", "render", "atlas"));
    net.add_connection(wire("atlas", "metrics", "render", "metrics"));
    net.add_connection(wire("atlas", "atlas_size", "render", "atlas_size"));

    // Single timeline driven by tick → atomic values output to renderer
    net.add_connection(wire("tick", "trigger", "tl", "tick"));
    net.add_connection(wire("tl", "values", "render", "values"));

    net.add_connection(wire("render", "image", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Trigger font loading on start (one-shot)
    net.add_initial(iip("font", "tick", Message::Flow));
    net.add_initial(iip("tick", "start", Message::Flow));

    println!("DAG: font → atlas → renderer + timeline → Gpu2DRender → video");
    println!("Running...\n");

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("motion_2d.mp4");
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
    let total_time = start.elapsed();

    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!("\nSaved: motion_2d.mp4 ({} bytes)", size);
        println!("Total: {:.1}s ({:.1} effective fps)",
            total_time.as_secs_f64(), frames as f64 / total_time.as_secs_f64());
    }
    println!("Done!");
    Ok(())
}
