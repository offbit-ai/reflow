
// WebSocket server that exposes actor network to clients

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use reflow_network::message::Message;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};
use uuid::Uuid;
use reflow_network::network::{Network, NetworkConfig};


// ============================================================================
// SERVER CONFIGURATION
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub bind_address: String,
    pub max_connections: usize,
    pub heartbeat_interval_ms: u64,
    pub message_timeout_ms: u64,
    pub max_message_size_bytes: usize,
    pub compression_enabled: bool,
    pub rate_limit_requests_per_minute: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind_address: "0.0.0.0:8080".to_string(),
            max_connections: 10000,
            heartbeat_interval_ms: 30000,
            message_timeout_ms: 60000,
            max_message_size_bytes: 10 * 1024 * 1024, // 10MB
            compression_enabled: true,
            rate_limit_requests_per_minute: 1000,
        }
    }
}

// ============================================================================
// CLIENT SESSION MANAGEMENT
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: Option<String>,
    pub role: String,
    pub permissions: Vec<String>,
    pub domain: Option<String>,
}

#[derive(Debug)]
pub struct ClientSession {
    pub session_id: String,
    pub user_context: UserContext,
    pub connected_at: Instant,
    pub last_activity: Instant,
    pub sender: mpsc::UnboundedSender<ServerMessage>,
    pub subscriptions: RwLock<HashMap<String, SubscriptionConfig>>,
    pub rate_limiter: RateLimiter,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionConfig {
    pub actor_id: String,
    pub message_types: Vec<String>,
    pub filter_expression: Option<String>,
}

#[derive(Debug)]
pub struct RateLimiter {
    requests: Arc<Mutex<Vec<Instant>>>,
    max_requests: usize,
    window_duration: Duration,
}

impl RateLimiter {
    pub fn new(max_requests: usize, window_minutes: u64) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            max_requests,
            window_duration: Duration::from_secs(window_minutes * 60),
        }
    }
    
    pub fn check_rate_limit(&self) -> bool {
        let now = Instant::now();
        let mut requests = self.requests.lock().unwrap();
        
        // Remove old requests outside the window
        requests.retain(|&time| now.duration_since(time) <= self.window_duration);
        
        if requests.len() >= self.max_requests {
            false
        } else {
            requests.push(now);
            true
        }
    }
}

