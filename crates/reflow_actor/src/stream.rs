use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bitcode::{Decode, Encode};
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Globally unique stream identifier.
pub type StreamId = u64;

/// Default bounded channel buffer size for stream backpressure.
pub const DEFAULT_STREAM_BUFFER: usize = 64;

/// Frames that flow through a stream's side-channel.
///
/// These travel through bounded `flume` channels, NOT through the `Message`
/// enum or connectors. The `Message::StreamHandle` variant carries only the
/// serializable [`StreamHandle`]; actual data flows out-of-band.
#[derive(Clone, Debug)]
pub enum StreamFrame {
    /// Stream metadata sent before data chunks.
    Begin {
        content_type: Option<String>,
        size_hint: Option<u64>,
        metadata: Option<serde_json::Value>,
    },
    /// A chunk of binary data.
    Data(Arc<Vec<u8>>),
    /// Stream completed successfully.
    End,
    /// Stream terminated with an error.
    Error(String),
}

impl StreamFrame {
    /// Returns true if this is a terminal frame (End or Error).
    pub fn is_terminal(&self) -> bool {
        matches!(self, StreamFrame::End | StreamFrame::Error(_))
    }
}

/// Serializable handle that travels through the `Message` enum and connectors.
///
/// Contains only metadata and a [`StreamId`] — the actual data channel is
/// managed by the [`StreamRegistry`].
#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode, PartialEq)]
pub struct StreamHandle {
    pub stream_id: StreamId,
    /// Actor that created this stream (for debugging / tracing).
    pub origin_actor: String,
    /// Port the stream was created on.
    pub origin_port: String,
    /// MIME content type hint (e.g. "application/octet-stream").
    pub content_type: Option<String>,
    /// Expected total size in bytes, if known.
    pub size_hint: Option<u64>,
}

/// Process-global stream registry.
///
/// Maps [`StreamId`] → bounded `flume` channel pair. Producers obtain the
/// sender when creating a stream; consumers take the receiver via
/// [`take_receiver`].
///
/// Using `flume::bounded` provides natural backpressure: when the buffer is
/// full, `sender.send_async().await` suspends the producer until the consumer
/// reads. This works on both native and WASM targets.
pub static STREAM_REGISTRY: Lazy<StreamRegistry> = Lazy::new(StreamRegistry::new);

pub struct StreamRegistry {
    next_id: AtomicU64,
    senders: RwLock<HashMap<StreamId, flume::Sender<StreamFrame>>>,
    receivers: RwLock<HashMap<StreamId, flume::Receiver<StreamFrame>>>,
}

impl StreamRegistry {
    fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            senders: RwLock::new(HashMap::new()),
            receivers: RwLock::new(HashMap::new()),
        }
    }

    /// Allocate a new stream with a bounded channel.
    ///
    /// Returns `(stream_id, sender)`. The receiver is stored in the registry
    /// and must be taken by the consumer via [`take_receiver`].
    pub fn create_stream(
        &self,
        buffer_size: Option<usize>,
    ) -> (StreamId, flume::Sender<StreamFrame>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = flume::bounded(buffer_size.unwrap_or(DEFAULT_STREAM_BUFFER));
        self.senders.write().insert(id, tx.clone());
        self.receivers.write().insert(id, rx);
        (id, tx)
    }

    /// Take ownership of a stream's receiver (single-consumer).
    ///
    /// Returns `None` if the stream doesn't exist or has already been taken.
    pub fn take_receiver(&self, stream_id: StreamId) -> Option<flume::Receiver<StreamFrame>> {
        self.receivers.write().remove(&stream_id)
    }

    /// Clone the sender for a given stream (used for fan-out).
    pub fn clone_sender(&self, stream_id: StreamId) -> Option<flume::Sender<StreamFrame>> {
        self.senders.read().get(&stream_id).cloned()
    }

    /// Remove all entries for a stream (cleanup after End/Error).
    pub fn remove(&self, stream_id: StreamId) {
        self.senders.write().remove(&stream_id);
        self.receivers.write().remove(&stream_id);
    }

    /// Drop all senders, causing all consumer receivers to yield `None`.
    /// Called during network shutdown.
    pub fn close_all(&self) {
        self.senders.write().clear();
        self.receivers.write().clear();
    }

    /// Number of active streams (diagnostic).
    pub fn active_count(&self) -> usize {
        self.senders.read().len()
    }
}

/// Broadcasts frames from one source receiver to multiple downstream senders.
///
/// Spawns an async task that reads from `source` and replicates each frame to
/// all `downstreams`. Backpressure from the slowest downstream propagates to
/// the broadcaster, which in turn propagates to the original producer.
pub struct StreamBroadcaster;

impl StreamBroadcaster {
    /// Spawn the broadcast fan-out task. Returns immediately.
    ///
    /// Each downstream gets a clone of every frame. If any downstream is
    /// closed, it is silently removed. When the source closes or all
    /// downstreams are gone, the task exits.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn(
        source: flume::Receiver<StreamFrame>,
        downstreams: Vec<flume::Sender<StreamFrame>>,
    ) {
        tokio::spawn(async move {
            Self::run(source, downstreams).await;
        });
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn(
        source: flume::Receiver<StreamFrame>,
        downstreams: Vec<flume::Sender<StreamFrame>>,
    ) {
        wasm_bindgen_futures::spawn_local(async move {
            Self::run(source, downstreams).await;
        });
    }

    async fn run(
        source: flume::Receiver<StreamFrame>,
        mut downstreams: Vec<flume::Sender<StreamFrame>>,
    ) {
        use futures::StreamExt;
        let mut stream = source.into_stream();
        while let Some(frame) = stream.next().await {
            let is_terminal = frame.is_terminal();
            // Send to all live downstreams
            downstreams.retain(|tx| tx.try_send(frame.clone()).is_ok());
            if downstreams.is_empty() || is_terminal {
                break;
            }
        }
    }
}
