//! # Reflow Tracing Protocol
//!
//! Shared protocol definitions for communication between Reflow tracing clients and servers.
//! This crate provides lightweight message types without heavy dependencies.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use derive_more::Display;

pub mod client;

/// Unique identifier for a trace
#[derive(Debug, Display, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TraceId(pub Uuid);

/// Flow identifier
#[derive(Debug, Display, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FlowId(pub String);

/// Execution instance identifier
#[derive(Debug, Display, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ExecutionId(pub Uuid);

/// Event identifier
#[derive(Debug, Display, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct EventId(pub Uuid);

/// Flow version for versioning support
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub git_hash: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// WebSocket protocol messages for tracing communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TracingRequest {
    /// Start a new flow trace
    StartTrace {
        flow_id: FlowId,
        version: FlowVersion,
    },
    /// Record a trace event
    RecordEvent {
        trace_id: TraceId,
        event: TraceEvent,
    },
    /// Get a specific trace by ID
    GetTrace {
        trace_id: TraceId,
    },
    /// Query traces with filters
    QueryTraces {
        query: TraceQuery,
    },
    /// Get all versions of a flow
    GetFlowVersions {
        flow_id: FlowId,
    },
    /// Health check
    Ping,
    /// Subscribe to real-time trace events
    Subscribe {
        filters: SubscriptionFilters,
    },
    /// Unsubscribe from real-time events
    Unsubscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TracingResponse {
    /// Response to StartTrace
    TraceStarted {
        trace_id: TraceId,
    },
    /// Response to RecordEvent
    EventRecorded {
        success: bool,
        error: Option<String>,
    },
    /// Response to GetTrace
    TraceData {
        trace: Option<FlowTrace>,
    },
    /// Response to QueryTraces
    QueryResults {
        traces: Vec<FlowTrace>,
        total_count: usize,
    },
    /// Response to GetFlowVersions
    FlowVersions {
        versions: Vec<FlowVersion>,
    },
    /// Response to Ping
    Pong,
    /// Real-time event notification
    EventNotification {
        trace_id: TraceId,
        event: TraceEvent,
    },
    /// Error response
    Error {
        message: String,
        code: ErrorCode,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ErrorCode {
    NotFound,
    InvalidRequest,
    StorageError,
    SerializationError,
    Unauthorized,
    InternalError,
}

/// Subscription filters for real-time events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionFilters {
    pub flow_ids: Option<Vec<FlowId>>,
    pub actor_ids: Option<Vec<String>>,
    pub event_types: Option<Vec<TraceEventType>>,
    pub status_filter: Option<Vec<ExecutionStatus>>,
}

/// Enhanced flow execution trace with comprehensive observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowTrace {
    pub trace_id: TraceId,
    pub flow_id: FlowId,
    pub execution_id: ExecutionId,
    pub version: FlowVersion,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub status: ExecutionStatus,
    pub events: Vec<TraceEvent>,
    pub metadata: TraceMetadata,
}

/// Execution status tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Completed,
    Failed { error: String },
    Cancelled,
}



/// Comprehensive trace event for all actor interactions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub event_type: TraceEventType,
    pub actor_id: String,
    pub data: TraceEventData,
    pub causality: CausalityInfo,
}

/// Types of trace events for comprehensive coverage
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum TraceEventType {
    ActorCreated,
    ActorStarted,
    ActorCompleted,
    ActorFailed,
    MessageSent,
    MessageReceived,
    StateChanged,
    PortConnected,
    PortDisconnected,
    DataFlow {
        to_actor: String,
        to_port: String,
    },
    NetworkEvent,
}

/// Event data with rich context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEventData {
    pub port: Option<String>,
    pub message: Option<MessageSnapshot>,
    pub state_diff: Option<StateDiff>,
    pub error: Option<String>,
    pub performance_metrics: PerformanceMetrics,
    pub custom_attributes: HashMap<String, serde_json::Value>,
}

/// Snapshot of message data for replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSnapshot {
    pub message_type: String,
    pub size_bytes: usize,
    pub checksum: String,
    pub serialized_data: Vec<u8>, // Compressed message data
}

/// State differences for time travel debugging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDiff {
    pub before: Option<Vec<u8>>, // Serialized state before
    pub after: Vec<u8>,          // Serialized state after
    pub diff_type: StateDiffType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StateDiffType {
    Full,
    Incremental,
    MemoryOnly,
}

/// Performance metrics for observability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub execution_time_ns: u64,
    pub memory_usage_bytes: usize,
    pub cpu_usage_percent: f32,
    pub queue_depth: usize,
    pub throughput_msgs_per_sec: f64,
}

/// Causality information for dependency tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalityInfo {
    pub parent_event_id: Option<EventId>,
    pub root_cause_event_id: EventId,
    pub dependency_chain: Vec<EventId>,
    pub span_id: String, // For distributed tracing integration
}

/// Metadata for trace context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceMetadata {
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub environment: String,
    pub hostname: String,
    pub process_id: u32,
    pub thread_id: String,
    pub tags: HashMap<String, String>,
}

