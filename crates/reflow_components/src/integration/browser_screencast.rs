//! Headless browser actor via CDP (Chrome DevTools Protocol).
//!
//! Provides full browser automation: navigation, clicks, scrolling, typing,
//! waiting for selectors, and frame capture via screencast. Uses chromiumoxide
//! to drive a headless Chrome/Chromium.
//!
//! ## Inports
//!
//! - `url`: Initial URL to navigate to (one-shot, or from config).
//! - `tick`: Drives frame emission — each tick emits the latest screencast frame.
//! - `action`: Object commands for browser interaction:
//!   - `{ "type": "navigate", "url": "..." }`
//!   - `{ "type": "click", "selector": "..." }` or `{ "type": "click", "x": N, "y": N }`
//!   - `{ "type": "type", "selector": "...", "text": "..." }`
//!   - `{ "type": "scroll", "x": N, "y": N }` (delta pixels)
//!   - `{ "type": "wait", "selector": "..." }` (wait for element to appear)
//!   - `{ "type": "screenshot" }` (emit a full-page PNG on `frame`)
//!   - `{ "type": "evaluate", "expression": "..." }` (run JS, result on `result`)
//!
//! ## Outports
//!
//! - `frame`: `Message::Bytes` — RGBA pixels of the latest screencast frame.
//! - `metadata`: Object with `{ width, height, format, frameNumber }`.
//! - `result`: Object — return value from `evaluate` actions.
//! - `done`: `Boolean(true)` when browser closes or maxFrames reached.
//!
//! Feature-gated behind `browser` (native-only, not wasm-safe).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::Mutex as ParkMutex;

// ═══════════════════════════════════════════════════════════════
// Shared state between background browser task and tick handler
// ═══════════════════════════════════════════════════════════════

struct FrameBuffer {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
    frame_number: u64,
}

/// Commands sent to a browser session (from actor ticks or external callers).
#[derive(Debug)]
pub enum BrowserCommand {
    Navigate(String),
    ClickSelector(String),
    Mouse { event_type: String, x: f64, y: f64, button: String, delta_x: f64, delta_y: f64 },
    Type { selector: String, text: String },
    WaitForSelector(String),
    Evaluate(String),
    Screenshot,
    /// Insert text at cursor via CDP Input.insertText (no element finding needed)
    InsertText(String),
    /// Stop screencast and close the browser. The session is removed from the registry.
    Stop,
}

struct BrowserSession {
    frame_buf: Arc<ParkMutex<Option<FrameBuffer>>>,
    cmd_tx: flume::Sender<BrowserCommand>,
    result_rx: flume::Receiver<Value>,
    /// Request a screenshot: send () → receive RGBA bytes
    screenshot_tx: flume::Sender<()>,
    screenshot_rx: flume::Receiver<Vec<u8>>,
}

static SESSIONS: std::sync::OnceLock<ParkMutex<HashMap<u64, Arc<BrowserSession>>>> =
    std::sync::OnceLock::new();

fn session_registry() -> &'static ParkMutex<HashMap<u64, Arc<BrowserSession>>> {
    SESSIONS.get_or_init(|| ParkMutex::new(HashMap::new()))
}

static NEXT_SESSION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Send a command to a browser session by ID.
/// Callable from anywhere (example main, other actors, etc.).
pub fn send_browser_command(session_id: u64, cmd: BrowserCommand) -> bool {
    if let Some(session) = session_registry().lock().get(&session_id) {
        session.cmd_tx.send(cmd).is_ok()
    } else {
        false
    }
}

/// Stop a browser session and remove it from the registry.
pub fn stop_browser_session(session_id: u64) {
    send_browser_command(session_id, BrowserCommand::Stop);
    session_registry().lock().remove(&session_id);
}

// ═══════════════════════════════════════════════════════════════
// Actor
// ═══════════════════════════════════════════════════════════════

