use super::*;
use chrono::Duration;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Flow versioning and lineage tracking system
pub struct FlowVersioningSystem {
    version_storage: Arc<dyn VersionStorage>,
    lineage_tracker: LineageTracker,
    diff_engine: FlowDiffEngine,
    migration_assistant: MigrationAssistant,
}

/// Flow version information with comprehensive metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowVersionInfo {
    pub version_id: VersionId,
    pub flow_id: FlowId,
    pub semantic_version: Version,
    pub git_hash: Option<String>,
    pub content_hash: String,
    pub created_at: DateTime<Utc>,
    pub created_by: UserId,
    pub parent_version: Option<VersionId>,
    pub tags: Vec<String>,
    pub changelog: VersionChangelog,
    pub flow_definition: FlowDefinition,
    pub compatibility_info: CompatibilityInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct VersionId(pub Uuid);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserId(pub String);

/// Complete flow definition for versioning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDefinition {
    pub actors: HashMap<String, ActorDefinition>,
    pub connections: Vec<ConnectionDefinition>,
    pub initial_packets: Vec<InitialPacketDefinition>,
    pub metadata: FlowMetadata,
    pub configuration: FlowConfiguration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorDefinition {
    pub actor_type: String,
    pub configuration: HashMap<String, serde_json::Value>,
    pub input_ports: Vec<PortDefinition>,
    pub output_ports: Vec<PortDefinition>,
    pub position: Option<(f64, f64)>, // For visual editor
    pub metadata: ActorMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionDefinition {
    pub id: String,
    pub from_actor: String,
    pub from_port: String,
    pub to_actor: String,
    pub to_port: String,
    pub metadata: ConnectionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialPacketDefinition {
    pub target_actor: String,
    pub target_port: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortDefinition {
    pub name: String,
    pub data_type: DataType,
    pub required: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DataType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    Binary,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorMetadata {
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub performance_profile: Option<PerformanceProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionMetadata {
    pub description: Option<String>,
    pub expected_throughput: Option<u64>,
    pub criticality: ConnectionCriticality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionCriticality {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowMetadata {
    pub name: String,
    pub description: Option<String>,
    pub author: String,
    pub tags: Vec<String>,
    pub category: FlowCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FlowCategory {
    DataTransformation,
    ETL,
    Stream,
    Batch,
    Realtime,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowConfiguration {
    pub timeout_ms: Option<u64>,
    pub retry_policy: RetryPolicy,
    pub resource_limits: ResourceLimits,
    pub environment_variables: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub backoff_strategy: BackoffStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BackoffStrategy {
    Linear { interval_ms: u64 },
    Exponential { base_ms: u64, multiplier: f64 },
    Custom(String),
}

impl PartialEq for BackoffStrategy {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Linear { interval_ms: l_interval_ms }, Self::Linear { interval_ms: r_interval_ms }) => l_interval_ms == r_interval_ms,
            (Self::Exponential { base_ms: l_base_ms, multiplier: l_multiplier }, Self::Exponential { base_ms: r_base_ms, multiplier: r_multiplier }) => l_base_ms == r_base_ms && l_multiplier == r_multiplier,
            (Self::Custom(l0), Self::Custom(r0)) => l0 == r0,
            _ => false,
        }
    }
}

impl Eq for BackoffStrategy{}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_cpu_cores: Option<u32>,
    pub max_execution_time_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceProfile {
    pub avg_execution_time_ms: f64,
    pub memory_usage_mb: f64,
    pub cpu_utilization: f64,
    pub throughput_msgs_per_sec: f64,
}

/// Version changelog tracking changes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionChangelog {
    pub summary: String,
    pub changes: Vec<VersionChange>,
    pub breaking_changes: Vec<BreakingChange>,
    pub migration_notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionChange {
    pub change_type: ChangeType,
    pub description: String,
    pub affected_components: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeType {
    Added,
    Modified,
    Removed,
    Deprecated,
    Fixed,
    Security,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChange {
    pub description: String,
    pub migration_guide: String,
    pub affected_apis: Vec<String>,
}

/// Compatibility information between versions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityInfo {
    pub backward_compatible: bool,
    pub forward_compatible: bool,
    pub api_compatibility: ApiCompatibility,
    pub data_compatibility: DataCompatibility,
    pub runtime_compatibility: RuntimeCompatibility,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiCompatibility {
    pub compatible: bool,
    pub removed_apis: Vec<String>,
    pub added_apis: Vec<String>,
    pub changed_apis: Vec<ApiChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiChange {
    pub api_name: String,
    pub change_description: String,
    pub severity: ChangeSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChangeSeverity {
    Minor,
    Major,
    Breaking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataCompatibility {
    pub schema_compatible: bool,
    pub data_migration_required: bool,
    pub migration_complexity: MigrationComplexity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationComplexity {
    None,
    Simple,
    Moderate,
    Complex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCompatibility {
    pub runtime_version_required: Option<String>,
    pub dependency_changes: Vec<DependencyChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyChange {
    pub dependency_name: String,
    pub old_version: Option<String>,
    pub new_version: String,
    pub change_impact: DependencyImpact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyImpact {
    Low,
    Medium,
    High,
}

/// Flow lineage tracking
pub struct LineageTracker {
    lineage_graph: LineageGraph,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageGraph {
    pub nodes: HashMap<VersionId, LineageNode>,
    pub edges: Vec<LineageEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageNode {
    pub version_id: VersionId,
    pub flow_id: FlowId,
    pub version_info: FlowVersionInfo,
    pub execution_history: Vec<ExecutionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineageEdge {
    pub from_version: VersionId,
    pub to_version: VersionId,
    pub relationship_type: LineageRelationship,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LineageRelationship {
    DirectEvolution, // Version A -> Version B (linear evolution)
    Fork,            // Version A -> Version B (branched from A)
    Merge,           // Version A + B -> Version C
    Revert,          // Version B -> Version A (rollback)
    Derive,          // Version A -> New Flow (derived/cloned)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSummary {
    pub execution_id: ExecutionId,
    pub start_time: DateTime<Utc>,
    pub duration: Duration,
    pub status: ExecutionStatus,
    pub performance_summary: PerformanceSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub total_messages_processed: u64,
    pub avg_throughput: f64,
    pub peak_memory_usage: u64,
    pub error_rate: f64,
}

/// Flow difference engine
pub struct FlowDiffEngine;

impl FlowDiffEngine {
    pub fn compare_versions(
        &self,
        version_a: &FlowVersionInfo,
        version_b: &FlowVersionInfo,
    ) -> FlowDiff {
        FlowDiff {
            version_a: version_a.version_id.clone(),
            version_b: version_b.version_id.clone(),
            actor_changes: self.compare_actors(
                &version_a.flow_definition.actors,
                &version_b.flow_definition.actors,
            ),
            connection_changes: self.compare_connections(
                &version_a.flow_definition.connections,
                &version_b.flow_definition.connections,
            ),
            configuration_changes: self.compare_configurations(
                &version_a.flow_definition.configuration,
                &version_b.flow_definition.configuration,
            ),
            metadata_changes: self.compare_metadata(
                &version_a.flow_definition.metadata,
                &version_b.flow_definition.metadata,
            ),
            compatibility_impact: self.assess_compatibility_impact(version_a, version_b),
        }
    }

    fn compare_actors(
        &self,
        actors_a: &HashMap<String, ActorDefinition>,
        actors_b: &HashMap<String, ActorDefinition>,
    ) -> Vec<ActorChange> {
        let mut changes = Vec::new();

        // Find added actors
        for (id, actor) in actors_b {
            if !actors_a.contains_key(id) {
                changes.push(ActorChange::Added {
                    actor_id: id.clone(),
                    definition: actor.clone(),
                });
            }
        }

        // Find removed actors
        for (id, actor) in actors_a {
            if !actors_b.contains_key(id) {
                changes.push(ActorChange::Removed {
                    actor_id: id.clone(),
                    definition: actor.clone(),
                });
            }
        }

        // Find modified actors
        for (id, actor_a) in actors_a {
            if let Some(actor_b) = actors_b.get(id) {
                if !std::ptr::eq(actor_a, actor_b) {
                    changes.push(ActorChange::Modified {
                        actor_id: id.clone(),
                        old_definition: actor_a.clone(),
                        new_definition: actor_b.clone(),
                        specific_changes: self.analyze_actor_changes(actor_a, actor_b),
                    });
                }
            }
        }

        changes
    }

    fn compare_connections(
        &self,
        connections_a: &[ConnectionDefinition],
        connections_b: &[ConnectionDefinition],
    ) -> Vec<ConnectionChange> {
        let mut changes = Vec::new();

        let connections_a_map: HashMap<_, _> = connections_a.iter().map(|c| (&c.id, c)).collect();
        let connections_b_map: HashMap<_, _> = connections_b.iter().map(|c| (&c.id, c)).collect();

        // Find added connections
        for (id, connection) in &connections_b_map {
            if !connections_a_map.contains_key(id) {
                changes.push(ConnectionChange::Added {
                    connection: (*connection).clone(),
                });
            }
        }

        // Find removed connections
        for (id, connection) in &connections_a_map {
            if !connections_b_map.contains_key(id) {
                changes.push(ConnectionChange::Removed {
                    connection: (*connection).clone(),
                });
            }
        }

        // Find modified connections
        for (id, connection_a) in &connections_a_map {
            if let Some(connection_b) = connections_b_map.get(id) {
                if !std::ptr::eq(connection_a, connection_b) {
                    changes.push(ConnectionChange::Modified {
                        old_connection: (*connection_a).clone(),
                        new_connection: (*connection_b).clone(),
                    });
                }
            }
        }

        changes
    }

    fn compare_configurations(
        &self,
        config_a: &FlowConfiguration,
        config_b: &FlowConfiguration,
    ) -> Vec<ConfigurationChange> {
        let mut changes = Vec::new();

        if config_a.timeout_ms != config_b.timeout_ms {
            changes.push(ConfigurationChange::TimeoutChanged {
                old_value: config_a.timeout_ms,
                new_value: config_b.timeout_ms,
            });
        }

        if config_a.retry_policy != config_b.retry_policy {
            changes.push(ConfigurationChange::RetryPolicyChanged {
                old_policy: config_a.retry_policy.clone(),
                new_policy: config_b.retry_policy.clone(),
            });
        }

        // Compare environment variables
        let env_changes = self.compare_env_vars(
            &config_a.environment_variables,
            &config_b.environment_variables,
        );
        changes.extend(env_changes);

        changes
    }

    fn compare_metadata(
        &self,
        metadata_a: &FlowMetadata,
        metadata_b: &FlowMetadata,
    ) -> Vec<MetadataChange> {
        let mut changes = Vec::new();

        if metadata_a.name != metadata_b.name {
            changes.push(MetadataChange::NameChanged {
                old_name: metadata_a.name.clone(),
                new_name: metadata_b.name.clone(),
            });
        }

        if metadata_a.description != metadata_b.description {
            changes.push(MetadataChange::DescriptionChanged {
                old_description: metadata_a.description.clone(),
                new_description: metadata_b.description.clone(),
            });
        }

        if metadata_a.tags != metadata_b.tags {
            changes.push(MetadataChange::TagsChanged {
                added_tags: metadata_b
                    .tags
                    .iter()
                    .filter(|tag| !metadata_a.tags.contains(tag))
                    .cloned()
                    .collect(),
                removed_tags: metadata_a
                    .tags
                    .iter()
                    .filter(|tag| !metadata_b.tags.contains(tag))
                    .cloned()
                    .collect(),
            });
        }

        changes
    }

    fn analyze_actor_changes(
        &self,
        actor_a: &ActorDefinition,
        actor_b: &ActorDefinition,
    ) -> Vec<SpecificActorChange> {
        let mut changes = Vec::new();

        if actor_a.actor_type != actor_b.actor_type {
            changes.push(SpecificActorChange::TypeChanged {
                old_type: actor_a.actor_type.clone(),
                new_type: actor_b.actor_type.clone(),
            });
        }

        if actor_a.configuration != actor_b.configuration {
            changes.push(SpecificActorChange::ConfigurationChanged);
        }

        if !actor_a.input_ports.clone().into_iter().filter(|p| actor_b.input_ports.contains(p)).collect::<Vec<_>>().is_empty() {
            changes.push(SpecificActorChange::InputPortsChanged);
        }

        if actor_a.output_ports.clone().into_iter().filter(|p| actor_b.output_ports.contains(p)).collect::<Vec<_>>().is_empty() {
            changes.push(SpecificActorChange::OutputPortsChanged);
        }

        changes
    }

    fn compare_env_vars(
        &self,
        env_a: &HashMap<String, String>,
        env_b: &HashMap<String, String>,
    ) -> Vec<ConfigurationChange> {
        let mut changes = Vec::new();

        for (key, value_b) in env_b {
            match env_a.get(key) {
                Some(value_a) if value_a != value_b => {
                    changes.push(ConfigurationChange::EnvironmentVariableChanged {
                        key: key.clone(),
                        old_value: Some(value_a.clone()),
                        new_value: value_b.clone(),
                    });
                }
                None => {
                    changes.push(ConfigurationChange::EnvironmentVariableChanged {
                        key: key.clone(),
                        old_value: None,
                        new_value: value_b.clone(),
                    });
                }
                _ => {}
            }
        }

        for (key, value_a) in env_a {
            if !env_b.contains_key(key) {
                changes.push(ConfigurationChange::EnvironmentVariableRemoved {
                    key: key.clone(),
                    old_value: value_a.clone(),
                });
            }
        }

        changes
    }

    fn assess_compatibility_impact(
        &self,
        version_a: &FlowVersionInfo,
        version_b: &FlowVersionInfo,
    ) -> CompatibilityImpact {
        CompatibilityImpact {
            breaking_changes: self.detect_breaking_changes(version_a, version_b),
            migration_required: self.is_migration_required(version_a, version_b),
            risk_level: self.assess_risk_level(version_a, version_b),
            recommended_actions: self.generate_migration_recommendations(version_a, version_b),
        }
    }

    fn detect_breaking_changes(
        &self,
        _version_a: &FlowVersionInfo,
        _version_b: &FlowVersionInfo,
    ) -> Vec<BreakingChangeDetection> {
        // Implementation would analyze for breaking changes
        Vec::new()
    }

    fn is_migration_required(
        &self,
        _version_a: &FlowVersionInfo,
        _version_b: &FlowVersionInfo,
    ) -> bool {
        // Determine if migration is needed
        false
    }

    fn assess_risk_level(
        &self,
        _version_a: &FlowVersionInfo,
        _version_b: &FlowVersionInfo,
    ) -> RiskLevel {
        RiskLevel::Low
    }

    fn generate_migration_recommendations(
        &self,
        _version_a: &FlowVersionInfo,
        _version_b: &FlowVersionInfo,
    ) -> Vec<MigrationRecommendation> {
        Vec::new()
    }
}

/// Migration assistant for version upgrades
pub struct MigrationAssistant {
    migration_strategies: HashMap<String, Box<dyn MigrationStrategy>>,
}

pub trait MigrationStrategy: Send + Sync {
    fn can_handle(&self, from_version: &FlowVersionInfo, to_version: &FlowVersionInfo) -> bool;
    fn create_migration_plan(
        &self,
        from_version: &FlowVersionInfo,
        to_version: &FlowVersionInfo,
    ) -> Result<MigrationPlan, MigrationError>;
    fn execute_migration(&self, plan: &MigrationPlan) -> Result<MigrationResult, MigrationError>;
}

// Supporting types for the diff system

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlowDiff {
    pub version_a: VersionId,
    pub version_b: VersionId,
    pub actor_changes: Vec<ActorChange>,
    pub connection_changes: Vec<ConnectionChange>,
    pub configuration_changes: Vec<ConfigurationChange>,
    pub metadata_changes: Vec<MetadataChange>,
    pub compatibility_impact: CompatibilityImpact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorChange {
    Added {
        actor_id: String,
        definition: ActorDefinition,
    },
    Removed {
        actor_id: String,
        definition: ActorDefinition,
    },
    Modified {
        actor_id: String,
        old_definition: ActorDefinition,
        new_definition: ActorDefinition,
        specific_changes: Vec<SpecificActorChange>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SpecificActorChange {
    TypeChanged { old_type: String, new_type: String },
    ConfigurationChanged,
    InputPortsChanged,
    OutputPortsChanged,
    PositionChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionChange {
    Added {
        connection: ConnectionDefinition,
    },
    Removed {
        connection: ConnectionDefinition,
    },
    Modified {
        old_connection: ConnectionDefinition,
        new_connection: ConnectionDefinition,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConfigurationChange {
    TimeoutChanged {
        old_value: Option<u64>,
        new_value: Option<u64>,
    },
    RetryPolicyChanged {
        old_policy: RetryPolicy,
        new_policy: RetryPolicy,
    },
    EnvironmentVariableChanged {
        key: String,
        old_value: Option<String>,
        new_value: String,
    },
    EnvironmentVariableRemoved {
        key: String,
        old_value: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetadataChange {
    NameChanged {
        old_name: String,
        new_name: String,
    },
    DescriptionChanged {
        old_description: Option<String>,
        new_description: Option<String>,
    },
    TagsChanged {
        added_tags: Vec<String>,
        removed_tags: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityImpact {
    pub breaking_changes: Vec<BreakingChangeDetection>,
    pub migration_required: bool,
    pub risk_level: RiskLevel,
    pub recommended_actions: Vec<MigrationRecommendation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChangeDetection {
    pub change_type: String,
    pub description: String,
    pub affected_components: Vec<String>,
    pub severity: ChangeSeverity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationRecommendation {
    pub action: String,
    pub description: String,
    pub priority: RecommendationPriority,
    pub estimated_effort: EstimatedEffort,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RecommendationPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstimatedEffort {
    pub time_estimate: Duration,
    pub complexity: ComplexityLevel,
    pub risk_level: RiskLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComplexityLevel {
    Simple,
    Moderate,
    Complex,
    Expert,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    pub plan_id: String,
    pub from_version: VersionId,
    pub to_version: VersionId,
    pub steps: Vec<MigrationStep>,
    pub estimated_duration: Duration,
    pub rollback_plan: Option<RollbackPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStep {
    pub step_id: String,
    pub description: String,
    pub step_type: MigrationStepType,
    pub dependencies: Vec<String>,
    pub validation_criteria: Vec<ValidationCriterion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationStepType {
    ConfigurationUpdate,
    ActorReplacement,
    ConnectionUpdate,
    DataMigration,
    ValidationStep,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationCriterion {
    pub name: String,
    pub description: String,
    pub validation_type: ValidationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationType {
    PortCompatibility,
    DataIntegrity,
    PerformanceRegression,
    ConfigurationValidation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackPlan {
    pub steps: Vec<RollbackStep>,
    pub validation_points: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackStep {
    pub description: String,
    pub action: RollbackAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RollbackAction {
    RestoreConfiguration,
    RestoreConnections,
    RestoreActors,
    RestoreData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationResult {
    pub success: bool,
    pub completed_steps: Vec<String>,
    pub failed_steps: Vec<MigrationFailure>,
    pub performance_impact: Option<PerformanceImpact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationFailure {
    pub step_id: String,
    pub error_message: String,
    pub recovery_suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceImpact {
    pub throughput_change_percent: f64,
    pub latency_change_ms: f64,
    pub memory_change_mb: f64,
}

/// Storage trait for version information
pub trait VersionStorage: Send + Sync {
    fn store_version(&self, version: &FlowVersionInfo) -> Result<(), VersionStorageError>;
    fn get_version(
        &self,
        version_id: &VersionId,
    ) -> Result<Option<FlowVersionInfo>, VersionStorageError>;
    fn list_versions(&self, flow_id: &FlowId) -> Result<Vec<FlowVersionInfo>, VersionStorageError>;
    fn get_latest_version(
        &self,
        flow_id: &FlowId,
    ) -> Result<Option<FlowVersionInfo>, VersionStorageError>;
    fn search_versions(
        &self,
        query: &VersionQuery,
    ) -> Result<Vec<FlowVersionInfo>, VersionStorageError>;
}

#[derive(Debug, Clone)]
pub struct VersionQuery {
    pub flow_id: Option<FlowId>,
    pub semantic_version_range: Option<String>, // e.g., ">=1.0.0, <2.0.0"
    pub tags: Option<Vec<String>>,
    pub created_by: Option<UserId>,
    pub date_range: Option<(DateTime<Utc>, DateTime<Utc>)>,
}

#[derive(Debug, thiserror::Error)]
pub enum VersionStorageError {
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("Version not found")]
    NotFound,
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("Migration plan creation failed: {0}")]
    PlanCreation(String),
    #[error("Migration execution failed: {0}")]
    Execution(String),
    #[error("Validation failed: {0}")]
    Validation(String),
    #[error("Rollback failed: {0}")]
    Rollback(String),
}

// Implementation of the main versioning system
impl FlowVersioningSystem {
    pub fn new(version_storage: Arc<dyn VersionStorage>) -> Self {
        Self {
            version_storage,
            lineage_tracker: LineageTracker::new(),
            diff_engine: FlowDiffEngine,
            migration_assistant: MigrationAssistant::new(),
        }
    }

    /// Create a new version of a flow
    pub fn create_version(
        &mut self,
        flow_definition: FlowDefinition,
        parent_version: Option<VersionId>,
        changelog: VersionChangelog,
        created_by: UserId,
    ) -> Result<FlowVersionInfo, VersionStorageError> {
        let version_id = VersionId(Uuid::new_v4());
        let flow_id = FlowId(flow_definition.metadata.name.clone());

        // Calculate content hash
        let content_hash = self.calculate_content_hash(&flow_definition);

        // Determine semantic version
        let semantic_version =
            self.determine_semantic_version(&flow_id, &parent_version, &changelog)?;

        // Analyze compatibility
        let compatibility_info = if let Some(ref parent_id) = parent_version {
            self.analyze_compatibility(parent_id, &flow_definition)?
        } else {
            CompatibilityInfo {
                backward_compatible: true,
                forward_compatible: true,
                api_compatibility: ApiCompatibility {
                    compatible: true,
                    removed_apis: vec![],
                    added_apis: vec![],
                    changed_apis: vec![],
                },
                data_compatibility: DataCompatibility {
                    schema_compatible: true,
                    data_migration_required: false,
                    migration_complexity: MigrationComplexity::None,
                },
                runtime_compatibility: RuntimeCompatibility {
                    runtime_version_required: None,
                    dependency_changes: vec![],
                },
            }
        };

        let version_info = FlowVersionInfo {
            version_id: version_id.clone(),
            flow_id: flow_id.clone(),
            semantic_version,
            git_hash: self.get_git_hash(),
            content_hash,
            created_at: Utc::now(),
            created_by,
            parent_version,
            tags: vec![],
            changelog,
            flow_definition,
            compatibility_info,
        };

        // Store version
        self.version_storage.store_version(&version_info)?;

        // Update lineage
        self.lineage_tracker.add_version(&version_info);

        Ok(version_info)
    }

    /// Compare two versions
    pub fn compare_versions(
        &self,
        version_a_id: &VersionId,
        version_b_id: &VersionId,
    ) -> Result<FlowDiff, VersionStorageError> {
        let version_a = self
            .version_storage
            .get_version(version_a_id)?
            .ok_or(VersionStorageError::NotFound)?;
        let version_b = self
            .version_storage
            .get_version(version_b_id)?
            .ok_or(VersionStorageError::NotFound)?;

        Ok(self.diff_engine.compare_versions(&version_a, &version_b))
    }

    /// Get lineage for a flow
    pub fn get_flow_lineage(&self, flow_id: &FlowId) -> Result<LineageGraph, VersionStorageError> {
        self.lineage_tracker.get_lineage_graph(flow_id)
    }

    fn calculate_content_hash(&self, flow_definition: &FlowDefinition) -> String {
        let serialized = serde_json::to_string(flow_definition).unwrap();
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn determine_semantic_version(
        &self,
        flow_id: &FlowId,
        parent_version: &Option<VersionId>,
        changelog: &VersionChangelog,
    ) -> Result<Version, VersionStorageError> {
        if let Some(parent_id) = parent_version {
            let parent = self
                .version_storage
                .get_version(parent_id)?
                .ok_or(VersionStorageError::NotFound)?;

            let mut new_version = parent.semantic_version.clone();

            // Increment version based on changelog
            if changelog.breaking_changes.is_empty() {
                if changelog
                    .changes
                    .iter()
                    .any(|c| matches!(c.change_type, ChangeType::Added))
                {
                    new_version.minor += 1;
                    new_version.patch = 0;
                } else {
                    new_version.patch += 1;
                }
            } else {
                new_version.major += 1;
                new_version.minor = 0;
                new_version.patch = 0;
            }

            Ok(new_version)
        } else {
            Ok(Version::new(1, 0, 0))
        }
    }

    fn analyze_compatibility(
        &self,
        parent_version_id: &VersionId,
        new_flow_definition: &FlowDefinition,
    ) -> Result<CompatibilityInfo, VersionStorageError> {
        let parent_version = self
            .version_storage
            .get_version(parent_version_id)?
            .ok_or(VersionStorageError::NotFound)?;

        // Analyze compatibility between parent and new version
        // This would involve detailed analysis of APIs, data formats, etc.

        Ok(CompatibilityInfo {
            backward_compatible: true,
            forward_compatible: false,
            api_compatibility: ApiCompatibility {
                compatible: true,
                removed_apis: vec![],
                added_apis: vec![],
                changed_apis: vec![],
            },
            data_compatibility: DataCompatibility {
                schema_compatible: true,
                data_migration_required: false,
                migration_complexity: MigrationComplexity::None,
            },
            runtime_compatibility: RuntimeCompatibility {
                runtime_version_required: None,
                dependency_changes: vec![],
            },
        })
    }

    fn get_git_hash(&self) -> Option<String> {
        // Get git hash from environment or git command
        std::env::var("GIT_HASH").ok()
    }
}

impl LineageTracker {
    pub fn new() -> Self {
        Self {
            lineage_graph: LineageGraph {
                nodes: HashMap::new(),
                edges: Vec::new(),
            },
        }
    }

    pub fn add_version(&mut self, version_info: &FlowVersionInfo) {
        let node = LineageNode {
            version_id: version_info.version_id.clone(),
            flow_id: version_info.flow_id.clone(),
            version_info: version_info.clone(),
            execution_history: Vec::new(),
        };

        self.lineage_graph
            .nodes
            .insert(version_info.version_id.clone(), node);

        // Add edge from parent if exists
        if let Some(ref parent_id) = version_info.parent_version {
            let edge = LineageEdge {
                from_version: parent_id.clone(),
                to_version: version_info.version_id.clone(),
                relationship_type: LineageRelationship::DirectEvolution,
                metadata: HashMap::new(),
            };
            self.lineage_graph.edges.push(edge);
        }
    }

    pub fn get_lineage_graph(&self, flow_id: &FlowId) -> Result<LineageGraph, VersionStorageError> {
        // Filter lineage graph for specific flow
        let nodes: HashMap<_, _> = self
            .lineage_graph
            .nodes
            .iter()
            .filter(|(_, node)| &node.flow_id == flow_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let edges: Vec<_> = self
            .lineage_graph
            .edges
            .iter()
            .filter(|edge| {
                nodes.contains_key(&edge.from_version) || nodes.contains_key(&edge.to_version)
            })
            .cloned()
            .collect();

        Ok(LineageGraph { nodes, edges })
    }
}

impl MigrationAssistant {
    pub fn new() -> Self {
        Self {
            migration_strategies: HashMap::new(),
        }
    }

    pub fn register_strategy(&mut self, name: String, strategy: Box<dyn MigrationStrategy>) {
        self.migration_strategies.insert(name, strategy);
    }

    pub fn create_migration_plan(
        &self,
        from_version: &FlowVersionInfo,
        to_version: &FlowVersionInfo,
    ) -> Result<MigrationPlan, MigrationError> {
        for strategy in self.migration_strategies.values() {
            if strategy.can_handle(from_version, to_version) {
                return strategy.create_migration_plan(from_version, to_version);
            }
        }

        Err(MigrationError::PlanCreation(
            "No suitable migration strategy found".to_string(),
        ))
    }
}
