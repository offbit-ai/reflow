//! # Abstract Motion — GPU SDF Rendering
//!
//! All shapes rendered in ONE GPU draw call via SDF instancing.
//! Timelines drive per-shape transforms. No CPU rasterization.
//!
//! ```text
//! tick → 4 timelines → anim_0..3 → Gpu2DRender(shapes config) → image
//!                                     → FrameCollector → VideoEncoder → FileSave
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
        "tpl_gpu_2d_render",
        "tpl_render_frame_collector", "tpl_video_encoder", "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ TIMING ═══
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": ms, "maxExecutions": frames, "startImmediately": true,
    })))?;
    net.add_node("time", "tpl_animation_time", config(json!({ "fps": fps, "speed": 1.0 })))?;

    // ═══ TIMELINES ═══

    // Shape 0: Orange rect — spirals, scales in/out, rotates 2 full turns
    net.add_node("tl_0", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [
                { "time": 0.0, "value": cx, "easing": "easeInOutCubic" },
                { "time": 2.0, "value": cx + 150.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cx, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cx - 150.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cx },
            ]},
            "y": { "keyframes": [
                { "time": 0.0, "value": cy, "easing": "easeInOutCubic" },
                { "time": 2.0, "value": cy - 100.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cy + 100.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cy - 100.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cy },
            ]},
            "scale": { "keyframes": [
                { "time": 0.0, "value": 0.0, "easing": "easeOutBack" },
                { "time": 1.5, "value": 1.0 },
                { "time": 6.5, "value": 1.0, "easing": "easeInCubic" },
                { "time": 8.0, "value": 0.0 },
            ]},
            "rotation": { "keyframes": [
                { "time": 0.0, "value": 0.0 }, { "time": 8.0, "value": 720.0 },
            ]},
        }
    })))?;

    // Shape 1: Blue circle — wide orbit
    net.add_node("tl_1", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [
                { "time": 0.0, "value": cx + 250.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cx, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cx - 250.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cx, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cx + 250.0 },
            ]},
            "y": { "keyframes": [
                { "time": 0.0, "value": cy - 120.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cy + 120.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cy - 120.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cy + 120.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cy - 120.0 },
            ]},
            "scale": { "keyframes": [
                { "time": 0.0, "value": 0.0 },
                { "time": 0.5, "value": 0.0, "easing": "easeOutBack" },
                { "time": 1.5, "value": 1.0 },
                { "time": 7.0, "value": 1.0, "easing": "easeInCubic" },
                { "time": 8.0, "value": 0.0 },
            ]},
        }
    })))?;

    // Shape 2: Pink circle — counter-orbit
    net.add_node("tl_2", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [
                { "time": 0.0, "value": cx - 200.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cx + 200.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cx - 200.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cx + 200.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cx - 200.0 },
            ]},
            "y": { "keyframes": [
                { "time": 0.0, "value": cy + 100.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cy - 100.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cy + 100.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cy - 100.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cy + 100.0 },
            ]},
            "scale": { "keyframes": [
                { "time": 0.0, "value": 0.0 },
                { "time": 1.0, "value": 0.0, "easing": "easeOutBack" },
                { "time": 2.0, "value": 1.0 },
                { "time": 6.5, "value": 1.0, "easing": "easeInCubic" },
                { "time": 8.0, "value": 0.0 },
            ]},
        }
    })))?;

    // Shape 3: Purple hex border — pulses at center, slow counter-rotation
    net.add_node("tl_3", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [{ "time": 0.0, "value": cx }, { "time": 8.0, "value": cx }] },
            "y": { "keyframes": [{ "time": 0.0, "value": cy }, { "time": 8.0, "value": cy }] },
            "scale": { "keyframes": [
                { "time": 0.0, "value": 0.0 },
                { "time": 2.0, "value": 0.0, "easing": "easeOutElastic" },
                { "time": 3.5, "value": 1.0 },
                { "time": 4.5, "value": 0.85, "easing": "easeInOutSine" },
                { "time": 5.5, "value": 1.0, "easing": "easeInOutSine" },
                { "time": 6.5, "value": 0.85, "easing": "easeInCubic" },
                { "time": 8.0, "value": 0.0 },
            ]},
            "rotation": { "keyframes": [
                { "time": 0.0, "value": 0.0 }, { "time": 8.0, "value": -180.0 },
            ]},
        }
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
        ]
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": frames, "width": w, "height": h, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 3000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "motion_2d.mp4" })))?;

    // ═══ WIRING ═══
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "tl_0", "tick"));
    net.add_connection(wire("tick", "trigger", "tl_1", "tick"));
    net.add_connection(wire("tick", "trigger", "tl_2", "tick"));
    net.add_connection(wire("tick", "trigger", "tl_3", "tick"));
    net.add_connection(wire("tick", "trigger", "render", "tick"));

    net.add_connection(wire("tl_0", "values", "render", "anim_0"));
    net.add_connection(wire("tl_1", "values", "render", "anim_1"));
    net.add_connection(wire("tl_2", "values", "render", "anim_2"));
    net.add_connection(wire("tl_3", "values", "render", "anim_3"));

    net.add_connection(wire("render", "image", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    net.add_initial(iip("tick", "start", Message::Flow));

    println!("DAG: 4 timelines → Gpu2DRender (SDF instanced) → video");
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