#[actor(
    BrowserScreencastActor,
    inports::<100>(url, tick, action),
    outports::<100>(frame, metadata, result, done, ready),
    state(MemoryState)
)]
pub async fn browser_screencast_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let payload = ctx.get_payload();

    let vp_width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(1280) as u32;
    let vp_height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(720) as u32;
    let quality = config.get("quality").and_then(|v| v.as_u64()).unwrap_or(80) as i64;
    let every_nth = config.get("everyNthFrame").and_then(|v| v.as_u64()).unwrap_or(1) as i64;
    let wait_ms = config.get("waitBeforeCapture").and_then(|v| v.as_u64()).unwrap_or(2000);

    // ── First invocation: launch browser ──

    let session_id: u64 = ctx
        .get_pool("_browser")
        .into_iter()
        .find(|(k, _)| k == "session_id")
        .and_then(|(_, v)| v.as_u64())
        .unwrap_or(0);

    if session_id == 0 {
        let url: String = payload
            .get("url")
            .and_then(|m| match m {
                Message::String(s) => Some(s.as_ref().clone()),
                _ => None,
            })
            .or_else(|| config.get("url").and_then(|v| v.as_str()).map(String::from))
            .unwrap_or_else(|| "about:blank".to_string());

        let frame_buf = Arc::new(ParkMutex::new(None));
        let (cmd_tx, cmd_rx) = flume::unbounded();
        let (result_tx, result_rx) = flume::unbounded();
        let (ss_request_tx, ss_request_rx) = flume::bounded::<()>(1);
        let (ss_response_tx, ss_response_rx) = flume::bounded::<Vec<u8>>(1);

        let sid = NEXT_SESSION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let session = Arc::new(BrowserSession {
            frame_buf: frame_buf.clone(),
            cmd_tx,
            result_rx,
            screenshot_tx: ss_request_tx,
            screenshot_rx: ss_response_rx,
        });
        session_registry().lock().insert(sid, session);

        ctx.pool_upsert("_browser", "session_id", json!(sid));
        ctx.pool_upsert("_browser", "frame_count", json!(0u64));

        let outport_tx = ctx.get_outports().0;

        let outport_tx2 = outport_tx.clone();
        tokio::spawn(async move {
            if let Err(e) = run_browser(
                url, vp_width, vp_height, quality, every_nth, wait_ms,
                frame_buf, cmd_rx, result_tx, outport_tx2,
                ss_request_rx, ss_response_tx,
            )
            .await
            {
                eprintln!("[browser] error: {e}");
            }

            let mut out = HashMap::new();
            out.insert("done".to_string(), Message::Boolean(true));
            let _ = outport_tx.send(out);

            session_registry().lock().remove(&sid);
        });

        return Ok(HashMap::new());
    }

    // ── Get session ──

    let session = match session_registry().lock().get(&session_id).cloned() {
        Some(s) => s,
        None => return Ok(HashMap::new()),
    };

    // ── Process action commands ──

    // Flow signal on action = stop (natural "done" signal from upstream)
    if let Some(Message::Flow) = payload.get("action") {
        let _ = session.cmd_tx.send(BrowserCommand::Stop);
        session_registry().lock().remove(&session_id);
        let mut out = HashMap::new();
        out.insert("done".to_string(), Message::Boolean(true));
        return Ok(out);
    }

    if let Some(msg) = payload.get("action") {
        eprintln!("[browser] action: {:?}", std::mem::discriminant(msg));
    }
    if let Some(Message::Object(obj)) = payload.get("action") {
        let v: Value = obj.as_ref().clone().into();
        eprintln!("[browser] action obj: {}", v);
        if let Some(action_type) = v.get("type").and_then(|t| t.as_str()) {
            let cmd = match action_type {
                "navigate" => v.get("url").and_then(|u| u.as_str()).map(|u| {
                    BrowserCommand::Navigate(u.to_string())
                }),
                "click" => {
                    // Selector-based click: { type: "click", selector: "..." }
                    if let Some(sel) = v.get("selector").and_then(|s| s.as_str()) {
                        Some(BrowserCommand::ClickSelector(sel.to_string()))
                    } else {
                        // Coordinate click: { type: "click", x: N, y: N }
                        Some(BrowserCommand::Mouse {
                            event_type: "click".to_string(),
                            x: v.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0),
                            y: v.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0),
                            button: "left".to_string(),
                            delta_x: 0.0, delta_y: 0.0,
                        })
                    }
                }
                // Raw mouse: { type: "mouse", event: "move"|"press"|"release"|"wheel", x, y, ... }
                "mouse" => Some(BrowserCommand::Mouse {
                    event_type: v.get("event").and_then(|s| s.as_str()).unwrap_or("move").to_string(),
                    x: v.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0),
                    y: v.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0),
                    button: v.get("button").and_then(|s| s.as_str()).unwrap_or("left").to_string(),
                    delta_x: v.get("deltaX").and_then(|n| n.as_f64()).unwrap_or(0.0),
                    delta_y: v.get("deltaY").and_then(|n| n.as_f64()).unwrap_or(0.0),
                }),
                // Scroll shorthand: { type: "scroll", x: deltaX, y: deltaY }
                "scroll" => Some(BrowserCommand::Mouse {
                    event_type: "wheel".to_string(),
                    x: 0.0, y: 0.0,
                    button: "none".to_string(),
                    delta_x: v.get("x").and_then(|n| n.as_f64()).unwrap_or(0.0),
                    delta_y: v.get("y").and_then(|n| n.as_f64()).unwrap_or(0.0),
                }),
                "type" => {
                    let sel = v.get("selector").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    let text = v.get("text").and_then(|s| s.as_str()).unwrap_or("").to_string();
                    Some(BrowserCommand::Type { selector: sel, text })
                }
                "wait" => v.get("selector").and_then(|s| s.as_str()).map(|s| {
                    BrowserCommand::WaitForSelector(s.to_string())
                }),
                "evaluate" => v.get("expression").and_then(|s| s.as_str()).map(|s| {
                    BrowserCommand::Evaluate(s.to_string())
                }),
                "screenshot" => Some(BrowserCommand::Screenshot),
                "insertText" => v.get("text").and_then(|s| s.as_str()).map(|s| {
                    BrowserCommand::InsertText(s.to_string())
                }),
                "stop" => Some(BrowserCommand::Stop),
                _ => None,
            };
            if let Some(cmd) = cmd {
                let _ = session.cmd_tx.send(cmd);
            }
        }
    }

    // ── Check for evaluate results ──

    let mut out = HashMap::new();
    if let Ok(result) = session.result_rx.try_recv() {
        out.insert(
            "result".to_string(),
            Message::object(EncodableValue::from(result)),
        );
    }

    // ── On tick: request a fresh screenshot (synchronous, unique per tick) ──

    if !payload.contains_key("tick") {
        return Ok(out);
    }

    // Request screenshot from background task and wait for response
    let _ = session.screenshot_tx.send_async(()).await;
    let rgba = match session.screenshot_rx.recv_async().await {
        Ok(data) => data,
        Err(_) => return Ok(out), // session closed
    };

    let mut frame_count: u64 = ctx
        .get_pool("_browser")
        .into_iter()
        .find(|(k, _)| k == "frame_count")
        .and_then(|(_, v)| v.as_u64())
        .unwrap_or(0);
    frame_count += 1;
    ctx.pool_upsert("_browser", "frame_count", json!(frame_count));

    let frame_pixels = rgba.len() / 4;
    let fw = vp_width;
    let fh = if fw > 0 { frame_pixels as u32 / fw } else { vp_height };
    out.insert("frame".to_string(), Message::bytes(rgba));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": fw,
            "height": fh,
            "format": "rgba",
            "frameNumber": frame_count,
        }))),
    );
    Ok(out)
}

