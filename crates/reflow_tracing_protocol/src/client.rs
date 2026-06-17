//! # Reflow Tracing Client
//!
//! A WebSocket client for communicating with the reflow_tracing server.
//! Provides automatic actor tracing, batching, and cross-platform support.

use super::*;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::{HashMap as StdHashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{debug, error, info, warn};
use web_time::Instant;

#[cfg(not(target_arch = "wasm32"))]
use futures_util::{SinkExt, StreamExt};
#[cfg(not(target_arch = "wasm32"))]
use tokio::net::TcpStream;
#[cfg(not(target_arch = "wasm32"))]
use tokio_tungstenite::{
    connect_async, tungstenite::Message as WsMessage, MaybeTlsStream, WebSocketStream,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

/// Route a server reply to the caller waiting on its `request_id`.
/// Replies without a correlation id (notifications, acks) are ignored here.
#[cfg(not(target_arch = "wasm32"))]
fn route_tracing_response(state: &Arc<Mutex<ClientState>>, text: &str) {
    match serde_json::from_str::<TracingResponse>(text) {
        Ok(response) => {
            let request_id = match &response {
                TracingResponse::QueryResults { request_id, .. } => Some(request_id.to_string()),
                TracingResponse::TraceData { request_id, .. } => Some(request_id.to_string()),
                _ => None,
            };
            if let Some(key) = request_id {
                let mut guard = state.lock().unwrap();
                if let Some(sender) = guard.pending_requests.remove(&key) {
                    let _ = sender.send(response);
                }
            }
        }
        Err(_) => debug!("Ignoring unparseable tracing response"),
    }
}

/// Decode a binary WebSocket frame to its JSON text. Frames may be
/// zstd-compressed (the server's default) or raw UTF-8 JSON.
#[cfg(not(target_arch = "wasm32"))]
fn decode_frame(data: &[u8]) -> Option<String> {
    if let Ok(decompressed) = zstd::decode_all(data) {
        if let Ok(text) = String::from_utf8(decompressed) {
            return Some(text);
        }
    }
    String::from_utf8(data.to_vec()).ok()
}

/// Configuration for the tracing client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracingConfig {
    /// WebSocket server URL (e.g., "ws://localhost:8080")
    pub server_url: String,
    /// Maximum number of events to buffer before sending
    pub batch_size: usize,
    /// Maximum time to wait before sending a batch
    pub batch_timeout: Duration,
    /// Whether to enable compression for large payloads
    pub enable_compression: bool,
    /// Connection retry configuration
    pub retry_config: RetryConfig,
    /// Whether tracing is enabled
    pub enabled: bool,
    /// Compute a content checksum for each traced message. Cheap; default on.
    /// See [`crate::checksum`].
    #[serde(default = "default_true")]
    pub capture_checksum: bool,
    /// Retain full message content in `serialized_data`. Heavy and potentially
    /// sensitive; default off. Independent of `capture_checksum`.
    #[serde(default)]
    pub capture_content: bool,
    /// Sample the heavy `PerformanceMetrics` fields (memory/cpu/throughput).
    /// Default off — cheap timing/queue-depth are always measured.
    #[serde(default)]
    pub enable_perf_sampling: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f64,
}

/// Errors that can occur during tracing operations
#[derive(Debug, thiserror::Error)]
pub enum TracingError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("Serialization failed: {0}")]
    SerializationFailed(#[from] serde_json::Error),
    #[error("WebSocket error: {0}")]
    WebSocketError(String),
    #[error("Client is disconnected")]
    Disconnected,
    #[error("Timeout occurred")]
    Timeout,
}

type Result<T> = std::result::Result<T, TracingError>;

/// Internal state for the tracing client
#[derive(Debug)]
struct ClientState {
    connection_status: ConnectionStatus,
    /// Buffered events paired with the trace they belong to, so the trace id
    /// survives the batch path instead of being replaced by a fresh random id.
    event_buffer: VecDeque<(TraceId, TraceEvent)>,
    #[cfg(not(target_arch = "wasm32"))]
    pending_requests: StdHashMap<String, tokio::sync::oneshot::Sender<TracingResponse>>,
    last_batch_time: Instant,
    reconnect_attempts: usize,
    current_trace_id: Option<TraceId>,
    #[cfg(not(target_arch = "wasm32"))]
    message_sender: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    #[cfg(target_arch = "wasm32")]
    websocket: Option<WebSocket>,
}

