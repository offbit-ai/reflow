use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio::sync::{RwLock, broadcast};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;
use uuid::Uuid;

/// Errors that can occur when using the PyExec client
#[derive(Error, Debug)]
pub enum PyExecError {
    #[error("WebSocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("Package installation error: {0}")]
    PackageInstall(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("RPC error (code: {code}): {message}")]
    Rpc { code: i32, message: String },
}

// Request models matching server expectations
#[derive(Debug, Serialize)]
struct RpcMessage {
    id: String,
    method: String,
    params: JsonValue,
}

// Response models matching server responses
#[derive(Debug, Deserialize, Clone)]
pub struct RpcResponse {
    pub id: String,
    #[serde(default)]
    pub result: Option<RpcResult>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum RpcResult {
    ExecutionResult {
        stdout: String,
        stderr: String,
        #[serde(default)]
        result: Option<JsonValue>,
        success: bool,
        execution_time_ms: u64,
        #[serde(default)]
        packages_installed: Vec<String>,
    },
    PackageInstallResult {
        success: bool,
        message: String,
        #[serde(default)]
        packages_installed: Vec<String>,
    },
    Pong {
        message: String,
    },
}

#[derive(Debug, Deserialize, Clone)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Deserialize)]
struct ClientMessage {
    session_id: String,
    message: JsonValue,
}

/// Represents a message received from the Python execution service
#[derive(Debug, Clone)]
pub enum PyExecMessage {
    /// A response to an RPC request
    Response(RpcResponse),
    /// A real-time message from a Python script
    ScriptMessage(JsonValue),
    /// A message about a connection event
    ConnectionEvent(String),
    /// A message about connection state changes
    ConnectionState(ConnectionState),
    /// A message about an error
    Error(String),
}

/// Connection state for the WebSocket
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    /// Initial state, not connected
    Disconnected,
    /// Attempting to connect
    Connecting,
    /// Connected and ready to use
    Connected,
    /// Connection lost, attempting to reconnect
    Reconnecting,
    /// Connection closed by client request
    Closed,
}

/// Input parameter for Python script execution
#[derive(Debug, Clone, Serialize)]
pub struct InputParameter {
    pub name: String,
    pub data: JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Options for script execution
#[derive(Debug, Clone)]
pub struct ExecutionOptions {
    /// Python packages to install before execution
    pub packages: Option<Vec<String>>,
    /// Timeout in seconds for script execution
    pub timeout: Option<u64>,
    /// Input parameters to pass to the script
    pub inputs: Option<Vec<InputParameter>>,
}

impl Default for ExecutionOptions {
    fn default() -> Self {
        Self {
            packages: None,
            timeout: Some(30), // Default 30-second timeout
            inputs: None,
        }
    }
}

/// Results from Python script execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Standard output from the script
    pub stdout: String,
    /// Standard error from the script
    pub stderr: String,
    /// Result value from the script (if any)
    pub result: Option<JsonValue>,
    /// Whether the script executed successfully
    pub success: bool,
    /// Execution time in milliseconds
    pub execution_time_ms: u64,
    /// Packages installed for this execution
    pub packages_installed: Vec<String>,
}

/// Results from package installation
#[derive(Debug, Clone)]
pub struct PackageInstallResult {
    /// Whether installation was successful
    pub success: bool,
    /// Message about the installation
    pub message: String,
    /// Packages that were installed
    pub packages_installed: Vec<String>,
}

/// Progress update for package installation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInstallProgress {
    /// Status of the installation ("start", "progress", "success", "error", "complete")
    pub status: String,
    /// Message about the installation
    pub message: String,
    /// Package being installed (if applicable)
    pub package: Option<String>,
}

impl Into<JsonValue> for PackageInstallProgress {
    fn into(self) -> JsonValue {
        serde_json::to_value(self).unwrap_or_default()
    }
}

// Connection manager type alias
type WSStream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type WSSender = SplitSink<WSStream, Message>;

