

use super::*;
use reflow_actor::message::Message;
use crate::network::Network;
use crate::tracing::storage::TraceStorageError;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use tokio::time::{Duration, Instant};

/// Flow replay engine for time travel debugging and flow replays
pub struct FlowReplayEngine {
    storage: Arc<dyn TraceStorage>,
    replay_state: Arc<Mutex<ReplayState>>,
    event_scheduler: EventScheduler,
    state_reconstructor: StateReconstructor,
}

#[derive(Debug, Clone)]
pub struct ReplayState {
    pub current_trace: Option<FlowTrace>,
    pub current_event_index: usize,
    pub replay_mode: ReplayMode,
    pub time_position: DateTime<Utc>,
    pub breakpoints: Vec<ReplayBreakpoint>,
    pub actor_states: HashMap<String, Vec<u8>>, // Serialized actor states
}

#[derive(Debug, Clone)]
pub enum ReplayMode {
    Realtime { speed_multiplier: f64 },
    StepByStep,
    FastForward,
    TimeTravel { target_time: DateTime<Utc> },
}

#[derive(Debug, Clone)]
pub struct ReplayBreakpoint {
    pub id: String,
    pub condition: BreakpointCondition,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub enum BreakpointCondition {
    ActorId(String),
    EventType(TraceEventType),
    MessageContent { actor_id: String, port: String, pattern: String },
    StateCondition { actor_id: String, condition: String },
    TimePoint(DateTime<Utc>),
    Custom(String), // Custom condition expression
}

/// Event scheduler for controlled replay
pub struct EventScheduler {
    event_queue: VecDeque<ScheduledEvent>,
    current_time: Arc<Mutex<DateTime<Utc>>>,
}

#[derive(Debug, Clone)]
pub struct ScheduledEvent {
    pub event: TraceEvent,
    pub scheduled_time: DateTime<Utc>,
    pub dependencies: Vec<EventId>,
}

/// State reconstruction for time travel
pub struct StateReconstructor {
    state_snapshots: HashMap<String, Vec<StateSnapshot>>,
}

#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub timestamp: DateTime<Utc>,
    pub actor_id: String,
    pub state_data: Vec<u8>,
    pub event_id: EventId,
}

impl FlowReplayEngine {
    pub fn new(storage: Arc<dyn TraceStorage>) -> Self {
        Self {
            storage,
            replay_state: Arc::new(Mutex::new(ReplayState::default())),
            event_scheduler: EventScheduler::new(),
            state_reconstructor: StateReconstructor::new(),
        }
    }

    /// Start replaying a flow execution
    pub async fn start_replay(
        &mut self,
        trace_id: TraceId,
        mode: ReplayMode,
    ) -> Result<ReplaySession, ReplayError> {
        let trace = self.storage.get_trace(&trace_id)?
            .ok_or(ReplayError::TraceNotFound)?;

        let mut state = self.replay_state.lock().unwrap();
        state.current_trace = Some(trace.clone());
        state.current_event_index = 0;
        state.replay_mode = mode;
        state.time_position = trace.start_time;
        
        // Initialize actor states
        self.state_reconstructor.initialize_states(&trace).await?;

        Ok(ReplaySession {
            trace_id,
            engine: self,
            start_time: Instant::now(),
        })
    }

    /// Step through replay one event at a time
    pub async fn step_forward(&self) -> Result<Option<ReplayStepResult>, ReplayError> {
        let state = self.replay_state.lock().unwrap();
        
        let trace = state.current_trace.as_ref()
            .ok_or(ReplayError::NoActiveReplay)?;

        if state.current_event_index >= trace.events.len() {
            return Ok(None); // Replay finished
        }

        let event = &trace.events[state.current_event_index];
        
        // Check breakpoints
        if self.should_break_at_event(event, &state.breakpoints) {
            return Ok(Some(ReplayStepResult::Breakpoint {
                event: event.clone(),
                reason: "Breakpoint hit".to_string(),
            }));
        }

        // Execute event
        let result = self.execute_replay_event(event).await?;
        
        let mut state = self.replay_state.lock().unwrap();
        state.current_event_index += 1;
        state.time_position = event.timestamp;

        Ok(Some(result))
    }

