//! # 2D Motion Graphics → Video
//!
//! Canvas2D composites all layers per frame. No blend chain race conditions.
//!
//! ```text
//! tick → star_timeline → star_render → Canvas2D.star ─┐
//!      → circle_timeline → circle_render → Canvas2D.circle ─┤
//!      → bg → Canvas2D.bg ──────────────────────────────────┤
//!      → star_render → glow_blur → Canvas2D.glow ───────────┘
//!        Canvas2D → frame → collector → encoder → save
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
    println!("=== 2D Motion Graphics → Video ===\n");

    let w = 640u32;
    let h = 360u32;
    let fps = 30u32;
    let dur = 6.0f64;
    let frames = (dur * fps as f64) as usize;
    let ms = 1000 / fps as u64;

    println!("{}×{}, {}fps, {:.0}s = {} frames\n", w, h, fps, dur, frames);

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger", "tpl_animation_time", "tpl_animation_timeline",
        "tpl_shape_2d", "tpl_vector_rasterize", "tpl_background",
        "tpl_gaussian_blur", "tpl_canvas_2d",
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
    net.add_node("star_tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "scale": {
                "keyframes": [
                    { "time": 0.0, "value": 0.0, "easing": "easeOutBack" },
                    { "time": 2.0, "value": 1.0 },
                    { "time": 6.0, "value": 1.0 },
                ]
            },
            "rotation": {
                "keyframes": [
                    { "time": 0.0, "value": 0.0, "easing": "linear" },
                    { "time": 6.0, "value": 180.0 },
                ]
            }
        }
    })))?;

    net.add_node("circle_tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": {
                "keyframes": [
                    { "time": 0.0, "value": 200.0, "easing": "easeInOutSine" },
                    { "time": 3.0, "value": 440.0, "easing": "easeInOutSine" },
                    { "time": 6.0, "value": 200.0 },
                ]
            },
            "y": {
                "keyframes": [
                    { "time": 0.0, "value": 130.0, "easing": "easeInOutSine" },
                    { "time": 3.0, "value": 230.0, "easing": "easeInOutSine" },
                    { "time": 6.0, "value": 130.0 },
                ]
            },
            "scale": {
                "keyframes": [
                    { "time": 0.0, "value": 0.0 },
                    { "time": 0.5, "value": 0.0, "easing": "easeOutBack" },
                    { "time": 1.5, "value": 1.0 },
                    { "time": 6.0, "value": 1.0 },
                ]
            }
        }
    })))?;

    // ═══ BACKGROUND ═══
    net.add_node("bg", "tpl_background", config(json!({
        "type": "radialGradient",
        "from": [35, 18, 90, 255], "to": [8, 4, 25, 255],
        "center": [0.5, 0.4], "radius": 1.0,
        "width": w, "height": h,
    })))?;

    // ═══ SHAPES ═══
    net.add_node("star_shape", "tpl_shape_2d", config(json!({
        "shape": "star", "outerRadius": 55.0, "innerRadius": 24.0, "points": 5,
    })))?;
    net.add_node("circle_shape", "tpl_shape_2d", config(json!({
        "shape": "circle", "radius": 22.0,
    })))?;

    // ═══ RASTERIZERS ═══
    net.add_node("star_render", "tpl_vector_rasterize", config(json!({
        "width": w, "height": h,
        "fill": { "color": "#ffa028" },
        "transform": { "x": 320.0, "y": 170.0, "scale": 1.0, "rotation": 0.0 },
    })))?;
    net.add_node("circle_render", "tpl_vector_rasterize", config(json!({
        "width": w, "height": h,
        "fill": { "color": "#50c8ff" },
        "transform": { "x": 200.0, "y": 130.0, "scale": 1.0 },
    })))?;

    // ═══ GLOW ═══
    net.add_node("glow_blur", "tpl_gaussian_blur", config(json!({
        "radius": 8, "width": w, "height": h,
    })))?;

    // ═══ CANVAS2D: composites ALL layers in order ═══
    net.add_node("canvas", "tpl_canvas_2d", config(json!({
        "width": w, "height": h,
        "background": [8, 4, 25, 255],
        "layers": [
            { "name": "layer_0", "blend": "normal", "opacity": 1.0 },
            { "name": "layer_1", "blend": "screen", "opacity": 0.5 },
            { "name": "layer_2", "blend": "normal", "opacity": 1.0 },
            { "name": "layer_3", "blend": "add", "opacity": 0.7 },
        ]
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": frames, "width": w, "height": h, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 2000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "motion_2d.mp4" })))?;

    // ═══ WIRING ═══

    // Timing
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "star_tl", "tick"));
    net.add_connection(wire("tick", "trigger", "circle_tl", "tick"));

    // Timeline values → rasterizer transform
    net.add_connection(wire("star_tl", "values", "star_render", "transform"));
    net.add_connection(wire("circle_tl", "values", "circle_render", "transform"));

    // Shapes (one-shot)
    net.add_connection(wire("star_shape", "path", "star_render", "path"));
    net.add_connection(wire("circle_shape", "path", "circle_render", "path"));

    // Background
    net.add_connection(wire("time", "time", "bg", "tick"));

    // Glow
    net.add_connection(wire("star_render", "image", "glow_blur", "image"));

    // All layers → Canvas2D (indexed inports, composited in config order)
    net.add_connection(wire("bg", "image", "canvas", "layer_0"));          // bg
    net.add_connection(wire("glow_blur", "image", "canvas", "layer_1"));   // glow
    net.add_connection(wire("star_render", "image", "canvas", "layer_2")); // star
    net.add_connection(wire("circle_render", "image", "canvas", "layer_3")); // circle

    // Canvas → video
    net.add_connection(wire("canvas", "frame", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Start
    net.add_initial(iip("tick", "start", Message::Flow));
    net.add_initial(iip("star_shape", "params", Message::Flow));
    net.add_initial(iip("circle_shape", "params", Message::Flow));

    println!("DAG: 2 timelines → 2 rasterizers + bg + glow → Canvas2D → video");
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