/// Internal state for the PyExec client
#[derive(Debug)]
struct ClientState {
    /// WebSocket sender
    ws_sender: Option<WSSender>,
    /// Connection state
    connection_state: ConnectionState,
    /// Pending responses map (id -> oneshot sender)
    pending_responses: HashMap<String, (flume::Sender<PyExecMessage>, flume::Receiver<PyExecMessage>)>,
    /// Reconnection attempts
    reconnect_attempts: u32,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            ws_sender: None,
            connection_state: ConnectionState::Disconnected,
            pending_responses: HashMap::new(),
            reconnect_attempts: 0,
        }
    }
}
#[derive(Clone, Debug)]
pub struct ClientConfig {
    /// Maximum number of reconnection attempts (0 = infinite)
    pub max_reconnect_attempts: u32,
    /// Base delay between reconnection attempts in milliseconds
    pub reconnect_base_delay_ms: u64,
    /// Maximum delay between reconnection attempts in milliseconds
    pub reconnect_max_delay_ms: u64,
    /// Heartbeat interval in milliseconds (0 = disabled)
    pub heartbeat_interval_ms: u64,
    /// Heartbeat timeout in milliseconds
    pub heartbeat_timeout_ms: u64,
    /// Initial connection timeout in seconds
    pub connection_timeout_seconds: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            max_reconnect_attempts: 0,      // Infinite reconnection attempts
            reconnect_base_delay_ms: 1000,  // Start with 1 second
            reconnect_max_delay_ms: 30000,  // Maximum 30 seconds
            heartbeat_interval_ms: 30000,   // 30 seconds
            heartbeat_timeout_ms: 10000,    // 10 seconds
            connection_timeout_seconds: 30, // 30 seconds for initial connection
        }
    }
}

/// The main client for the Python execution service with persistent connection
#[derive(Clone)]
pub struct PyExecClient {
    /// Server URL
    server_url: String,
    /// Client state
    state: Arc<RwLock<ClientState>>,
    /// Channel for receiving global messages
    message_rx: flume::Receiver<PyExecMessage>,
    /// Channel for sending global messages
    message_tx: flume::Sender<PyExecMessage>,
    /// Channel for broadcasting connection state
    state_tx: broadcast::Sender<ConnectionState>,
    /// Configuration
    config: ClientConfig,
    /// Flag to track if shutdown has been requested
    shutdown_requested: Arc<RwLock<bool>>,
}

impl PyExecClient {
    pub async fn create_without_waiting(server_url: &str, config: Option<ClientConfig>) -> Self {
        let config = config.unwrap_or_default();
        let (message_tx, message_rx) = flume::unbounded();
        let (state_tx, _) = broadcast::channel(100);

        let state = Arc::new(RwLock::new(ClientState::default()));
        let shutdown_requested = Arc::new(RwLock::new(false));

        let client = PyExecClient {
            server_url: server_url.to_string(),
            state,
            message_rx,
            message_tx: message_tx.clone(),
            state_tx: state_tx.clone(),
            config,
            shutdown_requested,
        };

        // Clone client for the connection manager
        let mut manager_client = client.clone();

        let server_url = server_url.to_string();
        // Spawn the connection manager task
        tokio::spawn(async move {
            info!("Connection manager started for {}", server_url);
            manager_client.connection_manager().await;
        });

        client
    }

    // Method to wait for connection with optional timeout
    pub async fn wait_for_connection(
        &self,
        timeout_seconds: Option<u64>,
    ) -> Result<(), PyExecError> {
        // If already connected, return immediately
        if self.connection_state().await == ConnectionState::Connected {
            return Ok(());
        }

        let mut state_rx = self.state_tx.subscribe();
        let timeout_fut = async {
            while let Ok(state) = state_rx.recv().await {
                if state == ConnectionState::Connected {
                    return Ok(());
                } else if state == ConnectionState::Closed {
                    return Err(PyExecError::Connection(
                        "Connection closed before established".to_string(),
                    ));
                }
            }
            Err(PyExecError::Connection(
                "Subscription channel closed".to_string(),
            ))
        };

        if let Some(timeout) = timeout_seconds {
            match tokio::time::timeout(Duration::from_secs(timeout), timeout_fut).await {
                Ok(result) => result,
                Err(_) => Err(PyExecError::Timeout(format!(
                    "Timed out waiting for connection after {} seconds",
                    timeout
                ))),
            }
        } else {
            // No timeout, just wait
            timeout_fut.await
        }
    }