    /// Jump to specific time point (time travel)
    pub async fn travel_to_time(&self, target_time: DateTime<Utc>) -> Result<(), ReplayError> {
        let mut state = self.replay_state.lock().unwrap();
        
        let trace = state.current_trace.as_ref()
            .ok_or(ReplayError::NoActiveReplay)?;

        // Find events up to target time
        let target_events: Vec<_> = trace.events.iter()
            .take_while(|event| event.timestamp <= target_time)
            .collect();

        // Reconstruct state at target time
        self.state_reconstructor.reconstruct_at_time(target_time, &target_events).await?;
        
        state.current_event_index = target_events.len();
        state.time_position = target_time;

        Ok(())
    }

    /// Set breakpoint for debugging
    pub fn set_breakpoint(&self, breakpoint: ReplayBreakpoint) {
        let mut state = self.replay_state.lock().unwrap();
        state.breakpoints.push(breakpoint);
    }

    /// Remove breakpoint
    pub fn remove_breakpoint(&self, breakpoint_id: &str) {
        let mut state = self.replay_state.lock().unwrap();
        state.breakpoints.retain(|bp| bp.id != breakpoint_id);
    }

    /// Get current state of an actor
    pub fn get_actor_state(&self, actor_id: &str) -> Result<Option<Vec<u8>>, ReplayError> {
        let state = self.replay_state.lock().unwrap();
        Ok(state.actor_states.get(actor_id).cloned())
    }

    /// Execute a single replay event
    async fn execute_replay_event(&self, event: &TraceEvent) -> Result<ReplayStepResult, ReplayError> {
        match &event.event_type {
            TraceEventType::MessageSent => {
                // Simulate message sending
                if let Some(ref message_snapshot) = event.data.message {
                    // Deserialize and send message
                    let message = self.deserialize_message(message_snapshot)?;
                    // Send to actor's input port
                }
            }
            TraceEventType::StateChanged => {
                // Update actor state
                if let Some(ref state_diff) = event.data.state_diff {
                    self.apply_state_diff(&event.actor_id, state_diff)?;
                }
            }
            TraceEventType::ActorStarted => {
                // Initialize actor for replay
            }
            _ => {
                // Handle other event types
            }
        }

        Ok(ReplayStepResult::EventExecuted {
            event: event.clone(),
            performance: event.data.performance_metrics.clone(),
        })
    }

    fn should_break_at_event(&self, event: &TraceEvent, breakpoints: &[ReplayBreakpoint]) -> bool {
        breakpoints.iter().any(|bp| {
            if !bp.enabled {
                return false;
            }

            match &bp.condition {
                BreakpointCondition::ActorId(actor_id) => event.actor_id == *actor_id,
                BreakpointCondition::EventType(event_type) => event.event_type == *event_type,
                BreakpointCondition::TimePoint(time) => event.timestamp >= *time,
                BreakpointCondition::MessageContent { actor_id, port, pattern } => {
                    event.actor_id == *actor_id 
                        && event.data.port.as_ref() == Some(port)
                        && self.matches_pattern(event, pattern)
                }
                _ => false,
            }
        })
    }

    fn matches_pattern(&self, event: &TraceEvent, pattern: &str) -> bool {
        // Simple pattern matching - could be enhanced with regex
        if let Some(ref message) = event.data.message {
            // Check message content against pattern
            String::from_utf8_lossy(&message.serialized_data).contains(pattern)
        } else {
            false
        }
    }

    fn apply_state_diff(&self, actor_id: &str, state_diff: &StateDiff) -> Result<(), ReplayError> {
        let mut state = self.replay_state.lock().unwrap();
        state.actor_states.insert(actor_id.to_string(), state_diff.after.clone());
        Ok(())
    }

    fn deserialize_message(&self, snapshot: &MessageSnapshot) -> Result<Message, ReplayError> {
        // Decompress and deserialize message
        let data = if snapshot.serialized_data.len() > 0 {
            // Assuming compression was used
            flate2::read::GzDecoder::new(&snapshot.serialized_data[..]);
            // Deserialize based on message_type
            Message::Optional(None) // Placeholder
        } else {
            return Err(ReplayError::InvalidMessageData);
        };
        
        Ok(data)
    }
}

