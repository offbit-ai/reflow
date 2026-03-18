//! # Button Click — FSM + Animation + Font + GPU 2D
//!
//! A cursor glides to a "Reflow Run" button, hovers, clicks, and leaves.
//! The button FSM tracks states (idle → hover → pressed → released).
//! SDF font rendering via FontLoad → GlyphAtlas pipeline.
//!
//! ```text
//! font → atlas ──→ renderer (atlas, metrics, atlas_size)
//! tick ──→ timeline + FSM (tick) + time
//! timeline:values ──→ renderer:values
//! renderer ──→ collector → encoder → save
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
    println!("=== Button Click — FSM + Animation + GPU 2D ===\n");

    let w = 640u32;
    let h = 360u32;
    let fps = 30u32;
    let dur = 6.0f64;
    let frames = (dur * fps as f64) as usize;
    let ms = 1000 / fps as u64;

    // Layout
    let btn_x = 320.0f64;
    let btn_y = 170.0f64;
    let btn_w = 200.0f64;
    let btn_h = 52.0f64;

    println!("{}x{}, {}fps, {:.0}s = {} frames\n", w, h, fps, dur, frames);

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger",
        "tpl_animation_time",
        "tpl_animation_timeline",
        "tpl_fsm",
        "tpl_font_load",
        "tpl_glyph_atlas",
        "tpl_gpu_2d_render",
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ TIMING ═══
    net.add_node(
        "tick",
        "tpl_interval_trigger",
        config(json!({
            "interval": ms,
            "maxExecutions": frames,
            "startImmediately": false,
        })),
    )?;
    net.add_node(
        "time",
        "tpl_animation_time",
        config(json!({ "fps": fps, "speed": 1.0 })),
    )?;

    // ═══ FONT PIPELINE ═══
    net.add_node(
        "font",
        "tpl_font_load",
        config(json!({
            "path": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        })),
    )?;
    net.add_node(
        "atlas",
        "tpl_glyph_atlas",
        config(json!({ "fontSize": 48, "sdf": true })),
    )?;

    // ═══ ANIMATION TIMELINE ═══
    // Choreography (6s total):
    //   0.0–0.5s  hold — cursor at start, everything settled
    //   0.5–2.0s  cursor glides to button (easeInOutCubic)
    //   2.0–3.0s  hover — cursor rests on button, button reacts
    //   3.0–3.2s  click down
    //   3.2–3.8s  click release, button bounces back
    //   3.8–4.5s  cursor rests post-click
    //   4.5–6.0s  cursor glides away

    let mut tracks = serde_json::Map::new();
    let kf = |frames: Value| -> Value {
        json!({ "keyframes": frames })
    };

    // ── Cursor (shape 2) ──
    tracks.insert(
        "s2_x".into(),
        kf(json!([
            {"time": 0.0, "value": 90.0},
            {"time": 0.5, "value": 90.0, "easing": "easeInOutCubic"},
            {"time": 2.0, "value": btn_x + 50.0},
            {"time": 3.0, "value": btn_x + 50.0},
            {"time": 3.2, "value": btn_x + 50.0},
            {"time": 3.8, "value": btn_x + 50.0},
            {"time": 4.5, "value": btn_x + 55.0, "easing": "easeInOutCubic"},
            {"time": 6.0, "value": 560.0}
        ])),
    );
    tracks.insert(
        "s2_y".into(),
        kf(json!([
            {"time": 0.0, "value": 290.0},
            {"time": 0.5, "value": 290.0, "easing": "easeInOutCubic"},
            {"time": 2.0, "value": btn_y + 16.0},
            {"time": 3.0, "value": btn_y + 16.0, "easing": "easeOutCubic"},
            {"time": 3.15, "value": btn_y + 20.0},
            {"time": 3.3, "value": btn_y + 16.0},
            {"time": 3.8, "value": btn_y + 16.0},
            {"time": 4.5, "value": btn_y + 30.0, "easing": "easeInOutCubic"},
            {"time": 6.0, "value": 70.0}
        ])),
    );
    tracks.insert(
        "s2_scale".into(),
        kf(json!([{"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}])),
    );

    // ── Button body (shape 0) ──
    tracks.insert(
        "s0_x".into(),
        kf(json!([{"time": 0.0, "value": btn_x}, {"time": 6.0, "value": btn_x}])),
    );
    tracks.insert(
        "s0_y".into(),
        kf(json!([{"time": 0.0, "value": btn_y}, {"time": 6.0, "value": btn_y}])),
    );
    tracks.insert(
        "s0_scale".into(),
        kf(json!([
            {"time": 0.0, "value": 1.0},
            {"time": 1.9, "value": 1.0, "easing": "easeOutCubic"},
            {"time": 2.2, "value": 1.04},
            {"time": 2.9, "value": 1.04, "easing": "easeOutCubic"},
            {"time": 3.1, "value": 0.96, "easing": "easeOutBack"},
            {"time": 3.4, "value": 0.96},
            {"time": 3.8, "value": 1.04, "easing": "easeOutBack"},
            {"time": 4.2, "value": 1.0},
            {"time": 6.0, "value": 1.0}
        ])),
    );
    tracks.insert(
        "s0_opacity".into(),
        kf(json!([
            {"time": 0.0, "value": 1.0},
            {"time": 3.0, "value": 1.0},
            {"time": 3.15, "value": 0.8},
            {"time": 3.5, "value": 1.0},
            {"time": 6.0, "value": 1.0}
        ])),
    );

    // ── Button shadow (shape 1) ──
    tracks.insert(
        "s1_x".into(),
        kf(json!([{"time": 0.0, "value": btn_x}, {"time": 6.0, "value": btn_x}])),
    );
    tracks.insert(
        "s1_y".into(),
        kf(json!([
            {"time": 0.0, "value": btn_y + 4.0},
            {"time": 3.0, "value": btn_y + 4.0},
            {"time": 3.15, "value": btn_y + 1.0},
            {"time": 3.8, "value": btn_y + 4.0},
            {"time": 6.0, "value": btn_y + 4.0}
        ])),
    );
    tracks.insert(
        "s1_scale".into(),
        kf(json!([
            {"time": 0.0, "value": 1.0},
            {"time": 1.9, "value": 1.0},
            {"time": 2.2, "value": 1.04},
            {"time": 2.9, "value": 1.04},
            {"time": 3.1, "value": 0.96},
            {"time": 3.4, "value": 0.96},
            {"time": 3.8, "value": 1.04, "easing": "easeOutBack"},
            {"time": 4.2, "value": 1.0},
            {"time": 6.0, "value": 1.0}
        ])),
    );

    // ── Text stagger on hover + press dip ──
    let label = "Reflow Run";
    for (i, _ch) in label.chars().enumerate() {
        let d = i as f64 * 0.04;
        tracks.insert(
            format!("c{}_scale", i),
            kf(json!([
                {"time": 0.0, "value": 1.0},
                {"time": 2.0 + d, "value": 1.0, "easing": "easeOutBack"},
                {"time": 2.25 + d, "value": 1.15},
                {"time": 2.6, "value": 1.0},
                {"time": 3.1, "value": 0.92},
                {"time": 3.5, "value": 1.0},
                {"time": 6.0, "value": 1.0}
            ])),
        );
        tracks.insert(
            format!("c{}_y", i),
            kf(json!([
                {"time": 0.0, "value": 0.0},
                {"time": 2.0 + d, "value": 0.0, "easing": "easeOutBack"},
                {"time": 2.25 + d, "value": -5.0},
                {"time": 2.6, "value": 0.0},
                {"time": 3.1, "value": 3.0},
                {"time": 3.5, "value": 0.0},
                {"time": 6.0, "value": 0.0}
            ])),
        );
        tracks.insert(
            format!("c{}_opacity", i),
            kf(json!([{"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}])),
        );
    }

    net.add_node(
        "tl",
        "tpl_animation_timeline",
        config(json!({
            "duration": dur,
            "autoplay": true,
            "dt": 1.0 / fps as f64,
            "tracks": Value::Object(tracks),
        })),
    )?;

    // ═══ FSM — button state ═══
    net.add_node(
        "fsm",
        "tpl_fsm",
        config(json!({
            "initial": "idle",
            "dt": 1.0 / fps as f64,
            "states": {
                "idle": {
                    "on": { "_timeout": { "target": "hover", "delay": 2.0 } },
                    "entry": { "emit": { "btn_state": "idle" } }
                },
                "hover": {
                    "on": { "_timeout": { "target": "pressed", "delay": 1.0 } },
                    "entry": { "emit": { "btn_state": "hover" } }
                },
                "pressed": {
                    "on": { "_timeout": { "target": "released", "delay": 0.2 } },
                    "entry": { "emit": { "btn_state": "pressed" } }
                },
                "released": {
                    "on": { "_timeout": { "target": "leaving", "delay": 0.8 } },
                    "entry": { "emit": { "btn_state": "released" } }
                },
                "leaving": {
                    "on": { "_timeout": { "target": "done", "delay": 2.0 } },
                    "entry": { "emit": { "btn_state": "idle" } }
                },
                "done": {
                    "type": "final",
                    "entry": { "emit": { "btn_state": "idle" } }
                }
            }
        })),
    )?;

    // ═══ RENDERER ═══
    net.add_node(
        "render",
        "tpl_gpu_2d_render",
        config(json!({
            "width": w, "height": h,
            "background": [0.95, 0.95, 0.97, 1.0],
            "shapes": [
                // 0: Button body
                {
                    "type": "rect",
                    "bounds": [0, 0, btn_w, btn_h],
                    "color": [0.20, 0.56, 0.98, 1.0],
                    "cornerRadius": 14.0,
                },
                // 1: Button shadow
                {
                    "type": "rect",
                    "bounds": [0, 0, btn_w, btn_h],
                    "color": [0.10, 0.28, 0.58, 0.22],
                    "cornerRadius": 14.0,
                },
                // 2: Cursor dot
                {
                    "type": "circle",
                    "bounds": [0, 0, 14, 14],
                    "color": [0.18, 0.18, 0.18, 0.9],
                    "shadow": { "x": 0, "y": 2, "blur": 8, "color": [0.0, 0.0, 0.0, 0.2] },
                },
            ],
            "text": [
                {
                    "content": label,
                    "x": btn_x,
                    "y": btn_y - 3.0,
                    "size": 20.0,
                    "color": [1.0, 1.0, 1.0, 1.0],
                    "tracking": 1.0,
                    "center": true,
                },
            ],
        })),
    )?;

    // ═══ VIDEO ═══
    net.add_node(
        "collector",
        "tpl_render_frame_collector",
        config(json!({ "totalFrames": frames, "width": w, "height": h, "fps": fps })),
    )?;
    net.add_node(
        "encoder",
        "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 5000 })),
    )?;
    net.add_node(
        "save",
        "tpl_file_save",
        config(json!({ "path": "button_click.mp4" })),
    )?;

    // ═══ WIRING ═══
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "tl", "tick"));
    net.add_connection(wire("tick", "trigger", "fsm", "tick"));

    // Font pipeline
    net.add_connection(wire("font", "font_data", "atlas", "font_data"));
    net.add_connection(wire("atlas", "atlas", "render", "atlas"));
    net.add_connection(wire("atlas", "metrics", "render", "metrics"));
    net.add_connection(wire("atlas", "atlas_size", "render", "atlas_size"));

    // Timeline → renderer
    net.add_connection(wire("tl", "values", "render", "values"));

    // Video pipeline
    net.add_connection(wire("render", "image", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Bootstrap
    net.add_initial(iip("font", "tick", Message::Flow));
    net.add_initial(iip("tick", "start", Message::Flow));

    println!("DAG: font → atlas → renderer ← timeline ← tick → FSM");
    println!("  Cursor: easeInOutCubic glide (0.5s hold → 1.5s travel → hover → click → leave)");
    println!("  Button: FSM idle→hover→pressed→released (timeout-scripted)");
    println!("  Font: Arial Bold SDF, \"{}\"", label);
    println!("\nRunning...\n");

    let start = std::time::Instant::now();
    net.start()?;

    let mp4_path = std::path::Path::new("button_click.mp4");
    let timeout = std::time::Duration::from_secs(60);
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if mp4_path.exists() && mp4_path.metadata().map(|m| m.len() > 100).unwrap_or(false) {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            break;
        }
        if start.elapsed() > timeout {
            eprintln!("Timed out");
            break;
        }
    }

    net.shutdown();
    let total = start.elapsed();
    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!("Saved: button_click.mp4 ({} bytes)", size);
        println!(
            "Total: {:.1}s ({:.1} effective fps)",
            total.as_secs_f64(),
            frames as f64 / total.as_secs_f64()
        );
    }
    println!("Done!");
    Ok(())
}
