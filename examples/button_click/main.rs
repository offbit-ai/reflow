//! # Button Click — HitTest + FSM + Animated State Transitions
//!
//! Cursor glides to a "Reflow Run" button. HitTest detects overlap.
//! FSM tracks state. Each FSM state triggers a micro-timeline for
//! smooth eased transitions (not instant jumps).
//!
//! ```text
//! timeline(cursor) ──→ renderer:values
//!                  ──→ hit_test:values
//!
//! hit_test:enter ──→ fsm:event (HOVER)
//! hit_test:leave ──→ fsm:event (LEAVE)
//!
//! fsm:emit ──→ sub_hover:signal → trigger → hover_anim:control (play)
//! fsm:emit ──→ sub_idle:signal  → trigger → idle_anim:control  (play)
//!
//! hover_anim:values ──→ renderer:values (s0_scale 1.0→1.08, 0.15s)
//! idle_anim:values  ──→ renderer:values (s0_scale 1.08→1.0, 0.15s)
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
    println!("=== Button Click — FSM + Animated Transitions ===\n");

    let w = 640u32;
    let h = 360u32;
    let fps = 30u32;
    let dur = 6.0f64;
    let frames = (dur * fps as f64) as usize;
    let ms = 1000 / fps as u64;
    let dt = 1.0 / fps as f64;

    let btn_x = 320.0f64;
    let btn_y = 170.0f64;
    let btn_w = 200.0f64;
    let btn_h = 52.0f64;
    let label = "Reflow Run";

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger",
        "tpl_animation_time",
        "tpl_animation_timeline",
        "tpl_hit_test",
        "tpl_fsm",
        "tpl_subscriber",
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
    net.add_node("tick", "tpl_interval_trigger", config(json!({
        "interval": ms, "maxExecutions": frames, "startImmediately": false,
    })))?;
    net.add_node("time", "tpl_animation_time", config(json!({
        "fps": fps, "speed": 1.0,
    })))?;


    // ═══ CURSOR TIMELINE — path only ═══
    let mut tracks = serde_json::Map::new();
    let kf = |f: Value| -> Value { json!({ "keyframes": f }) };

    tracks.insert("s1_x".into(), kf(json!([
        {"time": 0.0,  "value": 90.0,           "easing": "easeInOutQuart"},
        {"time": 2.5,  "value": btn_x + 10.0},
        {"time": 3.5,  "value": btn_x + 10.0},
        {"time": 4.2,  "value": btn_x + 20.0,   "easing": "easeInOutCubic"},
        {"time": 6.0,  "value": 560.0}
    ])));
    tracks.insert("s1_y".into(), kf(json!([
        {"time": 0.0,  "value": 290.0,           "easing": "easeInOutQuart"},
        {"time": 2.5,  "value": btn_y + 6.0},
        {"time": 3.5,  "value": btn_y + 6.0},
        {"time": 4.2,  "value": btn_y + 24.0,   "easing": "easeInOutCubic"},
        {"time": 6.0,  "value": 70.0}
    ])));
    tracks.insert("s1_scale".into(), kf(json!([
        {"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}
    ])));
    // Button position (static)
    tracks.insert("s0_x".into(), kf(json!([
        {"time": 0.0, "value": btn_x}, {"time": 6.0, "value": btn_x}
    ])));
    tracks.insert("s0_y".into(), kf(json!([
        {"time": 0.0, "value": btn_y}, {"time": 6.0, "value": btn_y}
    ])));
    // Text always visible
    for (i, _) in label.chars().enumerate() {
        tracks.insert(format!("c{}_scale", i), kf(json!([
            {"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}
        ])));
        tracks.insert(format!("c{}_opacity", i), kf(json!([
            {"time": 0.0, "value": 1.0}, {"time": 6.0, "value": 1.0}
        ])));
        tracks.insert(format!("c{}_y", i), kf(json!([
            {"time": 0.0, "value": 0.0}, {"time": 6.0, "value": 0.0}
        ])));
    }

    net.add_node("tl", "tpl_animation_timeline", config(json!({
        "duration": dur, "autoplay": true, "dt": dt,
        "tracks": Value::Object(tracks),
    })))?;

    // ═══ HIT TEST ═══
    net.add_node("hit_test", "tpl_hit_test", config(json!({
        "source": "s1", "target": "s0",
        "target_width": btn_w, "target_height": btn_h,
    })))?;

    // ═══ FSM — button state ═══
    // hover entry emits { cmd:"play" }  → btn_anim plays forward  1.0→1.08
    // idle  entry emits { cmd:"reverse" } → btn_anim plays backward 1.08→1.0
    net.add_node("fsm", "tpl_fsm", config(json!({
        "initial": "idle",
        "dt": dt,
        "states": {
            "idle":  { "on": { "HOVER": { "target": "hover" } }, "entry": { "emit": { "cmd": "reverse" } } },
            "hover": { "on": { "LEAVE": { "target": "idle"  } }, "entry": { "emit": { "cmd": "play"    } } }
        }
    })))?;

    // ═══ SUBSCRIBERS — filter FSM signals by state ═══
    net.add_node("sub_hover", "tpl_subscriber", config(json!({ "event": "hover" })))?;
    net.add_node("sub_idle",  "tpl_subscriber", config(json!({ "event": "idle"  })))?;

    // ═══ SINGLE BUTTON ANIMATION TIMELINE ═══
    // Plays forward (hover-in) or reverse (hover-out). One source of s0_scale truth.
    let anim_dur = 0.15;
    let mut btn_tracks = serde_json::Map::new();
    btn_tracks.insert("s0_scale".into(), kf(json!([
        {"time": 0.0, "value": 1.0, "easing": "easeOutCubic"},
        {"time": anim_dur, "value": 1.08}
    ])));
    net.add_node("btn_anim", "tpl_animation_timeline", config(json!({
        "duration": anim_dur, "autoplay": false, "loop": false, "dt": dt,
        "tracks": Value::Object(btn_tracks),
    })))?;

    // ═══ RENDERER ═══
    net.add_node("render", "tpl_gpu_2d_render", config(json!({
        "width": w, "height": h,
        "background": [0.95, 0.95, 0.97, 1.0],
        "shapes": [
            { "type": "rect", "bounds": [0, 0, btn_w, btn_h],
              "color": [0.20, 0.56, 0.98, 1.0], "cornerRadius": 14.0,
              "shadow": { "x": 0, "y": 4, "blur": 12, "color": [0.10, 0.28, 0.58, 0.35] } },
            { "type": "circle", "bounds": [0, 0, 14, 14], "z": 20,
              "color": [0.18, 0.18, 0.18, 0.9],
              "shadow": { "x": 0, "y": 2, "blur": 8, "color": [0.0, 0.0, 0.0, 0.2] } },
        ],
        "text": [{
            "content": label, "x": btn_x, "y": btn_y - 3.0,
            "size": 20.0, "color": [1.0, 1.0, 1.0, 1.0],
            "tracking": 1.0, "center": true,
            "font": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        }],
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector",
        config(json!({ "totalFrames": frames, "width": w, "height": h, "fps": fps })))?;
    net.add_node("encoder", "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 5000 })))?;
    net.add_node("save", "tpl_file_save",
        config(json!({ "path": "button_click.mp4" })))?;

    // ═══ WIRING ═══

    // Timing: tick drives cursor timeline + btn_anim + FSM
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "tl", "tick"));
    net.add_connection(wire("tick", "trigger", "fsm", "tick"));
    net.add_connection(wire("tick", "trigger", "btn_anim", "tick"));

    // Font is embedded in text config — no external font pipeline needed

    // Cursor timeline → renderer + hit_test
    net.add_connection(wire("tl", "values", "render", "values"));
    net.add_connection(wire("tl", "values", "hit_test", "values"));

    // Hit test → FSM events
    net.add_connection(wire("hit_test", "enter", "fsm", "event"));
    net.add_connection(wire("hit_test", "leave", "fsm", "event"));

    // FSM emit → subscribers → btn_anim:control via data (carries { "cmd": "play"|"reverse" })
    net.add_connection(wire("fsm", "emit", "sub_hover", "signal"));
    net.add_connection(wire("fsm", "emit", "sub_idle",  "signal"));
    net.add_connection(wire("sub_hover", "data", "btn_anim", "control"));
    net.add_connection(wire("sub_idle",  "data", "btn_anim", "control"));

    // btn_anim values → renderer (single source of s0_scale)
    net.add_connection(wire("btn_anim", "values", "render", "values"));

    // Video pipeline
    net.add_connection(wire("render", "image", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Bootstrap
    net.add_initial(iip("tick", "start", Message::Flow));

    println!("{}x{}, {}fps, {:.0}s = {} frames", w, h, fps, dur, frames);
    println!("  cursor tl → hit_test → FSM → subscriber:data → btn_anim:control → renderer");
    println!("  hover: play  1.0→1.08 ({:.2}s easeOutCubic)", anim_dur);
    println!("  idle:  reverse 1.08→1.0 ({:.2}s easeOutCubic)\n", anim_dur);

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
    let t = start.elapsed();
    if mp4_path.exists() {
        let sz = std::fs::metadata(mp4_path)?.len();
        println!("Saved: button_click.mp4 ({} bytes, {:.1}s, {:.1} fps)", sz, t.as_secs_f64(), frames as f64 / t.as_secs_f64());
    }
    Ok(())
}