    /// Connect to a Python execution service
    ///
    /// # Arguments
    ///
    /// * `server_url` - The WebSocket URL of the service (e.g., "ws://127.0.0.1:8080")
    /// * `config` - Optional configuration for the client
    ///
    /// # Returns
    ///
    /// A PyExecClient instance or an error
    pub async fn connect(
        server_url: &str,
        config: Option<ClientConfig>,
    ) -> Result<Self, PyExecError> {
        let config = config.unwrap_or_default();
        let (message_tx, message_rx) = flume::unbounded();
        let (state_tx, _) = broadcast::channel(100);

        let state = Arc::new(RwLock::new(ClientState::default()));
        let shutdown_requested = Arc::new(RwLock::new(false));

        let client = PyExecClient {
            server_url: server_url.to_string(),
            state,
            message_rx,
            message_tx: message_tx.clone(),
            state_tx: state_tx.clone(),
            config,
            shutdown_requested,
        };

        // Clone client for the connection manager
        let mut manager_client = client.clone();

        // Spawn the connection manager task
        tokio::spawn(async move {
            manager_client.connection_manager().await;
        });

        // Wait for the connection to be established with configured timeout
        let mut state_rx = client.state_tx.subscribe();
        let timeout = tokio::time::timeout(
            Duration::from_secs(client.config.connection_timeout_seconds),
            async {
                while let Ok(state) = state_rx.recv().await {
                    if state == ConnectionState::Connected {
                        return Ok(());
                    } else if state == ConnectionState::Closed {
                        return Err(PyExecError::Connection(
                            "Connection closed before established".to_string(),
                        ));
                    }
                }
                Err(PyExecError::Connection(
                    "Failed to establish connection".to_string(),
                ))
            },
        )
        .await;

        match timeout {
            Ok(result) => {
                result?;
                info!("Successfully connected to server at {}", server_url);
                Ok(client)
            }
            Err(_) => {
                // Don't abort the connection attempts, just return a timeout error
                warn!(
                    "Initial connection timed out after {} seconds, continuing reconnection attempts in background",
                    client.config.connection_timeout_seconds
                );

                Err(PyExecError::Timeout(format!(
                    "Initial connection timed out after {} seconds. The client will continue trying to connect in the background.",
                    client.config.connection_timeout_seconds
                )))
            }
        }
    }

    /// Get the current connection state
    pub async fn connection_state(&self) -> ConnectionState {
        self.state.read().await.connection_state.clone()
    }

    /// Subscribe to connection state changes
    pub fn subscribe_to_state_changes(&self) -> broadcast::Receiver<ConnectionState> {
        self.state_tx.subscribe()
    }

    /// Connection manager task
    async fn connection_manager(&mut self) {
        while !*self.shutdown_requested.read().await {
            let conn_state = self.state.read().await.connection_state.clone();
            // println!("Connection state: {:?}", conn_state);
            match conn_state {
                ConnectionState::Disconnected | ConnectionState::Reconnecting => {
                    self.attempt_connection().await;
                }
                ConnectionState::Connected => {
                    // We're connected, wait for state change or heartbeat interval
                    if self.config.heartbeat_interval_ms > 0 {
                        tokio::select! {
                            _ = sleep(Duration::from_millis(self.config.heartbeat_interval_ms)) => {
                                self.send_heartbeat().await;
                            }
                            _ = sleep(Duration::from_millis(100)) => {
                                // Just a short sleep to avoid busy waiting
                            }
                        }
                    } else {
                        sleep(Duration::from_millis(100)).await;
                    }
                }
                ConnectionState::Closed => {
                    // Connection was explicitly closed, exit the manager
                    break;
                }
                ConnectionState::Connecting => {
                    // Wait for the connection attempt to complete
                    sleep(Duration::from_millis(100)).await;
                }
            }
        }

        // Final cleanup
        let mut state = self.state.write().await;
        state.connection_state = ConnectionState::Closed;
        state.ws_sender = None;
        state.pending_responses.clear();

        info!("Connection manager shut down");
    }