// ═══════════════════════════════════════════════════════════════
// Background browser task
// ═══════════════════════════════════════════════════════════════

async fn run_browser(
    url: String,
    width: u32,
    height: u32,
    quality: i64,
    every_nth: i64,
    wait_ms: u64,
    frame_buf: Arc<ParkMutex<Option<FrameBuffer>>>,
    cmd_rx: flume::Receiver<BrowserCommand>,
    result_tx: flume::Sender<Value>,
    outport_tx: flume::Sender<HashMap<String, Message>>,
    ss_request_rx: flume::Receiver<()>,
    ss_response_tx: flume::Sender<Vec<u8>>,
) -> Result<()> {
    use chromiumoxide::browser::{Browser, BrowserConfig};
    use chromiumoxide::cdp::browser_protocol::page::{
        ScreencastFrameAckParams, StartScreencastFormat, StartScreencastParams,
    };
    use chromiumoxide::cdp::browser_protocol::input::{
        DispatchMouseEventParams, DispatchMouseEventType, MouseButton,
        InsertTextParams,
    };
    use futures::StreamExt;

    let config = BrowserConfig::builder()
        .window_size(width, height)
        .arg("--hide-scrollbars")
        .arg("--mute-audio")
        .build()
        .map_err(|e| anyhow::anyhow!("browser config: {e}"))?;

    let (mut browser, mut handler) = Browser::launch(config).await?;

    // Drive CDP handler
    let handler_task = tokio::spawn(async move {
        while handler.next().await.is_some() {}
    });

    let page = browser.new_page(&url).await?;

    // Set exact viewport dimensions via CDP (overrides window_size which is unreliable)
    use chromiumoxide::cdp::browser_protocol::emulation::SetDeviceMetricsOverrideParams;
    if let Ok(viewport) = SetDeviceMetricsOverrideParams::builder()
        .width(width as i64)
        .height(height as i64)
        .device_scale_factor(1.0)
        .mobile(false)
        .build()
    {
        let _ = page.execute(viewport).await;
    }

    // Listen for console output and JS exceptions
    if let Ok(mut console_events) = page
        .event_listener::<chromiumoxide::cdp::js_protocol::runtime::EventConsoleApiCalled>()
        .await
    {
        tokio::spawn(async move {
            while let Some(evt) = console_events.next().await {
                let args: Vec<String> = evt.args.iter()
                    .filter_map(|a| a.value.as_ref().map(|v| v.to_string()))
                    .collect();
                if !args.is_empty() {
                    eprintln!("[browser:console] {:?} {}", evt.r#type, args.join(" "));
                }
            }
        });
    }
    if let Ok(mut exception_events) = page
        .event_listener::<chromiumoxide::cdp::js_protocol::runtime::EventExceptionThrown>()
        .await
    {
        tokio::spawn(async move {
            while let Some(evt) = exception_events.next().await {
                let text = evt.exception_details.text.clone();
                eprintln!("[browser:exception] {}", text);
            }
        });
    }

    tokio::time::sleep(std::time::Duration::from_millis(wait_ms)).await;

    // Start screencast — restartable after page navigation
    async fn start_screencast(
        page: &chromiumoxide::Page,
        quality: i64,
        width: u32,
        height: u32,
        every_nth: i64,
    ) -> Result<()> {
        let params = StartScreencastParams::builder()
            .format(StartScreencastFormat::Jpeg)
            .quality(quality)
            .max_width(width as i64)
            .max_height(height as i64)
            .every_nth_frame(every_nth)
            .build();
        page.execute(params).await?;
        Ok(())
    }

    start_screencast(&page, quality, width, height, every_nth).await?;

    let mut events = page
        .event_listener::<chromiumoxide::cdp::browser_protocol::page::EventScreencastFrame>()
        .await?;

    let mut nav_events = page
        .event_listener::<chromiumoxide::cdp::browser_protocol::page::EventFrameNavigated>()
        .await?;

    let mut frame_num: u64 = 0;

    // Take initial screenshot and signal ready BEFORE entering select loop.
    // This breaks the chicken-and-egg: tick needs ready, ready needs screenshot.
    if let Ok(png_bytes) = page.screenshot(
        chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams::default()
    ).await {
        if let Ok(img) = image::load_from_memory(&png_bytes) {
            let rgba_img = img.to_rgba8();
            let (w, h) = (rgba_img.width(), rgba_img.height());
            frame_num = 1;
            let mut guard = frame_buf.lock();
            *guard = Some(FrameBuffer {
                rgba: rgba_img.into_raw(),
                width: w,
                height: h,
                frame_number: 1,
            });
            drop(guard);
            eprintln!("[browser] initial screenshot {}x{}, signaling ready", w, h);
            let mut ready_msg = HashMap::new();
            ready_msg.insert("ready".to_string(), Message::String(std::sync::Arc::new("LOADED".to_string())));
            let _ = outport_tx.send(ready_msg);
        }
    }

    loop {
        if outport_tx.is_disconnected() { break; }
        tokio::select! {
            // ── On-demand screenshot (requested by tick handler) ──
            Ok(_) = ss_request_rx.recv_async() => {
                frame_num += 1;
                if let Ok(png_bytes) = page.screenshot(
                    chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams::default()
                ).await {
                    if let Ok(img) = image::load_from_memory(&png_bytes) {
                        let rgba_img = img.to_rgba8();
                        let rgba_data = rgba_img.into_raw();
                        let _ = ss_response_tx.send(rgba_data);
                    }
                }
            }

            // ── Screencast frame (kept for navigation detection but not used for video) ──
            Some(event) = events.next() => {
                frame_num += 1;

                let jpeg_bytes = base64::Engine::decode(
                    &base64::engine::general_purpose::STANDARD,
                    &event.data,
                )?;
                let img = image::load_from_memory_with_format(
                    &jpeg_bytes, image::ImageFormat::Jpeg,
                )?;
                let rgba_img = img.to_rgba8();
                let (w, h) = (rgba_img.width(), rgba_img.height());
                let rgba_data = rgba_img.into_raw();

                // Store in shared buffer (for tick-based polling if needed)
                {
                    let mut guard = frame_buf.lock();
                    *guard = Some(FrameBuffer {
                        rgba: rgba_data.clone(),
                        width: w,
                        height: h,
                        frame_number: frame_num,
                    });
                }

                // Push frame directly to outport — every Chrome frame is unique
                eprintln!("[browser] pushed frame {frame_num} ({w}x{h})");
                {
                    let mut out = HashMap::new();
                    out.insert("frame".to_string(), Message::bytes(rgba_data));
                    out.insert(
                        "metadata".to_string(),
                        Message::object(EncodableValue::from(json!({
                            "width": w,
                            "height": h,
                            "format": "rgba",
                            "frameNumber": frame_num,
                        }))),
                    );
                    let _ = outport_tx.send(out);
                }

                // Signal ready on first frame so downstream (e.g., tick) can start
                if frame_num == 1 {
                    eprintln!("[browser] first frame {}x{}, signaling ready", w, h);
                    let mut ready_msg = HashMap::new();
                    ready_msg.insert("ready".to_string(), Message::String(std::sync::Arc::new("LOADED".to_string())));
                    let _ = outport_tx.send(ready_msg);
                }

                let ack = ScreencastFrameAckParams::new(event.session_id);
                let _ = page.execute(ack).await;
            }

            // ── Interaction commands (Err = channel closed → shutdown) ──
            result = cmd_rx.recv_async() => {
                let cmd = match result {
                    Ok(c) => c,
                    Err(_) => break, // network shut down, all senders dropped
                };
                match cmd {
                    BrowserCommand::Navigate(url) => {
                        let _ = page.goto(&url).await;
                    }
                    BrowserCommand::ClickSelector(sel) => {
                        let page_clone = page.clone();
                        tokio::spawn(async move {
                            if let Ok(Ok(el)) = tokio::time::timeout(
                                std::time::Duration::from_secs(10),
                                page_clone.find_element(&sel),
                            ).await {
                                let _ = el.click().await;
                            }
                        });
                    }
                    BrowserCommand::Mouse { event_type, x, y, button, delta_x, delta_y } => {
                        macro_rules! mouse_evt {
                            ($etype:expr) => {{
                                let btn = match button.as_str() {
                                    "right" => MouseButton::Right,
                                    "middle" => MouseButton::Middle,
                                    "none" => MouseButton::None,
                                    _ => MouseButton::Left,
                                };
                                DispatchMouseEventParams::builder()
                                    .r#type($etype).x(x).y(y).button(btn).click_count(1)
                                    .build()
                            }};
                        }
                        match event_type.as_str() {
                            "move" => { if let Ok(p) = mouse_evt!(DispatchMouseEventType::MouseMoved) { let _ = page.execute(p).await; } }
                            "press" => { if let Ok(p) = mouse_evt!(DispatchMouseEventType::MousePressed) { let _ = page.execute(p).await; } }
                            "release" => { if let Ok(p) = mouse_evt!(DispatchMouseEventType::MouseReleased) { let _ = page.execute(p).await; } }
                            "click" => {
                                if let Ok(p) = mouse_evt!(DispatchMouseEventType::MousePressed) { let _ = page.execute(p).await; }
                                if let Ok(p) = mouse_evt!(DispatchMouseEventType::MouseReleased) { let _ = page.execute(p).await; }
                            }
                            "wheel" => {
                                if let Ok(p) = DispatchMouseEventParams::builder()
                                    .r#type(DispatchMouseEventType::MouseWheel)
                                    .x(x).y(y).delta_x(delta_x).delta_y(delta_y).build()
                                { let _ = page.execute(p).await; }
                            }
                            _ => {}
                        }
                    }
                    BrowserCommand::Type { selector, text } => {
                        let page_clone = page.clone();
                        tokio::spawn(async move {
                            if let Ok(Ok(el)) = tokio::time::timeout(
                                std::time::Duration::from_secs(10),
                                page_clone.find_element(&selector),
                            ).await {
                                let _ = el.click().await;
                                let _ = el.type_str(&text).await;
                            }
                        });
                    }
                    BrowserCommand::WaitForSelector(sel) => {
                        let _ = page.find_element(&sel).await;
                    }
                    BrowserCommand::Evaluate(expr) => {
                        eprintln!("[browser] eval: {}...", &expr[..expr.len().min(60)]);
                        match page.evaluate(expr.as_str()).await {
                            Ok(val) => {
                                let json_val: Value = val.into_value().unwrap_or(Value::Null);
                                if !json_val.is_null() {
                                    eprintln!("[browser] eval result: {}", json_val);
                                }
                                let _ = result_tx.send(json_val);
                            }
                            Err(e) => {
                                eprintln!("[browser] eval error: {}", e);
                                let _ = result_tx.send(json!({"error": e.to_string()}));
                            }
                        }
                    }
                    BrowserCommand::Screenshot => {
                        if let Ok(png_bytes) = page.screenshot(
                            chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotParams::default()
                        ).await {
                            if let Ok(img) = image::load_from_memory(&png_bytes) {
                                let rgba_img = img.to_rgba8();
                                let (w, h) = (rgba_img.width(), rgba_img.height());
                                frame_num += 1;
                                let mut guard = frame_buf.lock();
                                *guard = Some(FrameBuffer {
                                    rgba: rgba_img.into_raw(),
                                    width: w,
                                    height: h,
                                    frame_number: frame_num,
                                });
                            }
                        }
                    }
                    BrowserCommand::InsertText(text) => {
                        // Insert entire string at once via CDP Input.insertText
                        if let Ok(params) = InsertTextParams::builder().text(&text).build() {
                            eprintln!("[browser] insertText: {}", text);
                            let _ = page.execute(params).await;
                        }
                    }
                    BrowserCommand::Stop => break,
                }
            }
        }
    }

    let _ = browser.close().await;
    handler_task.abort();
    Ok(())
}
