//! Monitoring Client Example - Real-time Trace Event Monitoring
//!
//! This example demonstrates:
//! - Connecting directly to the reflow_tracing server WebSocket
//! - Subscribing to real-time trace events with filters
//! - Displaying live trace data with formatting
//! - Querying historical traces and metrics

use anyhow::Result;
use clap::Parser;
use colored::*;
use futures_util::{SinkExt, StreamExt};
use futures::SinkExt as FuturesSinkExt;
use futures::StreamExt as FuturesStreamExt;
use reflow_tracing_protocol::*;
use serde_json;
use std::collections::HashMap;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, WebSocketStream, MaybeTlsStream};
use tracing::{info, warn, error, debug};
use url::Url;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Tracing server URL
    #[arg(short, long, default_value = "ws://127.0.0.1:8080")]
    server_url: String,

    /// Filter by flow IDs (comma-separated)
    #[arg(short, long)]
    flow_ids: Option<String>,

    /// Filter by actor IDs (comma-separated)
    #[arg(short, long)]
    actor_ids: Option<String>,

    /// Filter by event types (comma-separated)
    #[arg(short, long)]
    event_types: Option<String>,

    /// Show server metrics
    #[arg(short, long)]
    metrics: bool,

    /// Query historical traces instead of live monitoring
    #[arg(short, long)]
    query: bool,

    /// Limit for historical queries
    #[arg(short, long, default_value_t = 100)]
    limit: usize,
}

type WsStream = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct TraceMonitor {
    ws_stream: WsStream,
    stats: MonitoringStats,
}

#[derive(Default)]
struct MonitoringStats {
    events_received: u64,
    events_by_type: HashMap<TraceEventType, u64>,
    actors_seen: HashMap<String, u64>,
    flows_seen: HashMap<String, u64>,
    start_time: Option<chrono::DateTime<chrono::Utc>>,
}

impl TraceMonitor {
    async fn new(server_url: &str) -> Result<Self> {
        let url = Url::parse(server_url)?;
        let (ws_stream, _) = connect_async(url).await?;
        
        info!("🔗 Connected to tracing server: {}", server_url.yellow());
        
        Ok(Self {
            ws_stream,
            stats: MonitoringStats {
                start_time: Some(chrono::Utc::now()),
                ..Default::default()
            },
        })
    }

    async fn subscribe_to_events(&mut self, filters: SubscriptionFilters) -> Result<()> {
        let request = TracingRequest::Subscribe { filters };
        let message = serde_json::to_string(&request)?;
        
        self.ws_stream.send(Message::Text(message)).await?;
        info!("📡 Subscribed to trace events");
        
        Ok(())
    }

    async fn query_traces(&mut self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        let request = TracingRequest::QueryTraces { query };
        let message = serde_json::to_string(&request)?;
        
        self.ws_stream.send(Message::Text(message)).await?;
        
        // Wait for response
        while let Some(msg) = self.ws_stream.next().await {
            match msg? {
                Message::Text(text) => {
                    if let Ok(response) = serde_json::from_str::<TracingResponse>(&text) {
                        match response {
                            TracingResponse::QueryResults { traces, .. } => {
                                info!("📊 Query returned {} traces", traces.len());
                                return Ok(traces);
                            }
                            TracingResponse::Error { message, .. } => {
                                error!("❌ Query error: {}", message);
                                return Ok(Vec::new());
                            }
                            _ => continue,
                        }
                    }
                }
                _ => continue,
            }
        }
        
        Ok(Vec::new())
    }

