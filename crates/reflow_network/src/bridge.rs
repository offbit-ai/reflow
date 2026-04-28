use std::{collections::HashMap, sync::Arc, time::Duration};

use parking_lot::RwLock;
use tokio::sync::Mutex;

use crate::{
    actor::message::Message,
    discovery::DiscoveryService,
    distributed_network::DistributedConfig,
    network::Network,
    router::{MessageRouter, RemoteMessage},
};
use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::{
    net::{TcpListener, TcpStream},
    time::{Instant, interval},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, accept_async, connect_async, tungstenite::Message as WsMessage,
};

pub struct NetworkBridge {
    config: DistributedConfig,
    connections: Arc<RwLock<HashMap<String, RemoteConnection>>>,
    discovery: Arc<DiscoveryService>,
    router: Arc<MessageRouter>,
    transport: Arc<TransportLayer>,
    local_network: Arc<RwLock<Option<Arc<RwLock<Network>>>>>,
    shutdown_signal: Arc<tokio::sync::Notify>,
    /// Pending discovery requests awaiting responses, keyed by request_id
    pending_discovery:
        Arc<RwLock<HashMap<String, tokio::sync::oneshot::Sender<ActorDiscoveryResponse>>>>,
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
            pending_discovery: self.pending_discovery.clone(),
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

