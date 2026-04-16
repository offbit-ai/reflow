use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
    /// Non-blocking observer taps registered before the consumer takes the receiver.
    /// When `take_receiver` is called and observers exist, a `StreamBroadcaster` is
    /// automatically set up to fan out frames to both the consumer and all observers.
    observers: RwLock<HashMap<StreamId, Vec<flume::Sender<StreamFrame>>>>,
}

impl StreamRegistry {
    fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            senders: RwLock::new(HashMap::new()),
            receivers: RwLock::new(HashMap::new()),
            observers: RwLock::new(HashMap::new()),
        }
    }

    /// Allocate a new stream.
    ///
    /// `buffer_size = None` creates an **unbounded** channel (no backpressure).
    /// `buffer_size = Some(n)` creates a bounded channel with capacity n.
    ///
    /// Returns `(stream_id, sender)`. The receiver is stored in the registry
    /// and must be taken by the consumer via [`take_receiver`].
    pub fn create_stream(
        &self,
        buffer_size: Option<usize>,
    ) -> (StreamId, flume::Sender<StreamFrame>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = match buffer_size {
            None => flume::unbounded(),
            Some(n) => flume::bounded(n),
        };
        self.senders.write().insert(id, tx.clone());
        self.receivers.write().insert(id, rx);
        (id, tx)
    }

    /// Take ownership of a stream's receiver (single-consumer).
    ///
    /// If observers have been registered via [`add_observer`], a
    /// [`StreamBroadcaster`] is automatically spawned to fan out frames
    /// from the original receiver to both the consumer and all observers.
    /// The consumer receives a new channel; observers use `try_send` so
    /// a slow display node never stalls the processing pipeline.
    ///
    /// Returns `None` if the stream doesn't exist or has already been taken.
    pub fn take_receiver(&self, stream_id: StreamId) -> Option<flume::Receiver<StreamFrame>> {
        let original_rx = self.receivers.write().remove(&stream_id)?;

        // If observers are registered, interpose a broadcaster
        let observer_senders = self.observers.write().remove(&stream_id);
        if let Some(mut obs) = observer_senders
            && !obs.is_empty()
        {
            // Create a new channel for the primary consumer
            let (consumer_tx, consumer_rx) = flume::bounded(DEFAULT_STREAM_BUFFER);
            obs.push(consumer_tx);
            StreamBroadcaster::spawn(original_rx, obs);
            return Some(consumer_rx);
        }

        Some(original_rx)
    }

    /// Clone the sender for a given stream (used for fan-out).
    pub fn clone_sender(&self, stream_id: StreamId) -> Option<flume::Sender<StreamFrame>> {
        self.senders.read().get(&stream_id).cloned()
    }

    /// Register a non-blocking observer for a stream.
    ///
    /// Must be called **before** `take_receiver`. When the consumer later
    /// calls `take_receiver`, a broadcaster is spawned that replicates
    /// frames to both the consumer and all registered observers.
    ///
    /// Observer channels use `try_send` — if the observer is slow, frames
    /// are silently dropped. This makes observers safe for display/preview
    /// purposes without risking backpressure on the processing pipeline.
    ///
    /// Returns the observer's receiver, or `None` if the stream doesn't
    /// exist (or its receiver has already been taken).
    pub fn add_observer(
        &self,
        stream_id: StreamId,
        buffer_size: usize,
    ) -> Option<flume::Receiver<StreamFrame>> {
        // Only allow observers before the consumer has taken the receiver
        if !self.receivers.read().contains_key(&stream_id) {
            return None;
        }
        let (tx, rx) = flume::bounded(buffer_size);
        self.observers
            .write()
            .entry(stream_id)
            .or_default()
            .push(tx);
        Some(rx)
    }

    /// Remove all entries for a stream (cleanup after End/Error).
    pub fn remove(&self, stream_id: StreamId) {
        self.senders.write().remove(&stream_id);
        self.receivers.write().remove(&stream_id);
        self.observers.write().remove(&stream_id);
    }

    /// Drop all senders, causing all consumer receivers to yield `None`.
    /// Called during network shutdown.
    pub fn close_all(&self) {
        self.senders.write().clear();
        self.receivers.write().clear();
        self.observers.write().clear();
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

// ============================================================================
// Stream helpers — shared patterns used by AV actors
// ============================================================================

/// Spawn a background task (tokio on native, spawn_local on wasm).
///
/// Used by producer actors to push frames without blocking the actor return.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_stream_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    tokio::spawn(future);
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_stream_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

/// Consume a stream, apply a transform to each Data chunk, and send the
/// results to an output stream. Passes Begin/End/Error through unchanged.
///
/// This is the core pattern for all "Both" (consumer + producer) actors:
/// GrayscaleFilter, Resize, all audio DSP filters, etc.
///
/// `transform` receives the raw bytes and returns transformed bytes.
/// It is called synchronously for each chunk — keep it fast.
pub async fn stream_transform<F>(
    rx: flume::Receiver<StreamFrame>,
    tx: flume::Sender<StreamFrame>,
    mut transform: F,
) where
    F: FnMut(&[u8]) -> Vec<u8>,
{
    use futures::StreamExt;
    let mut stream = rx.into_stream();
    while let Some(frame) = stream.next().await {
        let is_terminal = frame.is_terminal();
        let out_frame = match frame {
            StreamFrame::Data(data) => {
                let transformed = transform(&data);
                StreamFrame::Data(Arc::new(transformed))
            }
            other => other,
        };
        if tx.send_async(out_frame).await.is_err() {
            break; // downstream closed
        }
        if is_terminal {
            break;
        }
    }
}

/// Consume a stream, apply a stateful transform that can change the Begin
/// metadata (e.g. resize changes dimensions, format conversion changes content_type).
///
/// `on_begin` is called with the Begin metadata and returns modified metadata.
/// `on_data` transforms each data chunk.
pub async fn stream_transform_with_begin<B, F>(
    rx: flume::Receiver<StreamFrame>,
    tx: flume::Sender<StreamFrame>,
    on_begin: B,
    mut on_data: F,
) where
    B: FnOnce(
        Option<String>,
        Option<u64>,
        Option<serde_json::Value>,
    ) -> (Option<String>, Option<u64>, Option<serde_json::Value>),
    F: FnMut(&[u8]) -> Vec<u8>,
{
    use futures::StreamExt;
    let mut stream = rx.into_stream();
    let mut begin_handled = false;
    let mut on_begin = Some(on_begin);

    while let Some(frame) = stream.next().await {
        let is_terminal = frame.is_terminal();
        let out_frame = match frame {
            StreamFrame::Begin {
                content_type,
                size_hint,
                metadata,
            } => {
                begin_handled = true;
                let (ct, sh, md) = if let Some(cb) = on_begin.take() {
                    cb(content_type, size_hint, metadata)
                } else {
                    (content_type, size_hint, metadata)
                };
                StreamFrame::Begin {
                    content_type: ct,
                    size_hint: sh,
                    metadata: md,
                }
            }
            StreamFrame::Data(data) => {
                let transformed = on_data(&data);
                StreamFrame::Data(Arc::new(transformed))
            }
            other => other,
        };

        if tx.send_async(out_frame).await.is_err() {
            break;
        }
        if is_terminal {
            break;
        }
    }

    // If we never saw Begin (malformed stream), ensure we still send End
    if !begin_handled {
        let _ = tx.send_async(StreamFrame::End).await;
    }
}

/// Collect an entire stream into a single `Vec<u8>`.
///
/// Used by consumer-only actors (ImageEncode, AudioEncode, StreamToBytes)
/// that need the full data before producing output.
///
/// Returns `(content_type, metadata, collected_bytes)` or an error string.
pub async fn stream_collect(
    rx: flume::Receiver<StreamFrame>,
) -> Result<(Option<String>, Option<serde_json::Value>, Vec<u8>), String> {
    use futures::StreamExt;
    let mut stream = rx.into_stream();
    let mut content_type = None;
    let mut metadata = None;
    let mut buf = Vec::new();

    while let Some(frame) = stream.next().await {
        match frame {
            StreamFrame::Begin {
                content_type: ct,
                size_hint,
                metadata: md,
            } => {
                content_type = ct;
                metadata = md;
                if let Some(hint) = size_hint {
                    buf.reserve(hint as usize);
                }
            }
            StreamFrame::Data(data) => {
                buf.extend_from_slice(&data);
            }
            StreamFrame::End => break,
            StreamFrame::Error(e) => return Err(e),
        }
    }

    Ok((content_type, metadata, buf))
}

/// Chunk a byte slice into stream frames and send them.
///
/// Sends Begin (with content_type and size_hint), then Data chunks of
/// `chunk_size` bytes, then End.
///
/// Used by BytesToStream and any actor that converts a complete buffer
/// into a stream.
pub async fn stream_from_bytes(
    tx: flume::Sender<StreamFrame>,
    bytes: &[u8],
    chunk_size: usize,
    content_type: Option<String>,
    metadata: Option<serde_json::Value>,
) -> Result<(), flume::SendError<StreamFrame>> {
    tx.send_async(StreamFrame::Begin {
        content_type,
        size_hint: Some(bytes.len() as u64),
        metadata,
    })
    .await?;

    for chunk in bytes.chunks(chunk_size) {
        tx.send_async(StreamFrame::Data(Arc::new(chunk.to_vec())))
            .await?;
    }

    tx.send_async(StreamFrame::End).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn test_stream_registry_create_and_take() {
        let registry = StreamRegistry::new();
        let (id, tx) = registry.create_stream(Some(4));

        assert!(id > 0);
        assert_eq!(registry.active_count(), 1);

        // Take receiver
        let rx = registry.take_receiver(id);
        assert!(rx.is_some());

        // Second take returns None (single-consumer)
        assert!(registry.take_receiver(id).is_none());

        // Send a frame through the channel
        tx.send(StreamFrame::Data(Arc::new(vec![1, 2, 3]))).unwrap();
        tx.send(StreamFrame::End).unwrap();

        let rx = rx.unwrap();
        match rx.recv().unwrap() {
            StreamFrame::Data(data) => assert_eq!(*data, vec![1, 2, 3]),
            _ => panic!("Expected Data frame"),
        }
        assert!(rx.recv().unwrap().is_terminal());
    }

    #[test]
    fn test_stream_registry_remove_and_close() {
        let registry = StreamRegistry::new();
        let (id1, _tx1) = registry.create_stream(None);
        let (_id2, _tx2) = registry.create_stream(None);
        assert_eq!(registry.active_count(), 2);

        registry.remove(id1);
        assert_eq!(registry.active_count(), 1);
        assert!(registry.take_receiver(id1).is_none());

        registry.close_all();
        assert_eq!(registry.active_count(), 0);
    }

    #[test]
    fn test_stream_frame_is_terminal() {
        assert!(
            !StreamFrame::Begin {
                content_type: None,
                size_hint: None,
                metadata: None,
            }
            .is_terminal()
        );
        assert!(!StreamFrame::Data(Arc::new(vec![])).is_terminal());
        assert!(StreamFrame::End.is_terminal());
        assert!(StreamFrame::Error("fail".into()).is_terminal());
    }

    #[test]
    fn test_stream_handle_serialization_roundtrip() {
        let handle = StreamHandle {
            stream_id: 42,
            origin_actor: "producer".into(),
            origin_port: "out".into(),
            content_type: Some("application/octet-stream".into()),
            size_hint: Some(1024),
        };

        let json = serde_json::to_string(&handle).unwrap();
        let deserialized: StreamHandle = serde_json::from_str(&json).unwrap();
        assert_eq!(handle, deserialized);
    }

    #[test]
    fn test_stream_backpressure() {
        let registry = StreamRegistry::new();
        let (id, tx) = registry.create_stream(Some(2));
        let rx = registry.take_receiver(id).unwrap();

        // Fill the bounded buffer
        tx.send(StreamFrame::Data(Arc::new(vec![1]))).unwrap();
        tx.send(StreamFrame::Data(Arc::new(vec![2]))).unwrap();

        // Third send should fail (buffer full, try_send is non-blocking)
        assert!(tx.try_send(StreamFrame::Data(Arc::new(vec![3]))).is_err());

        // Drain one — now send succeeds
        rx.recv().unwrap();
        tx.try_send(StreamFrame::Data(Arc::new(vec![3]))).unwrap();
    }

    #[tokio::test]
    async fn test_stream_broadcaster_fan_out() {
        // Create a source channel
        let (source_tx, source_rx) = flume::bounded(8);

        // Create two downstream channels
        let (down1_tx, down1_rx) = flume::bounded(8);
        let (down2_tx, down2_rx) = flume::bounded(8);

        StreamBroadcaster::spawn(source_rx, vec![down1_tx, down2_tx]);

        // Send frames
        source_tx
            .send_async(StreamFrame::Begin {
                content_type: Some("text/plain".into()),
                size_hint: None,
                metadata: None,
            })
            .await
            .unwrap();
        source_tx
            .send_async(StreamFrame::Data(Arc::new(b"hello".to_vec())))
            .await
            .unwrap();
        source_tx.send_async(StreamFrame::End).await.unwrap();

        // Both downstreams should receive all frames
        for rx in [&down1_rx, &down2_rx] {
            match rx.recv_async().await.unwrap() {
                StreamFrame::Begin { content_type, .. } => {
                    assert_eq!(content_type.as_deref(), Some("text/plain"));
                }
                _ => panic!("Expected Begin"),
            }
            match rx.recv_async().await.unwrap() {
                StreamFrame::Data(d) => assert_eq!(*d, b"hello".to_vec()),
                _ => panic!("Expected Data"),
            }
            assert!(rx.recv_async().await.unwrap().is_terminal());
        }
    }

    #[tokio::test]
    async fn test_stream_observer_tap() {
        let registry = StreamRegistry::new();
        let (id, tx) = registry.create_stream(Some(8));

        // Register an observer BEFORE the consumer takes the receiver
        let obs_rx = registry
            .add_observer(id, 8)
            .expect("observer should attach");

        // Consumer takes the receiver — broadcaster should be spawned
        let consumer_rx = registry.take_receiver(id).expect("take should succeed");

        // Send frames
        tx.send(StreamFrame::Begin {
            content_type: Some("image/raw-rgba".into()),
            size_hint: Some(1024),
            metadata: None,
        })
        .unwrap();
        tx.send(StreamFrame::Data(Arc::new(vec![1, 2, 3]))).unwrap();
        tx.send(StreamFrame::End).unwrap();

        // Give the broadcaster task a moment to fan out
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Consumer should receive all frames
        match consumer_rx.recv_async().await.unwrap() {
            StreamFrame::Begin { content_type, .. } => {
                assert_eq!(content_type.as_deref(), Some("image/raw-rgba"));
            }
            _ => panic!("Expected Begin"),
        }
        match consumer_rx.recv_async().await.unwrap() {
            StreamFrame::Data(d) => assert_eq!(*d, vec![1, 2, 3]),
            _ => panic!("Expected Data"),
        }
        assert!(consumer_rx.recv_async().await.unwrap().is_terminal());

        // Observer should also receive the same frames
        match obs_rx.recv_async().await.unwrap() {
            StreamFrame::Begin { content_type, .. } => {
                assert_eq!(content_type.as_deref(), Some("image/raw-rgba"));
            }
            _ => panic!("Expected Begin on observer"),
        }
        match obs_rx.recv_async().await.unwrap() {
            StreamFrame::Data(d) => assert_eq!(*d, vec![1, 2, 3]),
            _ => panic!("Expected Data on observer"),
        }
        assert!(obs_rx.recv_async().await.unwrap().is_terminal());
    }

    #[test]
    fn test_observer_rejected_after_take() {
        let registry = StreamRegistry::new();
        let (id, _tx) = registry.create_stream(Some(4));

        // Take receiver first
        let _rx = registry.take_receiver(id).unwrap();

        // Observer registration should fail (receiver already taken)
        assert!(registry.add_observer(id, 4).is_none());
    }

    #[tokio::test]
    async fn test_actor_to_actor_stream_via_context() {
        use crate::{
            ActorConfig, ActorContext, ActorLoad, ActorState, MemoryState, message::Message,
        };
        use parking_lot::Mutex;

        // --- Producer side ---
        let producer_config = ActorConfig {
            node: crate::types::GraphNode {
                id: "producer".into(),
                component: "ProducerComponent".into(),
                metadata: Some(HashMap::new()),
            },
            ..Default::default()
        };
        let (out_tx, _out_rx) = flume::unbounded();
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let load = Arc::new(ActorLoad::new(0));

        let producer_ctx = ActorContext::new(
            HashMap::new(),
            (out_tx, _out_rx),
            state.clone(),
            producer_config,
            load.clone(),
        );

        // Producer creates a stream
        let (stream_tx, handle) = producer_ctx.create_stream(
            "data_out",
            Some("application/octet-stream".into()),
            Some(300),
            Some(8),
        );

        assert_eq!(handle.origin_actor, "producer");
        assert_eq!(handle.origin_port, "data_out");
        assert_eq!(handle.size_hint, Some(300));

        // --- Consumer side ---
        let consumer_config = ActorConfig {
            node: crate::types::GraphNode {
                id: "consumer".into(),
                component: "ConsumerComponent".into(),
                metadata: Some(HashMap::new()),
            },
            ..Default::default()
        };
        let (con_tx, con_rx) = flume::unbounded();

        // Simulate what a connector does: deliver the StreamHandle via the payload
        let mut payload = HashMap::new();
        payload.insert("data_in".to_string(), Message::stream_handle(handle));

        let consumer_ctx = ActorContext::new(
            payload,
            (con_tx, con_rx),
            state.clone(),
            consumer_config,
            load.clone(),
        );

        // Consumer takes the stream receiver
        let stream_rx = consumer_ctx
            .take_stream_receiver("data_in")
            .expect("Should get stream receiver");

        // --- Simulate streaming data ---
        let chunks: Vec<Vec<u8>> = vec![
            b"chunk-1".to_vec(),
            b"chunk-2".to_vec(),
            b"chunk-3".to_vec(),
        ];

        // Producer sends frames
        stream_tx
            .send(StreamFrame::Begin {
                content_type: Some("application/octet-stream".into()),
                size_hint: Some(300),
                metadata: None,
            })
            .unwrap();

        for chunk in &chunks {
            stream_tx
                .send(StreamFrame::Data(Arc::new(chunk.clone())))
                .unwrap();
        }
        stream_tx.send(StreamFrame::End).unwrap();

        // Consumer reads frames
        match stream_rx.recv().unwrap() {
            StreamFrame::Begin {
                content_type,
                size_hint,
                ..
            } => {
                assert_eq!(content_type.as_deref(), Some("application/octet-stream"));
                assert_eq!(size_hint, Some(300));
            }
            _ => panic!("Expected Begin frame"),
        }

        let mut received = Vec::new();
        loop {
            match stream_rx.recv().unwrap() {
                StreamFrame::Data(d) => received.push(d.to_vec()),
                StreamFrame::End => break,
                other => panic!("Unexpected frame: {:?}", other),
            }
        }

        assert_eq!(received, chunks);

        // Receiver is drained; second take returns None
        assert!(consumer_ctx.take_stream_receiver("data_in").is_none());
    }

    #[tokio::test]
    async fn test_stream_transform() {
        let (in_tx, in_rx) = flume::bounded(8);
        let (out_tx, out_rx) = flume::bounded(8);

        // Spawn the transform (doubles each byte)
        tokio::spawn(async move {
            stream_transform(in_rx, out_tx, |data| {
                data.iter().map(|b| b.wrapping_mul(2)).collect()
            })
            .await;
        });

        // Send frames
        in_tx
            .send_async(StreamFrame::Begin {
                content_type: Some("test".into()),
                size_hint: None,
                metadata: None,
            })
            .await
            .unwrap();
        in_tx
            .send_async(StreamFrame::Data(Arc::new(vec![1, 2, 3])))
            .await
            .unwrap();
        in_tx.send_async(StreamFrame::End).await.unwrap();

        // Verify: Begin passes through unchanged
        match out_rx.recv_async().await.unwrap() {
            StreamFrame::Begin { content_type, .. } => {
                assert_eq!(content_type.as_deref(), Some("test"));
            }
            _ => panic!("Expected Begin"),
        }

        // Data should be doubled
        match out_rx.recv_async().await.unwrap() {
            StreamFrame::Data(d) => assert_eq!(*d, vec![2, 4, 6]),
            _ => panic!("Expected Data"),
        }

        assert!(out_rx.recv_async().await.unwrap().is_terminal());
    }

    #[tokio::test]
    async fn test_stream_collect() {
        let (tx, rx) = flume::bounded(8);

        tx.send_async(StreamFrame::Begin {
            content_type: Some("application/octet-stream".into()),
            size_hint: Some(6),
            metadata: None,
        })
        .await
        .unwrap();
        tx.send_async(StreamFrame::Data(Arc::new(vec![1, 2, 3])))
            .await
            .unwrap();
        tx.send_async(StreamFrame::Data(Arc::new(vec![4, 5, 6])))
            .await
            .unwrap();
        tx.send_async(StreamFrame::End).await.unwrap();

        let (ct, _md, bytes) = stream_collect(rx).await.unwrap();
        assert_eq!(ct.as_deref(), Some("application/octet-stream"));
        assert_eq!(bytes, vec![1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn test_stream_collect_error() {
        let (tx, rx) = flume::bounded(8);

        tx.send_async(StreamFrame::Begin {
            content_type: None,
            size_hint: None,
            metadata: None,
        })
        .await
        .unwrap();
        tx.send_async(StreamFrame::Data(Arc::new(vec![1])))
            .await
            .unwrap();
        tx.send_async(StreamFrame::Error("broken".into()))
            .await
            .unwrap();

        let result = stream_collect(rx).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "broken");
    }

    #[tokio::test]
    async fn test_stream_from_bytes() {
        let (tx, rx) = flume::bounded(16);

        stream_from_bytes(
            tx,
            &[10, 20, 30, 40, 50],
            2, // chunk size
            Some("test/bytes".into()),
            None,
        )
        .await
        .unwrap();

        // Begin
        match rx.recv_async().await.unwrap() {
            StreamFrame::Begin {
                content_type,
                size_hint,
                ..
            } => {
                assert_eq!(content_type.as_deref(), Some("test/bytes"));
                assert_eq!(size_hint, Some(5));
            }
            _ => panic!("Expected Begin"),
        }

        // 3 chunks: [10,20], [30,40], [50]
        match rx.recv_async().await.unwrap() {
            StreamFrame::Data(d) => assert_eq!(*d, vec![10, 20]),
            _ => panic!("Expected Data"),
        }
        match rx.recv_async().await.unwrap() {
            StreamFrame::Data(d) => assert_eq!(*d, vec![30, 40]),
            _ => panic!("Expected Data"),
        }
        match rx.recv_async().await.unwrap() {
            StreamFrame::Data(d) => assert_eq!(*d, vec![50]),
            _ => panic!("Expected Data"),
        }

        assert!(rx.recv_async().await.unwrap().is_terminal());
    }
}