    async fn get_server_metrics(&mut self) -> Result<()> {
        // Send a metrics request using TracingRequest format
        let request = TracingRequest::Ping; // Use ping to test connectivity
        let message = serde_json::to_string(&request)?;
        
        self.ws_stream.send(Message::Text(message)).await?;
        info!("📤 Sent ping request to server");
        
        // Also try legacy metrics request
        let legacy_request = TraceMessage::GetMetrics;
        let legacy_message = serde_json::to_string(&legacy_request)?;
        self.ws_stream.send(Message::Text(legacy_message)).await?;
        info!("📤 Sent legacy metrics request to server");
        
        // Wait for any response
        let mut response_count = 0;
        while let Some(msg) = self.ws_stream.next().await {
            match msg? {
                Message::Text(text) => {
                    info!("📥 Received response: {}", text);
                    response_count += 1;
                    
                    // Try parsing as TracingResponse first
                    if let Ok(response) = serde_json::from_str::<TracingResponse>(&text) {
                        match response {
                            TracingResponse::Pong => {
                                println!("✅ Server is responsive (pong received)");
                            }
                            TracingResponse::Error { message, .. } => {
                                error!("❌ Server error: {}", message);
                            }
                            _ => {
                                println!("📊 Server response: {:?}", response);
                            }
                        }
                    }
                    // Try parsing as TraceResponse for legacy metrics
                    else if let Ok(response) = serde_json::from_str::<TraceResponse>(&text) {
                        match response {
                            TraceResponse::Metrics { data } => {
                                println!("✅ Server metrics: {}", serde_json::to_string_pretty(&data)?);
                                return Ok(());
                            }
                            TraceResponse::Error { message } => {
                                error!("❌ Metrics error: {}", message);
                            }
                            _ => {
                                println!("📊 Legacy response: {:?}", response);
                            }
                        }
                    } else {
                        warn!("🤷 Could not parse response: {}", text);
                    }
                    
                    // Stop after getting a few responses
                    if response_count >= 2 {
                        break;
                    }
                }
                _ => continue,
            }
        }
        
        Ok(())
    }

    async fn monitor_events(&mut self) -> Result<()> {
        info!("👀 Monitoring live trace events... (Press Ctrl+C to stop)");
        println!();
        
        while let Some(msg) = self.ws_stream.next().await {
            match msg? {
                Message::Text(text) => {
                    self.handle_message(&text).await?;
                }
                Message::Close(_) => {
                    warn!("🔌 WebSocket connection closed by server");
                    break;
                }
                _ => {}
            }
        }
        
        Ok(())
    }

    async fn handle_message(&mut self, text: &str) -> Result<()> {
        debug!("📥 Raw message received: {}", text);
        
        // Try TracingResponse first (new protocol)
        if let Ok(response) = serde_json::from_str::<TracingResponse>(text) {
            match response {
                TracingResponse::EventNotification { trace_id, event } => {
                    self.update_stats(&event);
                    self.display_event(&trace_id, &event);
                    return Ok(());
                }
                TracingResponse::Error { message, .. } => {
                    error!("❌ Server error: {}", message);
                    return Ok(());
                }
                TracingResponse::EventRecorded { success, error } => {
                    if success {
                        info!("✅ Subscription confirmed - now listening for events");
                    } else if let Some(err) = error {
                        error!("❌ Subscription error: {}", err);
                    }
                    return Ok(());
                }
                TracingResponse::Pong => {
                    debug!("🏓 Received pong from server");
                    return Ok(());
                }
                _ => {
                    info!("📥 Other TracingResponse: {:?}", response);
                    return Ok(());
                }
            }
        } 
        // Fallback to TraceResponse (old protocol)
        else if let Ok(response) = serde_json::from_str::<TraceResponse>(text) {
            match response {
                TraceResponse::Error { message } => {
                    error!("❌ Server error: {}", message);
                }
                TraceResponse::Metrics { data } => {
                    info!("📊 Server metrics: {}", serde_json::to_string_pretty(&data)?);
                }
                _ => {
                    info!("📥 Other TraceResponse: {:?}", response);
                }
            }
            return Ok(());
        }
        else {
            warn!("🤷 Unable to parse message as either TracingResponse or TraceResponse: {}", text);
        }
        
        Ok(())
    }

    fn update_stats(&mut self, event: &TraceEvent) {
        self.stats.events_received += 1;
        
        *self.stats.events_by_type.entry(event.event_type.clone()).or_insert(0) += 1;
        *self.stats.actors_seen.entry(event.actor_id.clone()).or_insert(0) += 1;
        
        // Extract flow ID from event if available
        if let Some(flow_id) = self.extract_flow_id(event) {
            *self.stats.flows_seen.entry(flow_id).or_insert(0) += 1;
        }
    }

    fn extract_flow_id(&self, _event: &TraceEvent) -> Option<String> {
        // In a real implementation, this would extract flow ID from event context
        Some("default-flow".to_string())
    }