// `web_sys::WebSocket` (and other wasm-bindgen types) are `!Send`
// because they're handles into the JS heap. The browser is
// single-threaded, so concurrent access through the auto-trait check
// is moot — we only need this marker to satisfy generic bounds in
// downstream async code (e.g. `Box::pin(future) where Future: Send`).
#[cfg(target_arch = "wasm32")]
unsafe impl Send for ClientState {}
#[cfg(target_arch = "wasm32")]
unsafe impl Sync for ClientState {}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum ConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting,
}

/// Main tracing client for communicating with reflow_tracing server
pub struct TracingClient {
    config: TracingConfig,
    state: Arc<Mutex<ClientState>>,
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(dead_code)]
    runtime: Option<tokio::runtime::Handle>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            server_url: "ws://localhost:8080".to_string(),
            batch_size: 1, // Send events immediately by default
            batch_timeout: Duration::from_millis(100), // Shorter timeout
            enable_compression: true,
            retry_config: RetryConfig {
                max_retries: 5,
                initial_delay: Duration::from_millis(1000),
                max_delay: Duration::from_secs(30),
                backoff_multiplier: 2.0,
            },
            enabled: true,
            capture_checksum: true,
            capture_content: false,
            enable_perf_sampling: false,
        }
    }
}

impl TracingClient {
    /// Create a new tracing client with the given configuration
    pub fn new(config: TracingConfig) -> Self {
        let state = Arc::new(Mutex::new(ClientState {
            connection_status: ConnectionStatus::Disconnected,
            event_buffer: VecDeque::new(),
            #[cfg(not(target_arch = "wasm32"))]
            pending_requests: StdHashMap::new(),
            last_batch_time: Instant::now(),
            reconnect_attempts: 0,
            current_trace_id: None,
            #[cfg(not(target_arch = "wasm32"))]
            message_sender: None,
            #[cfg(target_arch = "wasm32")]
            websocket: None,
        }));

        Self {
            config: config.clone(),
            state,
            #[cfg(not(target_arch = "wasm32"))]
            runtime: if config.enabled {
                tokio::runtime::Handle::try_current().ok()
            } else {
                None
            },
        }
    }

    /// Create a new tracing client with default configuration
    pub fn with_default_config() -> Self {
        Self::new(TracingConfig::default())
    }

    /// Create a new tracing client with custom server URL
    pub fn with_server_url(url: impl Into<String>) -> Self {
        let config = TracingConfig {
            server_url: url.into(),
            ..TracingConfig::default()
        };
        Self::new(config)
    }

    /// Connect to the tracing server
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn connect(&self) -> Result<()> {
        if !self.config.enabled {
            debug!("Tracing is disabled, skipping connection");
            return Ok(());
        }

        info!("Connecting to tracing server at {}", self.config.server_url);

        {
            let mut state = self.state.lock().unwrap();
            state.connection_status = ConnectionStatus::Connecting;
        }

        let (ws_stream, _) = connect_async(&self.config.server_url)
            .await
            .map_err(|e| TracingError::ConnectionFailed(e.to_string()))?;

        {
            let mut state = self.state.lock().unwrap();
            state.connection_status = ConnectionStatus::Connected;
            state.reconnect_attempts = 0;
        }

        info!("Successfully connected to tracing server");

        // Start the connection handler in a separate task
        let state_clone = Arc::clone(&self.state);
        let config_clone = self.config.clone();
        tokio::spawn(async move {
            Self::handle_connection_static(state_clone, config_clone, ws_stream).await;
        });

        // Wait a moment for connection to be fully established
        tokio::time::sleep(Duration::from_millis(100)).await;

        Ok(())
    }

