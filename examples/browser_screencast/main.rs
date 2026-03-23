//! # Browser Screencast — Capture-then-Encode
//!
//! Phase 1 (CAPTURE): Browser runs in real-time. Chrome screencast pushes
//! frames at its natural rate. FSM choreographs the journey. All frames
//! go directly to the collector which streams them to the encoder.
//!
//! Phase 2 (ENCODE): Encoder processes frames as they arrive, writes MP4.
//! The fps metadata determines playback speed, not capture speed.
//!
//! No tick-driven frame release. No frame buffer. No dropped frames.

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
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://google.com".to_string());

    println!("=== Browser Screencast — Capture-then-Encode ===\n");

    let w = 640u32;
    let h = 360u32;
    let fps = 24u32;
    let capture_frames = 300; // 300 ticks at 24fps = 12.5s video
    let dt = 1.0 / fps as f64;

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger",
        "tpl_browser_screencast",
        "tpl_fsm",
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ BROWSER — pushes frames at Chrome's natural screencast rate ═══
    net.add_node("browser", "tpl_browser_screencast", config(json!({
        "url": url,
        "width": w,
        "height": h,
        "quality": 80,
        "everyNthFrame": 1,
        "waitBeforeCapture": 1500,
    })))?;

    // ═══ TICK — drives browser screencast + FSM timing ═══
    // Browser tick controls how often we poll the latest frame.
    // FSM tick — drives journey timing. Browser pushes frames independently.
    let tick_ms = 1000 / fps as u64;
    net.add_node("fsm_tick", "tpl_interval_trigger", config(json!({
        "interval": tick_ms, "maxExecutions": capture_frames, "startImmediately": false,
    })))?;

    // ═══ JOURNEY FSM ═══
    net.add_node("journey", "tpl_fsm", config(json!({
        "initial": "waiting",
        "dt": dt,
        "states": {
            "waiting": {
                "on": { "LOADED": { "target": "viewing_consent" } }
            },
            "viewing_consent": {
                "on": { "_timeout": { "target": "scroll_to_accept", "delay": 0.5 } }
            },
            "scroll_to_accept": {
                "on": { "_timeout": { "target": "click_accept", "delay": 0.5 } },
                "entry": { "emit": { "type": "scroll", "x": 0, "y": 500 } }
            },
            "click_accept": {
                "on": { "_timeout": { "target": "focus_search", "delay": 1.5 } },
                "entry": { "emit": { "type": "evaluate", "expression": "[...document.querySelectorAll('button')].find(b => b.textContent.includes('Accept all'))?.click()" } }
            },
            "focus_search": {
                "on": { "_timeout": { "target": "type_search", "delay": 0.3 } },
                "entry": { "emit": { "type": "evaluate", "expression": "(document.querySelector('textarea[name=q]')||document.querySelector('input[name=q]'))?.focus()" } }
            },
            "type_search": {
                "on": { "_timeout": { "target": "submit_search", "delay": 0.5 } },
                "entry": { "emit": { "type": "insertText", "text": "Reflow DAG engine" } }
            },
            "submit_search": {
                "on": { "_timeout": { "target": "browsing", "delay": 5.0 } },
                "entry": { "emit": { "type": "evaluate", "expression": "window.location.href='https://www.google.com/search?q=Reflow+DAG+engine'" } }
            },
            "browsing": {}
        }
    })))?;

    // ═══ VIDEO — collector receives frames directly from browser ═══
    net.add_node("collector", "tpl_render_frame_collector",
        config(json!({ "totalFrames": capture_frames, "width": w, "height": h, "fps": fps })))?;
    net.add_node("encoder", "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 8000 })))?;
    net.add_node("save", "tpl_file_save",
        config(json!({ "path": "browser_screencast.mp4" })))?;

    // ═══ WIRING ═══

    // Single tick drives both browser + FSM (synchronized)
    net.add_connection(wire("fsm_tick", "trigger", "browser", "tick"));
    net.add_connection(wire("fsm_tick", "trigger", "journey", "tick"));

    // Browser ready → start tick + signal FSM
    net.add_connection(wire("browser", "ready", "fsm_tick", "start"));
    net.add_connection(wire("browser", "ready", "journey", "event"));

    // FSM → browser actions
    net.add_connection(wire("journey", "data", "browser", "action"));

    // Browser frame → collector (tick-driven, screenshot-backed)
    net.add_connection(wire("browser", "frame", "collector", "frame"));


    // Collector → encoder → file
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Bootstrap
    net.add_initial(iip("browser", "url", Message::String(
        std::sync::Arc::new(url.clone()),
    )));

    println!("{}x{}, {}fps, {} capture frames", w, h, fps, capture_frames);
    println!("  browser:frame → collector → encoder → mp4 (direct, no frame buffer)");
    println!("  FSM journey: consent → scroll → accept → type → search\n");

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
            sz, t.as_secs_f64(), capture_frames as f64 / t.as_secs_f64()
        );
    }
    std::process::exit(0);
}