    /// Attempt to connect to the server
    async fn attempt_connection(&self) {
        {
            let mut state = self.state.write().await;
            state.connection_state = ConnectionState::Connecting;
            self.state_tx
                .send(ConnectionState::Connecting)
                .unwrap_or_default();
        }

        info!("Attempting to connect to {}", self.server_url);

        match self.establish_connection().await {
            Ok((ws_sender, ws_receiver)) => {
                // Store the sender and update connection state
                {
                    let mut state = self.state.write().await;
                    state.ws_sender = Some(ws_sender);
                    state.connection_state = ConnectionState::Connected;
                    state.reconnect_attempts = 0;
                }

                // Broadcast the connected state
                self.state_tx
                    .send(ConnectionState::Connected)
                    .unwrap_or_default();
                self.message_tx
                    .send(PyExecMessage::ConnectionState(ConnectionState::Connected))
                    .unwrap_or_default();

                info!("Connected to {}", self.server_url);

                // Spawn a task to handle incoming messages
                let client_clone = self.clone();
                tokio::spawn(async move {
                    client_clone.handle_incoming_messages(ws_receiver).await;
                });
            }
            Err(e) => {
                let reconnect_delay = self.calculate_reconnect_delay().await;

                {
                    let mut state = self.state.write().await;
                    state.connection_state = ConnectionState::Reconnecting;
                    state.reconnect_attempts += 1;
                }

                self.state_tx
                    .send(ConnectionState::Reconnecting)
                    .unwrap_or_default();
                self.message_tx
                    .send(PyExecMessage::ConnectionState(
                        ConnectionState::Reconnecting,
                    ))
                    .unwrap_or_default();
                self.message_tx
                    .send(PyExecMessage::Error(format!("Connection failed: {}", e)))
                    .unwrap_or_default();

                error!(
                    "Connection failed: {}. Reconnecting in {}ms",
                    e, reconnect_delay
                );

                // Sleep before attempting to reconnect
                sleep(Duration::from_millis(reconnect_delay)).await;
            }
        }
    }

    /// Establish a WebSocket connection to the server
    async fn establish_connection(&self) -> Result<(WSSender, SplitStream<WSStream>), PyExecError> {
        info!("Attempting to establish WebSocket connection to {}", self.server_url);
        let url = Url::parse(&self.server_url)?;
        info!("Parsed URL: {}", url);
    
        // Set up a timeout for the connection attempt
        let connect_timeout = Duration::from_secs(10); // 10-second timeout for actual connection
        
        info!("Starting connection with {} second timeout", connect_timeout.as_secs());
        let connection_result = tokio::time::timeout(connect_timeout, async {
            match connect_async(url.as_str().into_client_request().unwrap()).await {
                Ok(result) => {
                    info!("WebSocket connection successful");
                    Ok(result)
                },
                Err(e) => {
                    error!("WebSocket connection failed: {}", e);
                    Err(PyExecError::Connection(format!("Failed to connect: {}", e)))
                }
            }
        }).await;
    
        match connection_result {
            Ok(Ok((ws_stream, _))) => {
                info!("Successfully created WebSocket stream");
                let (ws_sender, ws_receiver) = ws_stream.split();
                Ok((ws_sender, ws_receiver))
            },
            Ok(Err(e)) => {
                error!("Connection error: {}", e);
                Err(e)
            },
            Err(_) => {
                error!("Connection attempt timed out after {:?}", connect_timeout);
                Err(PyExecError::Timeout(format!("Connection attempt timed out after {:?}", connect_timeout)))
            },
        }
    }

