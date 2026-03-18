//! Event bridge — drains [`EngineEvent`]s and forwards them to consumers.
//!
//! The bridge connects the engine's per-execution event channel to:
//! - [`TraceCollector`] for HTTP trace session submission to Zeal
//! - [`ZipSession`] for real-time WebSocket event emission to Zeal
//!
//! One bridge is spawned per execution. It runs until the channel closes
//! (execution finishes) or the bridge is explicitly dropped.

use std::sync::Arc;

use log::{debug, error, info, warn};
use reflow_network::stream::STREAM_REGISTRY;

use crate::engine::{EngineEvent, EngineEventType};
use crate::trace_collector::TraceCollector;
use crate::zip_session::ZipSession;

/// Buffer size for observer taps attached to display-forwarded streams.
/// Uses `try_send` so dropping frames is acceptable for display.
const OBSERVER_BUFFER_SIZE: usize = 32;

// ============================================================================
// Event Bridge
// ============================================================================

/// Shared observability consumers that live for the lifetime of the server.
/// Cloned into each per-execution bridge task.
pub struct EventBridge {
    trace_collector: Option<Arc<TraceCollector>>,
    zip_session: Option<Arc<ZipSession>>,
}

impl EventBridge {
    pub fn new(
        trace_collector: Option<Arc<TraceCollector>>,
        zip_session: Option<Arc<ZipSession>>,
    ) -> Self {
        Self {
            trace_collector,
            zip_session,
        }
    }

    /// Spawn a background task that drains `event_rx` for a single execution,
    /// forwarding events to the TraceCollector and ZipSession.
    ///
    /// The task exits when the channel closes (sender dropped).
    pub fn attach(
        &self,
        workflow_id: String,
        execution_id: String,
        event_rx: flume::Receiver<EngineEvent>,
    ) {
        let trace_collector = self.trace_collector.clone();
        let zip_session = self.zip_session.clone();

        tokio::spawn(async move {
            info!(
                "[EventBridge] attached to execution {} (workflow {})",
                execution_id, workflow_id
            );

            // Begin trace session if collector is available
            if let Some(tc) = &trace_collector
                && let Err(e) = tc.begin_session(&workflow_id, &execution_id).await
            {
                error!(
                    "[EventBridge] failed to begin trace session for {}: {}",
                    execution_id, e
                );
            }

            let mut final_success = true;

            while let Ok(event) = event_rx.recv_async().await {
                // Forward to trace collector
                if let Some(tc) = &trace_collector
                    && let Err(e) = tc.process_event(&event).await
                {
                    debug!(
                        "[EventBridge] trace collector error for {}: {}",
                        execution_id, e
                    );
                }

                // Forward to ZIP session (real-time WebSocket)
                if let Some(zs) = &zip_session
                    && let Err(e) = zs.emit_engine_event(&event).await
                {
                    debug!(
                        "[EventBridge] zip session error for {}: {}",
                        execution_id, e
                    );
                }

                // When a stream opens, attach an observer and spawn a
                // forwarding task that pipes frames to Zeal via ZipSession.
                if let EngineEventType::StreamOpened {
                    stream_id, node_id, ..
                } = &event.event_type
                {
                    if let Some(zs) = &zip_session {
                        let sid = *stream_id;
                        let nid = node_id.clone();
                        let wid = workflow_id.clone();
                        let eid = execution_id.clone();
                        let zs = Arc::clone(zs);

                        if let Some(obs_rx) =
                            STREAM_REGISTRY.add_observer(sid, OBSERVER_BUFFER_SIZE)
                        {
                            tokio::spawn(async move {
                                Self::forward_stream_to_zeal(obs_rx, sid, nid, wid, eid, zs).await;
                            });
                        } else {
                            warn!(
                                "[EventBridge] could not attach observer for stream {} \
                                 (receiver already taken or stream gone)",
                                stream_id
                            );
                        }
                    }
                }

                // Forward output metadata to Zeal for display components.
                // When an actor completes with metadata (e.g. thumbnail),
                // push it to Zeal so the display component can render it.
                if let EngineEventType::ActorCompleted {
                    actor_id,
                    output_metadata: Some(metadata),
                    ..
                } = &event.event_type
                {
                    if let Some(zs) = &zip_session {
                        let wid = workflow_id.clone();
                        let nid = actor_id.clone();
                        let meta = metadata.clone();
                        let zs = Arc::clone(zs);
                        tokio::spawn(async move {
                            if let Err(e) = zs.update_node_properties(&wid, &nid, meta).await {
                                debug!(
                                    "[EventBridge] failed to update node display for {}: {}",
                                    nid, e
                                );
                            }
                        });
                    }
                }

                // Track terminal state
                match &event.event_type {
                    EngineEventType::Failed { .. } => {
                        final_success = false;
                    }
                    EngineEventType::Completed { nodes_failed, .. } => {
                        if *nodes_failed > 0 {
                            final_success = false;
                        }
                    }
                    _ => {}
                }
            }

            // Complete trace session
            if let Some(tc) = &trace_collector
                && let Err(e) = tc.complete_session(&execution_id, final_success).await
            {
                error!(
                    "[EventBridge] failed to complete trace session for {}: {}",
                    execution_id, e
                );
            }

            info!(
                "[EventBridge] detached from execution {} (success={})",
                execution_id, final_success
            );
        });
    }

    /// Drain an observer receiver and forward each frame to Zeal as binary
    /// WebSocket messages. Emits `StreamClosed` / `StreamError` engine events
    /// when the stream terminates.
    async fn forward_stream_to_zeal(
        obs_rx: flume::Receiver<reflow_network::stream::StreamFrame>,
        stream_id: u64,
        node_id: String,
        workflow_id: String,
        execution_id: String,
        zip_session: Arc<ZipSession>,
    ) {
        use reflow_network::stream::StreamFrame;

        let mut total_bytes: u64 = 0;

        loop {
            match obs_rx.recv_async().await {
                Ok(frame) => {
                    let is_terminal = frame.is_terminal();

                    if let StreamFrame::Data(ref data) = frame {
                        total_bytes += data.len() as u64;
                    }

                    if let Err(e) = zip_session.send_stream_frame(stream_id, &frame).await {
                        debug!(
                            "[EventBridge] stream {} binary send failed: {}",
                            stream_id, e
                        );
                    }

                    if is_terminal {
                        // Emit the appropriate engine event via raw JSON
                        match frame {
                            StreamFrame::End => {
                                let raw = serde_json::json!({
                                    "type": "stream.closed",
                                    "workflowId": workflow_id,
                                    "executionId": execution_id,
                                    "nodeId": node_id,
                                    "streamId": stream_id,
                                    "totalBytes": total_bytes,
                                });
                                let _ = zip_session.ws_send_raw(&raw).await;
                            }
                            StreamFrame::Error(ref msg) => {
                                let raw = serde_json::json!({
                                    "type": "stream.error",
                                    "workflowId": workflow_id,
                                    "executionId": execution_id,
                                    "nodeId": node_id,
                                    "streamId": stream_id,
                                    "error": msg,
                                });
                                let _ = zip_session.ws_send_raw(&raw).await;
                            }
                            _ => {}
                        }
                        break;
                    }
                }
                Err(_) => {
                    // Channel closed — producer gone
                    break;
                }
            }
        }

        debug!(
            "[EventBridge] stream {} forwarding done ({} bytes)",
            stream_id, total_bytes
        );
    }
}