impl EventScheduler {
    pub fn new() -> Self {
        Self {
            event_queue: VecDeque::new(),
            current_time: Arc::new(Mutex::new(Utc::now())),
        }
    }

    pub fn schedule_events(&mut self, events: Vec<TraceEvent>, mode: &ReplayMode) {
        let base_time = *self.current_time.lock().unwrap();
        
        for (i, event) in events.into_iter().enumerate() {
            let scheduled_time = match mode {
                ReplayMode::Realtime { speed_multiplier } => {
                    base_time + chrono::Duration::milliseconds(
                        (i as f64 * 100.0 / speed_multiplier) as i64
                    )
                }
                ReplayMode::StepByStep => base_time, // Execute immediately when requested
                ReplayMode::FastForward => base_time, // Execute as fast as possible
                ReplayMode::TimeTravel { .. } => event.timestamp,
            };

            self.event_queue.push_back(ScheduledEvent {
                event,
                scheduled_time,
                dependencies: Vec::new(),
            });
        }
    }
}

impl StateReconstructor {
    pub fn new() -> Self {
        Self {
            state_snapshots: HashMap::new(),
        }
    }

    pub async fn initialize_states(&mut self, trace: &FlowTrace) -> Result<(), ReplayError> {
        // Extract state snapshots from trace events
        for event in &trace.events {
            if let Some(ref state_diff) = event.data.state_diff {
                let snapshot = StateSnapshot {
                    timestamp: event.timestamp,
                    actor_id: event.actor_id.clone(),
                    state_data: state_diff.after.clone(),
                    event_id: event.event_id.clone(),
                };

                self.state_snapshots
                    .entry(event.actor_id.clone())
                    .or_insert_with(Vec::new)
                    .push(snapshot);
            }
        }

        Ok(())
    }

    pub async fn reconstruct_at_time(
        &self,
        target_time: DateTime<Utc>,
        events: &[&TraceEvent],
    ) -> Result<HashMap<String, Vec<u8>>, ReplayError> {
        let mut reconstructed_states = HashMap::new();

        for actor_snapshots in self.state_snapshots.values() {
            // Find the latest state before or at target_time
            if let Some(snapshot) = actor_snapshots.iter()
                .filter(|s| s.timestamp <= target_time)
                .max_by_key(|s| s.timestamp) {
                
                reconstructed_states.insert(
                    snapshot.actor_id.clone(),
                    snapshot.state_data.clone(),
                );
            }
        }

        Ok(reconstructed_states)
    }
}

/// Replay session handle
pub struct ReplaySession<'a> {
    trace_id: TraceId,
    engine: &'a FlowReplayEngine,
    start_time: Instant,
}

impl<'a> ReplaySession<'a> {
    pub async fn play_until_breakpoint(&self) -> Result<ReplayStepResult, ReplayError> {
        loop {
            match self.engine.step_forward().await? {
                Some(ReplayStepResult::Breakpoint { event, reason }) => {
                    return Ok(ReplayStepResult::Breakpoint { event, reason });
                }
                Some(_) => continue, // Keep playing
                None => {
                    return Ok(ReplayStepResult::ReplayFinished {
                        total_events: 0, // Would track actual count
                        duration: self.start_time.elapsed(),
                    });
                }
            }
        }
    }

    pub async fn step(&self) -> Result<Option<ReplayStepResult>, ReplayError> {
        self.engine.step_forward().await
    }

    pub async fn travel_to(&self, target_time: DateTime<Utc>) -> Result<(), ReplayError> {
        self.engine.travel_to_time(target_time).await
    }
}

#[derive(Debug, Clone)]
pub enum ReplayStepResult {
    EventExecuted {
        event: TraceEvent,
        performance: PerformanceMetrics,
    },
    Breakpoint {
        event: TraceEvent,
        reason: String,
    },
    ReplayFinished {
        total_events: usize,
        duration: Duration,
    },
}