    fn display_event(&self, trace_id: &TraceId, event: &TraceEvent) {
        let event_icon = match event.event_type {
            TraceEventType::ActorCreated => "🆕",
            TraceEventType::ActorStarted => "▶️",
            TraceEventType::ActorCompleted => "✅",
            TraceEventType::ActorFailed => "❌",
            TraceEventType::MessageSent => "📤",
            TraceEventType::MessageReceived => "📥",
            TraceEventType::StateChanged => "🔄",
            TraceEventType::PortConnected => "🔗",
            TraceEventType::PortDisconnected => "❌",
            TraceEventType::NetworkEvent => "🌐",
            TraceEventType::DataFlow{..} => "➡️",
        };

        let event_color = match event.event_type {
            TraceEventType::ActorCreated | TraceEventType::ActorStarted => "green",
            TraceEventType::ActorCompleted => "blue",
            TraceEventType::ActorFailed => "red",
            TraceEventType::MessageSent | TraceEventType::MessageReceived => "yellow",
            TraceEventType::StateChanged => "magenta",
            TraceEventType::PortConnected | TraceEventType::PortDisconnected => "cyan",
            TraceEventType::NetworkEvent => "white",
            TraceEventType::DataFlow{..} => "orange"
        };

        let timestamp = event.timestamp.format("%H:%M:%S%.3f");
        let actor_id = if event.actor_id.len() > 15 {
            format!("{}...", &event.actor_id[..12])
        } else {
            event.actor_id.clone()
        };

        println!(
            "{} {} {} {} {} {}",
            event_icon,
            timestamp.to_string().bright_black(),
            format!("{:?}", event.event_type).color(event_color),
            actor_id.bright_white(),
            format!("trace:{}", &trace_id.to_string()[..8]).bright_black(),
            self.format_event_details(event).bright_black()
        );
    }

    fn format_event_details(&self, event: &TraceEvent) -> String {
        let mut details = Vec::new();
        
        if let Some(port) = &event.data.port {
            details.push(format!("port:{}", port));
        }
        
        if let Some(message) = &event.data.message {
            details.push(format!("msg:{} ({} bytes)", message.message_type, message.size_bytes));
        }
        
        if let Some(error) = &event.data.error {
            details.push(format!("error:{}", error));
        }
        
        if details.is_empty() {
            String::new()
        } else {
            format!("| {}", details.join(" "))
        }
    }

    fn display_stats(&self) {
        if let Some(start_time) = self.stats.start_time {
            let duration = chrono::Utc::now().signed_duration_since(start_time);
            let duration_secs = duration.num_seconds() as f64;
            let events_per_sec = if duration_secs > 0.0 {
                self.stats.events_received as f64 / duration_secs
            } else {
                0.0
            };

            println!();
            println!("{}", "📊 Monitoring Statistics".bold().blue());
            println!("Duration: {:.1}s", duration_secs);
            println!("Events received: {}", self.stats.events_received.to_string().green());
            println!("Events/sec: {:.2}", events_per_sec.to_string().yellow());
            println!("Unique actors: {}", self.stats.actors_seen.len().to_string().cyan());
            println!("Unique flows: {}", self.stats.flows_seen.len().to_string().magenta());
            
            if !self.stats.events_by_type.is_empty() {
                println!("\n{}", "Event Types:".bold());
                for (event_type, count) in &self.stats.events_by_type {
                    println!("  {:?}: {}", event_type, count);
                }
            }
            
            if !self.stats.actors_seen.is_empty() {
                println!("\n{}", "Most Active Actors:".bold());
                let mut sorted_actors: Vec<_> = self.stats.actors_seen.iter().collect();
                sorted_actors.sort_by(|a, b| b.1.cmp(a.1));
                for (actor_id, count) in sorted_actors.iter().take(5) {
                    println!("  {}: {}", actor_id, count);
                }
            }
        }
    }
}

fn parse_filters(args: &Args) -> SubscriptionFilters {
    let flow_ids = args.flow_ids.as_ref().map(|s| {
        s.split(',').map(|id| FlowId::new(id.trim())).collect()
    });
    
    let actor_ids = args.actor_ids.as_ref().map(|s| {
        s.split(',').map(|id| id.trim().to_string()).collect()
    });
    
    let event_types = args.event_types.as_ref().map(|s| {
        s.split(',').filter_map(|t| {
            match t.trim().to_lowercase().as_str() {
                "created" => Some(TraceEventType::ActorCreated),
                "started" => Some(TraceEventType::ActorStarted),
                "completed" => Some(TraceEventType::ActorCompleted),
                "failed" => Some(TraceEventType::ActorFailed),
                "message_sent" => Some(TraceEventType::MessageSent),
                "message_received" => Some(TraceEventType::MessageReceived),
                "state_changed" => Some(TraceEventType::StateChanged),
                "port_connected" => Some(TraceEventType::PortConnected),
                "port_disconnected" => Some(TraceEventType::PortDisconnected),
                "network" => Some(TraceEventType::NetworkEvent),
                _ => None,
            }
        }).collect()
    });
    
    SubscriptionFilters {
        flow_ids,
        actor_ids,
        event_types,
        status_filter: None,
    }
}

