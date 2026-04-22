//! Frame buffer actor — smooths bursty frame delivery for video pipelines.
//!
//! Sits between a frame producer (renderer, browser screencast) and the
//! render_frame_collector. Receives frames as fast as the producer emits
//! them, buffers up to N frames, and releases them at a steady interval
//! matching the target FPS.
//!
//! This decouples the producer's variable frame rate from the video
//! pipeline's fixed frame rate, preventing both starvation and bursts.
//!
//! ## Config
//!
//! - `fps` (u32, default 30): Target output frame rate
//! - `bufferSize` (usize, default 60): Max buffered frames before dropping oldest

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use parking_lot::Mutex as ParkMutex;
use reflow_actor::ActorContext;
use std::collections::HashMap;
use std::sync::Arc;

struct SharedBuffer {
    frames: std::collections::VecDeque<Vec<u8>>,
    last_frame: Option<Vec<u8>>,
    max_size: usize,
}

static FRAME_BUFS: std::sync::OnceLock<ParkMutex<HashMap<String, Arc<ParkMutex<SharedBuffer>>>>> =
    std::sync::OnceLock::new();

fn buf_registry() -> &'static ParkMutex<HashMap<String, Arc<ParkMutex<SharedBuffer>>>> {
    FRAME_BUFS.get_or_init(|| ParkMutex::new(HashMap::new()))
}

#[actor(
    FrameBufferActor,
    inports::<100>(frame: latest, tick),
    outports::<100>(frame, metadata),
    state(MemoryState)
)]
pub async fn frame_buffer_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();
    let payload = ctx.get_payload();
    let node_id = ctx.get_config().get_node_id().to_string();

    let max_size = config
        .get("bufferSize")
        .and_then(|v| v.as_u64())
        .unwrap_or(60) as usize;

    // Get or create shared buffer for this node
    let buf = {
        let mut reg = buf_registry().lock();
        reg.entry(node_id.clone())
            .or_insert_with(|| {
                Arc::new(ParkMutex::new(SharedBuffer {
                    frames: std::collections::VecDeque::with_capacity(max_size),
                    last_frame: None,
                    max_size,
                }))
            })
            .clone()
    };

    let pool_name = config
        .get("framePool")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    // Ingest: buffer incoming frames
    if let Some(msg) = payload.get("frame") {
        let frame_data = match msg {
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
        let mut b = buf.lock();
        if b.frames.len() >= b.max_size {
            b.frames.pop_front();
        }
        b.last_frame = Some(frame_data.clone());
        b.frames.push_back(frame_data);
    }

    // On tick: release buffered frame, or repeat last frame if buffer empty
    if payload.contains_key("tick") {
        static FBUF_TICK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let t = FBUF_TICK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if t % 50 == 0 {
            let b = buf.lock();
            eprintln!(
                "[fbuf] tick={t} buffered={} last={}",
                b.frames.len(),
                b.last_frame.is_some()
            );
        }
        let mut b = buf.lock();
        let frame = if let Some(f) = b.frames.pop_front() {
            f
        } else if let Some(ref last) = b.last_frame {
            last.clone()
        } else {
            return Ok(HashMap::new());
        };
        let mut out = HashMap::new();
        out.insert("frame".to_string(), Message::bytes(frame));
        return Ok(out);
    }

    Ok(HashMap::new())
}
