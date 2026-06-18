use crate::{bridge::RemoteConnection, message::Message, network::Network};
use anyhow::Result;
use parking_lot::RwLock;
use reflow_tracing_protocol::{EventId, FlowId, TraceId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct MessageRouter {
    remote_actor_registry: Arc<RwLock<HashMap<String, RemoteActorInfo>>>,
    connection_pool: Arc<RwLock<HashMap<String, RemoteConnection>>>,
    local_network: Arc<RwLock<Option<Arc<RwLock<Network>>>>>,
    local_network_id: Arc<RwLock<String>>,
}

unsafe impl Sync for MessageRouter {}
unsafe impl Send for MessageRouter {}

#[derive(Debug, Clone)]
pub struct RemoteActorInfo {
    pub actor_id: String,
    pub network_id: String,
    pub capabilities: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoteMessage {
    pub message_id: String,
    pub source_network: String,
    pub source_actor: String,
    pub target_network: String,
    pub target_actor: String,
    pub target_port: String,
    pub payload: Message,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// Trace context propagated across the process boundary. `Option` +
    /// `serde(default)` keeps the wire backward-compatible: older peers that
    /// don't send this field simply deserialize to `None`.
    #[serde(default)]
    pub trace_context: Option<TraceContext>,
}

/// Trace context carried on a [`RemoteMessage`] so a flow that spans multiple
/// processes aggregates into one end-to-end trace on a shared collector.
///
/// The originating network's session `trace_id` propagates unchanged; the
/// receiving network records the cross-process hop under that same id, so —
/// with every network pointed at one tracing server — both processes' events
/// land in the same `FlowTrace`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceContext {
    /// The shared trace id the whole flow is recorded under.
    pub trace_id: TraceId,
    /// The originating flow id, when available.
    #[serde(default)]
    pub flow_id: Option<FlowId>,
    /// Span id of the sending hop, for linking the inbound event to its parent.
    pub parent_span_id: String,
    /// Event id that caused this hop, when available.
    #[serde(default)]
    pub parent_event_id: Option<EventId>,
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageRouter {
    pub fn new() -> Self {
        MessageRouter {
            remote_actor_registry: Arc::new(RwLock::new(HashMap::new())),
            connection_pool: Arc::new(RwLock::new(HashMap::new())),
            local_network: Arc::new(RwLock::new(None)),
            local_network_id: Arc::new(RwLock::new("local".to_string())),
        }
    }

    pub fn with_connection_pool(
        connections: Arc<RwLock<HashMap<String, RemoteConnection>>>,
    ) -> Self {
        MessageRouter {
            remote_actor_registry: Arc::new(RwLock::new(HashMap::new())),
            connection_pool: connections,
            local_network: Arc::new(RwLock::new(None)),
            local_network_id: Arc::new(RwLock::new("local".to_string())),
        }
    }

    pub fn set_local_network(&self, network: Arc<RwLock<Network>>, network_id: String) {
        *self.local_network.write() = Some(network);
        *self.local_network_id.write() = network_id;
    }

    pub async fn route_message(
        &self,
        network_id: &str,
        actor_id: &str,
        port: &str,
        message: Message,
        source_actor: Option<&str>,
    ) -> Result<()> {
        let source_network = self.get_local_network_id().await?;
        let source_actor_id = source_actor.unwrap_or("unknown").to_string();

        tracing::info!(
            "📨 ROUTER: Routing message from {}::{} to {}::{} on port {}",
            source_network,
            source_actor_id,
            network_id,
            actor_id,
            port
        );

        // Create remote message, attaching the local network's session trace
        // context so the receiving process can record this hop under the same
        // trace id (unified, shared-collector tracing).
        let remote_message = RemoteMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            source_network,
            source_actor: source_actor_id,
            target_network: network_id.to_string(),
            target_actor: actor_id.to_string(),
            target_port: port.to_string(),
            payload: message,
            timestamp: chrono::Utc::now(),
            trace_context: self.local_trace_context(),
        };

        // Find connection for target network
        let connection = {
            let connections = self.connection_pool.read();
            tracing::info!(
                "🔍 ROUTER: Available connections: {:?}",
                connections.keys().collect::<Vec<_>>()
            );
            connections.get(network_id).cloned()
        };

        if let Some(connection) = connection {
            tracing::info!(
                "✅ ROUTER: Found connection for network {}, sending message {}",
                network_id,
                remote_message.message_id
            );
            match self.send_over_connection(&connection, remote_message).await {
                Ok(_) => {
                    tracing::info!("✅ ROUTER: Successfully sent message over connection");
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("❌ ROUTER: Failed to send message over connection: {}", e);
                    Err(e)
                }
            }
        } else {
            tracing::error!("❌ ROUTER: No connection to network: {}", network_id);
            Err(anyhow::anyhow!("No connection to network: {}", network_id))
        }
    }

    /// Build the trace context to attach to outbound remote messages from the
    /// local network's current session trace. `None` when tracing is disabled
    /// or no flow trace has been started.
    fn local_trace_context(&self) -> Option<TraceContext> {
        let guard = self.local_network.read();
        let network = guard.as_ref()?.read();
        let tracing = network.tracing_integration.as_ref()?;
        let trace_id = tracing.current_trace_id()?;
        Some(TraceContext {
            trace_id,
            flow_id: None,
            parent_span_id: uuid::Uuid::new_v4().to_string(),
            parent_event_id: None,
        })
    }

    /// Prepare the cross-process hop event for an inbound message, attributed to
    /// the propagated trace id so it aggregates with the origin's events on a
    /// shared collector. Returns the client + trace id + event to record, or
    /// `None` if the message carries no context or tracing is disabled.
    fn build_inbound_hop(
        &self,
        message: &RemoteMessage,
    ) -> Option<(
        Arc<reflow_tracing_protocol::client::TracingClient>,
        TraceId,
        reflow_tracing_protocol::TraceEvent,
    )> {
        use reflow_tracing_protocol::{MessageSnapshot, PerformanceMetrics, TraceEvent};

        let ctx = message.trace_context.as_ref()?;
        let guard = self.local_network.read();
        let network = guard.as_ref()?.read();
        let tracing = network.tracing_integration.as_ref()?;
        let client = tracing.client();

        let snapshot = MessageSnapshot::capture(
            message.payload.type_name(),
            &message.payload,
            client.capture_checksum(),
            client.capture_content(),
        );
        // The cross-process delivery, modeled as a data-flow from the remote
        // source actor into the local target actor.
        let mut event = TraceEvent::data_flow(
            message.source_actor.clone(),
            message.target_port.clone(),
            message.target_actor.clone(),
            message.target_port.clone(),
            snapshot,
            PerformanceMetrics::default(),
        );
        // Link this hop to the sending span from the originating process.
        event.causality.span_id = ctx.parent_span_id.clone();
        event.causality.parent_event_id = ctx.parent_event_id.clone();

        Some((client, ctx.trace_id.clone(), event))
    }

    pub async fn handle_incoming_message(
        &self,
        message: RemoteMessage,
    ) -> Result<(), anyhow::Error> {
        // Route to local network
        tracing::info!(
            "🎯 ROUTER: Routing message from {} to local actor: {} port: {}",
            message.source_network,
            message.target_actor,
            message.target_port
        );

        // Build the cross-process hop event (and grab the tracing client) before
        // the payload is delivered, so we can attribute it to the propagated
        // trace id once delivery succeeds.
        let hop = self.build_inbound_hop(&message);

        // Send message to local network. Scope the (non-async) network guard so
        // it is never held across the await below.
        let deliver = {
            let local_network_guard = self.local_network.read();
            if let Some(ref local_network_arc) = *local_network_guard {
                let network = local_network_arc.read();
                tracing::info!(
                    "🔍 ROUTER: Sending to local network, available actors: {:?}",
                    network.actors.keys().collect::<Vec<_>>()
                );
                network.send_to_actor(
                    &message.target_actor,
                    &message.target_port,
                    message.payload,
                )
            } else {
                tracing::error!("❌ ROUTER: No local network configured");
                return Err(anyhow::anyhow!("No local network configured"));
            }
        };

        match deliver {
            Ok(_) => {
                tracing::info!(
                    "✅ ROUTER: Successfully routed message to local actor {}",
                    message.target_actor
                );
            }
            Err(e) => {
                tracing::error!(
                    "❌ ROUTER: Failed to route message to local actor {}: {}",
                    message.target_actor,
                    e
                );
                return Err(e);
            }
        }

        // Attribute the inbound hop to the propagated trace (fire-and-forget so
        // tracing never blocks the data plane).
        if let Some((client, trace_id, event)) = hop {
            tokio::spawn(async move {
                let _ = client.record_event(trace_id, event).await;
            });
        }

        Ok(())
    }

    async fn send_over_connection(
        &self,
        connection: &RemoteConnection,
        message: RemoteMessage,
    ) -> Result<()> {
        tracing::info!("🔗 ROUTER: Serializing message {}", message.message_id);
        let serialized = match serde_json::to_string(&message) {
            Ok(s) => {
                tracing::info!("✅ ROUTER: Serialized message {} bytes", s.len());
                s
            }
            Err(e) => {
                tracing::error!("❌ ROUTER: Failed to serialize message: {}", e);
                return Err(e.into());
            }
        };

        tracing::info!(
            "📡 ROUTER: Sending message over WebSocket to {}",
            connection.network_id
        );

        // Send over WebSocket using the ConnectionWebSocket's send method
        match connection
            .websocket
            .send(tokio_tungstenite::tungstenite::Message::Text(
                serialized.into(),
            ))
            .await
        {
            Ok(_) => {
                tracing::info!(
                    "✅ ROUTER: Successfully sent message {} over WebSocket",
                    message.message_id
                );
                Ok(())
            }
            Err(e) => {
                tracing::error!("❌ ROUTER: Failed to send message over WebSocket: {}", e);
                Err(e.into())
            }
        }
    }

    async fn get_local_network_id(&self) -> Result<String> {
        Ok(self.local_network_id.read().clone())
    }

    /// Returns actor info for all actors registered in the local network.
    pub fn get_local_actor_list(&self) -> Vec<crate::bridge::ActorInfo> {
        let local_network_guard = self.local_network.read();
        if let Some(ref local_network_arc) = *local_network_guard {
            let network = local_network_arc.read();
            network
                .actors
                .keys()
                .map(|actor_id| crate::bridge::ActorInfo {
                    actor_id: actor_id.clone(),
                    capabilities: vec!["actor_messaging".to_string()],
                    description: None,
                })
                .collect()
        } else {
            vec![]
        }
    }

    pub async fn register_remote_actor(
        &self,
        actor_id: &str,
        remote_network_id: &str,
        capabilities: Option<Vec<String>>,
    ) -> Result<(), anyhow::Error> {
        let remote_info = RemoteActorInfo {
            actor_id: actor_id.to_string(),
            network_id: remote_network_id.to_string(),
            capabilities: capabilities.unwrap_or_else(|| vec!["actor_messaging".to_string()]),
        };

        self.remote_actor_registry
            .write()
            .insert(actor_id.to_string(), remote_info);

        tracing::info!(
            "Registered remote actor {} from network {}",
            actor_id,
            remote_network_id
        );
        Ok(())
    }
}