impl Default for ReplayState {
    fn default() -> Self {
        Self {
            current_trace: None,
            current_event_index: 0,
            replay_mode: ReplayMode::StepByStep,
            time_position: Utc::now(),
            breakpoints: Vec::new(),
            actor_states: HashMap::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("Trace not found")]
    TraceNotFound,
    #[error("No active replay session")]
    NoActiveReplay,
    #[error("Invalid message data")]
    InvalidMessageData,
    #[error("State reconstruction failed: {0}")]
    StateReconstruction(String),
    #[error("Storage error: {0}")]
    Storage(#[from] TraceStorageError),
}

/// Visual debugging interface
pub struct VisualDebugger {
    replay_engine: FlowReplayEngine,
    ui_state: Arc<Mutex<DebuggerUIState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebuggerUIState {
    pub current_view: DebuggerView,
    pub selected_actor: Option<String>,
    pub timeline_position: f64, // 0.0 to 1.0
    pub visible_events: Vec<TraceEvent>,
    pub inspector_data: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DebuggerView {
    FlowGraph,
    Timeline,
    StateInspector,
    MessageInspector,
    PerformanceView,
}

impl VisualDebugger {
    pub fn new(storage: Arc<dyn TraceStorage>) -> Self {
        Self {
            replay_engine: FlowReplayEngine::new(storage),
            ui_state: Arc::new(Mutex::new(DebuggerUIState::default())),
        }
    }

    /// Generate visual debugging data for frontend
    pub fn get_debug_view_data(&self, trace_id: &TraceId) -> Result<DebugViewData, ReplayError> {
        let state = self.ui_state.lock().unwrap();
        
        Ok(DebugViewData {
            trace_id: trace_id.clone(),
            timeline_data: self.generate_timeline_data()?,
            flow_graph_data: self.generate_flow_graph_data()?,
            performance_data: self.generate_performance_data()?,
            current_state: state.clone(),
        })
    }

    fn generate_timeline_data(&self) -> Result<TimelineData, ReplayError> {
        // Generate timeline visualization data
        Ok(TimelineData {
            events: Vec::new(), // Populate with actual timeline events
            duration_ms: 0,
            performance_markers: Vec::new(),
        })
    }

    fn generate_flow_graph_data(&self) -> Result<FlowGraphData, ReplayError> {
        // Generate flow graph visualization data
        Ok(FlowGraphData {
            nodes: Vec::new(),
            edges: Vec::new(),
            current_state_overlay: HashMap::new(),
        })
    }

    fn generate_performance_data(&self) -> Result<PerformanceData, ReplayError> {
        // Generate performance visualization data
        Ok(PerformanceData {
            metrics_over_time: Vec::new(),
            hotspots: Vec::new(),
            bottlenecks: Vec::new(),
        })
    }
}

impl Default for DebuggerUIState {
    fn default() -> Self {
        Self {
            current_view: DebuggerView::FlowGraph,
            selected_actor: None,
            timeline_position: 0.0,
            visible_events: Vec::new(),
            inspector_data: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugViewData {
    pub trace_id: TraceId,
    pub timeline_data: TimelineData,
    pub flow_graph_data: FlowGraphData,
    pub performance_data: PerformanceData,
    pub current_state: DebuggerUIState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineData {
    pub events: Vec<TimelineEvent>,
    pub duration_ms: u64,
    pub performance_markers: Vec<PerformanceMarker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub actor_id: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMarker {
    pub timestamp: DateTime<Utc>,
    pub marker_type: String,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowGraphData {
    pub nodes: Vec<FlowGraphNode>,
    pub edges: Vec<FlowGraphEdge>,
    pub current_state_overlay: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowGraphNode {
    pub id: String,
    pub label: String,
    pub position: (f64, f64),
    pub status: String,
    pub metrics: PerformanceMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowGraphEdge {
    pub from: String,
    pub to: String,
    pub port: String,
    pub message_count: usize,
    pub data_volume: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceData {
    pub metrics_over_time: Vec<MetricDataPoint>,
    pub hotspots: Vec<PerformanceHotspot>,
    pub bottlenecks: Vec<PerformanceBottleneck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDataPoint {
    pub timestamp: DateTime<Utc>,
    pub metric_name: String,
    pub value: f64,
    pub actor_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceHotspot {
    pub actor_id: String,
    pub metric_type: String,
    pub severity: f64,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceBottleneck {
    pub location: String,
    pub impact_score: f64,
    pub suggested_fix: String,
}