// ============================================================================
// MESSAGE PROTOCOL
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    // Connection lifecycle
    Connected {
        session_id: String,
        server_capabilities: ServerCapabilities,
        timestamp: u64,
    },
    
    Disconnected {
        reason: String,
        timestamp: u64,
    },
    
    // Actor execution
    ActorResult {
        request_id: String,
        actor_id: String,
        result: ActorExecutionResult,
        execution_time_ms: u64,
        timestamp: u64,
    },
    
    ActorError {
        request_id: String,
        actor_id: String,
        error: String,
        error_code: String,
        timestamp: u64,
    },
    
    // Real-time updates
    ActorStateUpdate {
        actor_id: String,
        state_diff: StateDiff,
        version: u64,
        timestamp: u64,
    },
    
    NetworkStatusUpdate {
        status: NetworkStatus,
        active_actors: Vec<String>,
        message_queue_size: usize,
        timestamp: u64,
    },
    
    // Streaming data
    StreamData {
        stream_id: String,
        batch_id: u64,
        data: Vec<u8>,
        metadata: StreamMetadata,
        is_final: bool,
        timestamp: u64,
    },
    
    StreamError {
        stream_id: String,
        error: String,
        timestamp: u64,
    },
    
    // System messages
    Heartbeat {
        timestamp: u64,
    },
    
    RateLimitExceeded {
        retry_after_seconds: u64,
        timestamp: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    // Authentication and session setup
    Connect {
        user_context: UserContext,
        capabilities_requested: Vec<String>,
    },
    
    // Actor execution requests
    ExecuteActor {
        request_id: String,
        actor_id: String,
        payload: serde_json::Value,
        timeout_ms: Option<u64>,
        priority: MessagePriority,
    },
    
    // Subscription management
    Subscribe {
        subscription_id: String,
        config: SubscriptionConfig,
    },
    
    Unsubscribe {
        subscription_id: String,
    },
    
    // Streaming operations
    StartStream {
        stream_id: String,
        config: StreamConfig,
    },
    
    StopStream {
        stream_id: String,
    },
    
    // System messages
    Heartbeat {
        timestamp: u64,
    },
    
    GetNetworkStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePriority {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerCapabilities {
    pub supported_message_types: Vec<String>,
    pub max_concurrent_actors: usize,
    pub streaming_enabled: bool,
    pub ai_agents_available: Vec<String>,
    pub sql_dialects_supported: Vec<String>,
}

// ============================================================================
// MAIN SERVER IMPLEMENTATION
// ============================================================================

pub struct ZflowServer {
    network: Arc<Network>,
    clients: Arc<DashMap<String, Arc<ClientSession>>>,
    config: ServerConfig,
    message_router: Arc<MessageRouter>,
    metrics: Arc<ServerMetrics>,
}

impl ZflowServer {
    pub async fn new(network: Arc<Network>, config: ServerConfig) -> Result<Self> {
        Ok(Self {
            network,
            clients: Arc::new(DashMap::new()),
            config,
            message_router: Arc::new(MessageRouter::new()),
            metrics: Arc::new(ServerMetrics::new()),
        })
    }
    
    pub async fn start(&self) -> Result<()> {
        let addr: SocketAddr = self.config.bind_address.parse()?;
        let listener = TcpListener::bind(addr).await?;
        
        println!("🚀 zflow WebSocket server listening on: {}", addr);
        println!("📊 Max connections: {}", self.config.max_connections);
        println!("🔧 Actor network ready with {} registered actors", self.network.actor_count());
        
        // Start background tasks
        self.start_heartbeat_task().await;
        self.start_metrics_task().await;
        self.start_cleanup_task().await;
        
        // Accept connections
        while let Ok((stream, remote_addr)) = listener.accept().await {
            if self.clients.len() >= self.config.max_connections {
                println!("⚠️ Connection limit reached, rejecting connection from {}", remote_addr);
                continue;
            }
            
            let clients = self.clients.clone();
            let network = self.network.clone();
            let router = self.message_router.clone();
            let config = self.config.clone();
            let metrics = self.metrics.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::handle_client_connection(
                    stream, remote_addr, clients, network, router, config, metrics
                ).await {
                    eprintln!("❌ Client connection error from {}: {}", remote_addr, e);
                }
            });
        }
        
        Ok(())
    }
    
    async fn handle_client_connection(
        stream: TcpStream,
        remote_addr: SocketAddr,
        clients: Arc<DashMap<String, Arc<ClientSession>>>,
        network: Arc<Network>,
        router: Arc<MessageRouter>,
        config: ServerConfig,
        metrics: Arc<ServerMetrics>,
    ) -> Result<()> {
        println!("🔗 New connection from: {}", remote_addr);
        
        let ws_stream = accept_async(stream).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        let session_id = Uuid::new_v4().to_string();
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        // Send initial connection message
        let connect_message = ServerMessage::Connected {
            session_id: session_id.clone(),
            server_capabilities: ServerCapabilities {
                supported_message_types: vec![
                    "ExecuteActor".to_string(),
                    "Subscribe".to_string(),
                    "StartStream".to_string(),
                ],
                max_concurrent_actors: 100,
                streaming_enabled: true,
                ai_agents_available: vec![
                    "SchemaIntelligence".to_string(),
                    "CodeGenerator".to_string(),
                    "MigrationPlanner".to_string(),
                ],
                sql_dialects_supported: vec![
                    "postgresql".to_string(),
                    "mysql".to_string(),
                    "snowflake".to_string(),
                    "bigquery".to_string(),
                ],
            },
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };
        
        let _ = tx.send(connect_message);
        
        // Handle outgoing messages to client
        let session_id_clone = session_id.clone();
        let clients_clone = clients.clone();
        let metrics_clone = metrics.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let ws_message = WsMessage::Text(
                    serde_json::to_string(&message).unwrap_or_else(|_| "{}".to_string())
                );
                
                if ws_sender.send(ws_message).await.is_err() {
                    break;
                }
                
                metrics_clone.increment_messages_sent();
            }
            
            // Clean up session on disconnect
            clients_clone.remove(&session_id_clone);
            println!("🔌 Client disconnected: {}", session_id_clone);
        });
        
        // Initialize session (will be updated on Connect message)
        let session = Arc::new(ClientSession {
            session_id: session_id.clone(),
            user_context: UserContext {
                user_id: None,
                role: "anonymous".to_string(),
                permissions: vec![],
                domain: None,
            },
            connected_at: Instant::now(),
            last_activity: Instant::now(),
            sender: tx,
            subscriptions: RwLock::new(HashMap::new()),
            rate_limiter: RateLimiter::new(config.rate_limit_requests_per_minute, 1),
        });
        
        clients.insert(session_id.clone(), session.clone());
        
        // Handle incoming messages from client
        while let Some(msg) = ws_receiver.next().await {
            match msg? {
                WsMessage::Text(text) => {
                    metrics.increment_messages_received();
                    
                    // Check rate limit
                    if !session.rate_limiter.check_rate_limit() {
                        let rate_limit_msg = ServerMessage::RateLimitExceeded {
                            retry_after_seconds: 60,
                            timestamp: chrono::Utc::now().timestamp_millis() as u64,
                        };
                        let _ = session.sender.send(rate_limit_msg);
                        continue;
                    }
                    
                    // Update last activity
                    {
                        let mut session_guard = session.clone();
                        let session_mut = Arc::get_mut(&mut session_guard).unwrap();
                        session_mut.last_activity = Instant::now();
                    }
                    
                    // Parse and route message
                    match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(client_message) => {
                            if let Err(e) = router.route_client_message(
                                client_message,
                                &session,
                                &network,
                                &clients,
                            ).await {
                                eprintln!("Message routing error: {}", e);
                                
                                let error_msg = ServerMessage::ActorError {
                                    request_id: "unknown".to_string(),
                                    actor_id: "system".to_string(),
                                    error: e.to_string(),
                                    error_code: "ROUTING_ERROR".to_string(),
                                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                                };
                                let _ = session.sender.send(error_msg);
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to parse client message: {}", e);
                            
                            let error_msg = ServerMessage::ActorError {
                                request_id: "unknown".to_string(),
                                actor_id: "system".to_string(),
                                error: format!("Invalid message format: {}", e),
                                error_code: "PARSE_ERROR".to_string(),
                                timestamp: chrono::Utc::now().timestamp_millis() as u64,
                            };
                            let _ = session.sender.send(error_msg);
                        }
                    }
                }
                
                WsMessage::Binary(data) => {
                    // Handle binary data (Arrow batches, file uploads, etc.)
                    if let Err(e) = router.handle_binary_data(data, &session, &network).await {
                        eprintln!("Binary data handling error: {}", e);
                    }
                }
                
                WsMessage::Ping(data) => {
                    let _ = session.sender.send(ServerMessage::Heartbeat {
                        timestamp: chrono::Utc::now().timestamp_millis() as u64,
                    });
                }
                
                WsMessage::Close(_) => {
                    println!("👋 Client requested connection close: {}", session_id);
                    break;
                }
                
                _ => {
                    // Handle other message types as needed
                }
            }
        }
        
        Ok(())
    }
    
    async fn start_heartbeat_task(&self) {
        let clients = self.clients.clone();
        let interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            
            loop {
                ticker.tick().await;
                
                let heartbeat_msg = ServerMessage::Heartbeat {
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                };
                
                // Send heartbeat to all connected clients
                for session in clients.iter() {
                    let _ = session.sender.send(heartbeat_msg.clone());
                }
            }
        });
    }
    
    async fn start_metrics_task(&self) {
        let metrics = self.metrics.clone();
        let clients = self.clients.clone();
        
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(60));
            
            loop {
                ticker.tick().await;
                
                let client_count = clients.len();
                println!("📊 Server metrics: {} active connections", client_count);
                
                metrics.set_active_connections(client_count);
                metrics.log_summary();
            }
        });
    }
    
    async fn start_cleanup_task(&self) {
        let clients = self.clients.clone();
        let timeout = Duration::from_millis(self.config.message_timeout_ms);
        
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                ticker.tick().await;
                
                let now = Instant::now();
                let mut to_remove = Vec::new();
                
                // Find inactive sessions
                for session_ref in clients.iter() {
                    let session = session_ref.value();
                    if now.duration_since(session.last_activity) > timeout {
                        to_remove.push(session.session_id.clone());
                    }
                }
                
                // Remove inactive sessions
                for session_id in to_remove {
                    if let Some((_, session)) = clients.remove(&session_id) {
                        let disconnect_msg = ServerMessage::Disconnected {
                            reason: "Session timeout".to_string(),
                            timestamp: chrono::Utc::now().timestamp_millis() as u64,
                        };
                        let _ = session.sender.send(disconnect_msg);
                        println!("🧹 Cleaned up inactive session: {}", session_id);
                    }
                }
            }
        });
    }
}

