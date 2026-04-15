//! Shared frame pool — zero-copy ring buffer for passing large frames
//! between actors without channel congestion.
//!
//! Producers write RGBA frames into pool slots. Consumers read by slot
//! index. Channels carry only the slot index (8 bytes), never the frame
//! data. This eliminates channel congestion regardless of frame size.
//!
//! ## Usage
//!
//! ```ignore
//! // Producer (e.g., renderer, browser screencast):
//! let pool = FramePool::get_or_create("video_pipe", 8, 640 * 360 * 4);
//! let slot = pool.write(rgba_bytes);
//! // Send Message::Integer(slot as i64) through DAG
//!
//! // Consumer (e.g., collector, frame buffer):
//! let pool = FramePool::get("video_pipe").unwrap();
//! let frame_data = pool.read(slot);
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use once_cell::sync::Lazy;
use parking_lot::{Mutex, RwLock};

/// A slot in the frame pool.
struct Slot {
    data: RwLock<Vec<u8>>,
    generation: AtomicU64,
}

/// Fixed-size ring buffer for frame data.
/// Producers write to the next slot, consumers read by index.
/// No channels involved — data stays in place, only indices move.
pub struct FramePool {
    slots: Vec<Slot>,
    slot_count: usize,
    frame_size: usize,
    write_cursor: AtomicUsize,
    latest_slot: AtomicUsize,
    generation: AtomicU64,
}

impl FramePool {
    /// Create a new frame pool.
    ///
    /// - `slot_count`: number of ring buffer slots (e.g., 8)
    /// - `frame_size`: max bytes per frame (e.g., 640*360*4 for RGBA)
    pub fn new(slot_count: usize, frame_size: usize) -> Self {
        let slots: Vec<Slot> = (0..slot_count)
            .map(|_| Slot {
                data: RwLock::new(vec![0u8; frame_size]),
                generation: AtomicU64::new(0),
            })
            .collect();

        Self {
            slots,
            slot_count,
            frame_size,
            write_cursor: AtomicUsize::new(0),
            latest_slot: AtomicUsize::new(0),
            generation: AtomicU64::new(0),
        }
    }

    /// Write frame data into the next available slot.
    /// Returns the slot index for consumers to read.
    /// If the data is larger than frame_size, it's truncated.
    pub fn write(&self, data: &[u8]) -> usize {
        let slot_idx = self.write_cursor.fetch_add(1, Ordering::Relaxed) % self.slot_count;
        let g = self.generation.fetch_add(1, Ordering::Relaxed) + 1;

        let slot = &self.slots[slot_idx];
        {
            let mut buf = slot.data.write();
            let len = data.len().min(buf.len());
            buf[..len].copy_from_slice(&data[..len]);
            // If frame is smaller than slot, zero the rest
            if len < buf.len() {
                buf[len..].fill(0);
            }
        }
        slot.generation.store(g, Ordering::Release);
        self.latest_slot.store(slot_idx, Ordering::Release);

        slot_idx
    }

    /// Write frame data, resizing the slot if needed.
    pub fn write_dynamic(&self, data: &[u8]) -> usize {
        let slot_idx = self.write_cursor.fetch_add(1, Ordering::Relaxed) % self.slot_count;
        let g = self.generation.fetch_add(1, Ordering::Relaxed) + 1;

        let slot = &self.slots[slot_idx];
        {
            let mut buf = slot.data.write();
            if buf.len() != data.len() {
                buf.resize(data.len(), 0);
            }
            buf.copy_from_slice(data);
        }
        slot.generation.store(g, Ordering::Release);
        self.latest_slot.store(slot_idx, Ordering::Release);

        slot_idx
    }

    /// Read frame data from a slot (zero-copy borrow via closure).
    pub fn read<R>(&self, slot_idx: usize, f: impl FnOnce(&[u8]) -> R) -> R {
        let slot = &self.slots[slot_idx % self.slot_count];
        let buf = slot.data.read();
        f(&buf)
    }

    /// Read the latest written frame (zero-copy borrow).
    pub fn read_latest<R>(&self, f: impl FnOnce(&[u8], usize) -> R) -> R {
        let slot_idx = self.latest_slot.load(Ordering::Acquire);
        let slot = &self.slots[slot_idx % self.slot_count];
        let buf = slot.data.read();
        f(&buf, slot_idx)
    }

    /// Clone frame data out of a slot.
    pub fn clone_slot(&self, slot_idx: usize) -> Vec<u8> {
        self.read(slot_idx, |data| data.to_vec())
    }

    /// Get the latest slot index.
    pub fn latest(&self) -> usize {
        self.latest_slot.load(Ordering::Acquire)
    }

    /// Get the generation counter (incremented on each write).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Number of slots in the pool.
    pub fn capacity(&self) -> usize {
        self.slot_count
    }

    /// Configured maximum bytes per fixed-size frame slot.
    pub fn frame_size(&self) -> usize {
        self.frame_size
    }
}

// ═══════════════════════════════════════════════════════════════
// Global registry — actors look up pools by name
// ═══════════════════════════════════════════════════════════════

static POOLS: Lazy<Mutex<HashMap<String, Arc<FramePool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

impl FramePool {
    /// Get or create a named frame pool.
    pub fn get_or_create(name: &str, slot_count: usize, frame_size: usize) -> Arc<FramePool> {
        let mut pools = POOLS.lock();
        pools
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(FramePool::new(slot_count, frame_size)))
            .clone()
    }

    /// Get an existing named frame pool.
    pub fn get(name: &str) -> Option<Arc<FramePool>> {
        POOLS.lock().get(name).cloned()
    }

    /// Remove a named frame pool.
    pub fn remove(name: &str) {
        POOLS.lock().remove(name);
    }
}
