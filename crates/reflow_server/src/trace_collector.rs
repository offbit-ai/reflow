//! Trace collector — translates engine events into ZIP trace sessions.
//!
//! Uses `zeal-sdk` TracesAPI to submit per-node execution data to Zeal
//! for observability, replay, and debugging.

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use log::{debug, error, info};
use zeal_sdk::traces::{
    CompleteSessionRequest, SessionCompletionStatus, SessionSummary, TracesAPI,
};
use zeal_sdk::types::{
    CreateTraceSessionRequest, TraceData, TraceError, TraceEvent, TraceEventType,
};

use crate::engine::{EngineEvent, EngineEventType};

// ============================================================================
// Trace Session State
// ============================================================================

/// Tracks an active trace session for a single workflow execution.
struct ActiveSession {
    session_id: String,
    #[allow(dead_code)]
    workflow_id: String,
    #[allow(dead_code)]
    execution_id: String,
    start_time: Instant,
    nodes_completed: u32,
    nodes_failed: u32,
    total_data_processed: u64,
    pending_events: Vec<TraceEvent>,
}

// ============================================================================
// Trace Collector
// ============================================================================

/// Collects engine events and submits them as ZIP trace data to Zeal.
///
/// The collector batches events and submits them periodically or on
/// session completion. Each workflow execution maps to one trace session.
pub struct TraceCollector {
    traces_api: tokio::sync::Mutex<TracesAPI>,
    sessions: tokio::sync::Mutex<HashMap<String, ActiveSession>>,
    /// Maximum events to buffer before flushing
    batch_size: usize,
}

impl TraceCollector {
    pub fn new(base_url: &str) -> Self {
        Self {
            traces_api: tokio::sync::Mutex::new(TracesAPI::new(base_url)),
            sessions: tokio::sync::Mutex::new(HashMap::new()),
            batch_size: 50,
        }
    }

    /// Begin a trace session for a workflow execution.
    pub async fn begin_session(&self, workflow_id: &str, execution_id: &str) -> Result<String> {
        let request = CreateTraceSessionRequest {
            workflow_id: workflow_id.to_string(),
            workflow_version_id: None,
            execution_id: execution_id.to_string(),
            metadata: None,
        };

        let response = self.traces_api.lock().await.create_session(request).await?;

        let session = ActiveSession {
            session_id: response.session_id.clone(),
            workflow_id: workflow_id.to_string(),
            execution_id: execution_id.to_string(),
            start_time: Instant::now(),
            nodes_completed: 0,
            nodes_failed: 0,
            total_data_processed: 0,
            pending_events: Vec::new(),
        };

        self.sessions
            .lock()
            .await
            .insert(execution_id.to_string(), session);

        info!(
            "Trace session {} started for execution {}",
            response.session_id, execution_id
        );

        Ok(response.session_id)
    }