// ============================================================================
// MESSAGE ROUTING
// ============================================================================

pub struct MessageRouter {
    pending_requests: Arc<DashMap<String, PendingRequest>>,
}

#[derive(Debug)]
struct PendingRequest {
    session_id: String,
    actor_id: String,
    started_at: Instant,
    timeout: Duration,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            pending_requests: Arc::new(DashMap::new()),
        }
    }
    
    pub async fn route_client_message(
        &self,
        message: ClientMessage,
        session: &Arc<ClientSession>,
        network: &Arc<Network>,
        clients: &Arc<DashMap<String, Arc<ClientSession>>>,
    ) -> Result<()> {
        match message {
            ClientMessage::Connect { user_context, capabilities_requested } => {
                self.handle_connect(user_context, capabilities_requested, session).await
            }
            
            ClientMessage::ExecuteActor { request_id, actor_id, payload, timeout_ms, priority } => {
                self.handle_execute_actor(
                    request_id, actor_id, payload, timeout_ms, priority, session, network
                ).await
            }
            
            ClientMessage::Subscribe { subscription_id, config } => {
                self.handle_subscribe(subscription_id, config, session).await
            }
            
            ClientMessage::Unsubscribe { subscription_id } => {
                self.handle_unsubscribe(subscription_id, session).await
            }
            
            ClientMessage::StartStream { stream_id, config } => {
                self.handle_start_stream(stream_id, config, session, network).await
            }
            
            ClientMessage::StopStream { stream_id } => {
                self.handle_stop_stream(stream_id, session, network).await
            }
            
            ClientMessage::Heartbeat { timestamp: _ } => {
                // Heartbeat acknowledged, no action needed
                Ok(())
            }
            
            ClientMessage::GetNetworkStatus => {
                self.handle_get_network_status(session, network).await
            }
        }
    }
    
    async fn handle_connect(
        &self,
        user_context: UserContext,
        _capabilities_requested: Vec<String>,
        session: &Arc<ClientSession>,
    ) -> Result<()> {
        // In a real implementation, this would validate authentication
        // and update the session with the authenticated user context
        
        println!("👤 User connected: {} (role: {})", 
                user_context.user_id.as_deref().unwrap_or("anonymous"), 
                user_context.role);
        
        Ok(())
    }
    
    async fn handle_execute_actor(
        &self,
        request_id: String,
        actor_id: String,
        payload: serde_json::Value,
        timeout_ms: Option<u64>,
        _priority: MessagePriority,
        session: &Arc<ClientSession>,
        network: &Arc<Network>,
    ) -> Result<()> {
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(60000));
        
        // Record pending request
        let pending = PendingRequest {
            session_id: session.session_id.clone(),
            actor_id: actor_id.clone(),
            started_at: Instant::now(),
            timeout,
        };
        self.pending_requests.insert(request_id.clone(), pending);
        
        // Convert JSON payload to zflow Message
        let actor_message = self.json_to_zflow_message(payload)?;
        
        // Execute actor in network
        let start_time = Instant::now();
        
        match network.execute_actor(&actor_id, actor_message).await {
            Ok(result) => {
                let execution_time = start_time.elapsed();
                
                let response = ServerMessage::ActorResult {
                    request_id: request_id.clone(),
                    actor_id,
                    result: ActorExecutionResult {
                        data: serde_json::to_value(&result)?,
                        success: true,
                    },
                    execution_time_ms: execution_time.as_millis() as u64,
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                };
                
                let _ = session.sender.send(response);
            }
            
            Err(e) => {
                let error_response = ServerMessage::ActorError {
                    request_id: request_id.clone(),
                    actor_id,
                    error: e.to_string(),
                    error_code: "EXECUTION_ERROR".to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                };
                
                let _ = session.sender.send(error_response);
            }
        }
        
        // Clean up pending request
        self.pending_requests.remove(&request_id);
        
        Ok(())
    }
    
    async fn handle_subscribe(
        &self,
        subscription_id: String,
        config: SubscriptionConfig,
        session: &Arc<ClientSession>,
    ) -> Result<()> {
        let mut subscriptions = session.subscriptions.write().await;
        subscriptions.insert(subscription_id, config);
        Ok(())
    }
    
    async fn handle_unsubscribe(
        &self,
        subscription_id: String,
        session: &Arc<ClientSession>,
    ) -> Result<()> {
        let mut subscriptions = session.subscriptions.write().await;
        subscriptions.remove(&subscription_id);
        Ok(())
    }
    
    async fn handle_start_stream(
        &self,
        stream_id: String,
        config: StreamConfig,
        session: &Arc<ClientSession>,
        network: &Arc<Network>,
    ) -> Result<()> {
        // Convert stream config to zflow message and send to streaming actor
        let stream_message = Message::object(serde_json::to_value(config)?.into());
        
        match network.execute_actor("streaming_sql_actor", stream_message).await {
            Ok(_) => {
                println!("🌊 Started stream: {} for session: {}", stream_id, session.session_id);
            }
            Err(e) => {
                let error_msg = ServerMessage::StreamError {
                    stream_id,
                    error: e.to_string(),
                    timestamp: chrono::Utc::now().timestamp_millis() as u64,
                };
                let _ = session.sender.send(error_msg);
            }
        }
        
        Ok(())
    }
    
    async fn handle_stop_stream(
        &self,
        stream_id: String,
        _session: &Arc<ClientSession>,
        _network: &Arc<Network>,
    ) -> Result<()> {
        // Implementation would stop the specified stream
        println!("🛑 Stopping stream: {}", stream_id);
        Ok(())
    }
    
    async fn handle_get_network_status(
        &self,
        session: &Arc<ClientSession>,
        network: &Arc<Network>,
    ) -> Result<()> {
        let status_msg = ServerMessage::NetworkStatusUpdate {
            status: NetworkStatus::Healthy,
            active_actors: network.get_active_actors(),
            message_queue_size: network.get_message_queue_size(),
            timestamp: chrono::Utc::now().timestamp_millis() as u64,
        };
        
        let _ = session.sender.send(status_msg);
        Ok(())
    }
    
    pub async fn handle_binary_data(
        &self,
        data: Vec<u8>,
        session: &Arc<ClientSession>,
        network: &Arc<Network>,
    ) -> Result<()> {
        // Handle binary data (Arrow batches, file uploads, etc.)
        println!("📦 Received binary data: {} bytes from session: {}", 
                data.len(), session.session_id);
        
        // For now, just acknowledge receipt
        // In a real implementation, this would process the binary data
        // and potentially route it to appropriate actors
        
        Ok(())
    }
    
    fn json_to_zflow_message(&self, value: serde_json::Value) -> Result<Message> {
        Ok(Message::from(value))
    }
}