/// Query interface for traces
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceQuery {
    pub flow_id: Option<FlowId>,
    pub execution_id: Option<ExecutionId>,
    pub time_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
    pub status: Option<ExecutionStatus>,
    pub actor_filter: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Simplified message types for WebSocket server compatibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceMessage {
    StoreTrace { trace: FlowTrace },
    QueryTraces { query: TraceQuery },
    GetTrace { trace_id: TraceId },
    Subscribe { filter: SubscriptionFilters },
    GetMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TraceResponse {
    TraceStored { trace_id: TraceId },
    TracesFound { traces: Vec<FlowTrace> },
    TraceData { trace: FlowTrace },
    Error { message: String },
    Metrics { data: serde_json::Value },
}

// Convenience constructors
impl TraceId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
    
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl ExecutionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
    
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    
    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
    
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl FlowId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ExecutionId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self {
            execution_time_ns: 0,
            memory_usage_bytes: 0,
            cpu_usage_percent: 0.0,
            queue_depth: 0,
            throughput_msgs_per_sec: 0.0,
        }
    }
}

// Helper functions for creating common event types
impl TraceEvent {
    pub fn actor_created(actor_id: String) -> Self {
        Self {
            event_id: EventId::new(),
            timestamp: Utc::now(),
            event_type: TraceEventType::ActorCreated,
            actor_id,
            data: TraceEventData {
                port: None,
                message: None,
                state_diff: None,
                error: None,
                performance_metrics: PerformanceMetrics::default(),
                custom_attributes: HashMap::new(),
            },
            causality: CausalityInfo {
                parent_event_id: None,
                root_cause_event_id: EventId::new(),
                dependency_chain: Vec::new(),
                span_id: Uuid::new_v4().to_string(),
            },
        }
    }

    pub fn data_flow(from_actor: String, from_port: String, to_actor: String, to_port: String, message_type: String, size_bytes: usize) -> Self {
        Self {
            event_id: EventId::new(),
            timestamp: Utc::now(),
            event_type: TraceEventType::DataFlow {
                to_actor,
                to_port,
            },
            actor_id: from_actor,
            data: TraceEventData {
                port: Some(from_port),
                message: Some(MessageSnapshot {
                    message_type,
                    size_bytes,
                    checksum: String::new(), // TODO: Calculate actual checksum
                    serialized_data: Vec::new(), // TODO: Add optional data capture
                }),
                state_diff: None,
                error: None,
                performance_metrics: PerformanceMetrics::default(),
                custom_attributes: HashMap::new(),
            },
            causality: CausalityInfo {
                parent_event_id: None,
                root_cause_event_id: EventId::new(),
                dependency_chain: Vec::new(),
                span_id: Uuid::new_v4().to_string(),
            },
        }
    }
    
    pub fn message_sent(actor_id: String, port: String, message_type: String, size_bytes: usize) -> Self {
        Self {
            event_id: EventId::new(),
            timestamp: Utc::now(),
            event_type: TraceEventType::MessageSent,
            actor_id,
            data: TraceEventData {
                port: Some(port),
                message: Some(MessageSnapshot {
                    message_type,
                    size_bytes,
                    checksum: String::new(), // TODO: Calculate actual checksum
                    serialized_data: Vec::new(), // TODO: Add optional data capture
                }),
                state_diff: None,
                error: None,
                performance_metrics: PerformanceMetrics::default(),
                custom_attributes: HashMap::new(),
            },
            causality: CausalityInfo {
                parent_event_id: None,
                root_cause_event_id: EventId::new(),
                dependency_chain: Vec::new(),
                span_id: Uuid::new_v4().to_string(),
            },
        }
    }
    
    pub fn actor_completed(actor_id: String) -> Self {
        Self {
            event_id: EventId::new(),
            timestamp: Utc::now(),
            event_type: TraceEventType::ActorCompleted,
            actor_id,
            data: TraceEventData {
                port: None,
                message: None,
                state_diff: None,
                error: None,
                performance_metrics: PerformanceMetrics::default(),
                custom_attributes: HashMap::new(),
            },
            causality: CausalityInfo {
                parent_event_id: None,
                root_cause_event_id: EventId::new(),
                dependency_chain: Vec::new(),
                span_id: Uuid::new_v4().to_string(),
            },
        }
    }
    
    pub fn actor_failed(actor_id: String, error: String) -> Self {
        Self {
            event_id: EventId::new(),
            timestamp: Utc::now(),
            event_type: TraceEventType::ActorFailed,
            actor_id,
            data: TraceEventData {
                port: None,
                message: None,
                state_diff: None,
                error: Some(error),
                performance_metrics: PerformanceMetrics::default(),
                custom_attributes: HashMap::new(),
            },
            causality: CausalityInfo {
                parent_event_id: None,
                root_cause_event_id: EventId::new(),
                dependency_chain: Vec::new(),
                span_id: Uuid::new_v4().to_string(),
            },
        }
    }
}
