//! # Reflow Tracing Protocol
//!
//! Shared protocol definitions for communication between Reflow tracing clients and servers.
//! This crate provides lightweight message types without heavy dependencies.

use chrono::{DateTime, Utc};
use derive_more::Display;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub mod checksum;
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

/// Correlation identifier for matching a request/response pair over the
/// (otherwise fire-and-forget) WebSocket channel.
#[derive(Debug, Display, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct RequestId(pub Uuid);

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
#[allow(clippy::large_enum_variant)]
pub enum TracingRequest {
    /// Start a new flow trace. The client owns the `trace_id` so that the
    /// same id can be propagated peer-to-peer in distributed flows before any
    /// server round-trip; the server creates/stores the trace under this id.
    StartTrace {
        #[serde(default)]
        trace_id: TraceId,
        flow_id: FlowId,
        version: FlowVersion,
    },
    /// Record a trace event
    RecordEvent {
        trace_id: TraceId,
        event: TraceEvent,
    },
    /// Finalize a trace, marking its terminal status.
    EndTrace {
        trace_id: TraceId,
        status: ExecutionStatus,
    },
    /// Get a specific trace by ID
    GetTrace {
        #[serde(default)]
        request_id: RequestId,
        trace_id: TraceId,
    },
    /// Query traces with filters
    QueryTraces {
        #[serde(default)]
        request_id: RequestId,
        query: TraceQuery,
    },
    /// Get all versions of a flow
    GetFlowVersions { flow_id: FlowId },
    /// Health check
    Ping,
    /// Subscribe to real-time trace events
    Subscribe { filters: SubscriptionFilters },
    /// Unsubscribe from real-time events
    Unsubscribe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TracingResponse {
    /// Response to StartTrace
    TraceStarted { trace_id: TraceId },
    /// Response to RecordEvent
    EventRecorded {
        success: bool,
        error: Option<String>,
    },
    /// Response to GetTrace
    TraceData {
        #[serde(default)]
        request_id: RequestId,
        trace: Option<FlowTrace>,
    },
    /// Response to QueryTraces
    QueryResults {
        #[serde(default)]
        request_id: RequestId,
        traces: Vec<FlowTrace>,
        total_count: usize,
    },
    /// Response to GetFlowVersions
    FlowVersions { versions: Vec<FlowVersion> },
    /// Response to Ping
    Pong,
    /// Real-time event notification
    EventNotification {
        trace_id: TraceId,
        event: TraceEvent,
    },
    /// Error response
    Error { message: String, code: ErrorCode },
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
    DataFlow { to_actor: String, to_port: String },
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

/// Current version of the `serialized_data` content format. Bump when the
/// canonical content encoding or codec semantics change.
pub const CONTENT_FORMAT_VERSION: u32 = 1;

/// Snapshot of a single traced message.
///
/// Two independent capture decisions populate this (see [`MessageSnapshot::capture`]):
/// - `checksum` — cheap, content-only identity (default on). See [`crate::checksum`].
/// - `serialized_data` — heavy, opt-in full content (default off).
///
/// Invariant when content is captured: `checksum == sha256(canonical(decompress(serialized_data)))`,
/// and `serialized_data` round-trips to a message equal to the original.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSnapshot {
    /// Discriminant/type name of the message (e.g. `"Integer"`).
    pub message_type: String,
    /// Pre-compression content size in bytes — the same bytes the `checksum`
    /// covers (length of the canonical JSON form). Stays meaningful whether or
    /// not content was captured and regardless of compression.
    pub size_bytes: usize,
    /// `"sha256:<64 lowercase hex>"`, or `""` if not computed. Never chained.
    pub checksum: String,
    /// Optional captured content (default empty). Decoded per `content_codec`.
    /// Empty strictly means "not captured", distinct from a captured typed-empty.
    #[serde(default)]
    pub serialized_data: Vec<u8>,
    /// Codec for `serialized_data`. `"none"` = uncompressed canonical JSON.
    #[serde(default = "default_content_codec")]
    pub content_codec: String,
    /// Format/version tag so a reader in another SDK can decode safely.
    #[serde(default)]
    pub content_format_version: u32,
    /// Compressed/stored footprint of `serialized_data`, when captured.
    /// `None` when content is not captured. Do not overload `size_bytes` for this.
    #[serde(default)]
    pub stored_bytes: Option<usize>,
}

fn default_content_codec() -> String {
    "none".to_string()
}

impl MessageSnapshot {
    /// Capture a snapshot of `content` honoring the two orthogonal toggles.
    ///
    /// `capture_checksum` (cheap, recommended default-on) computes the
    /// content-only SHA-256. `capture_content` (heavy, default-off) additionally
    /// retains the canonical content bytes in `serialized_data` (codec `"none"`,
    /// i.e. uncompressed canonical JSON), preserving the checksum invariant.
    pub fn capture<T: Serialize>(
        message_type: impl Into<String>,
        content: &T,
        capture_checksum: bool,
        capture_content: bool,
    ) -> Self {
        // Canonicalize once if either toggle needs it.
        let canonical = if capture_checksum || capture_content {
            crate::checksum::canonical_json(content).ok()
        } else {
            None
        };
        let size_bytes = canonical.as_ref().map(|c| c.len()).unwrap_or(0);
        let checksum = if capture_checksum {
            canonical
                .as_ref()
                .map(|c| crate::checksum::checksum_of_canonical(c))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let (serialized_data, stored_bytes) = if capture_content {
            let bytes = canonical
                .as_ref()
                .map(|c| c.as_bytes().to_vec())
                .unwrap_or_default();
            let len = bytes.len();
            (bytes, Some(len))
        } else {
            (Vec::new(), None)
        };
        Self {
            message_type: message_type.into(),
            size_bytes,
            checksum,
            serialized_data,
            content_codec: default_content_codec(),
            content_format_version: CONTENT_FORMAT_VERSION,
            stored_bytes,
        }
    }
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

/// Per-event performance metrics.
///
/// Each field is `Option`: `None` means **unmeasured**, `Some(0)` means
/// **measured and zero**. The distinction is load-bearing — downstream
/// aggregation (averages, percentiles, alerts) must exclude unmeasured samples
/// rather than averaging zeros into them. Units are pinned in each field's doc
/// and must not drift across SDKs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// Wall-clock duration of the traced operation, in **nanoseconds**
    /// (monotonic clock). Cheap — measured on the always-on path.
    #[serde(default)]
    pub execution_time_ns: Option<u64>,
    /// Memory attributable to the operation as a **delta** (allocation
    /// high-water during the tick), in **bytes** — not process RSS. Heavy:
    /// gated behind `enable_perf_sampling`; `None` when off.
    #[serde(default)]
    pub memory_usage_bytes: Option<usize>,
    /// CPU usage averaged over the operation window, **0–100 percent**. Heavy
    /// and interval-shaped: gated; `None` when off.
    #[serde(default)]
    pub cpu_usage_percent: Option<f32>,
    /// Instantaneous inbound mailbox depth at capture time, a **count**. Cheap —
    /// measured on the always-on path.
    #[serde(default)]
    pub queue_depth: Option<usize>,
    /// Throughput over a documented rolling window, in **messages per second**.
    /// Needs a window denominator: gated; `None` when off.
    #[serde(default)]
    pub throughput_msgs_per_sec: Option<f64>,
}

impl PerformanceMetrics {
    /// Cheap-path metrics: the always-affordable fields measured (or `None` if
    /// not available here), the heavy/gated fields left unmeasured.
    pub fn cheap(execution_time_ns: Option<u64>, queue_depth: Option<usize>) -> Self {
        Self {
            execution_time_ns,
            memory_usage_bytes: None,
            cpu_usage_percent: None,
            queue_depth,
            throughput_msgs_per_sec: None,
        }
    }
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
#[allow(clippy::large_enum_variant)]
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

impl RequestId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for RequestId {
    fn default() -> Self {
        Self::new()
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
    /// All fields unmeasured.
    fn default() -> Self {
        Self {
            execution_time_ns: None,
            memory_usage_bytes: None,
            cpu_usage_percent: None,
            queue_depth: None,
            throughput_msgs_per_sec: None,
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

    pub fn data_flow(
        from_actor: String,
        from_port: String,
        to_actor: String,
        to_port: String,
        snapshot: MessageSnapshot,
        metrics: PerformanceMetrics,
    ) -> Self {
        Self {
            event_id: EventId::new(),
            timestamp: Utc::now(),
            event_type: TraceEventType::DataFlow { to_actor, to_port },
            actor_id: from_actor,
            data: TraceEventData {
                port: Some(from_port),
                message: Some(snapshot),
                state_diff: None,
                error: None,
                performance_metrics: metrics,
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

    pub fn message_sent(
        actor_id: String,
        port: String,
        snapshot: MessageSnapshot,
        metrics: PerformanceMetrics,
    ) -> Self {
        Self {
            event_id: EventId::new(),
            timestamp: Utc::now(),
            event_type: TraceEventType::MessageSent,
            actor_id,
            data: TraceEventData {
                port: Some(port),
                message: Some(snapshot),
                state_diff: None,
                error: None,
                performance_metrics: metrics,
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
