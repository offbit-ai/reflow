//! Image overlay actor — composites a cached overlay RGBA image onto each
//! incoming frame using CPU alpha_over. The overlay is received once and
//! cached; subsequent frames are composited instantly.
//!
//! ## Config
//! - `x`, `y`: overlay position (default 0, 0)
//! - `width`, `height`: frame dimensions
//!
//! ## Inports
//! - `frame`: base RGBA frame (Bytes or pool Integer)
//! - `overlay`: overlay RGBA image to composite (Bytes, received once, cached)
//!
//! ## Outports
//! - `image`: composited RGBA frame (Bytes)

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex as ParkMutex;

// Cached overlay image (rendered once by upstream, e.g. VectorRasterizeActor)
struct CachedOverlay {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

static OVERLAYS: std::sync::OnceLock<ParkMutex<HashMap<String, Arc<CachedOverlay>>>> =
    std::sync::OnceLock::new();

fn overlay_registry() -> &'static ParkMutex<HashMap<String, Arc<CachedOverlay>>> {
    OVERLAYS.get_or_init(|| ParkMutex::new(HashMap::new()))
}

#[actor(
    ImageOverlayActor,
    inports::<100>(frame: latest, overlay),
    outports::<100>(image),
    state(MemoryState)
)]
pub async fn image_overlay_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let payload = ctx.get_payload();
    let node_id = ctx.get_config().get_node_id().to_string();

    let ox = config.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let oy = config.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let frame_w = config.get("width").and_then(|v| v.as_u64()).unwrap_or(640) as u32;
    let frame_h = config.get("height").and_then(|v| v.as_u64()).unwrap_or(360) as u32;
    let pool_name = config.get("framePool").and_then(|v| v.as_str()).unwrap_or("");

    // Cache overlay image when it arrives (once)
    if let Some(Message::Bytes(data)) = payload.get("overlay") {
        // Determine overlay dimensions from metadata or assume square-ish
        let len = data.len();
        let pixels = (len / 4) as u32;
        let ow = config.get("overlayWidth").and_then(|v| v.as_u64()).unwrap_or_else(|| {
            (pixels as f64).sqrt() as u64
        }) as u32;
        let oh = if ow > 0 { pixels / ow } else { 0 };
        overlay_registry().lock().insert(
            node_id.clone(),
            Arc::new(CachedOverlay { rgba: (**data).clone(), width: ow, height: oh }),
        );
    }

    // Process frame
    if let Some(msg) = payload.get("frame") {
        let mut frame = match msg {
            Message::Integer(slot_idx) if !pool_name.is_empty() => {
                if let Some(pool) = reflow_actor::frame_pool::FramePool::get(pool_name) {
                    pool.clone_slot(*slot_idx as usize)
                } else {
                    return Ok(HashMap::new());
                }
            }
            Message::Bytes(data) => (**data).clone(),
            _ => return Ok(HashMap::new()),
        };

        // Composite overlay onto frame
        if let Some(overlay) = overlay_registry().lock().get(&node_id) {
            composite_at(
                &mut frame, frame_w,
                &overlay.rgba, overlay.width, overlay.height,
                ox as u32, oy as u32,
            );
        }

        let mut out = HashMap::new();
        out.insert("image".to_string(), Message::bytes(frame));
        return Ok(out);
    }

    Ok(HashMap::new())
}

fn composite_at(
    base: &mut [u8],
    base_w: u32,
    overlay: &[u8],
    ov_w: u32,
    ov_h: u32,
    ox: u32,
    oy: u32,
) {
    for row in 0..ov_h {
        let dy = oy + row;
        for col in 0..ov_w {
            let dx = ox + col;
            if dx >= base_w { continue; }
            let si = ((row * ov_w + col) * 4) as usize;
            let di = ((dy * base_w + dx) * 4) as usize;
            if si + 3 >= overlay.len() || di + 3 >= base.len() { continue; }
            if overlay[si + 3] == 0 { continue; }
            let mut bg = [base[di], base[di+1], base[di+2], base[di+3]];
            let fg = [overlay[si], overlay[si+1], overlay[si+2], overlay[si+3]];
            reflow_pixel::blend::alpha_over(&mut bg, &fg);
            base[di..di+4].copy_from_slice(&bg);
        }
    }
}
