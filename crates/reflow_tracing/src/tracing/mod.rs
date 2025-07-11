//! Comprehensive tracing framework for flow execution with WebSocket integration
//! 
//! This module provides the core tracing functionality that was moved from reflow_network
//! to resolve SQLite dependency conflicts.

use anyhow::Result;
use chrono::{DateTime, Utc};
use prometheus::{HistogramOpts, Opts};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::protocol::{
    TraceId, FlowId, ExecutionId, FlowVersion, ExecutionStatus, TraceEvent, TraceEventType,
    TraceEventData, EventId, MessageSnapshot, StateDiff, StateDiffType, PerformanceMetrics,
    CausalityInfo, FlowTrace, TraceMetadata, TraceQuery
};

pub mod events;
pub mod replay;
pub mod versioning;

use crate::storage::TraceStorage;

/// Enhanced network event with tracing context
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum EnhancedNetworkEvent {
    FlowTrace {
        trace_id: TraceId,
        event: TraceEvent,
    },
    ActorLifecycle {
        trace_id: TraceId,
        actor_id: String,
        lifecycle_event: ActorLifecycleEvent,
    },
    PerformanceAlert {
        trace_id: TraceId,
        alert_type: PerformanceAlertType,
        threshold: f64,
        actual_value: f64,
    },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ActorLifecycleEvent {
    Created,
    Initialized,
    Started,
    Suspended,
    Resumed,
    Stopped,
    Failed { error: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum PerformanceAlertType {
    HighMemoryUsage,
    SlowExecution,
    QueueBacklog,
    ErrorRate,
}

/// Main tracing framework
pub struct FlowTracingFramework {
    storage: Arc<dyn TraceStorage>,
    config: TracingConfig,
    event_buffer: flume::Sender<EnhancedNetworkEvent>,
    metrics_collector: MetricsCollector,
}

#[derive(Debug, Clone)]
pub struct TracingConfig {
    pub enable_state_diffs: bool,
    pub enable_message_snapshots: bool,
    pub compression_enabled: bool,
    pub max_trace_size_mb: usize,
    pub retention_days: u32,
    pub performance_sampling_rate: f32,
    pub metrics_collector_config: MetricsCollectorConfig,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enable_state_diffs: true,
            enable_message_snapshots: true,
            compression_enabled: true,
            max_trace_size_mb: 100,
            retention_days: 30,
            performance_sampling_rate: 0.1, // 10% sampling
            metrics_collector_config: MetricsCollectorConfig::default(),
        }
    }
}

/// Metrics collection for observability
pub struct MetricsCollector {
    // Prometheus-style metrics
    pub flow_executions_total: prometheus::CounterVec,
    pub flow_execution_duration: prometheus::HistogramVec,
    pub actor_execution_duration: prometheus::HistogramVec,
    pub message_queue_depth: prometheus::GaugeVec,
    pub memory_usage: prometheus::Gauge,
}

#[derive(Debug, Clone)]
pub struct MetricsCollectorConfig {
    pub executions_total_config: (Opts, Vec<String>),
    pub execution_duration_config: (HistogramOpts, Vec<String>),
    pub actor_execution_duration_config: (HistogramOpts, Vec<String>),
    pub message_queue_depth_config: (Opts, Vec<String>),
    pub memory_usage_config: (String, String),
}

impl Default for MetricsCollectorConfig {
    fn default() -> Self {
        Self {
            executions_total_config: (
                Opts::new("executions_total", "The total number of flow executions"),
                Default::default(),
            ),
            execution_duration_config: (
                HistogramOpts::new("executions_duration", "Duration of flow execution"),
                Default::default(),
            ),
            actor_execution_duration_config: (
                HistogramOpts::new("actor_execution_duration", "Duration of an actor execution"),
                Default::default(),
            ),
            message_queue_depth_config: (
                Opts::new("message_queue_depth", "Measure of message queue depth"),
                Default::default(),
            ),
            memory_usage_config: ("memory_usage".to_string(), "Memory usage".to_string()),
        }
    }
}

impl MetricsCollector {
    pub fn new(config: MetricsCollectorConfig) -> Result<Self> {
        Ok(Self {
            flow_executions_total: prometheus::CounterVec::new(
                config.executions_total_config.0,
                config
                    .executions_total_config
                    .1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
            )?,
            flow_execution_duration: prometheus::HistogramVec::new(
                config.execution_duration_config.0,
                config
                    .execution_duration_config
                    .1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
            )?,
            actor_execution_duration: prometheus::HistogramVec::new(
                config.actor_execution_duration_config.0,
                config
                    .actor_execution_duration_config
                    .1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
            )?,
            message_queue_depth: prometheus::GaugeVec::new(
                config.message_queue_depth_config.0,
                config
                    .message_queue_depth_config
                    .1
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .as_slice(),
            )?,
            memory_usage: prometheus::Gauge::new(
                config.memory_usage_config.0,
                config.memory_usage_config.1,
            )?,
        })
    }
}

impl FlowTracingFramework {
    pub fn new(storage: Arc<dyn TraceStorage>, config: TracingConfig) -> Result<Self> {
        let (sender, receiver) = flume::unbounded();

        // Start background event processor
        let storage_clone = storage.clone();
        tokio::spawn(async move {
            Self::process_events(receiver, storage_clone).await;
        });

        let metrics_collector_config = config.metrics_collector_config.clone();

        Ok(Self {
            storage,
            config,
            event_buffer: sender,
            metrics_collector: MetricsCollector::new(metrics_collector_config)?,
        })
    }

    /// Start tracing a flow execution
    pub fn start_flow_trace(&self, flow_id: FlowId, version: FlowVersion) -> TraceId {
        let trace_id = TraceId::new();
        let execution_id = ExecutionId::new();

        let trace = FlowTrace {
            trace_id: trace_id.clone(),
            flow_id,
            execution_id,
            version,
            start_time: Utc::now(),
            end_time: None,
            status: ExecutionStatus::Running,
            events: Vec::new(),
            metadata: self.create_trace_metadata(),
        };

        // Store initial trace (async operation, spawn task)
        let storage = self.storage.clone();
        tokio::spawn(async move {
            if let Err(e) = storage.store_trace(trace).await {
                eprintln!("Failed to store initial trace: {}", e);
            }
        });

        trace_id
    }

    /// Record a trace event
    pub fn record_event(&self, trace_id: TraceId, event: TraceEvent) {
        let enhanced_event = EnhancedNetworkEvent::FlowTrace { trace_id, event };

        if let Err(e) = self.event_buffer.send(enhanced_event) {
            eprintln!("Failed to buffer trace event: {}", e);
        }
    }

    /// Process events in background
    async fn process_events(
        receiver: flume::Receiver<EnhancedNetworkEvent>,
        storage: Arc<dyn TraceStorage>,
    ) {
        while let Ok(event) = receiver.recv_async().await {
            match event {
                EnhancedNetworkEvent::FlowTrace { trace_id, event } => {
                    // Update trace with new event
                    if let Ok(Some(mut trace)) = storage.get_trace(trace_id).await {
                        trace.events.push(event);
                        let _ = storage.store_trace(trace).await;
                    }
                }
                _ => {
                    // Handle other event types
                }
            }
        }
    }

    fn create_trace_metadata(&self) -> TraceMetadata {
        TraceMetadata {
            user_id: None, // Can be populated from context
            session_id: None,
            environment: std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string()),
            hostname: hostname::get()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            process_id: std::process::id(),
            thread_id: format!("{:?}", std::thread::current().id()),
            tags: HashMap::new(),
        }
    }

    /// Get trace by ID
    pub async fn get_trace(&self, trace_id: TraceId) -> Result<Option<FlowTrace>> {
        self.storage.get_trace(trace_id).await
    }

    /// Query traces
    pub async fn query_traces(&self, query: TraceQuery) -> Result<Vec<FlowTrace>> {
        self.storage.query_traces(query).await
    }

    /// Get storage stats
    pub async fn get_stats(&self) -> Result<crate::storage::StorageStats> {
        self.storage.get_stats().await
    }
}

/// Helper function to create trace event with performance metrics
pub fn create_trace_event(
    event_type: TraceEventType,
    actor_id: String,
    data: TraceEventData,
) -> TraceEvent {
    TraceEvent {
        event_id: EventId::new(),
        timestamp: Utc::now(),
        event_type,
        actor_id,
        data,
        causality: CausalityInfo {
            parent_event_id: None,
            root_cause_event_id: EventId::new(),
            dependency_chain: Vec::new(),
            span_id: Uuid::new_v4().to_string(),
        },
    }
}

/// Helper function to create performance metrics
pub fn create_performance_metrics(execution_time: std::time::Duration) -> PerformanceMetrics {
    PerformanceMetrics {
        execution_time_ns: execution_time.as_nanos() as u64,
        memory_usage_bytes: 0, // Would need actual measurement
        cpu_usage_percent: 0.0,
        queue_depth: 0,
        throughput_msgs_per_sec: 0.0,
    }
}