    /// Handle the WebSocket connection (static method for spawning)
    #[cfg(not(target_arch = "wasm32"))]
    async fn handle_connection_static(
        state: Arc<Mutex<ClientState>>,
        config: TracingConfig,
        ws_stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    ) {
        let (ws_sender, mut ws_receiver) = ws_stream.split();

        // Create a channel for sending messages
        let (message_tx, mut message_rx) = tokio::sync::mpsc::unbounded_channel();

        // Store the sender in state for use by send_message
        {
            let mut state_guard = state.lock().unwrap();
            state_guard.message_sender = Some(message_tx);
        }

        // Spawn a task to handle outgoing messages
        let mut ws_sender = ws_sender;
        let state_for_sender = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(message) = message_rx.recv().await {
                debug!("Sending WebSocket message: {}", message);
                if let Err(e) = ws_sender.send(WsMessage::Text(message.into())).await {
                    error!("Failed to send message: {}", e);
                    // Mark connection as disconnected
                    let mut state_guard = state_for_sender.lock().unwrap();
                    state_guard.connection_status = ConnectionStatus::Disconnected;
                    state_guard.message_sender = None;
                    break;
                }
            }
        });

        // Spawn a task to handle incoming messages
        let state_for_receiver = Arc::clone(&state);
        tokio::spawn(async move {
            while let Some(msg) = ws_receiver.next().await {
                match msg {
                    Ok(WsMessage::Text(text)) => {
                        // The server replies with `TracingResponse`. Route replies
                        // that carry a `request_id` to their waiting caller.
                        route_tracing_response(&state_for_receiver, &text);
                    }
                    Ok(WsMessage::Binary(data)) => {
                        // Large responses are compressed (zstd by default).
                        if let Some(text) = decode_frame(&data) {
                            route_tracing_response(&state_for_receiver, &text);
                        }
                    }
                    Ok(WsMessage::Close(_)) => {
                        warn!("WebSocket connection closed by server");
                        let mut state_guard = state_for_receiver.lock().unwrap();
                        state_guard.connection_status = ConnectionStatus::Disconnected;
                        state_guard.message_sender = None;
                        break;
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        let mut state_guard = state_for_receiver.lock().unwrap();
                        state_guard.connection_status = ConnectionStatus::Disconnected;
                        state_guard.message_sender = None;
                        break;
                    }
                    _ => {}
                }
            }
        });

        // Handle event batching and sending
        let state_for_batching = Arc::clone(&state);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(config.batch_timeout);

            loop {
                interval.tick().await;

                let (events_to_send, sender_ref) = {
                    let mut state_guard = state_for_batching.lock().unwrap();
                    if state_guard.connection_status != ConnectionStatus::Connected {
                        continue;
                    }

                    let should_send = !state_guard.event_buffer.is_empty()
                        && (state_guard.event_buffer.len() >= config.batch_size
                            || state_guard.last_batch_time.elapsed() >= config.batch_timeout);

                    if should_send {
                        let events = state_guard.event_buffer.drain(..).collect::<Vec<_>>();
                        state_guard.last_batch_time = Instant::now();
                        (events, state_guard.message_sender.clone())
                    } else {
                        (Vec::new(), None)
                    }
                };

                if !events_to_send.is_empty() {
                    info!("Sending batch of {} trace events", events_to_send.len());

                    if let Some(sender) = sender_ref {
                        for (trace_id, event) in events_to_send {
                            // Send individual events as TracingRequest::RecordEvent,
                            // preserving the trace id the event was recorded under.
                            let request = TracingRequest::RecordEvent { trace_id, event };

                            if let Ok(message) = serde_json::to_string(&request) {
                                if let Err(e) = sender.send(message) {
                                    error!("Failed to send trace event: {}", e);
                                    let mut state_guard = state_for_batching.lock().unwrap();
                                    state_guard.connection_status = ConnectionStatus::Disconnected;
                                    state_guard.message_sender = None;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// Connect to the tracing server (WASM version)
    #[cfg(target_arch = "wasm32")]
    pub async fn connect(&self) -> Result<()> {
        if !self.config.enabled {
            debug!("Tracing is disabled, skipping connection");
            return Ok(());
        }

        info!("Connecting to tracing server at {}", self.config.server_url);

        let ws = WebSocket::new(&self.config.server_url).map_err(|_| {
            TracingError::ConnectionFailed("Failed to create WebSocket".to_string())
        })?;

        // Set up event handlers
        let state_clone = Arc::clone(&self.state);
        let onopen = Closure::wrap(Box::new(move || {
            let mut state = state_clone.lock().unwrap();
            state.connection_status = ConnectionStatus::Connected;
            state.reconnect_attempts = 0;
            info!("WebSocket connection opened");
        }) as Box<dyn FnMut()>);
        ws.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        onopen.forget();

        let state_clone = Arc::clone(&self.state);
        let onclose = Closure::wrap(Box::new(move |_: CloseEvent| {
            let mut state = state_clone.lock().unwrap();
            state.connection_status = ConnectionStatus::Disconnected;
            warn!("WebSocket connection closed");
        }) as Box<dyn FnMut(CloseEvent)>);
        ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
        onclose.forget();

        let onerror = Closure::wrap(Box::new(move |_: ErrorEvent| {
            error!("WebSocket error occurred");
        }) as Box<dyn FnMut(ErrorEvent)>);
        ws.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onerror.forget();

        // Store websocket reference
        {
            let mut state = self.state.lock().unwrap();
            state.websocket = Some(ws);
            state.connection_status = ConnectionStatus::Connected;
        }

        Ok(())
    }

    /// Start a new trace for a flow
    pub async fn start_trace(&self, flow_id: FlowId, version: FlowVersion) -> Result<TraceId> {
        if !self.config.enabled {
            return Ok(TraceId::new());
        }

        let trace_id = TraceId::new();

        // Send StartTrace request. The client owns the trace id; the server
        // creates/stores the trace under this exact id.
        let request = TracingRequest::StartTrace {
            trace_id: trace_id.clone(),
            flow_id,
            version,
        };
        let message = serde_json::to_string(&request)?;

        // Send the message immediately
        self.send_message(message).await?;

        // Store current trace ID
        {
            let mut state = self.state.lock().unwrap();
            state.current_trace_id = Some(trace_id.clone());
        }

        Ok(trace_id)
    }

    /// Record a trace event under the given trace.
    pub async fn record_event(&self, trace_id: TraceId, event: TraceEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!("Recording trace event: {:?}", event.event_type);

        // Check if we should send immediately
        let should_send_immediately = {
            let mut state = self.state.lock().unwrap();
            state
                .event_buffer
                .push_back((trace_id.clone(), event.clone()));

            // Check if we should send immediately
            if self.config.batch_size == 1 {
                // Pop the event for immediate sending
                state.event_buffer.pop_front().is_some()
            } else if state.event_buffer.len() >= self.config.batch_size {
                debug!("Event buffer full, will be sent by batching task");
                false
            } else {
                false
            }
        }; // Lock is dropped here

        // Send immediately if needed (lock is already dropped)
        if should_send_immediately {
            let request = TracingRequest::RecordEvent { trace_id, event };

            if let Ok(message) = serde_json::to_string(&request) {
                self.send_message(message).await?;
            }
        }

        Ok(())
    }

    /// Finalize a trace, marking its terminal status on the server.
    pub async fn end_trace(&self, trace_id: TraceId, status: ExecutionStatus) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let request = TracingRequest::EndTrace { trace_id, status };
        let message = serde_json::to_string(&request)?;
        self.send_message(message).await?;
        Ok(())
    }

    /// Send a ping to the server
    pub async fn ping(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let request = TracingRequest::Ping;
        let message = serde_json::to_string(&request)?;
        self.send_message(message).await?;
        Ok(())
    }

    /// Query traces from the server.
    pub async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let request_id = RequestId::new();
            let rx = self.register_request(&request_id);

            let request = TracingRequest::QueryTraces {
                request_id: request_id.clone(),
                query,
            };
            self.send_message(serde_json::to_string(&request)?).await?;

            match self.await_response(&request_id, rx).await? {
                TracingResponse::QueryResults { traces, .. } => Ok(traces),
                TracingResponse::Error { message, .. } => {
                    Err(TracingError::WebSocketError(message))
                }
                _ => Ok(Vec::new()),
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = query;
            Ok(Vec::new())
        }
    }

    /// Get a specific trace by ID.
    pub async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        if !self.config.enabled {
            return Ok(None);
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let request_id = RequestId::new();
            let rx = self.register_request(&request_id);

            let request = TracingRequest::GetTrace {
                request_id: request_id.clone(),
                trace_id,
            };
            self.send_message(serde_json::to_string(&request)?).await?;

            match self.await_response(&request_id, rx).await? {
                TracingResponse::TraceData { trace, .. } => Ok(trace),
                TracingResponse::Error { message, .. } => {
                    Err(TracingError::WebSocketError(message))
                }
                _ => Ok(None),
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let _ = trace_id;
            Ok(None)
        }
    }

    /// Register a pending request and return the receiver to await its reply.
    #[cfg(not(target_arch = "wasm32"))]
    fn register_request(
        &self,
        request_id: &RequestId,
    ) -> tokio::sync::oneshot::Receiver<TracingResponse> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let mut state = self.state.lock().unwrap();
        state.pending_requests.insert(request_id.to_string(), tx);
        rx
    }

    /// Await a correlated reply, cleaning up the pending entry on timeout.
    #[cfg(not(target_arch = "wasm32"))]
    async fn await_response(
        &self,
        request_id: &RequestId,
        rx: tokio::sync::oneshot::Receiver<TracingResponse>,
    ) -> Result<TracingResponse> {
        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => Err(TracingError::Disconnected),
            Err(_) => {
                let mut state = self.state.lock().unwrap();
                state.pending_requests.remove(&request_id.to_string());
                Err(TracingError::Timeout)
            }
        }
    }

    /// Subscribe to real-time trace events
    pub async fn subscribe(&self, filters: SubscriptionFilters) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let request = TracingRequest::Subscribe { filters };
        let message = serde_json::to_string(&request)?;
        self.send_message(message).await?;
        Ok(())
    }

    /// Check if the client is connected
    pub fn is_connected(&self) -> bool {
        let state = self.state.lock().unwrap();
        state.connection_status == ConnectionStatus::Connected
    }

    /// Whether to compute a content checksum for each traced message (cheap).
    pub fn capture_checksum(&self) -> bool {
        self.config.capture_checksum
    }

    /// Whether to retain full message content in snapshots (heavy, opt-in).
    pub fn capture_content(&self) -> bool {
        self.config.capture_content
    }

    /// Whether the heavy `PerformanceMetrics` fields should be sampled.
    pub fn enable_perf_sampling(&self) -> bool {
        self.config.enable_perf_sampling
    }

    /// Get the current connection status
    pub fn connection_status(&self) -> String {
        let state = self.state.lock().unwrap();
        format!("{:?}", state.connection_status)
    }

    /// Shutdown the tracing client and flush pending events
    pub async fn shutdown(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        info!("Shutting down tracing client...");

        // Flush any remaining events in the buffer
        let events_to_send = {
            let mut state = self.state.lock().unwrap();
            state.connection_status = ConnectionStatus::Disconnected;
            state.event_buffer.drain(..).collect::<Vec<_>>()
        };

        if !events_to_send.is_empty() {
            info!("Flushing {} pending trace events", events_to_send.len());

            for (trace_id, event) in events_to_send {
                let request = TracingRequest::RecordEvent { trace_id, event };

                if let Ok(message) = serde_json::to_string(&request) {
                    let _ = self.send_message(message).await;
                }
            }
        }

        info!("Tracing client shutdown complete");
        Ok(())
    }

    /// Force flush any pending events
    pub async fn flush(&self) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        let events_to_send = {
            let mut state = self.state.lock().unwrap();
            if state.event_buffer.is_empty() {
                return Ok(());
            }

            let events = state.event_buffer.drain(..).collect::<Vec<_>>();
            state.last_batch_time = Instant::now();
            events
        };

        info!("Flushing {} trace events", events_to_send.len());

        for (trace_id, event) in events_to_send {
            let request = TracingRequest::RecordEvent { trace_id, event };

            if let Ok(message) = serde_json::to_string(&request) {
                self.send_message(message).await?;
            }
        }

        Ok(())
    }

    /// Send a message through the WebSocket connection
    async fn send_message(&self, message: String) -> Result<()> {
        debug!("Attempting to send message: {}", message);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let sender_ref = {
                let state = self.state.lock().unwrap();
                if state.connection_status != ConnectionStatus::Connected {
                    return Err(TracingError::Disconnected);
                }
                state.message_sender.clone()
            };

            if let Some(sender) = sender_ref {
                sender
                    .send(message.clone())
                    .map_err(|e| TracingError::WebSocketError(e.to_string()))?;
                debug!("Message queued for sending: {}", message);
            } else {
                return Err(TracingError::Disconnected);
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            let websocket = {
                let state = self.state.lock().unwrap();
                if state.connection_status != ConnectionStatus::Connected {
                    return Err(TracingError::Disconnected);
                }
                state.websocket.clone()
            };

            if let Some(ws) = websocket {
                ws.send_with_str(&message).map_err(|_| {
                    TracingError::WebSocketError("Failed to send message".to_string())
                })?;
                debug!("Message sent via WebSocket: {}", message);
            } else {
                return Err(TracingError::Disconnected);
            }
        }

        Ok(())
    }
}

