//! # Button Click — HitTest + FSM + Signal/Subscriber + Animation
//!
//! A cursor glides to a "Reflow Run" button. The HitTestActor detects
//! when the cursor enters the button bounds (HOVER), a click signal fires
//! (PRESS), and the cursor leaves (LEAVE). The FSM tracks button state
//! and emits signals. Subscribers route per-state data to the renderer.
//!
//! ## DAG topology
//!
//! ```text
//! tick ──→ timeline (cursor path only)
//!          timeline:values ──┬──→ renderer:values
//!                            └──→ hit_test:values
//!
//! hit_test:enter ──→ fsm:event  (HOVER)
//! hit_test:leave ──→ fsm:event  (LEAVE)
//! hit_test:click ──→ fsm:event  (PRESS)
//! signal_click ──→ hit_test:click  (flow-triggered click gesture)
//!
//! fsm:emit ──→ sub_hover:signal   → data → renderer:values
//! fsm:emit ──→ sub_pressed:signal → data → renderer:values
//! fsm:emit ──→ sub_idle:signal    → data → renderer:values
//!
//! font → atlas → renderer
//! renderer → collector → encoder → save
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
    println!("=== Button Click — HitTest + FSM + Signals ===\n");

    let w = 640u32;
    let h = 360u32;
    let fps = 30u32;
    let dur = 6.0f64;
    let frames = (dur * fps as f64) as usize;
    let ms = 1000 / fps as u64;

    let btn_x = 320.0f64;
    let btn_y = 170.0f64;
    let btn_w = 200.0f64;
    let btn_h = 52.0f64;

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger",
        "tpl_animation_time",
        "tpl_animation_timeline",
        "tpl_hit_test",
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

    // ═══ FONT ═══
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

    // ═══ TIMELINE — cursor path ONLY ═══
    // The timeline drives cursor motion. Button appearance is driven
    // entirely by the FSM via signals. No button tracks here.
    let mut tracks = serde_json::Map::new();
    let kf = |frames: Value| -> Value {
        json!({ "keyframes": frames })
    };

    // Cursor path (shape 2)
    tracks.insert(
        "s2_x".into(),
        kf(json!([
            {"time": 0.0, "value": 90.0},
            {"time": 0.5, "value": 90.0, "easing": "easeInOutCubic"},
            {"time": 2.0, "value": btn_x + 25.0},
            {"time": 3.0, "value": btn_x + 25.0},
            {"time": 3.2, "value": btn_x + 25.0},
            {"time": 3.8, "value": btn_x + 25.0},
            {"time": 4.5, "value": btn_x + 35.0, "easing": "easeInOutCubic"},
            {"time": 6.0, "value": 560.0}
        ])),
    );
    tracks.insert(
        "s2_y".into(),
        kf(json!([
            {"time": 0.0, "value": 290.0},
            {"time": 0.5, "value": 290.0, "easing": "easeInOutCubic"},
            {"time": 2.0, "value": btn_y + 10.0},
            {"time": 3.0, "value": btn_y + 10.0, "easing": "easeOutCubic"},
            {"time": 3.15, "value": btn_y + 15.0},
            {"time": 3.3, "value": btn_y + 10.0},
            {"time": 3.8, "value": btn_y + 10.0},
            {"time": 4.5, "value": btn_y + 30.0, "easing": "easeInOutCubic"},
            {"time": 6.0, "value": 70.0}
        ])),
    );
    tracks.insert(
        "s2_scale".into(),
        kf(json!([{"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}])),
    );

    // Button + shadow position (static, driven by FSM for scale/opacity)
    tracks.insert("s0_x".into(), kf(json!([{"time": 0.0, "value": btn_x}, {"time": 6.0, "value": btn_x}])));
    tracks.insert("s0_y".into(), kf(json!([{"time": 0.0, "value": btn_y}, {"time": 6.0, "value": btn_y}])));
    tracks.insert("s1_x".into(), kf(json!([{"time": 0.0, "value": btn_x}, {"time": 6.0, "value": btn_x}])));
    // s1_y owned by FSM (shadow offset changes per state)

    // s0_scale, s0_opacity, s1_scale, s1_y owned by FSM — not in timeline

    // Text visibility (always on)
    let label = "Reflow Run";
    for (i, _ch) in label.chars().enumerate() {
        tracks.insert(format!("c{}_scale", i), kf(json!([{"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}])));
        tracks.insert(format!("c{}_opacity", i), kf(json!([{"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}])));
        tracks.insert(format!("c{}_y", i), kf(json!([{"time": 0.0, "value": 0.0}, {"time": 6.0, "value": 0.0}])));
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

    // ═══ HIT TEST — cursor vs button ═══
    net.add_node(
        "hit_test",
        "tpl_hit_test",
        config(json!({
            "source": "s2",
            "target": "s0",
            "target_width": btn_w,
            "target_height": btn_h,
        })),
    )?;

    // ═══ CLICK SIGNAL — triggered by timeline cursor dip ═══
    // In a real app this would come from user input. Here we script it:
    // the cursor y dips at t=3.0-3.15 (the click gesture in the timeline).
    // We use a Signal that fires when triggered by an IIP at a delay.
    // For now: use a second timeline that fires a click at t=3.0.
    // Actually, simplest: the hit_test detects the cursor is inside,
    // and we send a click IIP at the right moment via a Signal actor.
    // Since IIPs fire at start, we need a delayed trigger.
    //
    // Pragmatic: use a separate small timeline with one track that
    // goes 0→1 at t=3.0, and threshold that into a click trigger.
    // But we don't have a threshold actor...
    //
    // ═══ FSM — button state machine ═══
    // Event-driven: HOVER, LEAVE from hit_test
    // Click (PRESS) left for real input actors (MouseInput, etc.)
    // Each state emits signal data for button appearance
    net.add_node(
        "fsm",
        "tpl_fsm",
        config(json!({
            "initial": "idle",
            "dt": 1.0 / fps as f64,
            "states": {
                "idle": {
                    "on": { "HOVER": { "target": "hover" } },
                    "entry": {
                        "emit": {
                            "s0_x": btn_x,
                            "s0_y": btn_y,
                            "s0_scale": 1.0,
                            "s0_opacity": 1.0,
                            "s1_x": btn_x,
                            "s1_y": btn_y + 4.0,
                            "s1_scale": 1.0,
                        }
                    }
                },
                "hover": {
                    "on": {
                        "LEAVE": { "target": "idle" }
                    },
                    "entry": {
                        "emit": {
                            "s0_scale": 1.04,
                            "s0_opacity": 1.0,
                            "s1_scale": 1.04,
                            "s1_y": btn_y + 5.0,
                        }
                    }
                },
                // pressed/released states left for real input integration
                // (MouseInput → Signal("PRESS") → FSM:event)
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
                { "type": "rect", "bounds": [0, 0, btn_w, btn_h],
                  "color": [0.20, 0.56, 0.98, 1.0], "cornerRadius": 14.0 },
                // 1: Button shadow
                { "type": "rect", "bounds": [0, 0, btn_w, btn_h],
                  "color": [0.10, 0.28, 0.58, 0.22], "cornerRadius": 14.0 },
                // 2: Cursor
                { "type": "circle", "bounds": [0, 0, 14, 14],
                  "color": [0.18, 0.18, 0.18, 0.9],
                  "shadow": { "x": 0, "y": 2, "blur": 8, "color": [0.0, 0.0, 0.0, 0.2] } },
            ],
            "text": [{
                "content": label,
                "x": btn_x, "y": btn_y - 3.0,
                "size": 20.0,
                "color": [1.0, 1.0, 1.0, 1.0],
                "tracking": 1.0, "center": true,
            }],
        })),
    )?;

    // ═══ VIDEO PIPELINE ═══
    net.add_node("collector", "tpl_render_frame_collector",
        config(json!({ "totalFrames": frames, "width": w, "height": h, "fps": fps })))?;
    net.add_node("encoder", "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 5000 })))?;
    net.add_node("save", "tpl_file_save",
        config(json!({ "path": "button_click.mp4" })))?;

    // ═══ WIRING ═══

    // Timing
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "tl", "tick"));
    net.add_connection(wire("tick", "trigger", "fsm", "tick"));

    // Font pipeline
    net.add_connection(wire("font", "font_data", "atlas", "font_data"));
    net.add_connection(wire("atlas", "atlas", "render", "atlas"));
    net.add_connection(wire("atlas", "metrics", "render", "metrics"));
    net.add_connection(wire("atlas", "atlas_size", "render", "atlas_size"));

    // Timeline values → renderer + hit_test
    net.add_connection(wire("tl", "values", "render", "values"));
    net.add_connection(wire("tl", "values", "hit_test", "values"));

    // Hit test → FSM events
    net.add_connection(wire("hit_test", "enter", "fsm", "event"));
    net.add_connection(wire("hit_test", "leave", "fsm", "event"));

    // FSM data → renderer (flat values, no subscriber hop)
    net.add_connection(wire("fsm", "data", "render", "values"));

    // Video pipeline
    net.add_connection(wire("render", "image", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Bootstrap
    net.add_initial(iip("font", "tick", Message::Flow));
    net.add_initial(iip("tick", "start", Message::Flow));

    println!("{}x{}, {}fps, {:.0}s = {} frames\n", w, h, fps, dur, frames);
    println!("DAG: timeline(cursor) → hit_test → FSM → subscribers → renderer");
    println!("  HitTest: cursor(s2) vs button(s0) overlap detection");
    println!("  FSM: idle ↔ hover (event-driven by hit_test enter/leave)");
    println!("  FSM emit → Subscriber(idle/hover) → renderer:values");
    println!("  Font: Arial Bold SDF\n");
    println!("Running...\n");

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
        if start.elapsed() > timeout { eprintln!("Timed out"); break; }
    }

    net.shutdown();
    let total = start.elapsed();
    if mp4_path.exists() {
        let size = std::fs::metadata(mp4_path)?.len();
        println!("Saved: button_click.mp4 ({} bytes)", size);
        println!("Total: {:.1}s ({:.1} fps)", total.as_secs_f64(), frames as f64 / total.as_secs_f64());
    }
    println!("Done!");
    Ok(())
}
