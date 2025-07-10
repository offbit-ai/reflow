use std::{collections::HashMap, sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::sync::Mutex;

use crate::{
    discovery::DiscoveryService, distributed_network::DistributedConfig, actor::message::Message, network::Network, router::{MessageRouter, RemoteMessage}
};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    time::{Instant, interval},
};
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream, accept_async, connect_async, tungstenite::Message as WsMessage};

pub struct NetworkBridge {
    config: DistributedConfig,
    connections: Arc<RwLock<HashMap<String, RemoteConnection>>>,
    discovery: Arc<DiscoveryService>,
    router: Arc<MessageRouter>,
    transport: Arc<TransportLayer>,
    local_network: Arc<RwLock<Option<Arc<RwLock<Network>>>>>,
    shutdown_signal: Arc<tokio::sync::Notify>,
}

unsafe impl Sync for NetworkBridge {}
unsafe impl Send for NetworkBridge {}

impl Clone for NetworkBridge {
    fn clone(&self) -> Self {
        NetworkBridge {
            config: self.config.clone(),
            connections: self.connections.clone(),
            discovery: self.discovery.clone(),
            router: self.router.clone(),
            transport: self.transport.clone(),
            local_network: self.local_network.clone(),
            shutdown_signal: self.shutdown_signal.clone(),
        }
    }
}

#[derive(Debug)]
pub struct RemoteConnection {
    pub network_id: String,
    pub instance_id: String,
    pub connection_type: ConnectionType,
    pub websocket: ConnectionWebSocket,
    pub last_heartbeat: Instant,
    pub status: ConnectionStatus,
}

impl Clone for RemoteConnection {
    fn clone(&self) -> Self {
        RemoteConnection {
            network_id: self.network_id.clone(),
            instance_id: self.instance_id.clone(),
            connection_type: self.connection_type.clone(),
            websocket: self.websocket.clone(),
            last_heartbeat: self.last_heartbeat,
            status: self.status.clone(),
        }
    }
}

use futures_util::stream::{SplitSink, SplitStream};

#[derive(Debug)]
pub enum ConnectionWebSocket {
    Server {
        sink: Arc<Mutex<SplitSink<WebSocketStream<TcpStream>, WsMessage>>>,
        stream: Arc<Mutex<SplitStream<WebSocketStream<TcpStream>>>>,
    },
    Client {
        sink: Arc<Mutex<SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, WsMessage>>>,
        stream: Arc<Mutex<SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>>>,
    },
}

impl Clone for ConnectionWebSocket {
    fn clone(&self) -> Self {
        match self {
            ConnectionWebSocket::Server { sink, stream } => ConnectionWebSocket::Server {
                sink: sink.clone(),
                stream: stream.clone(),
            },
            ConnectionWebSocket::Client { sink, stream } => ConnectionWebSocket::Client {
                sink: sink.clone(),
                stream: stream.clone(),
            },
        }
    }
}

impl ConnectionWebSocket {
    pub fn from_server_websocket(ws: WebSocketStream<TcpStream>) -> Self {
        let (sink, stream) = ws.split();
        ConnectionWebSocket::Server {
            sink: Arc::new(Mutex::new(sink)),
            stream: Arc::new(Mutex::new(stream)),
        }
    }

    pub fn from_client_websocket(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        let (sink, stream) = ws.split();
        ConnectionWebSocket::Client {
            sink: Arc::new(Mutex::new(sink)),
            stream: Arc::new(Mutex::new(stream)),
        }
    }

