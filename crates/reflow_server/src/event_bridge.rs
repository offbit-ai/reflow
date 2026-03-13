//! Event bridge — drains [`EngineEvent`]s and forwards them to consumers.
//!
//! The bridge connects the engine's per-execution event channel to:
//! - [`TraceCollector`] for HTTP trace session submission to Zeal
//! - [`ZipSession`] for real-time WebSocket event emission to Zeal
//!
//! One bridge is spawned per execution. It runs until the channel closes
//! (execution finishes) or the bridge is explicitly dropped.

use std::sync::Arc;

use log::{debug, error, info};

use crate::engine::{EngineEvent, EngineEventType};
use crate::trace_collector::TraceCollector;
use crate::zip_session::ZipSession;

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
}
