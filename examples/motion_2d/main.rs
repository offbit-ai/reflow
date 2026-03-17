//! # Abstract Motion Choreography
//!
//! Multiple shapes with coordinated animation: a star spirals from center
//! while circles orbit outward. Glow blur and screen blend create depth.
//! All driven by AnimationTimeline actors through the Reflow DAG.

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
    println!("=== Abstract Motion Choreography ===\n");

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

    // ═══ STAR: spirals from center outward, scales up, rotates ═══
    net.add_node("star_tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [
                { "time": 0.0, "value": cx, "easing": "easeInOutCubic" },
                { "time": 2.0, "value": cx + 120.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cx, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cx - 120.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cx },
            ]},
            "y": { "keyframes": [
                { "time": 0.0, "value": cy, "easing": "easeInOutCubic" },
                { "time": 2.0, "value": cy - 80.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cy + 80.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cy - 80.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cy },
            ]},
            "scale": { "keyframes": [
                { "time": 0.0, "value": 0.0, "easing": "easeOutBack" },
                { "time": 1.5, "value": 1.2 },
                { "time": 3.0, "value": 0.8, "easing": "easeInOutSine" },
                { "time": 5.0, "value": 1.0, "easing": "easeInOutSine" },
                { "time": 7.0, "value": 1.2, "easing": "easeInCubic" },
                { "time": 8.0, "value": 0.0 },
            ]},
            "rotation": { "keyframes": [
                { "time": 0.0, "value": 0.0, "easing": "linear" },
                { "time": 8.0, "value": 720.0 },
            ]},
        }
    })))?;

    // ═══ CIRCLE 1: orbits wide, delayed entrance ═══
    net.add_node("c1_tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [
                { "time": 0.0, "value": cx + 200.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cx, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cx - 200.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cx, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cx + 200.0 },
            ]},
            "y": { "keyframes": [
                { "time": 0.0, "value": cy - 100.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cy + 100.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cy - 100.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cy + 100.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cy - 100.0 },
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

    // ═══ CIRCLE 2: counter-orbit, different phase ═══
    net.add_node("c2_tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [
                { "time": 0.0, "value": cx - 180.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cx + 180.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cx - 180.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cx + 180.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cx - 180.0 },
            ]},
            "y": { "keyframes": [
                { "time": 0.0, "value": cy + 80.0, "easing": "easeInOutSine" },
                { "time": 2.0, "value": cy - 80.0, "easing": "easeInOutSine" },
                { "time": 4.0, "value": cy + 80.0, "easing": "easeInOutSine" },
                { "time": 6.0, "value": cy - 80.0, "easing": "easeInOutSine" },
                { "time": 8.0, "value": cy + 80.0 },
            ]},
            "scale": { "keyframes": [
                { "time": 0.0, "value": 0.0 },
                { "time": 1.0, "value": 0.0, "easing": "easeOutBack" },
                { "time": 2.0, "value": 0.8 },
                { "time": 6.5, "value": 0.8, "easing": "easeInCubic" },
                { "time": 8.0, "value": 0.0 },
            ]},
        }
    })))?;

    // ═══ POLYGON: hexagon pulses at center ═══
    net.add_node("hex_tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": 1.0 / fps as f64,
        "tracks": {
            "x": { "keyframes": [
                { "time": 0.0, "value": cx },
                { "time": 8.0, "value": cx },
            ]},
            "y": { "keyframes": [
                { "time": 0.0, "value": cy },
                { "time": 8.0, "value": cy },
            ]},
            "scale": { "keyframes": [
                { "time": 0.0, "value": 0.0 },
                { "time": 2.0, "value": 0.0, "easing": "easeOutElastic" },
                { "time": 3.5, "value": 0.6 },
                { "time": 4.5, "value": 0.5, "easing": "easeInOutSine" },
                { "time": 5.5, "value": 0.6, "easing": "easeInOutSine" },
                { "time": 6.5, "value": 0.5, "easing": "easeInCubic" },
                { "time": 8.0, "value": 0.0 },
            ]},
            "rotation": { "keyframes": [
                { "time": 0.0, "value": 0.0, "easing": "linear" },
                { "time": 8.0, "value": -180.0 },
            ]},
        }
    })))?;

    // ═══ BACKGROUND ═══
    net.add_node("bg", "tpl_background", config(json!({
        "type": "radialGradient",
        "from": [25, 12, 60, 255], "to": [5, 2, 18, 255],
        "center": [0.5, 0.45], "radius": 0.9,
        "width": w, "height": h,
    })))?;

    // ═══ SHAPES ═══
    net.add_node("star_shape", "tpl_shape_2d", config(json!({
        "shape": "star", "outerRadius": 50.0, "innerRadius": 22.0, "points": 5,
    })))?;
    net.add_node("c1_shape", "tpl_shape_2d", config(json!({
        "shape": "circle", "radius": 28.0,
    })))?;
    net.add_node("c2_shape", "tpl_shape_2d", config(json!({
        "shape": "circle", "radius": 20.0,
    })))?;
    net.add_node("hex_shape", "tpl_shape_2d", config(json!({
        "shape": "polygon", "radius": 70.0, "sides": 6,
    })))?;

    // ═══ RASTERIZERS ═══
    net.add_node("star_r", "tpl_vector_rasterize", config(json!({
        "width": w, "height": h, "fill": { "color": "#ff8820" },
    })))?;
    net.add_node("c1_r", "tpl_vector_rasterize", config(json!({
        "width": w, "height": h, "fill": { "color": "#40c0ff" },
    })))?;
    net.add_node("c2_r", "tpl_vector_rasterize", config(json!({
        "width": w, "height": h, "fill": { "color": "#ff4080" },
    })))?;
    net.add_node("hex_r", "tpl_vector_rasterize", config(json!({
        "width": w, "height": h,
        "fill": { "color": "#ffffff" },
        "stroke": { "width": 2.0, "color": "#8060ff" },
    })))?;

    // ═══ GLOW ═══
    net.add_node("glow", "tpl_gaussian_blur", config(json!({
        "radius": 12, "width": w, "height": h,
    })))?;

    // ═══ CANVAS: 6 layers ═══
    net.add_node("canvas", "tpl_canvas_2d", config(json!({
        "width": w, "height": h,
        "background": [5, 2, 18, 255],
        "layers": [
            { "name": "layer_0", "blend": "normal", "opacity": 1.0 },
            { "name": "layer_1", "blend": "screen", "opacity": 0.4 },
            { "name": "layer_2", "blend": "normal", "opacity": 0.9 },
            { "name": "layer_3", "blend": "add", "opacity": 0.6 },
            { "name": "layer_4", "blend": "add", "opacity": 0.5 },
            { "name": "layer_5", "blend": "normal", "opacity": 0.8 },
        ]
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector", config(json!({
        "totalFrames": frames, "width": w, "height": h, "fps": fps,
    })))?;
    net.add_node("encoder", "tpl_video_encoder", config(json!({ "fps": fps, "bitrate": 3000 })))?;
    net.add_node("save", "tpl_file_save", config(json!({ "path": "motion_2d.mp4" })))?;

    // ═══ WIRING ═══

    // Timing
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "star_tl", "tick"));
    net.add_connection(wire("tick", "trigger", "c1_tl", "tick"));
    net.add_connection(wire("tick", "trigger", "c2_tl", "tick"));
    net.add_connection(wire("tick", "trigger", "hex_tl", "tick"));

    // Timelines → rasterizer transforms
    net.add_connection(wire("star_tl", "values", "star_r", "transform"));
    net.add_connection(wire("c1_tl", "values", "c1_r", "transform"));
    net.add_connection(wire("c2_tl", "values", "c2_r", "transform"));
    net.add_connection(wire("hex_tl", "values", "hex_r", "transform"));

    // Shapes (one-shot)
    net.add_connection(wire("star_shape", "path", "star_r", "path"));
    net.add_connection(wire("c1_shape", "path", "c1_r", "path"));
    net.add_connection(wire("c2_shape", "path", "c2_r", "path"));
    net.add_connection(wire("hex_shape", "path", "hex_r", "path"));

    // Background
    net.add_connection(wire("time", "time", "bg", "tick"));

    // Glow from star
    net.add_connection(wire("star_r", "image", "glow", "image"));

    // Tick drives canvas (one composite per tick)
    net.add_connection(wire("tick", "trigger", "canvas", "tick"));

    // All layers → Canvas2D
    net.add_connection(wire("bg", "image", "canvas", "layer_0"));
    net.add_connection(wire("glow", "image", "canvas", "layer_1"));
    net.add_connection(wire("hex_r", "image", "canvas", "layer_2"));
    net.add_connection(wire("c1_r", "image", "canvas", "layer_3"));
    net.add_connection(wire("c2_r", "image", "canvas", "layer_4"));
    net.add_connection(wire("star_r", "image", "canvas", "layer_5"));

    // Video
    net.add_connection(wire("canvas", "frame", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Start
    net.add_initial(iip("tick", "start", Message::Flow));
    net.add_initial(iip("star_shape", "params", Message::Flow));
    net.add_initial(iip("c1_shape", "params", Message::Flow));
    net.add_initial(iip("c2_shape", "params", Message::Flow));
    net.add_initial(iip("hex_shape", "params", Message::Flow));

    println!("DAG: 4 timelines → 4 shapes → 4 rasterizers + glow → 6-layer canvas → video");
    println!("Running...\n");

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("motion_2d.mp4");
    let timeout = std::time::Duration::from_secs(180);
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
