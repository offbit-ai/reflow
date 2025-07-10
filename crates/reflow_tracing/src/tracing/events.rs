//! Event handling and processing for tracing framework

use crate::protocol::{TraceEvent, TraceEventData, TraceEventType, TraceId, PerformanceMetrics, EventId};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::{DateTime, Utc};

/// Event buffer for high-throughput event processing
pub struct EventBuffer {
    events: VecDeque<TraceEvent>,
    capacity: usize,
    total_events: u64,
    dropped_events: u64,
}

impl EventBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(capacity),
            capacity,
            total_events: 0,
            dropped_events: 0,
        }
    }

    pub fn push(&mut self, event: TraceEvent) -> bool {
        self.total_events += 1;
        
        if self.events.len() >= self.capacity {
            self.events.pop_front();
            self.dropped_events += 1;
        }
        
        self.events.push_back(event);
        true
    }

    pub fn pop(&mut self) -> Option<TraceEvent> {
        self.events.pop_front()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn stats(&self) -> EventBufferStats {
        EventBufferStats {
            total_events: self.total_events,
            dropped_events: self.dropped_events,
            current_size: self.events.len(),
            capacity: self.capacity,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventBufferStats {
    pub total_events: u64,
    pub dropped_events: u64,
    pub current_size: usize,
    pub capacity: usize,
}

/// Event filter for selective processing
pub struct EventFilter {
    pub include_types: Option<Vec<TraceEventType>>,
    pub exclude_types: Option<Vec<TraceEventType>>,
    pub include_actors: Option<Vec<String>>,
    pub exclude_actors: Option<Vec<String>>,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

impl EventFilter {
    pub fn matches(&self, event: &TraceEvent) -> bool {
        // Check event type filters
        if let Some(ref include_types) = self.include_types {
            if !include_types.contains(&event.event_type) {
                return false;
            }
        }

        if let Some(ref exclude_types) = self.exclude_types {
            if exclude_types.contains(&event.event_type) {
                return false;
            }
        }

        // Check actor filters
        if let Some(ref include_actors) = self.include_actors {
            if !include_actors.contains(&event.actor_id) {
                return false;
            }
        }

        if let Some(ref exclude_actors) = self.exclude_actors {
            if exclude_actors.contains(&event.actor_id) {
                return false;
            }
        }

        // Check time range
        if let Some((start, end)) = self.time_range {
            if event.timestamp < start || event.timestamp > end {
                return false;
            }
        }

        true
    }
}

/// Event processor for handling trace events
pub struct EventProcessor {
    buffer: Arc<RwLock<EventBuffer>>,
    filter: Option<EventFilter>,
    handlers: Vec<Box<dyn EventHandler>>,
}

pub trait EventHandler: Send + Sync {
    fn handle_event(&self, event: &TraceEvent) -> anyhow::Result<()>;
    fn name(&self) -> &str;
}

impl EventProcessor {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: Arc::new(RwLock::new(EventBuffer::new(buffer_size))),
            filter: None,
            handlers: Vec::new(),
        }
    }

    pub fn with_filter(mut self, filter: EventFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    pub fn add_handler<H: EventHandler + 'static>(mut self, handler: H) -> Self {
        self.handlers.push(Box::new(handler));
        self
    }

    pub async fn process_event(&self, event: TraceEvent) -> anyhow::Result<()> {
        // Apply filter if configured
        if let Some(ref filter) = self.filter {
            if !filter.matches(&event) {
                return Ok(());
            }
        }

        // Add to buffer
        {
            let mut buffer = self.buffer.write().await;
            buffer.push(event.clone());
        }

        // Process with handlers
        for handler in &self.handlers {
            if let Err(e) = handler.handle_event(&event) {
                eprintln!("Handler '{}' failed to process event: {}", handler.name(), e);
            }
        }

        Ok(())
    }

    pub async fn get_buffer_stats(&self) -> EventBufferStats {
        let buffer = self.buffer.read().await;
        buffer.stats()
    }

    pub async fn drain_events(&self) -> Vec<TraceEvent> {
        let mut buffer = self.buffer.write().await;
        let mut events = Vec::new();
        while let Some(event) = buffer.pop() {
            events.push(event);
        }
        events
    }
}

/// Built-in event handlers

/// Console logger handler
pub struct ConsoleLogHandler {
    name: String,
}

impl ConsoleLogHandler {
    pub fn new() -> Self {
        Self {
            name: "ConsoleLogHandler".to_string(),
        }
    }
}

impl EventHandler for ConsoleLogHandler {
    fn handle_event(&self, event: &TraceEvent) -> anyhow::Result<()> {
        println!(
            "[{}] {} - {} ({}): {:?}",
            event.timestamp.format("%Y-%m-%d %H:%M:%S%.3f"),
            event.event_type,
            event.actor_id,
            event.event_id.0,
            event.data.performance_metrics
        );
        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Metrics collector handler
pub struct MetricsHandler {
    name: String,
}

impl MetricsHandler {
    pub fn new() -> Self {
        Self {
            name: "MetricsHandler".to_string(),
        }
    }
}

impl EventHandler for MetricsHandler {
    fn handle_event(&self, event: &TraceEvent) -> anyhow::Result<()> {
        // Collect metrics from the event
        let metrics = &event.data.performance_metrics;
        
        // In a real implementation, you would send these to your metrics system
        // For now, we'll just log them
        tracing::info!(
            event_type = ?event.event_type,
            actor_id = %event.actor_id,
            execution_time_ns = metrics.execution_time_ns,
            memory_usage_bytes = metrics.memory_usage_bytes,
            cpu_usage_percent = metrics.cpu_usage_percent,
            "Event metrics collected"
        );

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

/// Alert handler for performance issues
pub struct AlertHandler {
    name: String,
    memory_threshold_mb: usize,
    execution_time_threshold_ms: u64,
}

impl AlertHandler {
    pub fn new(memory_threshold_mb: usize, execution_time_threshold_ms: u64) -> Self {
        Self {
            name: "AlertHandler".to_string(),
            memory_threshold_mb,
            execution_time_threshold_ms,
        }
    }
}

impl EventHandler for AlertHandler {
    fn handle_event(&self, event: &TraceEvent) -> anyhow::Result<()> {
        let metrics = &event.data.performance_metrics;
        
        // Check memory usage
        let memory_mb = metrics.memory_usage_bytes / (1024 * 1024);
        if memory_mb > self.memory_threshold_mb {
            tracing::warn!(
                actor_id = %event.actor_id,
                memory_mb = memory_mb,
                threshold_mb = self.memory_threshold_mb,
                "High memory usage detected"
            );
        }

        // Check execution time
        let execution_time_ms = metrics.execution_time_ns / 1_000_000;
        if execution_time_ms > self.execution_time_threshold_ms {
            tracing::warn!(
                actor_id = %event.actor_id,
                execution_time_ms = execution_time_ms,
                threshold_ms = self.execution_time_threshold_ms,
                "Slow execution detected"
            );
        }

        Ok(())
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// Helper functions for creating events
pub fn create_actor_started_event(actor_id: String, performance_metrics: PerformanceMetrics) -> TraceEvent {
    TraceEvent {
        event_id: EventId::new(),
        timestamp: chrono::Utc::now(),
        event_type: TraceEventType::ActorStarted,
        actor_id,
        data: TraceEventData {
            port: None,
            message: None,
            state_diff: None,
            error: None,
            performance_metrics,
            custom_attributes: std::collections::HashMap::new(),
        },
        causality: crate::protocol::CausalityInfo {
            parent_event_id: None,
            root_cause_event_id: EventId::new(),
            dependency_chain: Vec::new(),
            span_id: uuid::Uuid::new_v4().to_string(),
        },
    }
}

pub fn create_message_sent_event(
    actor_id: String,
    port: String,
    message_snapshot: Option<crate::protocol::MessageSnapshot>,
    performance_metrics: PerformanceMetrics,
) -> TraceEvent {
    TraceEvent {
        event_id: EventId::new(),
        timestamp: chrono::Utc::now(),
        event_type: TraceEventType::MessageSent,
        actor_id,
        data: TraceEventData {
            port: Some(port),
            message: message_snapshot,
            state_diff: None,
            error: None,
            performance_metrics,
            custom_attributes: std::collections::HashMap::new(),
        },
        causality: crate::protocol::CausalityInfo {
            parent_event_id: None,
            root_cause_event_id: EventId::new(),
            dependency_chain: Vec::new(),
            span_id: uuid::Uuid::new_v4().to_string(),
        },
    }
}
