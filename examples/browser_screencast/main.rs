//! # Browser Screencast — GPU Render + AssetDB Watermark
//!
//! Phase 1: GPU renders "Reflow" watermark text → stores in AssetDB
//! Phase 2: Browser captures screenshots, GPU composites browser frame
//!          (PRIM_IMAGE layer) + watermark text in one pass
//! Phase 3: Collector → encoder → MP4

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
        .unwrap_or_else(|| "https://duckduckgo.com".to_string());

    println!("=== Browser Screencast — GPU Render + Watermark ===\n");

    let w = 1280u32;
    let h = 720u32;
    let fps = 24u32;
    let dt = 1.0 / fps as f64;

    let mut net = Network::new(NetworkConfig::default());

    for tpl in [
        "tpl_interval_trigger",
        "tpl_browser_screencast",
        "tpl_fsm",
        "tpl_gpu_2d_render",
        "tpl_render_frame_collector",
        "tpl_video_encoder",
        "tpl_file_save",
    ] {
        net.register_actor_arc(tpl, reflow_components::get_actor_for_template(tpl).unwrap())?;
    }

    // ═══ BROWSER ═══
    net.add_node("browser", "tpl_browser_screencast", config(json!({
        "url": url,
        "width": w,
        "height": h,
        "quality": 80,
        "everyNthFrame": 1,
        "waitBeforeCapture": 1500,
    })))?;

    // ═══ FSM TICK ═══
    net.add_node("fsm_tick", "tpl_interval_trigger", config(json!({
        "interval": 1000 / fps as u64,
        "maxExecutions": 500,
        "startImmediately": false,
    })))?;

    // ═══ JOURNEY FSM ═══
    net.add_node("journey", "tpl_fsm", config(json!({
        "initial": "waiting",
        "dt": dt,
        "states": {
            "waiting": {
                "on": { "LOADED": { "target": "focus_search" } }
            },
            "focus_search": {
                "on": { "_timeout": { "target": "type_search", "delay": 0.5 } },
                "entry": { "emit": { "type": "evaluate", "expression": "document.querySelector('input[name=q]')?.focus()" } }
            },
            "type_search": {
                "on": { "_timeout": { "target": "submit_search", "delay": 1.0 } },
                "entry": { "emit": { "type": "insertText", "text": "Reflow DAG engine" } }
            },
            "submit_search": {
                "on": { "_timeout": { "target": "browsing", "delay": 5.0 } },
                "entry": { "emit": { "type": "evaluate", "expression": "document.querySelector('form')?.submit()" } }
            },
            "browsing": {}
        }
    })))?;

    // ═══ GPU RENDERER — browser frame as layer + "Reflow" watermark text ═══
    net.add_node("render", "tpl_gpu_2d_render", config(json!({
        "width": w, "height": h, "msaa": 1,
        "background": [0.0, 0.0, 0.0, 0.0],
        "shapes": [
            { "type": "image", "bounds": [0, 0, w, h], "z": 0 },
        ],
        "text": [{
            "content": "Reflow", "x": w as f64 - 150.0, "y": h as f64 - 30.0,
            "size": 24.0, "color": [1.0, 1.0, 1.0, 0.7],
            "tracking": 1.0, "center": false,
            "font": "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        }],
    })))?;

    // ═══ VIDEO ═══
    net.add_node("collector", "tpl_render_frame_collector",
        config(json!({ "totalFrames": 0, "width": w, "height": h, "fps": fps })))?;
    net.add_node("encoder", "tpl_video_encoder",
        config(json!({ "fps": fps, "bitrate": 8000 })))?;
    net.add_node("save", "tpl_file_save",
        config(json!({ "path": "browser_screencast.mp4" })))?;

    // ═══ WIRING ═══

    // FSM tick
    net.add_connection(wire("fsm_tick", "trigger", "journey", "tick"));

    // Browser ready → start FSM + signal
    net.add_connection(wire("browser", "ready", "fsm_tick", "start"));
    net.add_connection(wire("browser", "ready", "journey", "event"));

    // FSM → browser actions
    net.add_connection(wire("journey", "data", "browser", "action"));

    // Browser frame → GPU render (layer texture via data port)
    net.add_connection(wire("browser", "frame", "render", "data"));

    // Browser done → collector done (page settled = end capture)
    net.add_connection(wire("browser", "done", "collector", "done"));

    // GPU render → collector → encoder → file
    net.add_connection(wire("render", "image", "collector", "frame"));
    net.add_connection(wire("collector", "stream", "encoder", "stream"));
    net.add_connection(wire("encoder", "output", "save", "input"));

    // Bootstrap
    net.add_initial(iip("browser", "url", Message::String(
        std::sync::Arc::new(url.clone()),
    )));

    println!("{}x{}, {}fps playback", w, h, fps);
    println!("  browser:frame → GPU render (layer + watermark) → collector → mp4\n");

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
    let timeout = std::time::Duration::from_secs(300);
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
        println!("Saved: browser_screencast.mp4 ({} bytes, {:.1}s)", sz, t.as_secs_f64());
    }
    std::process::exit(0);
}
