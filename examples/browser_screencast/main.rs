//! # Browser Screencast — Headless Chrome + GPU Overlay + Video
//!
//! Launches a headless browser, navigates to a URL, captures screencast
//! frames, composites a "Reflow" watermark via the GPU 2D renderer's
//! image layer primitive, and encodes to MP4.
//!
//! ```text
//! tick ──→ time:trigger (frame counter)
//!      ──→ browser:tick  (emit latest screencast frame)
//!      ──→ render:tick   (render overlay)
//!
//! IIP(url) ──→ browser:url
//!
//! browser:frame ──→ render:data  (RGBA bytes → cached as layer texture)
//!
//! render config: shapes=[{ type: "image", bounds: [0,0,W,H], z: 0 }]
//!                text=[{ content: "Reflow", ... }]  ← rendered on top
//!
//! render:image ──→ collector:frame
//! time:frame_number ──→ collector:frame_number
//! collector:stream ──→ encoder:stream
//! encoder:output ──→ save:input
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
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://google.com".to_string());

    println!("=== Browser Screencast — Headless Chrome + GPU Overlay ===\n");

    let w = 640u32;
    let h = 360u32;
    let fps = 24u32;
    let dur = 12.0f64;
    let frames = (dur * fps as f64) as usize;
    let ms = 1000 / fps as u64;

    let mut net = Network::new(NetworkConfig::default());

    let dt = 1.0 / fps as f64;

    for tpl in [
        "tpl_interval_trigger",
        "tpl_animation_time",
        "tpl_browser_screencast",
        "tpl_fsm",
        "tpl_frame_buffer",
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

    // ═══ BROWSER ═══
    net.add_node("browser", "tpl_browser_screencast", config(json!({
        "url": url,
        "width": w,
        "height": h,
        "quality": 80,
        "everyNthFrame": 1,
        "waitBeforeCapture": 1500,
        "framePool": "video_pipe",
    })))?;

    // ═══ JOURNEY FSM — choreographs browser interactions ═══
    // States: waiting → consent_visible → accepted → browsing
    // Each state entry emits an action Object routed to browser:action
    net.add_node("journey", "tpl_fsm", config(json!({
        "initial": "waiting",
        "dt": dt,
        "states": {
            "waiting": {
                "on": { "LOADED": { "target": "viewing_consent" } }
            },
            "viewing_consent": {
                "on": {
                    "_timeout": { "target": "scroll_to_accept", "delay": 0.5 }
                }
            },
            "scroll_to_accept": {
                "on": {
                    "_timeout": { "target": "click_accept", "delay": 0.5 }
                },
                "entry": {
                    "emit": { "type": "scroll", "x": 0, "y": 500 }
                }
            },
            "click_accept": {
                "on": {
                    "_timeout": { "target": "focus_search", "delay": 1.5 }
                },
                "entry": {
                    "emit": { "type": "evaluate", "expression": "[...document.querySelectorAll('button')].find(b => b.textContent.includes('Accept all'))?.click()" }
                }
            },
            "focus_search": {
                "on": {
                    "_timeout": { "target": "type_search", "delay": 0.5 }
                },
                "entry": {
                    "emit": { "type": "evaluate", "expression": "(document.querySelector('textarea[name=q]')||document.querySelector('input[name=q]'))?.focus()" }
                }
            },
            "type_search": {
                "on": {
                    "_timeout": { "target": "submit_search", "delay": 1.5 }
                },
                "entry": {
                    "emit": { "type": "insertText", "text": "Reflow DAG engine" }
                }
            },
            "submit_search": {
                "on": {
                    "_timeout": { "target": "browsing", "delay": 4.0 }
                },
                "entry": {
                    "emit": { "type": "evaluate", "expression": "window.location.href='https://www.google.com/search?q=Reflow+DAG+engine'" }
                }
            },
            "browsing": {}
        }
    })))?;


    // ═══ FRAME BUFFER — smooth frame delivery ═══
    net.add_node("fbuf", "tpl_frame_buffer", config(json!({
        "bufferSize": 30,
        "framePool": "video_pipe",
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector",
        config(json!({ "totalFrames": frames, "width": w, "height": h, "fps": fps })))?;
    net.add_node("encoder", "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 8000 })))?;
    net.add_node("save", "tpl_file_save",
        config(json!({ "path": "browser_screencast.mp4" })))?;

    // ═══ FSM TICK — independent from render pipeline to avoid backpressure deadlock ═══
    net.add_node("fsm_tick", "tpl_interval_trigger", config(json!({
        "interval": ms, "maxExecutions": frames, "startImmediately": false,
    })))?;

    // ═══ WIRING ═══

    // Timing
    net.add_connection(wire("tick", "trigger", "time", "trigger"));
    net.add_connection(wire("tick", "trigger", "browser", "tick"));

    // FSM gets its own tick
    net.add_connection(wire("fsm_tick", "trigger", "journey", "tick"));

    // Browser ready → start tick sources + signal FSM
    net.add_connection(wire("browser", "ready", "tick", "start"));
    net.add_connection(wire("browser", "ready", "fsm_tick", "start"));
    net.add_connection(wire("browser", "ready", "journey", "event"));

    // FSM data → browser action
    net.add_connection(wire("journey", "data", "browser", "action"));

    // Browser frame → frame buffer (no GPU render, direct pass-through)
    net.add_connection(wire("browser", "frame", "fbuf", "frame"));
    net.add_connection(wire("tick", "trigger", "fbuf", "tick"));
    net.add_connection(wire("fbuf", "frame", "collector", "frame"));
    net.add_connection(wire("time", "frame_number", "collector", "frame_number"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Bootstrap
    net.add_initial(iip("browser", "url", Message::String(
        std::sync::Arc::new(url.clone()),
    )));

    println!("{}x{}, {}fps, {:.0}s = {} frames", w, h, fps, dur, frames);
    println!("  browser:frame → render:data (layer) → collector → encoder → mp4");
    println!("  Watermark: \"Reflow\" bottom-right\n");

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

    let mp4_path = std::path::Path::new("browser_screencast.mp4");
    let timeout = std::time::Duration::from_secs(120);
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

    let t = start.elapsed();
    if mp4_path.exists() {
        let sz = std::fs::metadata(mp4_path)?.len();
        println!(
            "Saved: browser_screencast.mp4 ({} bytes, {:.1}s, {:.1} fps)",
            sz,
            t.as_secs_f64(),
            frames as f64 / t.as_secs_f64()
        );
    }
    // Force exit — net.shutdown() can hang waiting for Chrome browser task
    std::process::exit(0);
}