    pub async fn send(
        &self,
        message: WsMessage,
    ) -> Result<(), tokio_tungstenite::tungstenite::Error> {
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
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
            pending_discovery: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub async fn start(&self, local_network: Arc<RwLock<Network>>) -> Result<(), anyhow::Error> {
        *self.local_network.write() = Some(local_network.clone());

        // Configure router with local network
        self.router
            .set_local_network(local_network, self.config.network_id.clone());

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
        let pending_discovery = self.pending_discovery.clone();

        tokio::spawn(async move {
            while let Ok((stream, addr)) = listener.accept().await {
                let websocket = accept_async(stream).await.unwrap();

                // Handle new connection in separate task
                let connections = connections.clone();
                let router = router.clone();
                let config = config.clone();
                let pending_discovery = pending_discovery.clone();

                tokio::spawn(async move {
                    Self::handle_connection(
                        websocket,
                        addr,
                        connections,
                        router,
                        config,
                        pending_discovery,
                    )
                    .await;
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
        pending_discovery: Arc<
            RwLock<HashMap<String, tokio::sync::oneshot::Sender<ActorDiscoveryResponse>>>,
        >,
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
            connections
                .write()
                .insert(handshake.network_id.clone(), connection);

            tracing::info!(
                "Established connection with network: {}",
                handshake.network_id
            );

            // Handle incoming messages
            Self::handle_incoming_messages(
                handshake.network_id,
                connections.clone(),
                router,
                config,
                pending_discovery,
            )
            .await;
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
        source_actor: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        let router = self.router.clone();
        router
            .route_message(network_id, actor_id, port, message, source_actor)
            .await
    }

    pub async fn register_remote_actor(
        &self,
        actor_id: &str,
        remote_network_id: &str,
        capabilities: Option<Vec<String>>,
    ) -> Result<(), anyhow::Error> {
        self.router
            .register_remote_actor(actor_id, remote_network_id, capabilities)
            .await
    }

    async fn perform_handshake(
        websocket: &mut WebSocketStream<TcpStream>,
        config: &DistributedConfig,
    ) -> Option<NetworkHandshake> {
        // Wait for handshake message
        if let Some(Ok(WsMessage::Text(text))) = websocket.next().await
            && let Ok(handshake) = serde_json::from_str::<NetworkHandshake>(&text)
        {
            tracing::info!("Received handshake from network: {}", handshake.network_id);

            // Auth check: if our config has an auth_token set, the
            // peer's token must match exactly. Local config without a
            // token (None) accepts anyone — backward compatible with
            // local-loopback dev setups that don't bother with auth.
            if let Some(local_token) = config.auth_token.as_deref() {
                let peer_token = handshake.auth_token.as_deref().unwrap_or("");
                if peer_token != local_token {
                    tracing::warn!(
                        "Rejecting handshake from network '{}': auth_token mismatch",
                        handshake.network_id
                    );
                    let response = HandshakeResponse {
                        success: false,
                        network_id: config.network_id.clone(),
                        instance_id: config.instance_id.clone(),
                        error: Some("auth_token mismatch".to_string()),
                    };
                    if let Ok(response_text) = serde_json::to_string(&response) {
                        let _ = websocket.send(WsMessage::Text(response_text.into())).await;
                    }
                    return None;
                }
            }

            let response = HandshakeResponse {
                success: true,
                network_id: config.network_id.clone(),
                instance_id: config.instance_id.clone(),
                error: None,
            };

            if let Ok(response_text) = serde_json::to_string(&response) {
                let _ = websocket.send(WsMessage::Text(response_text.into())).await;
                tracing::info!(
                    "Sent handshake response to network: {}",
                    handshake.network_id
                );
                return Some(handshake);
            }
        }
        None
    }

    async fn handle_incoming_messages(
        network_id: String,
        connections: Arc<RwLock<HashMap<String, RemoteConnection>>>,
        router: Arc<MessageRouter>,
        config: DistributedConfig,
        pending_discovery: Arc<
            RwLock<HashMap<String, tokio::sync::oneshot::Sender<ActorDiscoveryResponse>>>,
        >,
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
                        tracing::info!(
                            "🌐 BRIDGE: Received remote message: {} from {} to {}::{}",
                            remote_message.message_id,
                            remote_message.source_network,
                            remote_message.target_network,
                            remote_message.target_actor
                        );
                        if let Err(e) = router.handle_incoming_message(remote_message).await {
                            tracing::error!("Failed to handle incoming message: {}", e);
                        }
                    } else if let Ok(_heartbeat) = serde_json::from_str::<HeartbeatMessage>(&text) {
                        // Update heartbeat timestamp
                        if let Some(conn) = connections.write().get_mut(&network_id) {
                            conn.last_heartbeat = Instant::now();
                        }
                    } else if let Ok(discovery_request) =
                        serde_json::from_str::<ActorDiscoveryRequest>(&text)
                    {
                        // Handle actor discovery request
                        if let Err(e) = Self::handle_discovery_request(
                            discovery_request,
                            &connection,
                            &router,
                            &config,
                        )
                        .await
                        {
                            tracing::error!("Failed to handle discovery request: {}", e);
                        }
                    } else if let Ok(discovery_response) =
                        serde_json::from_str::<ActorDiscoveryResponse>(&text)
                    {
                        tracing::info!(
                            "Received actor discovery response from {}: {} actors available",
                            discovery_response.network_id,
                            discovery_response.actors.len()
                        );

                        // If there's a pending request waiting for this response, fulfil it
                        let sender = pending_discovery
                            .write()
                            .remove(&discovery_response.request_id);
                        if let Some(tx) = sender {
                            let _ = tx.send(discovery_response);
                        } else {
                            // Unsolicited discovery response — auto-register actors
                            for actor in &discovery_response.actors {
                                if let Err(e) = router
                                    .register_remote_actor(
                                        &actor.actor_id,
                                        &discovery_response.network_id,
                                        Some(actor.capabilities.clone()),
                                    )
                                    .await
                                {
                                    tracing::warn!(
                                        "Failed to register discovered actor {}: {}",
                                        actor.actor_id,
                                        e
                                    );
                                } else {
                                    tracing::info!(
                                        "Auto-registered discovered actor {} from network {}",
                                        actor.actor_id,
                                        discovery_response.network_id
                                    );
                                }
                            }
                        }
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
                    if let Some(conn) = connections.write().get_mut(&network_id) {
                        conn.last_heartbeat = Instant::now();
                    }
                }
                Some(Ok(WsMessage::Close(_))) => {
                    tracing::info!("Connection closed by remote network: {}", network_id);
                    // Mark as failed before cleanup
                    if let Some(conn) = connections.write().get_mut(&network_id) {
                        conn.status = ConnectionStatus::Failed;
                    }
                    break;
                }
                Some(Ok(WsMessage::Frame(_))) => {
                    tracing::debug!("Received raw frame from network: {}", network_id);
                }
                Some(Err(e)) => {
                    tracing::error!("WebSocket error for network {}: {}", network_id, e);
                    // Mark as reconnecting — will be cleaned up if reconnect fails
                    if let Some(conn) = connections.write().get_mut(&network_id) {
                        conn.status = ConnectionStatus::Reconnecting;
                    }
                    break;
                }
                None => {
                    tracing::warn!("WebSocket stream ended for network: {}", network_id);
                    if let Some(conn) = connections.write().get_mut(&network_id) {
                        conn.status = ConnectionStatus::Reconnecting;
                    }
                    break;
                }
            }
        }

        // Attempt to determine if we should remove or if the connection was gracefully closed
        let should_remove = {
            let conns = connections.read();
            conns
                .get(&network_id)
                .map(|c| {
                    matches!(
                        c.status,
                        ConnectionStatus::Failed | ConnectionStatus::Reconnecting
                    )
                })
                .unwrap_or(true)
        };

        if should_remove {
            connections.write().remove(&network_id);
            tracing::info!("Cleaned up connection for network: {}", network_id);
        }
    }

    async fn start_heartbeat_monitor(&self) -> Result<()> {
        let connections = self.connections.clone();
        let heartbeat_interval = Duration::from_millis(self.config.heartbeat_interval_ms);
        let timeout_threshold = heartbeat_interval * 3; // 3 missed heartbeats = timeout
        let local_network_id = self.config.network_id.clone();

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
                        network_id: local_network_id.clone(),
                        timestamp: chrono::Utc::now(),
                    };

                    if let Ok(heartbeat_text) = serde_json::to_string(&heartbeat)
                        && let Err(e) = connection
                            .websocket
                            .send(WsMessage::Text(heartbeat_text.into()))
                            .await
                    {
                        tracing::warn!("Failed to send heartbeat to {}: {}", network_id, e);
                        networks_to_remove.push(network_id.clone());
                    }
                }

                // Transition timed-out connections to Failed and remove them
                if !networks_to_remove.is_empty() {
                    let mut connections_write = connections.write();
                    for network_id in networks_to_remove {
                        if let Some(conn) = connections_write.get_mut(&network_id) {
                            conn.status = ConnectionStatus::Failed;
                        }
                        connections_write.remove(&network_id);
                        tracing::warn!(
                            "Removed timed out connection: {} (status: Failed)",
                            network_id
                        );
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
                    websocket
                        .send(WsMessage::Text(handshake_text.into()))
                        .await?;

                    // Wait for response. A successful handshake unwraps to
                    // the next block; a refused one (auth mismatch,
                    // protocol incompatibility, etc.) returns Err with
                    // the server's reason so callers know connect_to_network
                    // failed and why.
                    let response = match websocket.next().await {
                        Some(Ok(WsMessage::Text(response_text))) => {
                            serde_json::from_str::<HandshakeResponse>(&response_text)
                                .map_err(|e| anyhow::anyhow!("malformed handshake response: {e}"))?
                        }
                        other => {
                            return Err(anyhow::anyhow!(
                                "no handshake response from {endpoint}: {other:?}"
                            ));
                        }
                    };
                    if !response.success {
                        return Err(anyhow::anyhow!(
                            "handshake refused by {endpoint}: {}",
                            response.error.as_deref().unwrap_or("(no reason)")
                        ));
                    }
                    {
                        let connection = RemoteConnection {
                            network_id: response.network_id.clone(),
                            instance_id: response.instance_id.clone(),
                            connection_type: ConnectionType::Client,
                            websocket: ConnectionWebSocket::from_client_websocket(websocket),
                            last_heartbeat: Instant::now(),
                            status: ConnectionStatus::Connected,
                        };

                        self.connections
                            .write()
                            .insert(response.network_id.clone(), connection);

                        // Handle incoming messages
                        let connections = self.connections.clone();
                        let router = self.router.clone();
                        let network_id = response.network_id.clone();
                        let config = self.config.clone();
                        let pending_discovery = self.pending_discovery.clone();

                        tokio::spawn(async move {
                            Self::handle_incoming_messages(
                                network_id,
                                connections,
                                router,
                                config,
                                pending_discovery,
                            )
                            .await;
                        });

                        tracing::info!(
                            "Successfully connected to network: {}",
                            response.network_id
                        );
                        return Ok(());
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to connect to {}: {}", endpoint, e);
                return Err(e.into());
            }
        }

        Err(anyhow::anyhow!(
            "Failed to establish connection to {}",
            endpoint
        ))
    }

    /// Handle incoming actor discovery request
    async fn handle_discovery_request(
        request: ActorDiscoveryRequest,
        connection: &RemoteConnection,
        router: &Arc<MessageRouter>,
        config: &DistributedConfig,
    ) -> Result<()> {
        tracing::info!(
            "Handling actor discovery request from {}",
            request.requesting_network
        );

        // Get list of local actors from the router's local network
        let actors = router.get_local_actor_list();

        let response = ActorDiscoveryResponse {
            request_id: request.request_id,
            network_id: config.network_id.clone(),
            actors,
        };

        let response_text = serde_json::to_string(&response)?;
        connection
            .websocket
            .send(WsMessage::Text(response_text.into()))
            .await?;

        tracing::info!(
            "Sent actor discovery response with {} actors",
            response.actors.len()
        );
        Ok(())
    }

    /// Request actor discovery from a remote network.
    /// Sends a discovery request and waits for a response with a 10-second timeout.
    pub async fn discover_remote_actors(&self, network_id: &str) -> Result<Vec<ActorInfo>> {
        let request = ActorDiscoveryRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            requesting_network: self.config.network_id.clone(),
        };

        let request_text = serde_json::to_string(&request)?;

        // Set up a oneshot channel to receive the response
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_discovery
            .write()
            .insert(request.request_id.clone(), tx);

        // Find connection and send request
        let connection = self.connections.read().get(network_id).cloned();
        if let Some(connection) = connection {
            connection
                .websocket
                .send(WsMessage::Text(request_text.into()))
                .await?;
            tracing::info!("Sent actor discovery request to network: {}", network_id);
        } else {
            // Clean up the pending entry
            self.pending_discovery.write().remove(&request.request_id);
            return Err(anyhow::anyhow!("No connection to network: {}", network_id));
        }

        // Wait for the response with a timeout
        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(response)) => {
                tracing::info!(
                    "Received discovery response from {}: {} actors",
                    network_id,
                    response.actors.len()
                );
                Ok(response.actors)
            }
            Ok(Err(_)) => {
                tracing::warn!(
                    "Discovery response channel closed for network: {}",
                    network_id
                );
                Err(anyhow::anyhow!("Discovery response channel closed"))
            }
            Err(_) => {
                // Timeout — clean up the pending entry
                self.pending_discovery.write().remove(&request.request_id);
                tracing::warn!("Discovery request to {} timed out", network_id);
                Err(anyhow::anyhow!("Discovery request timed out after 10s"))
            }
        }
    }

    /// Shutdown the network bridge
    pub async fn shutdown(&self) -> Result<()> {
        tracing::info!(
            "Shutting down network bridge for network: {}",
            self.config.network_id
        );

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
    pub fn new(_config: DistributedConfig) -> Self {
        Self
    }
}
