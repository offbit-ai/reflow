use anyhow::Result;
use chrono::Utc;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore, broadcast};
use tokio_tungstenite::{
    WebSocketStream, accept_async,
    tungstenite::{Error as WsError, Message},
};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::storage::{StorageBackend, TraceStorage};
use reflow_tracing_protocol::*;

/// Subscription manager for real-time event streaming
pub struct SubscriptionManager {
    subscriptions: Arc<Mutex<HashMap<String, Subscription>>>,
    event_broadcaster: broadcast::Sender<EventNotification>,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    #[allow(dead_code)]
    pub id: String,
    pub filters: SubscriptionFilters,
    pub sender:
        Arc<Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>>,
}

#[derive(Debug, Clone)]
pub struct EventNotification {
    pub trace_id: TraceId,
    pub event: TraceEvent,
}

impl SubscriptionManager {
    pub fn new() -> Self {
        let (event_broadcaster, _) = broadcast::channel(1000);
        Self {
            subscriptions: Arc::new(Mutex::new(HashMap::new())),
            event_broadcaster,
        }
    }

    pub async fn add_subscription(
        &self,
        subscription_id: String,
        filters: SubscriptionFilters,
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
    ) -> Result<()> {
        let subscription = Subscription {
            id: subscription_id.clone(),
            filters,
            sender,
        };

        let mut subscriptions = self.subscriptions.lock().await;
        subscriptions.insert(subscription_id, subscription);

        info!("Added subscription for client");
        Ok(())
    }

    pub async fn remove_subscription(&self, subscription_id: &str) {
        let mut subscriptions = self.subscriptions.lock().await;
        subscriptions.remove(subscription_id);
        info!("Removed subscription for client: {}", subscription_id);
    }

    pub async fn notify_event(&self, trace_id: TraceId, event: TraceEvent) {
        let notification = EventNotification { trace_id, event };

        info!(
            "📡 Notifying subscribers about event: {:?} from actor: {}",
            notification.event.event_type, notification.event.actor_id
        );

        // Broadcast to all subscribers (non-blocking)
        if let Err(e) = self.event_broadcaster.send(notification.clone()) {
            debug!("No active subscribers for event notification: {}", e);
        }

        // Send directly to filtered subscribers
        let subscriptions = self.subscriptions.lock().await;
        info!("📊 Active subscriptions: {}", subscriptions.len());

        for (sub_id, subscription) in subscriptions.iter() {
            info!("🔍 Checking subscription {} against event", sub_id);

            if self
                .matches_filters(&subscription.filters, &notification)
                .await
            {
                info!("✅ Event matches subscription filters for {}", sub_id);

                let response = TracingResponse::EventNotification {
                    trace_id: notification.trace_id.clone(),
                    event: notification.event.clone(),
                };

                if let Ok(response_json) = serde_json::to_string(&response) {
                    info!(
                        "📤 Sending event notification to subscriber {}: {} bytes",
                        sub_id,
                        response_json.len()
                    );

                    match subscription.sender.try_lock() {
                        Ok(mut sender_guard) => {
                            if let Err(e) =
                                sender_guard.send(Message::Text(response_json.into())).await
                            {
                                warn!(
                                    "❌ Failed to send event notification to subscriber {}: {}",
                                    sub_id, e
                                );
                            } else {
                                info!(
                                    "✅ Successfully sent event notification to subscriber {}",
                                    sub_id
                                );
                            }
                        }
                        Err(e) => {
                            warn!(
                                "⚠️  Could not acquire sender lock for subscriber {}: {}",
                                sub_id, e
                            );
                        }
                    }
                } else {
                    error!(
                        "❌ Failed to serialize event notification for subscriber {}",
                        sub_id
                    );
                }
            } else {
                info!(
                    "❌ Event does not match subscription filters for {}",
                    sub_id
                );
            }
        }
    }

    async fn matches_filters(
        &self,
        filters: &SubscriptionFilters,
        notification: &EventNotification,
    ) -> bool {
        // Check flow_ids filter
        if let Some(_flow_ids) = &filters.flow_ids {
            // For now, we don't have flow_id in the event itself, so we skip this filter
            // In a real implementation, you'd need to resolve the trace_id to get the flow_id
        }

        // Check actor_ids filter
        if let Some(actor_ids) = &filters.actor_ids
            && !actor_ids.contains(&notification.event.actor_id)
        {
            return false;
        }

        // Check event_types filter
        if let Some(event_types) = &filters.event_types
            && !event_types.contains(&notification.event.event_type)
        {
            return false;
        }

        // Check status_filter (would need trace status, skipping for now)

        true
    }
}