/// High-level API for tracing integration with actor systems
#[derive(Clone)]
pub struct TracingIntegration {
    client: Arc<TracingClient>,
    current_trace_id: Arc<Mutex<Option<TraceId>>>,
}

unsafe impl Send for TracingIntegration {}
unsafe impl Sync for TracingIntegration {}

impl TracingIntegration {
    /// Create a new tracing integration
    pub fn new(client: TracingClient) -> Self {
        Self {
            client: Arc::new(client),
            current_trace_id: Arc::new(Mutex::new(None)),
        }
    }

    /// Start tracing for a flow
    pub async fn start_flow_trace(&self, flow_id: impl Into<String>) -> Result<TraceId> {
        let flow_id = FlowId::new(flow_id);
        let version = FlowVersion {
            major: 1,
            minor: 0,
            patch: 0,
            git_hash: None,
            timestamp: chrono::Utc::now(),
        };

        let trace_id = self.client.start_trace(flow_id, version).await?;
        {
            let mut current = self.current_trace_id.lock().unwrap();
            *current = Some(trace_id.clone());
        }
        Ok(trace_id)
    }

    /// Record an actor creation event
    pub async fn trace_actor_created(&self, actor_id: impl Into<String>) -> Result<()> {
        let event = TraceEvent::actor_created(actor_id.into());
        let trace_id = self.current_trace_id.lock().unwrap().clone();
        if let Some(trace_id) = trace_id {
            self.client.record_event(trace_id, event).await
        } else {
            // Use a default trace ID if none is set
            self.client.record_event(TraceId::new(), event).await
        }
    }

