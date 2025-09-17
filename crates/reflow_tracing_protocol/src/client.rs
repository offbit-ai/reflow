//! # Reflow Tracing Client
//!
//! A WebSocket client for communicating with the reflow_tracing server.
//! Provides automatic actor tracing, batching, and cross-platform support.

use super::*;
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::{HashMap as StdHashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

#[cfg(not(target_arch = "wasm32"))]
use futures_util::{SinkExt, StreamExt};
#[cfg(not(target_arch = "wasm32"))]
use tokio::net::TcpStream;
#[cfg(not(target_arch = "wasm32"))]
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message as WsMessage,
};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use web_sys::{CloseEvent, ErrorEvent, MessageEvent, WebSocket};

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
    event_buffer: VecDeque<TraceEvent>,
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

#[derive(Debug, Clone, PartialEq)]
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
        let mut config = TracingConfig::default();
        config.server_url = url.into();
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
                        debug!("Received WebSocket message: {}", text);
                        // Handle different response types
                        if let Ok(response) = serde_json::from_str::<TraceResponse>(&text) {
                            let mut state_guard = state_for_receiver.lock().unwrap();
                            
                            // Convert TraceResponse to TracingResponse for compatibility
                            let converted_response = match response {
                                TraceResponse::TraceStored { trace_id } => TracingResponse::TraceStarted { trace_id },
                                TraceResponse::TracesFound { traces } => TracingResponse::QueryResults { traces, total_count: 0 },
                                TraceResponse::TraceData { trace } => TracingResponse::TraceData { trace: Some(trace) },
                                TraceResponse::Error { message } => TracingResponse::Error { message, code: ErrorCode::InternalError },
                                TraceResponse::Metrics { data: _ } => TracingResponse::Pong,
                            };

                            // Notify any pending requests
                            #[cfg(not(target_arch = "wasm32"))]
                            if let Some((_, sender)) = state_guard.pending_requests.drain().next() {
                                let _ = sender.send(converted_response);
                            };
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
                        for event in events_to_send {
                            // Send individual events as TracingRequest::RecordEvent
                            let request = TracingRequest::RecordEvent {
                                trace_id: TraceId::new(),
                                event,
                            };

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
        
        // Send StartTrace request
        let request = TracingRequest::StartTrace { flow_id, version };
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

    /// Record a trace event
    pub async fn record_event(&self, _trace_id: TraceId, event: TraceEvent) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        debug!("Recording trace event: {:?}", event.event_type);

        // Check if we should send immediately
        let should_send_immediately = {
            let mut state = self.state.lock().unwrap();
            state.event_buffer.push_back(event.clone());

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
            let request = TracingRequest::RecordEvent {
                trace_id: TraceId::new(),
                event,
            };
            
            if let Ok(message) = serde_json::to_string(&request) {
                self.send_message(message).await?;
            }
        }

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

    /// Query traces from the server
    pub async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        if !self.config.enabled {
            return Ok(Vec::new());
        }

        let request = TracingRequest::QueryTraces { query };
        let message = serde_json::to_string(&request)?;
        self.send_message(message).await?;

        // For now, return empty - would need proper response handling
        Ok(Vec::new())
    }

    /// Get a specific trace by ID
    pub async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        if !self.config.enabled {
            return Ok(None);
        }

        let request = TracingRequest::GetTrace { trace_id };
        let message = serde_json::to_string(&request)?;
        self.send_message(message).await?;

        // For now, return None - would need proper response handling
        Ok(None)
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
            
            for event in events_to_send {
                let request = TracingRequest::RecordEvent {
                    trace_id: TraceId::new(),
                    event,
                };
                
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
        
        for event in events_to_send {
            let request = TracingRequest::RecordEvent {
                trace_id: TraceId::new(),
                event,
            };
            
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

    /// Record a message sent event
    pub async fn trace_message_sent(
        &self,
        actor_id: impl Into<String>,
        port: impl Into<String>,
        message_type: impl Into<String>,
        size_bytes: usize,
    ) -> Result<()> {
        let event = TraceEvent::message_sent(
            actor_id.into(),
            port.into(),
            message_type.into(),
            size_bytes,
        );
        let trace_id = self.current_trace_id.lock().unwrap().clone();
        if let Some(trace_id) = trace_id {
            self.client.record_event(trace_id, event).await
        } else {
            // Use a default trace ID if none is set
            self.client.record_event(TraceId::new(), event).await
        }
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

    /// Record a data flow event between two actors
    pub async fn trace_data_flow(
        &self,
        from_actor: impl Into<String>,
        from_port: impl Into<String>,
        to_actor: impl Into<String>,
        to_port: impl Into<String>,
        message_type: impl Into<String>,
        size_bytes: usize,
    ) -> Result<()> {
        let event = TraceEvent::data_flow(
            from_actor.into(),
            from_port.into(),
            to_actor.into(),
            to_port.into(),
            message_type.into(),
            size_bytes,
        );

        let trace_id = self.current_trace_id.lock().unwrap().clone();
        if let Some(trace_id) = trace_id {
            self.client.record_event(trace_id, event).await
        } else {
            // Use a default trace ID if none is set
            self.client.record_event(TraceId::new(), event).await
        }
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
        let mut config = TracingConfig::default();
        config.enabled = false; // Disable tracing for test
        let client = TracingClient::new(config);
        let integration = TracingIntegration::new(client);

        // Test starting a trace (should succeed even when disabled)
        let result = integration.start_flow_trace("test_flow").await;
        assert!(result.is_ok());

        // Test recording events (should also succeed when disabled)
        let actor_result = integration.trace_actor_created("test_actor").await;
        assert!(actor_result.is_ok());

        let message_result = integration
            .trace_message_sent("test_actor", "output", "TestMessage", 128)
            .await;
        assert!(message_result.is_ok());
    }
}