pub struct TraceServer {
    config: Config,
    storage: Box<dyn TraceStorage>,
    metrics: Arc<Mutex<ServerMetrics>>,
    subscription_manager: SubscriptionManager,
    /// In-flight traces being aggregated from a live stream of `RecordEvent`s.
    /// A trace lives here from `StartTrace` (or first event) until it is
    /// finalized (`EndTrace` or client disconnect), at which point it is
    /// written through to `storage`.
    active_traces: Arc<DashMap<TraceId, FlowTrace>>,
    /// Trace ids started by each connection, so a disconnect can finalize any
    /// traces the client left open.
    connection_traces: Arc<DashMap<String, Vec<TraceId>>>,
}

#[derive(Debug, Default)]
pub struct ServerMetrics {
    connections_active: u64,
    connections_total: u64,
    messages_received: u64,
    messages_sent: u64,
    errors_total: u64,
    traces_stored: u64,
    traces_queried: u64,
}

impl TraceServer {
    pub async fn new(config: Config) -> Result<Self> {
        config.validate()?;

        let storage = StorageBackend::create(&config.storage).await?;
        info!("Initialized storage backend: {}", config.storage.backend);

        Ok(Self {
            config,
            storage,
            metrics: Arc::new(Mutex::new(ServerMetrics::default())),
            subscription_manager: SubscriptionManager::new(),
            active_traces: Arc::new(DashMap::new()),
            connection_traces: Arc::new(DashMap::new()),
        })
    }

    pub async fn handle_message(
        &self,
        message: &str,
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
        client_id: String,
    ) -> Result<()> {
        debug!("Received message: {}", message);

        // Update metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.messages_received += 1;
        }

        // Try to parse as TracingRequest first, then fallback to TraceMessage
        if let Ok(tracing_request) = serde_json::from_str::<TracingRequest>(message) {
            let response = match self
                .process_tracing_request(tracing_request, sender.clone(), client_id)
                .await
            {
                Ok(response) => response,
                Err(e) => {
                    error!("Error processing tracing request: {}", e);
                    self.increment_error_count().await;
                    TracingResponse::Error {
                        message: format!("Processing error: {}", e),
                        code: ErrorCode::InternalError,
                    }
                }
            };
            self.send_tracing_response(sender, response).await?;
        } else if let Ok(trace_message) = serde_json::from_str::<TraceMessage>(message) {
            let response = match self.process_message(trace_message).await {
                Ok(response) => response,
                Err(e) => {
                    error!("Error processing message: {}", e);
                    self.increment_error_count().await;
                    TraceResponse::Error {
                        message: format!("Processing error: {}", e),
                    }
                }
            };
            self.send_trace_response(sender, response).await?;
        } else {
            error!("Failed to parse message as either TracingRequest or TraceMessage");
            self.send_error(sender, "Invalid message format".to_string())
                .await?;
            return Ok(());
        }

