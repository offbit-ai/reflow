use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use reflow_graph::types::PortType;

/// Runtime environment for script actors
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScriptRuntime {
    Python,
    JavaScript,
}

impl ScriptRuntime {
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "py" => Some(ScriptRuntime::Python),
            "js" | "mjs" => Some(ScriptRuntime::JavaScript),
            _ => None,
        }
    }
    
    pub fn to_string(&self) -> String {
        match self {
            ScriptRuntime::Python => "python".to_string(),
            ScriptRuntime::JavaScript => "javascript".to_string(),
        }
    }
}

/// Discovered script actor information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredScriptActor {
    /// Component name
    pub component: String,
    /// Actor description
    pub description: String,
    /// File path to the script
    pub file_path: PathBuf,
    /// Script runtime
    pub runtime: ScriptRuntime,
    /// Input port definitions
    pub inports: Vec<PortDefinition>,
    /// Output port definitions
    pub outports: Vec<PortDefinition>,
    /// Workspace metadata
    pub workspace_metadata: ScriptActorMetadata,
}

/// Port definition for script actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDefinition {
    pub name: String,
    pub port_type: PortType,
    pub required: bool,
    pub description: String,
    pub default: Option<serde_json::Value>,
}

/// Script actor metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptActorMetadata {
    pub namespace: String,
    pub version: String,
    pub author: Option<String>,
    pub dependencies: Vec<String>,
    pub runtime_requirements: RuntimeRequirements,
    pub config_schema: Option<serde_json::Value>,
    pub tags: Vec<String>,
    pub category: Option<String>,
    pub source_hash: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

/// Runtime requirements for script actors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRequirements {
    pub runtime_version: String,
    pub memory_limit: String,
    pub cpu_limit: Option<f32>,
    pub timeout: u32,
    pub env_vars: HashMap<String, String>,
}

/// Script discovery configuration
#[derive(Debug, Clone)]
pub struct ScriptDiscoveryConfig {
    pub root_path: PathBuf,
    pub patterns: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub max_depth: Option<usize>,
    pub auto_register: bool,
    pub validate_metadata: bool,
}

impl Default for ScriptDiscoveryConfig {
    fn default() -> Self {
        Self {
            root_path: PathBuf::from("."),
            patterns: vec![
                "**/*.actor.py".to_string(),
                "**/*.actor.js".to_string(),
                "**/actors/*.py".to_string(),
                "**/actors/*.js".to_string(),
            ],
            excluded_paths: vec![
                "**/node_modules/**".to_string(),
                "**/__pycache__/**".to_string(),
                "**/venv/**".to_string(),
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
            ],
            max_depth: Some(10),
            auto_register: true,
            validate_metadata: true,
        }
    }
}

/// Result of script actor discovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredActors {
    pub actors: Vec<DiscoveredScriptActor>,
    pub failed: Vec<FailedActor>,
    pub namespaces: HashMap<String, Vec<String>>,
    pub discovery_time: chrono::DateTime<chrono::Utc>,
}

/// Failed actor discovery information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedActor {
    pub file_path: PathBuf,
    pub error: String,
}

/// Extracted metadata from script files
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMetadata {
    pub component: String,
    pub description: String,
    pub version: String,
    pub inports: Vec<PortDefinition>,
    pub outports: Vec<PortDefinition>,
    pub dependencies: Vec<String>,
    pub config_schema: Option<serde_json::Value>,
    pub tags: Vec<String>,
    pub category: Option<String>,
}