    pub async fn send(&self, message: WsMessage) -> Result<(), tokio_tungstenite::tungstenite::Error> {
        tracing::debug!("🔒 WEBSOCKET: Acquiring sink lock for sending message");
        match self {
            ConnectionWebSocket::Server { sink, .. } => {
                let mut sink = sink.lock().await;
                tracing::debug!("✅ WEBSOCKET: Acquired sink lock, sending message");
                let result = sink.send(message).await;
                tracing::debug!("📤 WEBSOCKET: Message sent, result: {:?}", result.is_ok());
                result
            }
            ConnectionWebSocket::Client { sink, .. } => {
                let mut sink = sink.lock().await;
                tracing::debug!("✅ WEBSOCKET: Acquired sink lock, sending message");
                let result = sink.send(message).await;
                tracing::debug!("📤 WEBSOCKET: Message sent, result: {:?}", result.is_ok());
                result
            }
        }
    }

    pub async fn next(&self) -> Option<Result<WsMessage, tokio_tungstenite::tungstenite::Error>> {
        match self {
            ConnectionWebSocket::Server { stream, .. } => {
                let mut stream = stream.lock().await;
                stream.next().await
            }
            ConnectionWebSocket::Client { stream, .. } => {
                let mut stream = stream.lock().await;
                stream.next().await
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConnectionType {
    Server, // We accepted this connection
    Client, // We initiated this connection
}

#[derive(Debug, Clone)]
pub enum ConnectionStatus {
    Connected,
    Reconnecting,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkHandshake {
    pub network_id: String,
    pub instance_id: String,
    pub protocol_version: String,
    pub auth_token: Option<String>,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub success: bool,
    pub network_id: String,
    pub instance_id: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatMessage {
    pub network_id: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorDiscoveryRequest {
    pub request_id: String,
    pub requesting_network: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorDiscoveryResponse {
    pub request_id: String,
    pub network_id: String,
    pub actors: Vec<ActorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorInfo {
    pub actor_id: String,
    pub capabilities: Vec<String>,
    pub description: Option<String>,
}

impl NetworkBridge {
    pub async fn new(config: DistributedConfig) -> Result<Self, anyhow::Error> {
        let connections = Arc::new(RwLock::new(HashMap::new()));
        let discovery = Arc::new(DiscoveryService::new(config.clone()));
        let router = Arc::new(MessageRouter::with_connection_pool(connections.clone()));
        let transport = Arc::new(TransportLayer::new(config.clone()));

        Ok(NetworkBridge {
            config,
            connections,
            discovery,
            router,
            transport,
            local_network: Arc::new(RwLock::new(None)),
            shutdown_signal: Arc::new(tokio::sync::Notify::new()),
        })
    }

    pub async fn start(&self, local_network: Arc<RwLock<Network>>) -> Result<(), anyhow::Error> {
        *self.local_network.write() = Some(local_network.clone());
        
        // Configure router with local network
        self.router.set_local_network(local_network, self.config.network_id.clone());
        
        // Start server to accept incoming connections
        self.start_server().await?;

        // Connect to discovery endpoints
        self.discovery.start().await?;

        // Start heartbeat monitoring
        self.start_heartbeat_monitor().await?;

        Ok(())
    }

    async fn start_server(&self) -> Result<(), anyhow::Error> {
        let listener = TcpListener::bind(format!(
            "{}:{}",
            self.config.bind_address, self.config.bind_port
        ))
        .await?;

        let connections = self.connections.clone();
        let router = self.router.clone();
        let config = self.config.clone();

        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let websocket = accept_async(stream).await.unwrap();

                // Handle new connection in separate task
                let connections = connections.clone();
                let router = router.clone();
                let config = config.clone();

                tokio::spawn(async move {
                    Self::handle_connection(websocket, addr, connections, router, config).await;
                });
            }
        });

        Ok(())
    }

    async fn handle_connection(
        mut websocket: WebSocketStream<TcpStream>,
        addr: std::net::SocketAddr,
        connections: Arc<RwLock<HashMap<String, RemoteConnection>>>,
        router: Arc<MessageRouter>,
        config: DistributedConfig,
    ) {
        tracing::info!("New connection from: {}", addr);
        
        // Perform handshake to get network identity
        if let Some(handshake) = Self::perform_handshake(&mut websocket, &config).await {
            let connection = RemoteConnection {
                network_id: handshake.network_id.clone(),
                instance_id: handshake.instance_id.clone(),
                connection_type: ConnectionType::Server,
                websocket: ConnectionWebSocket::from_server_websocket(websocket),
                last_heartbeat: Instant::now(),
                status: ConnectionStatus::Connected,
            };
            
            // Store connection
            connections.write().insert(handshake.network_id.clone(), connection);
            
            tracing::info!("Established connection with network: {}", handshake.network_id);
            
            // Handle incoming messages
            Self::handle_incoming_messages(handshake.network_id, connections.clone(), router).await;
        } else {
            tracing::warn!("Failed handshake with {}", addr);
        }
    }

    pub async fn send_remote_message(
        &self,
        network_id: &str,
        actor_id: &str,
        port: &str,
        message: Message,
    ) -> Result<(), anyhow::Error> {
        let router = self.router.clone();
        router
            .route_message(&network_id, &actor_id, &port, message).await
    }

    pub async fn register_remote_actor(
        &self,
        actor_id: &str,
        remote_network_id: &str,
    ) -> Result<(), anyhow::Error> {
        self.router
            .register_remote_actor(actor_id, remote_network_id)
            .await
    }

    async fn perform_handshake(
        websocket: &mut WebSocketStream<TcpStream>, 
        config: &DistributedConfig
    ) -> Option<NetworkHandshake> {
        // Wait for handshake message
        if let Some(Ok(WsMessage::Text(text))) = websocket.next().await {
            if let Ok(handshake) = serde_json::from_str::<NetworkHandshake>(&text) {
                tracing::info!("Received handshake from network: {}", handshake.network_id);
                
                // Validate protocol version and auth (simplified for now)
                let response = HandshakeResponse {
                    success: true,
                    network_id: config.network_id.clone(),
                    instance_id: config.instance_id.clone(),
                    error: None,
                };
                
                if let Ok(response_text) = serde_json::to_string(&response) {
                    let _ = websocket.send(WsMessage::Text(response_text.into())).await;
                    tracing::info!("Sent handshake response to network: {}", handshake.network_id);
                    return Some(handshake);
                }
            }
        }
        None
    }

    async fn handle_incoming_messages(
        network_id: String,
        connections: Arc<RwLock<HashMap<String, RemoteConnection>>>,
        router: Arc<MessageRouter>,
    ) {
        loop {
            let connection = {
                let connections_guard = connections.read();
                if let Some(conn) = connections_guard.get(&network_id) {
                    conn.clone()
                } else {
                    break;
                }
            };
            
            match connection.websocket.next().await {
                Some(Ok(WsMessage::Text(text))) => {
                    if let Ok(remote_message) = serde_json::from_str::<RemoteMessage>(&text) {
                        // Route message to local network
                        tracing::info!("🌐 BRIDGE: Received remote message: {} from {} to {}::{}", 
                                       remote_message.message_id, 
                                       remote_message.source_network,
                                       remote_message.target_network,
                                       remote_message.target_actor);
                        if let Err(e) = router.handle_incoming_message(remote_message).await {
                            tracing::error!("Failed to handle incoming message: {}", e);
                        }
                    } else if let Ok(heartbeat) = serde_json::from_str::<HeartbeatMessage>(&text) {
                        // Update heartbeat timestamp
                        if let Some(mut conn) = connections.write().get_mut(&network_id) {
                            conn.last_heartbeat = Instant::now();
                        }
                    } else if let Ok(discovery_request) = serde_json::from_str::<ActorDiscoveryRequest>(&text) {
                        // Handle actor discovery request
                        if let Err(e) = Self::handle_discovery_request(discovery_request, &connection, &router).await {
                            tracing::error!("Failed to handle discovery request: {}", e);
                        }
                    } else if let Ok(discovery_response) = serde_json::from_str::<ActorDiscoveryResponse>(&text) {
                        // Handle actor discovery response
                        tracing::info!("Received actor discovery response from {}: {} actors available", 
                                      network_id, discovery_response.actors.len());
                        // TODO: Process discovered actors and register them
                    }
                }
                Some(Ok(WsMessage::Binary(_))) => {
                    // Handle binary messages if needed in the future
                    tracing::debug!("Received binary message from network: {}", network_id);
                }
                Some(Ok(WsMessage::Ping(data))) => {
                    // Respond to ping with pong
                    if let Err(e) = connection.websocket.send(WsMessage::Pong(data)).await {
                        tracing::warn!("Failed to send pong to {}: {}", network_id, e);
                    }
                }
                Some(Ok(WsMessage::Pong(_))) => {
                    // Update heartbeat on pong
                    if let Some(mut conn) = connections.write().get_mut(&network_id) {
                        conn.last_heartbeat = Instant::now();
                    }
                }
                Some(Ok(WsMessage::Close(_))) => {
                    tracing::info!("Connection closed by remote network: {}", network_id);
                    break;
                }
                Some(Ok(WsMessage::Frame(_))) => {
                    // Handle raw frames if needed
                    tracing::debug!("Received raw frame from network: {}", network_id);
                }
                Some(Err(e)) => {
                    tracing::error!("WebSocket error for network {}: {}", network_id, e);
                    break;
                }
                None => {
                    tracing::warn!("WebSocket stream ended for network: {}", network_id);
                    break;
                }
            }
        }
        
        // Clean up connection
        connections.write().remove(&network_id);
        tracing::info!("Cleaned up connection for network: {}", network_id);
    }

    async fn start_heartbeat_monitor(&self) -> Result<()> {
        let connections = self.connections.clone();
        let heartbeat_interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        let timeout_threshold = heartbeat_interval * 3; // 3 missed heartbeats = timeout
        
        tokio::spawn(async move {
            let mut interval = interval(heartbeat_interval);
            
            loop {
                interval.tick().await;
                
                let now = Instant::now();
                let mut networks_to_remove = Vec::new();
                
                // Check for expired connections and send heartbeats
                let connections_snapshot = {
                    let connections_read = connections.read();
                    connections_read.clone()
                };
                
                for (network_id, connection) in connections_snapshot.iter() {
                    // Check if connection has timed out
                    if now.duration_since(connection.last_heartbeat) > timeout_threshold {
                        networks_to_remove.push(network_id.clone());
                        continue;
                    }
                    
                    // Send heartbeat
                    let heartbeat = HeartbeatMessage {
                        network_id: "local".to_string(), // TODO: Get from config
                        timestamp: chrono::Utc::now(),
                    };
                    
                    if let Ok(heartbeat_text) = serde_json::to_string(&heartbeat) {
                        if let Err(e) = connection.websocket.send(WsMessage::Text(heartbeat_text.into())).await {
                            tracing::warn!("Failed to send heartbeat to {}: {}", network_id, e);
                            networks_to_remove.push(network_id.clone());
                        }
                    }
                }
                
                // Remove timed out connections
                if !networks_to_remove.is_empty() {
                    let mut connections_write = connections.write();
                    for network_id in networks_to_remove {
                        connections_write.remove(&network_id);
                        tracing::warn!("Removed timed out connection: {}", network_id);
                    }
                }
            }
        });
        
        Ok(())
    }

    /// Connect to a remote network
    pub async fn connect_to_network(&self, endpoint: &str) -> Result<()> {
        let url = format!("ws://{}", endpoint);
        
        match connect_async(&url).await {
            Ok((mut websocket, _)) => {
                // Send handshake
                let handshake = NetworkHandshake {
                    network_id: self.config.network_id.clone(),
                    instance_id: self.config.instance_id.clone(),
                    protocol_version: "1.0".to_string(),
                    auth_token: self.config.auth_token.clone(),
                    capabilities: vec!["actor_messaging".to_string()],
                };
                
                if let Ok(handshake_text) = serde_json::to_string(&handshake) {
                    websocket.send(WsMessage::Text(handshake_text.into())).await?;
                    
                    // Wait for response
                    if let Some(Ok(WsMessage::Text(response_text))) = websocket.next().await {
                        if let Ok(response) = serde_json::from_str::<HandshakeResponse>(&response_text) {
                            if response.success {
                                let connection = RemoteConnection {
                                    network_id: response.network_id.clone(),
                                    instance_id: response.instance_id.clone(),
                                    connection_type: ConnectionType::Client,
                                    websocket: ConnectionWebSocket::from_client_websocket(websocket),
                                    last_heartbeat: Instant::now(),
                                    status: ConnectionStatus::Connected,
                                };
                                
                                self.connections.write().insert(response.network_id.clone(), connection);
                                
                                // Handle incoming messages
                                let connections = self.connections.clone();
                                let router = self.router.clone();
                                let network_id = response.network_id.clone();
                                
                                tokio::spawn(async move {
                                    Self::handle_incoming_messages(network_id, connections, router).await;
                                });
                                
                                tracing::info!("Successfully connected to network: {}", response.network_id);
                                return Ok(());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to connect to {}: {}", endpoint, e);
                return Err(e.into());
            }
        }
        
        Err(anyhow::anyhow!("Failed to establish connection to {}", endpoint))
    }

    /// Handle incoming actor discovery request
    async fn handle_discovery_request(
        request: ActorDiscoveryRequest,
        connection: &RemoteConnection,
        router: &Arc<MessageRouter>,
    ) -> Result<()> {
        tracing::info!("Handling actor discovery request from {}", request.requesting_network);
        
        // Get list of local actors (simplified implementation)
        let actors = vec![
            ActorInfo {
                actor_id: "example_actor".to_string(),
                capabilities: vec!["processing".to_string()],
                description: Some("Example local actor".to_string()),
            }
            // TODO: Get actual list of local actors from the network
        ];
        
        let response = ActorDiscoveryResponse {
            request_id: request.request_id,
            network_id: "local".to_string(), // TODO: Get from config
            actors,
        };
        
        let response_text = serde_json::to_string(&response)?;
        connection.websocket.send(WsMessage::Text(response_text.into())).await?;
        
        tracing::info!("Sent actor discovery response with {} actors", response.actors.len());
        Ok(())
    }

    /// Request actor discovery from a remote network
    pub async fn discover_remote_actors(&self, network_id: &str) -> Result<Vec<ActorInfo>> {
        let request = ActorDiscoveryRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            requesting_network: self.config.network_id.clone(),
        };
        
        let request_text = serde_json::to_string(&request)?;
        
        // Find connection and send request
        if let Some(connection) = self.connections.read().get(network_id) {
            connection.websocket.send(WsMessage::Text(request_text.into())).await?;
            tracing::info!("Sent actor discovery request to network: {}", network_id);
            
            // TODO: Implement response handling with timeout
            // For now, return empty list
            Ok(vec![])
        } else {
            Err(anyhow::anyhow!("No connection to network: {}", network_id))
        }
    }

    /// Shutdown the network bridge
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!("Shutting down network bridge for network: {}", self.config.network_id);
        
        // Close all connections
        let connections_snapshot = {
            let connections_read = self.connections.read();
            connections_read.clone()
        };
        
        for (network_id, connection) in connections_snapshot.iter() {
            tracing::info!("Closing connection to network: {}", network_id);
            
            // Send close message
            let _ = connection.websocket.send(WsMessage::Close(None)).await;
        }
        
        // Clear all connections
        self.connections.write().clear();
        
        tracing::info!("Network bridge shutdown complete");
        Ok(())
    }
}

pub struct TransportLayer;

impl TransportLayer {
    pub fn new(config: DistributedConfig) -> Self {
        Self
    }
}