    /// Record an actor process completion event
    pub async fn trace_actor_completed(&self, actor_id: impl Into<String>) -> Result<()> {
        let event = TraceEvent::actor_completed(actor_id.into());
        let trace_id = self.current_trace_id.lock().unwrap().clone();
        if let Some(trace_id) = trace_id {
            self.client.record_event(trace_id, event).await
        } else {
            // Use a default trace ID if none is set
            self.client.record_event(TraceId::new(), event).await
        }
    }

    /// Record a message-sent event, capturing a snapshot of `content` per the
    /// configured `capture_checksum` / `capture_content` knobs.
    pub async fn trace_message_sent<T: Serialize>(
        &self,
        actor_id: impl Into<String>,
        port: impl Into<String>,
        message_type: impl Into<String>,
        content: &T,
        metrics: PerformanceMetrics,
    ) -> Result<()> {
        let snapshot = self.snapshot(message_type, content);
        let event = TraceEvent::message_sent(actor_id.into(), port.into(), snapshot, metrics);
        self.record(event).await
    }

    /// Record an actor failure event
    pub async fn trace_actor_failed(
        &self,
        actor_id: impl Into<String>,
        error: impl Into<String>,
    ) -> Result<()> {
        let event = TraceEvent::actor_failed(actor_id.into(), error.into());
        let trace_id = self.current_trace_id.lock().unwrap().clone();
        if let Some(trace_id) = trace_id {
            self.client.record_event(trace_id, event).await
        } else {
            // Use a default trace ID if none is set
            self.client.record_event(TraceId::new(), event).await
        }
    }