    /// Process an engine event, buffering it as a trace event.
    pub async fn process_event(&self, event: &EngineEvent) -> Result<()> {
        let mut sessions = self.sessions.lock().await;
        let session = match sessions.get_mut(&event.execution_id) {
            Some(s) => s,
            None => return Ok(()), // No active session for this execution
        };

        match &event.event_type {
            EngineEventType::NodeExecuting {
                node_id,
                component: _,
            } => {
                session.pending_events.push(TraceEvent {
                    timestamp: event.timestamp as i64,
                    node_id: node_id.clone(),
                    port_id: None,
                    event_type: TraceEventType::Input,
                    data: TraceData {
                        size: 0,
                        data_type: "lifecycle".to_string(),
                        preview: Some(serde_json::json!({"status": "executing"})),
                        full_data: None,
                    },
                    duration: None,
                    metadata: None,
                    error: None,
                });
            }

            EngineEventType::ActorCompleted {
                actor_id,
                duration_ms,
                output_size,
                ..
            } => {
                session.nodes_completed += 1;
                if let Some(size) = output_size {
                    session.total_data_processed += size;
                }

                let preview = if event.data != serde_json::Value::Null {
                    Some(event.data.clone())
                } else {
                    None
                };

                session.pending_events.push(TraceEvent {
                    timestamp: event.timestamp as i64,
                    node_id: actor_id.clone(),
                    port_id: None,
                    event_type: TraceEventType::Output,
                    data: TraceData {
                        size: output_size.unwrap_or(0) as usize,
                        data_type: "application/json".to_string(),
                        preview,
                        full_data: None,
                    },
                    duration: duration_ms.map(std::time::Duration::from_millis),
                    metadata: None,
                    error: None,
                });
            }

            EngineEventType::ActorFailed {
                actor_id, error, ..
            } => {
                session.nodes_failed += 1;
                session.pending_events.push(TraceEvent {
                    timestamp: event.timestamp as i64,
                    node_id: actor_id.clone(),
                    port_id: None,
                    event_type: TraceEventType::Error,
                    data: TraceData {
                        size: 0,
                        data_type: "error".to_string(),
                        preview: None,
                        full_data: None,
                    },
                    duration: None,
                    metadata: None,
                    error: Some(TraceError {
                        message: error.clone(),
                        stack: None,
                        code: None,
                    }),
                });
            }

            EngineEventType::MessageSent {
                from_node,
                from_port,
                to_node,
                to_port,
                size,
            } => {
                session.total_data_processed += *size as u64;
                session.pending_events.push(TraceEvent {
                    timestamp: event.timestamp as i64,
                    node_id: from_node.clone(),
                    port_id: Some(from_port.clone()),
                    event_type: TraceEventType::Output,
                    data: TraceData {
                        size: *size,
                        data_type: "message".to_string(),
                        preview: Some(serde_json::json!({
                            "to_node": to_node,
                            "to_port": to_port,
                        })),
                        full_data: None,
                    },
                    duration: None,
                    metadata: None,
                    error: None,
                });
            }

            // Started, Completed, Failed, NetworkIdle handled at session level
            _ => {}
        }

        // Flush if batch is full
        let should_flush = session.pending_events.len() >= self.batch_size;
        if should_flush {
            let events = std::mem::take(&mut session.pending_events);
            let session_id = session.session_id.clone();
            drop(sessions); // Release lock before network call
            self.flush_events(&session_id, events).await?;
        }

        Ok(())
    }

    /// Complete a trace session (on execution completion or failure).
    pub async fn complete_session(&self, execution_id: &str, success: bool) -> Result<()> {
        let session = self.sessions.lock().await.remove(execution_id);
        let session = match session {
            Some(s) => s,
            None => return Ok(()),
        };

        // Flush remaining events
        if !session.pending_events.is_empty() {
            self.flush_events(&session.session_id, session.pending_events)
                .await?;
        }

        // Complete the session
        let duration = session.start_time.elapsed().as_millis() as u64;
        let status = if success {
            SessionCompletionStatus::Success
        } else {
            SessionCompletionStatus::Error
        };

        let request = CompleteSessionRequest {
            status,
            summary: Some(SessionSummary {
                total_nodes: session.nodes_completed + session.nodes_failed,
                successful_nodes: session.nodes_completed,
                failed_nodes: session.nodes_failed,
                total_duration: duration,
                total_data_processed: session.total_data_processed,
            }),
            error: None,
        };

        self.traces_api
            .lock()
            .await
            .complete_session(&session.session_id, request)
            .await?;

        info!(
            "Trace session {} completed ({}ms, {} ok, {} failed, {} bytes processed)",
            session.session_id,
            duration,
            session.nodes_completed,
            session.nodes_failed,
            session.total_data_processed
        );

        Ok(())
    }

    /// Submit buffered events to Zeal.
    async fn flush_events(&self, session_id: &str, events: Vec<TraceEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }

        let count = events.len();
        let api = self.traces_api.lock().await;
        match api.submit_events(session_id, events).await {
            Ok(_) => {
                debug!("Flushed {} trace events for session {}", count, session_id);
            }
            Err(e) => {
                error!(
                    "Failed to flush trace events for session {}: {}",
                    session_id, e
                );
            }
        }

        Ok(())
    }
}