fn create_historical_query(args: &Args) -> TraceQuery {
    let flow_id = args.flow_ids.as_ref()
        .and_then(|s| s.split(',').next())
        .map(|id| FlowId::new(id.trim()));
    
    TraceQuery {
        flow_id,
        execution_id: None,
        time_range: None, // Could add time range parsing
        status: None,
        actor_filter: args.actor_ids.as_ref()
            .and_then(|s| s.split(',').next())
            .map(|id| id.trim().to_string()),
        limit: Some(args.limit),
        offset: None,
    }
}

async fn display_historical_traces(traces: Vec<FlowTrace>) {
    println!("{}", "📚 Historical Traces".bold().blue());
    
    if traces.is_empty() {
        println!("No traces found matching the query criteria.");
        return;
    }
    
    for trace in traces {
        let status_icon = match trace.status {
            ExecutionStatus::Pending => "⏳",
            ExecutionStatus::Running => "🏃",
            ExecutionStatus::Completed => "✅",
            ExecutionStatus::Failed { .. } => "❌",
            ExecutionStatus::Cancelled => "🚫",
        };
        
        let duration = trace.end_time
            .map(|end| end.signed_duration_since(trace.start_time))
            .map(|d| format!("{:.2}s", d.num_milliseconds() as f64 / 1000.0))
            .unwrap_or_else(|| "ongoing".to_string());
            
        println!(
            "{} {} {} {} events={} duration={}",
            status_icon,
            trace.trace_id.to_string().bright_white(),
            trace.flow_id.to_string().cyan(),
            format!("{:?}", trace.status).color(match trace.status {
                ExecutionStatus::Completed => "green",
                ExecutionStatus::Failed { .. } => "red",
                ExecutionStatus::Running => "yellow",
                _ => "white",
            }),
            trace.events.len().to_string().magenta(),
            duration.bright_black()
        );
        
        // Show first few events
        for event in trace.events.iter().take(3) {
            println!(
                "  {} {:?} {}",
                event.timestamp.format("%H:%M:%S").to_string().bright_black(),
                event.event_type,
                event.actor_id.bright_white()
            );
        }
        
        if trace.events.len() > 3 {
            println!("  ... and {} more events", trace.events.len() - 3);
        }
        
        println!();
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("monitoring_client=info")
        .init();
    
    println!("{}", "👁️  Reflow Tracing Monitor".bold().blue());
    println!("Server: {}", args.server_url.yellow());
    
    if args.query {
        println!("Mode: {}", "Historical Query".green());
    } else {
        println!("Mode: {}", "Live Monitoring".green());
    }
    println!();

    let mut monitor = TraceMonitor::new(&args.server_url).await?;
    
    if args.metrics {
        monitor.get_server_metrics().await?;
        return Ok(());
    }
    
    if args.query {
        let query = create_historical_query(&args);
        info!("🔍 Querying historical traces...");
        let traces = monitor.query_traces(query).await?;
        display_historical_traces(traces).await;
    } else {
        let filters = parse_filters(&args);
        info!("📡 Setting up live monitoring with filters...");
        monitor.subscribe_to_events(filters).await?;
        
        // Set up Ctrl+C handler
        let monitor_task = tokio::spawn(async move {
            if let Err(e) = monitor.monitor_events().await {
                error!("Monitoring error: {}", e);
            }
        });
        
        // Wait for Ctrl+C
        tokio::signal::ctrl_c().await?;
        println!();
        info!("🛑 Shutting down monitor...");
        
        monitor_task.abort();
        
        // Note: In a real implementation, we'd get stats from the monitor
        // before aborting the task
        println!("{}", "✅ Monitor stopped".green());
    }
    
    Ok(())
}