    /// Record a data-flow event between two actors, capturing a snapshot of
    /// `content` per the configured capture knobs.
    pub async fn trace_data_flow<T: Serialize>(
        &self,
        from_actor: impl Into<String>,
        from_port: impl Into<String>,
        to_actor: impl Into<String>,
        to_port: impl Into<String>,
        message_type: impl Into<String>,
        content: &T,
        metrics: PerformanceMetrics,
    ) -> Result<()> {
        let snapshot = self.snapshot(message_type, content);
        let event = TraceEvent::data_flow(
            from_actor.into(),
            from_port.into(),
            to_actor.into(),
            to_port.into(),
            snapshot,
            metrics,
        );
        self.record(event).await
    }

    /// Build a message snapshot honoring the client's capture configuration.
    fn snapshot<T: Serialize>(
        &self,
        message_type: impl Into<String>,
        content: &T,
    ) -> MessageSnapshot {
        MessageSnapshot::capture(
            message_type,
            content,
            self.client.capture_checksum(),
            self.client.capture_content(),
        )
    }

    /// Record an event under the active flow trace (or a fresh id if none).
    async fn record(&self, event: TraceEvent) -> Result<()> {
        let trace_id = self.current_trace_id.lock().unwrap().clone();
        match trace_id {
            Some(trace_id) => self.client.record_event(trace_id, event).await,
            None => self.client.record_event(TraceId::new(), event).await,
        }
    }