        Ok(())
    }

    pub async fn handle_binary_message(
        &self,
        data: &[u8],
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
    ) -> Result<()> {
        debug!("Received binary message of {} bytes", data.len());

        // For now, treat binary data as compressed JSON
        let decompressed = if self.config.compression.enabled {
            self.decompress_data(data)?
        } else {
            data.to_vec()
        };

        let message = String::from_utf8(decompressed)
            .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in binary message: {}", e))?;

        self.handle_message(&message, sender, "binary_client".to_string())
            .await
    }

    async fn process_tracing_request(
        &self,
        request: TracingRequest,
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
        client_id: String,
    ) -> Result<TracingResponse> {
        match request {
            TracingRequest::StartTrace {
                trace_id,
                flow_id,
                version,
            } => {
                // Create the in-flight trace under the client-owned id and
                // remember it against this connection so a disconnect can
                // finalize it if no explicit EndTrace arrives.
                let trace = Self::new_flow_trace(trace_id.clone(), flow_id.clone(), version);
                self.active_traces.insert(trace_id.clone(), trace);
                self.connection_traces
                    .entry(client_id)
                    .or_default()
                    .push(trace_id.clone());
                info!("Started trace {} for flow {}", trace_id, flow_id);
                Ok(TracingResponse::TraceStarted { trace_id })
            }
            TracingRequest::RecordEvent { trace_id, event } => {
                // Append to the in-flight trace (lazily create if an event
                // arrives before StartTrace), then broadcast to subscribers.
                {
                    let mut entry = self
                        .active_traces
                        .entry(trace_id.clone())
                        .or_insert_with(|| {
                            Self::new_flow_trace(
                                trace_id.clone(),
                                FlowId::new("unknown"),
                                Self::default_version(),
                            )
                        });
                    entry.events.push(event.clone());
                }
                self.subscription_manager
                    .notify_event(trace_id, event)
                    .await;
                Ok(TracingResponse::EventRecorded {
                    success: true,
                    error: None,
                })
            }
            TracingRequest::EndTrace { trace_id, status } => {
                self.finalize_trace(&trace_id, status).await?;
                Ok(TracingResponse::EventRecorded {
                    success: true,
                    error: None,
                })
            }
            TracingRequest::GetTrace {
                request_id,
                trace_id,
            } => {
                // In-flight traces take precedence over finalized storage.
                let trace = match self.active_traces.get(&trace_id) {
                    Some(t) => Some(t.clone()),
                    None => self.storage.get_trace(trace_id).await?,
                };
                Ok(TracingResponse::TraceData { request_id, trace })
            }
            TracingRequest::QueryTraces { request_id, query } => {
                let mut traces = self.storage.query_traces(query.clone()).await?;
                // Merge in matching in-flight traces not yet in storage.
                for entry in self.active_traces.iter() {
                    let t = entry.value();
                    if Self::query_matches(&query, t)
                        && !traces.iter().any(|s| s.trace_id == t.trace_id)
                    {
                        traces.push(t.clone());
                    }
                }
                if let Some(limit) = query.limit {
                    traces.truncate(limit);
                }
                let total_count = traces.len();
                Ok(TracingResponse::QueryResults {
                    request_id,
                    traces,
                    total_count,
                })
            }
            TracingRequest::GetFlowVersions { flow_id: _ } => {
                // Not implemented yet
                Ok(TracingResponse::Error {
                    message: "GetFlowVersions not implemented yet".to_string(),
                    code: ErrorCode::NotFound,
                })
            }
            TracingRequest::Ping => Ok(TracingResponse::Pong),
            TracingRequest::Subscribe { filters } => {
                // Add subscription
                self.subscription_manager
                    .add_subscription(client_id, filters, sender)
                    .await?;

                info!("Client subscribed to events");
                Ok(TracingResponse::EventRecorded {
                    success: true,
                    error: None,
                })
            }
            TracingRequest::Unsubscribe => {
                // Remove subscription
                self.subscription_manager
                    .remove_subscription(&client_id)
                    .await;

                info!("Client unsubscribed from events");
                Ok(TracingResponse::EventRecorded {
                    success: true,
                    error: None,
                })
            }
        }
    }

    /// Build a fresh in-flight `FlowTrace` with `Running` status.
    fn new_flow_trace(trace_id: TraceId, flow_id: FlowId, version: FlowVersion) -> FlowTrace {
        FlowTrace {
            trace_id,
            flow_id,
            execution_id: ExecutionId::new(),
            version,
            start_time: Utc::now(),
            end_time: None,
            status: ExecutionStatus::Running,
            events: Vec::new(),
            metadata: Self::default_metadata(),
        }
    }

    fn default_version() -> FlowVersion {
        FlowVersion {
            major: 0,
            minor: 0,
            patch: 0,
            git_hash: None,
            timestamp: Utc::now(),
        }
    }

    fn default_metadata() -> TraceMetadata {
        TraceMetadata {
            user_id: None,
            session_id: None,
            environment: "unknown".to_string(),
            hostname: "unknown".to_string(),
            process_id: std::process::id(),
            thread_id: format!("{:?}", std::thread::current().id()),
            tags: HashMap::new(),
        }
    }

    /// Move a trace out of the active map, stamp its terminal status, and
    /// write it through to storage. No-op if the trace is already finalized.
    async fn finalize_trace(&self, trace_id: &TraceId, status: ExecutionStatus) -> Result<()> {
        if let Some((_, mut trace)) = self.active_traces.remove(trace_id) {
            trace.status = status;
            trace.end_time = Some(Utc::now());
            self.storage.store_trace(trace).await?;
            let mut metrics = self.metrics.lock().await;
            metrics.traces_stored += 1;
        }
        Ok(())
    }

    /// Finalize every still-open trace a disconnected client started.
    async fn finalize_connection_traces(&self, client_id: &str) {
        if let Some((_, trace_ids)) = self.connection_traces.remove(client_id) {
            for trace_id in trace_ids {
                if let Err(e) = self
                    .finalize_trace(&trace_id, ExecutionStatus::Completed)
                    .await
                {
                    warn!("Failed to finalize trace {} on disconnect: {}", trace_id, e);
                }
            }
        }
    }

    /// Minimal filter for including in-flight traces in query results.
    fn query_matches(query: &TraceQuery, trace: &FlowTrace) -> bool {
        if let Some(flow_id) = &query.flow_id
            && &trace.flow_id != flow_id
        {
            return false;
        }
        if let Some(execution_id) = &query.execution_id
            && &trace.execution_id != execution_id
        {
            return false;
        }
        if let Some(status) = &query.status
            && &trace.status != status
        {
            return false;
        }
        if let Some((start, end)) = &query.time_range
            && (trace.start_time < *start || trace.start_time > *end)
        {
            return false;
        }
        if let Some(actor) = &query.actor_filter
            && !trace.events.iter().any(|e| &e.actor_id == actor)
        {
            return false;
        }
        true
    }

    async fn send_trace_response(
        &self,
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
        response: TraceResponse,
    ) -> Result<()> {
        let response_json = serde_json::to_string(&response)?;

        let message = if self.config.compression.enabled && response_json.len() > 1024 {
            // Compress large responses
            let compressed = self.compress_data(response_json.as_bytes())?;
            Message::Binary(compressed.into())
        } else {
            Message::Text(response_json.into())
        };

        let mut sender_guard = sender.lock().await;
        sender_guard.send(message).await?;

        // Update metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.messages_sent += 1;
        }

        Ok(())
    }

    async fn send_tracing_response(
        &self,
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
        response: TracingResponse,
    ) -> Result<()> {
        let response_json = serde_json::to_string(&response)?;

        let message = if self.config.compression.enabled && response_json.len() > 1024 {
            // Compress large responses
            let compressed = self.compress_data(response_json.as_bytes())?;
            Message::Binary(compressed.into())
        } else {
            Message::Text(response_json.into())
        };

        let mut sender_guard = sender.lock().await;
        sender_guard.send(message).await?;

        // Update metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.messages_sent += 1;
        }

        Ok(())
    }

    async fn process_message(&self, message: TraceMessage) -> Result<TraceResponse> {
        match message {
            TraceMessage::StoreTrace { trace } => {
                let trace_id = self.storage.store_trace(trace).await?;

                // Update metrics
                {
                    let mut metrics = self.metrics.lock().await;
                    metrics.traces_stored += 1;
                }

                Ok(TraceResponse::TraceStored { trace_id })
            }
            TraceMessage::QueryTraces { query } => {
                let traces = self.storage.query_traces(query).await?;

                // Update metrics
                {
                    let mut metrics = self.metrics.lock().await;
                    metrics.traces_queried += 1;
                }

                Ok(TraceResponse::TracesFound { traces })
            }
            TraceMessage::GetTrace { trace_id } => {
                let trace_id_clone = trace_id.clone();
                match self.storage.get_trace(trace_id).await? {
                    Some(trace) => Ok(TraceResponse::TraceData { trace }),
                    None => Ok(TraceResponse::Error {
                        message: format!("Trace not found: {}", trace_id_clone),
                    }),
                }
            }
            TraceMessage::Subscribe { filter: _ } => {
                // Legacy subscription support - redirect to new subscription manager
                let _filters = SubscriptionFilters {
                    flow_ids: None,
                    actor_ids: None,
                    event_types: None,
                    status_filter: None,
                };

                // Note: This is a simplified conversion for backwards compatibility
                warn!(
                    "Using legacy subscription format, consider upgrading to TracingRequest::Subscribe"
                );
                Ok(TraceResponse::Error {
                    message: "Legacy subscription format - use TracingRequest::Subscribe instead"
                        .to_string(),
                })
            }
            TraceMessage::GetMetrics => {
                let metrics = self.metrics.lock().await;
                let metrics_json = serde_json::json!({
                    "connections_active": metrics.connections_active,
                    "connections_total": metrics.connections_total,
                    "messages_received": metrics.messages_received,
                    "messages_sent": metrics.messages_sent,
                    "errors_total": metrics.errors_total,
                    "traces_stored": metrics.traces_stored,
                    "traces_queried": metrics.traces_queried,
                });

                Ok(TraceResponse::Metrics { data: metrics_json })
            }
        }
    }

    async fn send_response(
        &self,
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
        response: TraceResponse,
    ) -> Result<()> {
        let response_json = serde_json::to_string(&response)?;

        let message = if self.config.compression.enabled && response_json.len() > 1024 {
            // Compress large responses
            let compressed = self.compress_data(response_json.as_bytes())?;
            Message::Binary(compressed.into())
        } else {
            Message::Text(response_json.into())
        };

        let mut sender_guard = sender.lock().await;
        sender_guard.send(message).await?;

        // Update metrics
        {
            let mut metrics = self.metrics.lock().await;
            metrics.messages_sent += 1;
        }

        Ok(())
    }

    async fn send_error(
        &self,
        sender: Arc<
            Mutex<futures::stream::SplitSink<WebSocketStream<tokio::net::TcpStream>, Message>>,
        >,
        error_message: String,
    ) -> Result<()> {
        let response = TraceResponse::Error {
            message: error_message,
        };
        self.send_response(sender, response).await
    }

    async fn increment_error_count(&self) {
        let mut metrics = self.metrics.lock().await;
        metrics.errors_total += 1;
    }

    fn compress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.config.compression.algorithm.as_str() {
            "zstd" => Ok(zstd::encode_all(data, self.config.compression.level)?),
            "lz4" => Ok(lz4_flex::compress(data)),
            "brotli" => {
                let mut compressed = Vec::new();
                brotli::BrotliCompress(
                    &mut std::io::Cursor::new(data),
                    &mut compressed,
                    &brotli::enc::BrotliEncoderParams {
                        quality: self.config.compression.level,
                        ..Default::default()
                    },
                )?;
                Ok(compressed)
            }
            "gzip" => {
                use flate2::Compression;
                use flate2::write::GzEncoder;
                use std::io::Write;

                let mut encoder = GzEncoder::new(
                    Vec::new(),
                    Compression::new(self.config.compression.level as u32),
                );
                encoder.write_all(data)?;
                Ok(encoder.finish()?)
            }
            _ => Err(anyhow::anyhow!(
                "Unsupported compression algorithm: {}",
                self.config.compression.algorithm
            )),
        }
    }

    fn decompress_data(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self.config.compression.algorithm.as_str() {
            "zstd" => Ok(zstd::decode_all(data)?),
            "lz4" => lz4_flex::decompress_size_prepended(data)
                .map_err(|e| anyhow::anyhow!("LZ4 decompression failed: {}", e)),
            "brotli" => {
                let mut decompressed = Vec::new();
                brotli::BrotliDecompress(&mut std::io::Cursor::new(data), &mut decompressed)?;
                Ok(decompressed)
            }
            "gzip" => {
                use flate2::read::GzDecoder;
                use std::io::Read;

                let mut decoder = GzDecoder::new(data);
                let mut decompressed = Vec::new();
                decoder.read_to_end(&mut decompressed)?;
                Ok(decompressed)
            }
            _ => Err(anyhow::anyhow!(
                "Unsupported compression algorithm: {}",
                self.config.compression.algorithm
            )),
        }
    }

    pub async fn get_metrics(&self) -> ServerMetrics {
        let metrics = self.metrics.lock().await;
        ServerMetrics {
            connections_active: metrics.connections_active,
            connections_total: metrics.connections_total,
            messages_received: metrics.messages_received,
            messages_sent: metrics.messages_sent,
            errors_total: metrics.errors_total,
            traces_stored: metrics.traces_stored,
            traces_queried: metrics.traces_queried,
        }
    }

    /// Run the server with the provided TCP listener
    pub async fn run(self, listener: TcpListener) -> Result<()> {
        let server = Arc::new(self);

        // Create a semaphore to limit concurrent connections
        let connection_semaphore = Arc::new(Semaphore::new(server.config.server.max_connections));

        info!(
            "Starting WebSocket server, max connections: {}",
            server.config.server.max_connections
        );

        // Set up graceful shutdown
        let shutdown_signal = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
            info!("Received shutdown signal");
        };

        // Main server loop
        tokio::select! {
            result = Self::accept_connections(server.clone(), listener, connection_semaphore) => {
                match result {
                    Ok(_) => info!("Server stopped gracefully"),
                    Err(e) => error!("Server error: {}", e),
                }
            }
            _ = shutdown_signal => {
                info!("Shutting down server gracefully...");
            }
        }

        // Log final metrics
        let final_metrics = server.get_metrics().await;
        info!("Final server metrics: {:?}", final_metrics);

        Ok(())
    }

    async fn accept_connections(
        server: Arc<Self>,
        listener: TcpListener,
        connection_semaphore: Arc<Semaphore>,
    ) -> Result<()> {
        loop {
            // Wait for a connection slot
            let permit = connection_semaphore.clone().acquire_owned().await?;

            // Accept the next connection
            let (stream, addr) = match listener.accept().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to accept connection: {}", e);
                    continue;
                }
            };

            info!("New connection from: {}", addr);

            // Update connection metrics
            {
                let mut metrics = server.metrics.lock().await;
                metrics.connections_total += 1;
                metrics.connections_active += 1;
            }

            // Spawn a task to handle this connection
            let server_clone = server.clone();
            tokio::spawn(async move {
                // The permit will be dropped when this task ends, releasing the connection slot
                let _permit = permit;

                if let Err(e) = Self::handle_connection(server_clone.clone(), stream, addr).await {
                    error!("Connection error for {}: {}", addr, e);
                }

                // Update active connections count
                {
                    let mut metrics = server_clone.metrics.lock().await;
                    metrics.connections_active -= 1;
                }

                info!("Connection closed: {}", addr);
            });
        }
    }

    async fn handle_connection(
        server: Arc<Self>,
        stream: TcpStream,
        addr: std::net::SocketAddr,
    ) -> Result<()> {
        // Set buffer size
        if let Err(e) = stream.set_nodelay(true) {
            warn!("Failed to set TCP_NODELAY for {}: {}", addr, e);
        }

        // Upgrade to WebSocket
        let ws_stream = match accept_async(stream).await {
            Ok(ws) => ws,
            Err(e) => {
                error!("WebSocket upgrade failed for {}: {}", addr, e);
                return Err(e.into());
            }
        };

        info!("WebSocket connection established with {}", addr);

        // Split the WebSocket stream
        let (sender, mut receiver) = ws_stream.split();
        let sender = Arc::new(Mutex::new(sender));

        // Handle incoming messages
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    debug!("Received text message from {}: {} bytes", addr, text.len());
                    if let Err(e) = server
                        .handle_message(&text, sender.clone(), addr.to_string())
                        .await
                    {
                        error!("Error handling message from {}: {}", addr, e);
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                    debug!(
                        "Received binary message from {}: {} bytes",
                        addr,
                        data.len()
                    );
                    if let Err(e) = server.handle_binary_message(&data, sender.clone()).await {
                        error!("Error handling binary message from {}: {}", addr, e);
                        break;
                    }
                }
                Ok(Message::Ping(data)) => {
                    debug!("Received ping from {}", addr);
                    let mut sender_guard = sender.lock().await;
                    if let Err(e) = sender_guard.send(Message::Pong(data)).await {
                        error!("Failed to send pong to {}: {}", addr, e);
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    debug!("Received pong from {}", addr);
                    // Pongs are handled automatically, just log
                }
                Ok(Message::Close(close_frame)) => {
                    info!("Received close frame from {}: {:?}", addr, close_frame);
                    break;
                }
                Ok(msg) => {
                    // Handle any other message types we haven't explicitly handled
                    warn!("Received unhandled message type from {}: {:?}", addr, msg);
                }
                Err(WsError::ConnectionClosed) => {
                    info!("Connection closed by client: {}", addr);
                    break;
                }
                Err(WsError::AlreadyClosed) => {
                    info!("Connection already closed: {}", addr);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error for {}: {}", addr, e);
                    break;
                }
            }
        }

        // Persist any traces the client left open so they remain queryable.
        server.finalize_connection_traces(&addr.to_string()).await;

        Ok(())
    }
}