    /// Calculate the delay before the next reconnection attempt
    async fn calculate_reconnect_delay(&self) -> u64 {
        let attempts = self.state.read().await.reconnect_attempts;

        // Check if we've exceeded the maximum number of attempts
        if self.config.max_reconnect_attempts > 0 && attempts >= self.config.max_reconnect_attempts
        {
            return self.config.reconnect_max_delay_ms;
        }

        // Exponential backoff with jitter
        let base_delay = self.config.reconnect_base_delay_ms;
        let max_delay = self.config.reconnect_max_delay_ms;

        // Calculate exponential backoff
        let exp_backoff = base_delay * (2_u64.pow(attempts as u32).min(10)); // Cap at 2^10 to avoid overflow

        // Add some jitter (±20%)
        let jitter_factor = 0.8 + (rand::random::<f64>() * 0.4);
        let delay_with_jitter = (exp_backoff as f64 * jitter_factor) as u64;

        // Cap at max delay
        delay_with_jitter.min(max_delay)
    }

    /// Handle incoming WebSocket messages
    async fn handle_incoming_messages(&self, mut ws_receiver: SplitStream<WSStream>) {
        while !*self.shutdown_requested.read().await {
            match ws_receiver.next().await {
                Some(Ok(Message::Text(text))) => {
                    self.process_message(text.to_string()).await;
                }
                Some(Ok(Message::Close(reason))) => {
                    let reason_str = reason.map_or("Connection closed".to_string(), |r| {
                        format!("Connection closed: {} ({})", r.reason, r.code)
                    });

                    self.message_tx
                        .send(PyExecMessage::ConnectionEvent(reason_str.clone()))
                        .unwrap_or_default();

                    // Don't attempt to reconnect if shutdown was requested
                    if !*self.shutdown_requested.read().await {
                        warn!("{}", reason_str);
                        self.handle_connection_loss().await;
                    }

                    break;
                }
                Some(Ok(Message::Ping(data))) => {
                    // Automatically respond to ping messages
                    if let Some(ref mut sender) = self.state.write().await.ws_sender {
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                }
                Some(Err(e)) => {
                    error!("WebSocket error: {}", e);
                    self.message_tx
                        .send(PyExecMessage::Error(format!("WebSocket error: {}", e)))
                        .unwrap_or_default();

                    // Don't attempt to reconnect if shutdown was requested
                    if !*self.shutdown_requested.read().await {
                        self.handle_connection_loss().await;
                    }

                    break;
                }
                None => {
                    // Connection closed
                    warn!("WebSocket connection closed unexpectedly");

                    // Don't attempt to reconnect if shutdown was requested
                    if !*self.shutdown_requested.read().await {
                        self.handle_connection_loss().await;
                    }

                    break;
                }
                _ => {}
            }
        }
    }

    /// Process a received message
    async fn process_message(&self, text: String) {
        // Try to parse as a normal RPC response
        if let Ok(response) = serde_json::from_str::<RpcResponse>(&text) {
            let id = response.id.clone();
            let message = PyExecMessage::Response(response);

            // Check if we have a pending response for this ID
            let sender = {
                let mut state = self.state.write().await;
                state.pending_responses.remove(&id)
            };

            if let Some((sender, _)) = sender {
                // Send the response to the waiting task
                let _ = sender.send(message.clone());
            }

            // Also broadcast the message
            self.message_tx.send(message).unwrap_or_default();
        }
        // Try to parse as a client message (for real-time updates)
        else if let Ok(client_msg) = serde_json::from_str::<ClientMessage>(&text) {
            let message = PyExecMessage::ScriptMessage(client_msg.message);
            self.message_tx.send(message).unwrap_or_default();
        } else {
            let message = PyExecMessage::Error(format!("Unrecognized message: {}", text));
            self.message_tx.send(message).unwrap_or_default();
        }
    }

    /// Handle connection loss
    async fn handle_connection_loss(&self) {
        {
            let mut state = self.state.write().await;
            state.connection_state = ConnectionState::Reconnecting;
            state.ws_sender = None;

            // Fail all pending responses
            for (_, (sender, _)) in state.pending_responses.drain() {
                let _ = sender.send(PyExecMessage::Error("Connection lost".to_string()).into());
            }
        }

        self.state_tx
            .send(ConnectionState::Reconnecting)
            .unwrap_or_default();
        self.message_tx
            .send(PyExecMessage::ConnectionState(
                ConnectionState::Reconnecting,
            ))
            .unwrap_or_default();
    }

    /// Send a heartbeat to check the connection
    async fn send_heartbeat(&self) {
        // Only send heartbeat if we're connected
        if self.connection_state().await != ConnectionState::Connected {
            return;
        }

        debug!("Sending heartbeat ping");

        match self
            .ping_with_timeout(self.config.heartbeat_timeout_ms)
            .await
        {
            Ok(_) => {
                debug!("Heartbeat ping successful");
            }
            Err(e) => {
                warn!("Heartbeat ping failed: {}", e);
                self.handle_connection_loss().await;
            }
        }
    }

    /// Ping with timeout
    async fn ping_with_timeout(&self, timeout_ms: u64) -> Result<String, PyExecError> {
        let timeout = Duration::from_millis(timeout_ms);

        tokio::time::timeout(timeout, self.ping())
            .await
            .map_err(|_| PyExecError::Timeout("Heartbeat ping timed out".to_string()))?
    }

    /// Send an RPC message to the server
    async fn send_rpc(&self, method: &str, params: JsonValue) -> Result<String, PyExecError> {
        // Wait for connection if necessary
        if self.connection_state().await != ConnectionState::Connected {
            // For pings, we'll fail immediately if not connected since they may be part of reconnection logic
            if method == "ping" {
                return Err(PyExecError::Connection("Not connected to server".to_string()));
            }
            
            // For other methods, attempt to wait for connection
            match self.wait_for_connection(Some(10)).await {
                Ok(_) => {
                    // We're connected now, continue
                },
                Err(e) => {
                    return Err(PyExecError::Connection(format!("Failed to connect to server: {}", e)));
                }
            }
        }
    
        // Rest of the original send_rpc implementation
        let id = Uuid::new_v4().to_string();
        let request = RpcMessage {
            id: id.clone(),
            method: method.to_string(),
            params,
        };
    
        let json = serde_json::to_string(&request)?;
    
        // Register the response channel before sending the request
        let (sender, rx) = flume::unbounded();
        {
            let mut state = self.state.write().await;
            state.pending_responses.insert(id.clone(), (sender, rx));
        }
    
        // Send the request
        {
            let mut state = self.state.write().await;
            if let Some(ref mut sender) = state.ws_sender {
                sender.send(Message::Text(json.into())).await?;
            } else {
                return Err(PyExecError::Connection("WebSocket sender not available".to_string()));
            }
        }
    
        Ok(id)
    }

    /// Wait for a response with the specified ID
    async fn wait_for_response(
        &self,
        id: &str,
        timeout_ms: Option<u64>,
    ) -> Result<PyExecMessage, PyExecError> {
        let timeout = timeout_ms
            .map(Duration::from_millis)
            .unwrap_or(Duration::from_secs(60));
    
        info!("Waiting for response with ID: {} (timeout: {:?})", id, timeout);
    
        // Get the receiver directly, to avoid state lock issues
        let receiver = {
            let state = self.state.read().await;
            match state.pending_responses.get(id) {
                Some((_, rx)) => rx.clone(),
                None => return Err(PyExecError::Connection(format!("No pending response for ID {}", id)))
            }
        };
    
        // Wait for the response with timeout
        match tokio::time::timeout(timeout, receiver.recv_async()).await {
            Ok(Ok(message)) => {
                info!("Received response for ID: {}", id);
                
                // Check if the response indicates an error
                match &message {
                    PyExecMessage::Response(response) => {
                        if let Some(error) = &response.error {
                            return Err(PyExecError::Rpc {
                                code: error.code,
                                message: error.message.clone(),
                            });
                        }
                    },
                    PyExecMessage::Error(error_msg) => {
                        return Err(PyExecError::Connection(error_msg.clone()));
                    },
                    _ => {}
                }
    
                Ok(message)
            },
            Ok(Err(e)) => {
                error!("Response channel error for ID {}: {}", id, e);
                Err(PyExecError::Connection(format!("Response channel error: {}", e)))
            },
            Err(_) => {
                // Remove the pending response on timeout
                {
                    let mut state = self.state.write().await;
                    state.pending_responses.remove(id);
                    info!("Removed pending response for ID {} due to timeout", id);
                }
    
                Err(PyExecError::Timeout(format!(
                    "Response timed out after {}ms",
                    timeout.as_millis()
                )))
            }
        }
    }
    /// Install Python packages
    ///
    /// # Arguments
    ///
    /// * `packages` - Vector of package names to install
    /// * `progress_callback` - Optional callback for installation progress updates
    ///
    /// # Returns
    ///
    /// A PackageInstallResult or an error
    pub async fn install_packages<F>(
        &self,
        packages: Vec<String>,
        progress_callback: Option<F>,
    ) -> Result<PackageInstallResult, PyExecError>
    where
        F: FnMut(JsonValue) + Send + 'static,
    {
        if packages.is_empty() {
            return Ok(PackageInstallResult {
                success: true,
                message: "No packages to install".to_string(),
                packages_installed: Vec::new(),
            });
        }

        let id = self
            .send_rpc(
                "install_packages",
                serde_json::json!({
                    "packages": packages,
                }),
            )
            .await?;

        // If we have a progress callback, set up a listener for progress messages
        let message_rx = if progress_callback.is_some() {
            let mut callback = progress_callback;
            let message_rx = self.message_rx.clone();

            // Clone the message receiver for the progress listener
            let message_rx_clone = message_rx.clone();

            let progress_task = tokio::spawn(async move {
                while let Ok(message) = message_rx_clone.recv_async().await {
                    if let PyExecMessage::ScriptMessage(json) = &message {
                        if let Some(msg_type) = json.get("type").and_then(|t| t.as_str()) {
                            if msg_type == "package_install" {
                                if let Some(ref mut cb) = callback {
                                    cb(json.clone());
                                }
                            }
                        }
                    }
                }
            });

            // Store the task handle to allow cancellation later
            message_rx
        } else {
            self.message_rx.clone()
        };

        // Wait for the response with an appropriate timeout
        match self.wait_for_response(&id, Some(300000)).await {
            // 5 minute timeout for package installation
            Ok(PyExecMessage::Response(response)) => match response.result {
                Some(RpcResult::PackageInstallResult {
                    success,
                    message,
                    packages_installed,
                }) => {
                    if success {
                        Ok(PackageInstallResult {
                            success,
                            message,
                            packages_installed,
                        })
                    } else {
                        Err(PyExecError::PackageInstall(message))
                    }
                }
                _ => Err(PyExecError::PackageInstall(
                    "Unexpected response type".to_string(),
                )),
            },
            Ok(_) => Err(PyExecError::PackageInstall(
                "Unexpected message type".to_string(),
            )),
            Err(e) => Err(e),
        }
    }

    /// Execute a Python script
    ///
    /// # Arguments
    ///
    /// * `code` - The Python code to execute
    /// * `options` - Optional execution options
    /// * `message_callback` - Optional callback for real-time messages from the script
    ///
    /// # Returns
    ///
    /// An ExecutionResult or an error
    pub async fn execute<F>(
        &self,
        code: &str,
        options: Option<ExecutionOptions>,
        message_callback: Option<F>,
    ) -> Result<ExecutionResult, PyExecError>
    where
        F: FnMut(JsonValue) + Send + 'static,
    {
        let options = options.unwrap_or_default();

        // Install packages if requested (unless we're passing them directly to execute_script)
        if let Some(packages) = &options.packages {
            if !packages.is_empty() && !code.is_empty() {
                // We'll pass the packages directly to execute_script
            }
        }

        // Prepare inputs for the script
        let inputs = options.inputs.map(|inputs| {
            inputs
                .into_iter()
                .map(|input| {
                    serde_json::json!({
                        "name": input.name,
                        "data": input.data,
                        "description": input.description,
                    })
                })
                .collect::<Vec<_>>()
        });

        // Send the execution request
        let id = self
            .send_rpc(
                "execute_script",
                serde_json::json!({
                    "code": code,
                    "requirements": options.packages,
                    "timeout_seconds": options.timeout,
                    "inputs": inputs,
                }),
            )
            .await?;

        // If we have a message callback, set up a listener for real-time messages
        let message_rx = if message_callback.is_some() {
            let mut callback = message_callback;
            let message_rx = self.message_rx.clone();

            // Clone the message receiver for the message listener
            let message_rx_clone = message_rx.clone();

            let message_task = tokio::spawn(async move {
                while let Ok(message) = message_rx_clone.recv_async().await {
                    if let PyExecMessage::ScriptMessage(json) = &message {
                        if let Some(ref mut cb) = callback {
                            cb(json.clone());
                        }
                    }
                }
            });

            // Store the task handle to allow cancellation later
            message_rx
        } else {
            self.message_rx.clone()
        };

        // Wait for the response with an appropriate timeout
        let timeout_ms = options.timeout.map(|t| t * 1000 + 5000).unwrap_or(65000); // Add 5 seconds overhead for communication

        match self.wait_for_response(&id, Some(timeout_ms)).await {
            Ok(PyExecMessage::Response(response)) => match response.result {
                Some(RpcResult::ExecutionResult {
                    stdout,
                    stderr,
                    result,
                    success,
                    execution_time_ms,
                    packages_installed,
                }) => Ok(ExecutionResult {
                    stdout,
                    stderr,
                    result,
                    success,
                    execution_time_ms,
                    packages_installed,
                }),
                _ => Err(PyExecError::Execution(
                    "Unexpected response type".to_string(),
                )),
            },
            Ok(_) => Err(PyExecError::Execution(
                "Unexpected message type".to_string(),
            )),
            Err(e) => Err(e),
        }
    }

    /// Execute a Python script from a file
    ///
    /// # Arguments
    ///
    /// * `file_path` - Path to the Python file
    /// * `options` - Optional execution options
    /// * `message_callback` - Optional callback for real-time messages from the script
    ///
    /// # Returns
    ///
    /// An ExecutionResult or an error
    pub async fn execute_file<P, F>(
        &self,
        file_path: P,
        options: Option<ExecutionOptions>,
        message_callback: Option<F>,
    ) -> Result<ExecutionResult, PyExecError>
    where
        P: AsRef<Path>,
        F: FnMut(JsonValue) + Send + 'static,
    {
        // Read the file
        let mut file = tokio::fs::File::open(file_path).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;

        // Execute the code
        self.execute(&contents, options, message_callback).await
    }

    /// Ping the server to check if it's alive
    ///
    /// # Returns
    ///
    /// A string response from the server or an error
    pub async fn ping(&self) -> Result<String, PyExecError> {
        let id = self.send_rpc("ping", serde_json::json!({})).await?;

        match self.wait_for_response(&id, Some(5000)).await {
            Ok(PyExecMessage::Response(response)) => match response.result {
                Some(RpcResult::Pong { message }) => Ok(message),
                _ => Err(PyExecError::Connection(
                    "Unexpected response type".to_string(),
                )),
            },
            Ok(_) => Err(PyExecError::Connection(
                "Unexpected message type".to_string(),
            )),
            Err(e) => Err(e),
        }
    }

    /// Close the connection to the server explicitly
    ///
    /// This will stop the automatic reconnection behavior and shut down the client.
    pub async fn close(&self) -> Result<(), PyExecError> {
        // Mark as shutdown requested
        {
            let mut shutdown = self.shutdown_requested.write().await;
            *shutdown = true;
        }

        // Update connection state
        {
            let mut state = self.state.write().await;
            state.connection_state = ConnectionState::Closed;

            // Close the WebSocket if we have one
            if let Some(mut sender) = state.ws_sender.take() {
                let _ = sender.close().await;
            }

            // Fail all pending responses
            for (_, (sender, _)) in state.pending_responses.drain() {
                let _ = sender.send(PyExecMessage::ConnectionEvent(
                    "Connection closed by client".to_string(),
                ));
            }
        }

        // Broadcast the closed state
        self.state_tx
            .send(ConnectionState::Closed)
            .unwrap_or_default();
        self.message_tx
            .send(PyExecMessage::ConnectionState(ConnectionState::Closed))
            .unwrap_or_default();

        Ok(())
    }
}