    /// Finalize the current flow trace with a terminal status.
    pub async fn end_flow_trace(&self, status: ExecutionStatus) -> Result<()> {
        let trace_id = self.current_trace_id.lock().unwrap().clone();
        if let Some(trace_id) = trace_id {
            self.client.end_trace(trace_id, status).await
        } else {
            Ok(())
        }
    }

    /// The trace id of the active flow, if one has been started. Used by the
    /// distributed router to propagate trace context across process boundaries.
    pub fn current_trace_id(&self) -> Option<TraceId> {
        self.current_trace_id.lock().unwrap().clone()
    }

    /// Get access to the underlying client
    pub fn client(&self) -> Arc<TracingClient> {
        Arc::clone(&self.client)
    }
}

/// Global tracing client instance
static GLOBAL_CLIENT: std::sync::OnceLock<TracingIntegration> = std::sync::OnceLock::new();

/// Initialize the global tracing client
pub fn init_global_tracing(config: TracingConfig) -> Result<()> {
    let client = TracingClient::new(config);
    let integration = TracingIntegration::new(client);

    GLOBAL_CLIENT.set(integration).map_err(|_| {
        TracingError::WebSocketError("Global tracing client already initialized".to_string())
    })?;

    Ok(())
}

/// Get the global tracing client
pub fn global_tracing() -> Option<&'static TracingIntegration> {
    GLOBAL_CLIENT.get()
}

/// Convenience macro for tracing actor events
#[macro_export]
macro_rules! trace_actor_event {
    (created, $actor_id:expr) => {
        if let Some(tracing) = $crate::tracing::global_tracing() {
            let _ = tracing.trace_actor_created($actor_id).await;
        }
    };
    (message_sent, $actor_id:expr, $port:expr, $msg_type:expr, $size:expr) => {
        if let Some(tracing) = $crate::tracing::global_tracing() {
            let _ = tracing
                .trace_message_sent($actor_id, $port, $msg_type, $size)
                .await;
        }
    };
    (failed, $actor_id:expr, $error:expr) => {
        if let Some(tracing) = $crate::tracing::global_tracing() {
            let _ = tracing.trace_actor_failed($actor_id, $error).await;
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = TracingConfig::default();
        assert_eq!(config.server_url, "ws://localhost:8080");
        assert_eq!(config.batch_size, 1); // Updated to match new default
        assert!(config.enabled);
    }

    #[test]
    fn test_client_creation() {
        let client = TracingClient::with_default_config();
        assert!(!client.is_connected());
    }

    #[tokio::test]
    async fn test_tracing_integration() {
        let config = TracingConfig {
            enabled: false,
            ..Default::default()
        }; // Disable tracing for test

        let client = TracingClient::new(config);
        let integration = TracingIntegration::new(client);

        // Test starting a trace (should succeed even when disabled)
        let result = integration.start_flow_trace("test_flow").await;
        assert!(result.is_ok());

        // Test recording events (should also succeed when disabled)
        let actor_result = integration.trace_actor_created("test_actor").await;
        assert!(actor_result.is_ok());

        let message_result = integration
            .trace_message_sent(
                "test_actor",
                "output",
                "TestMessage",
                &serde_json::json!({ "k": 1 }),
                PerformanceMetrics::default(),
            )
            .await;
        assert!(message_result.is_ok());
    }
}