// ============================================================================
// SUPPORTING TYPES
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorExecutionResult {
    pub data: serde_json::Value,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    pub changes: Vec<StateChange>,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateChange {
    pub path: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMetadata {
    pub batch_size: usize,
    pub compression: Option<String>,
    pub schema_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub source: String,
    pub batch_size: usize,
    pub processing_config: serde_json::Value,
}

// ============================================================================
// SERVER METRICS
// ============================================================================

#[derive(Debug)]
pub struct ServerMetrics {
    messages_received: Arc<Mutex<u64>>,
    messages_sent: Arc<Mutex<u64>>,
    active_connections: Arc<Mutex<usize>>,
    errors: Arc<Mutex<u64>>,
}

impl ServerMetrics {
    pub fn new() -> Self {
        Self {
            messages_received: Arc::new(Mutex::new(0)),
            messages_sent: Arc::new(Mutex::new(0)),
            active_connections: Arc::new(Mutex::new(0)),
            errors: Arc::new(Mutex::new(0)),
        }
    }
    
    pub fn increment_messages_received(&self) {
        *self.messages_received.lock().unwrap() += 1;
    }
    
    pub fn increment_messages_sent(&self) {
        *self.messages_sent.lock().unwrap() += 1;
    }
    
    pub fn set_active_connections(&self, count: usize) {
        *self.active_connections.lock().unwrap() = count;
    }
    
    pub fn increment_errors(&self) {
        *self.errors.lock().unwrap() += 1;
    }
    
    pub fn log_summary(&self) {
        let received = *self.messages_received.lock().unwrap();
        let sent = *self.messages_sent.lock().unwrap();
        let connections = *self.active_connections.lock().unwrap();
        let errors = *self.errors.lock().unwrap();
        
        println!("📈 Metrics - Connections: {}, Messages: {}↓ {}↑, Errors: {}", 
                connections, received, sent, errors);
    }
}




#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_server_config_default() {
        let config = ServerConfig::default();
        assert_eq!(config.bind_address, "0.0.0.0:8080");
        assert_eq!(config.max_connections, 10000);
    }
    
    #[test]
    fn test_rate_limiter() {
        let limiter = RateLimiter::new(5, 1);
        
        // Should allow up to 5 requests
        for _ in 0..5 {
            assert!(limiter.check_rate_limit());
        }
        
        // 6th request should be rate limited
        assert!(!limiter.check_rate_limit());
    }
    
    #[tokio::test]
    async fn test_message_routing() {
        let router = MessageRouter::new();
        
        // Test JSON to zflow message conversion
        let json_value = serde_json::json!({"test": "value"});
        let message = router.json_to_zflow_message(json_value).unwrap();
        
        match message {
            Message::Object(obj) => {
                assert_eq!(serde_json::Value::from(obj.as_ref().clone()).get("test").unwrap().as_str().unwrap(), "value");
            }
            _ => panic!("Expected Object message"),
        }
    }
}

// Example usage
// #[tokio::main]
// async fn main() -> Result<()> {
//     // Initialize zflow network with actors
//     let network = Arc::new(Network::new(NetworkConfig::default()));
    
//     // Configure server
//     let config = ServerConfig::default();
    
//     // Create and start server
//     let server = ZflowServer::new(network, config).await?;
//     server.start().await?;
    
//     Ok(())
// }