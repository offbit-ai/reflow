use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::collections::HashSet;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Clone, Serialize, Deserialize, Debug, Default, PartialEq, Eq)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub component: String,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
    
    // Script runtime indicator (if this is a script actor)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_runtime: Option<ScriptRuntime>,
}

/// Runtime environment for script actors
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ScriptRuntime {
    Python,
    JavaScript,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub port_name: String,
    pub port_id: String,
    pub node_id: String,
    pub index: Option<usize>,
    /// Expose this port. If the graph is a subgraph, exposed port allow other graphs or nodes connect to a specific exposed port
    pub expose: bool,
    pub data: Option<Value>,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
    pub port_type: PortType,
}

/// Port types supported by the graph
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "type", content = "value")]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub enum PortType {
    #[default]
    #[serde(rename = "any")]
    Any,
    #[serde(rename = "flow")]
    Flow,
    #[serde(rename = "event")]
    Event,
    #[serde(rename = "boolean")]
    Boolean,
    #[serde(rename = "integer")]
    Integer,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "string")]
    String,
    #[serde(rename = "object")]
    Object(String),
    #[serde(rename = "array")]
    Array(Box<PortType>),
    #[serde(rename = "encoded")]
    Encoded,
    #[serde(rename = "stream")]
    Stream,
    #[serde(rename = "option")]
    Option(Box<PortType>),
}

#[cfg(target_arch = "wasm32")]
impl From<PortType> for JsValue {
    fn from(port_type: PortType) -> Self {
        use gloo_utils::format::JsValueSerdeExt;
        JsValue::from_serde(&port_type).unwrap()
    }
}

#[cfg(target_arch = "wasm32")]
impl TryFrom<JsValue> for PortType {
    type Error = serde_json::Error;

    fn try_from(value: JsValue) -> Result<Self, Self::Error> {
        use gloo_utils::format::JsValueSerdeExt;
        value.into_serde()
    }
}

// TypeScript type generation
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(typescript_custom_section))]
const TS_PORT_TYPE_DEF: &'static str = r#"
export type PortType =
  | { type: "flow" }
  | { type: "event" }
  | { type: "boolean" }
  | { type: "integer" }
  | { type: "float" }
  | { type: "string" }
  | { type: "object", value: string }
  | { type: "array", value: PortType }
  | { type: "stream" }
  | { type: "encoded" }
  | { type: "any" }
  | { type: "option", value: PortType };
"#;

// #[cfg(target_arch = "wasm32")]
// #[wasm_bindgen]
// impl PortType {
//     #[wasm_bindgen(js_name = "Any")]
//     pub fn any() -> Self {
//         Self::Any
//     }

//     #[wasm_bindgen(js_name = "Flow")]
//     pub fn flow() -> Self {
//         Self::Flow
//     }

//     #[wasm_bindgen(js_name = "Event")]
//     pub fn event() -> Self {
//         Self::Event
//     }
//     #[wasm_bindgen(js_name = "Boolean")]
//     pub fn boolean() -> Self {
//         Self::Boolean
//     }
//     #[wasm_bindgen(js_name = "Integer")]
//     pub fn integer() -> Self {
//         Self::Integer
//     }
//     #[wasm_bindgen(js_name = "Float")]
//     pub fn float() -> Self {
//         Self::Float
//     }
//     #[wasm_bindgen(js_name = "String")]
//     pub fn string() -> Self {
//         Self::String
//     }
//     #[wasm_bindgen(js_name = "Object")]
//     pub fn object(value: String) -> Self {
//         Self::Object(value)
//     }
//     #[wasm_bindgen(js_name = "Array")]
//     pub fn array(value: PortType) -> Self {
//         Self::Array(Box::new(value))
//     }
//     #[wasm_bindgen(js_name = "Stream")]
//     pub fn stream() -> Self {
//         Self::Stream
//     }
//     #[wasm_bindgen(js_name = "Encoded")]
//     pub fn encoded() -> Self {
//         Self::Encoded
//     }
//     #[wasm_bindgen(js_name = "Option")]
//     pub fn option(value: PortType) -> Self {
//         Self::Option(Box::new(value))
//     }
//     #[wasm_bindgen(js_name = "isAny")]
//     pub fn is_any(&self) -> bool {
//         matches!(self, Self::Any)
//     }
//     #[wasm_bindgen(js_name = "isFlow")]
//     pub fn is_flow(&self) -> bool {
//         matches!(self, Self::Flow)
//     }
//     #[wasm_bindgen(js_name = "isEvent")]
//     pub fn is_event(&self) -> bool {
//         matches!(self, Self::Event)
//     }
//     #[wasm_bindgen(js_name = "isBoolean")]
//     pub fn is_boolean(&self) -> bool {
//         matches!(self, Self::Boolean)
//     }
//     #[wasm_bindgen(js_name = "isInteger")]
//     pub fn is_integer(&self) -> bool {
//         matches!(self, Self::Integer)
//     }
//     #[wasm_bindgen(js_name = "isFloat")]
//     pub fn is_float(&self) -> bool {
//         matches!(self, Self::Float)
//     }
//     #[wasm_bindgen(js_name = "isString")]
//     pub fn is_string(&self) -> bool {
//         matches!(self, Self::String)
//     }
//     #[wasm_bindgen(js_name = "isObject")]
//     pub fn is_object(&self) -> bool {
//         matches!(self, Self::Object(_))
//     }
//     #[wasm_bindgen(js_name = "isArray")]
//     pub fn is_array(&self) -> bool {
//         matches!(self, Self::Array(_))
//     }
//     #[wasm_bindgen(js_name = "isStream")]
//     pub fn is_stream(&self) -> bool {
//         matches!(self, Self::Stream)
//     }
//     #[wasm_bindgen(js_name = "isEncoded")]
//     pub fn is_encoded(&self) -> bool {
//         matches!(self, Self::Encoded)
//     }
//     #[wasm_bindgen(js_name = "isOption")]
//     pub fn is_option(&self) -> bool {
//         matches!(self, Self::Option(_))
//     }
//     // #[wasm_bindgen(js_name = "isCompatibleWith")]
//     // pub fn is_compatible_with(&self, other: Self) -> bool {
//     //     match (self, other) {
//     //         (_, PortType::Any) | (PortType::Any, _) => true,
//     //         (a, b) if *a == b => true,
//     //         (PortType::Array(a), PortType::Array(b)) => a.is_compatible_with(b.as_ref().clone()),
//     //         (PortType::Option(a), b) => a.is_compatible_with(b.clone()),
//     //         (a, PortType::Option(b)) => a.is_compatible_with(*b),
//     //         // (PortType::Generic(_), _) | (_, PortType::Generic(_)) => true,
//     //         // (PortType::Tuple(a), PortType::Tuple(b)) => {
//     //         //     a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| a.is_compatible_with(b))
//     //         // }
//     //         (PortType::Integer, PortType::Float) => true,
//     //         (PortType::Stream, _) | (_, PortType::Stream) => true,
//     //         (PortType::Float, PortType::Integer) => true,
//     //         (PortType::Encoded, PortType::Encoded) => true,
//     //         _ => false,
//     //     }
//     // }
// }


#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphConnection {
    pub from: GraphEdge,
    pub to: GraphEdge,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
    pub data: Option<Value>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct GraphIIP {
    pub to: GraphEdge,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "any"))]
    pub data: Value,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphGroup {
    pub id: String,
    pub nodes: Vec<String>,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
}

/// Graph dependency for workspace composition
#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphDependency {
    pub graph_name: String,
    pub namespace: Option<String>,
    pub version_constraint: Option<String>,
    pub required: bool,
    pub description: Option<String>,
}

/// External connection to other graphs in workspace
#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ExternalConnection {
    pub connection_id: String,
    pub target_graph: String,
    pub target_namespace: Option<String>,
    pub from_process: String,
    pub from_port: String,
    pub to_process: String,
    pub to_port: String,
    pub description: Option<String>,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
}

/// Interface definition for workspace graph interfaces
#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct InterfaceDefinition {
    pub interface_id: String,
    pub process_name: String,
    pub port_name: String,
    pub data_type: Option<String>,
    pub description: Option<String>,
    pub required: bool,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphExport {
    pub case_sensitive: bool,
    #[cfg_attr(
        target_arch = "wasm32",
        tsify(type = "Map<string, any>"),
        serde(default = "default_properties")
    )]
    pub properties: HashMap<String, Value>,
    #[serde(default = "default_port")]
    pub inports: HashMap<String, GraphEdge>,
    #[serde(default = "default_port")]
    pub outports: HashMap<String, GraphEdge>,
    #[serde(default = "default_groups")]
    pub groups: Vec<GraphGroup>,
    #[serde(default = "default_processes")]
    pub processes: HashMap<String, GraphNode>,
    #[serde(default = "default_connections")]
    pub connections: Vec<GraphConnection>,
    
    // New workspace fields (Optional for backward compatibility)
    #[serde(default = "default_graph_dependencies", skip_serializing_if = "Vec::is_empty")]
    pub graph_dependencies: Vec<GraphDependency>,
    
    #[serde(default = "default_external_connections", skip_serializing_if = "Vec::is_empty")]
    pub external_connections: Vec<ExternalConnection>,
    
    #[serde(default = "default_provided_interfaces", skip_serializing_if = "HashMap::is_empty")]
    pub provided_interfaces: HashMap<String, InterfaceDefinition>,
    
    #[serde(default = "default_required_interfaces", skip_serializing_if = "HashMap::is_empty")]
    pub required_interfaces: HashMap<String, InterfaceDefinition>,
}

pub fn default_properties() -> HashMap<String, Value> {
    return HashMap::from_iter([("name".to_string(), json!("My Graph"))]);
}

pub fn default_processes() -> HashMap<String, GraphNode> {
    return HashMap::new();
}

pub fn default_port() -> HashMap<String, GraphEdge> {
    return HashMap::new();
}

pub fn default_groups() -> Vec<GraphGroup> {
    return Vec::new();
}

pub fn default_connections() -> Vec<GraphConnection> {
    return Vec::new();
}

pub fn default_graph_dependencies() -> Vec<GraphDependency> {
    return Vec::new();
}

pub fn default_external_connections() -> Vec<ExternalConnection> {
    return Vec::new();
}

pub fn default_provided_interfaces() -> HashMap<String, InterfaceDefinition> {
    return HashMap::new();
}

pub fn default_required_interfaces() -> HashMap<String, InterfaceDefinition> {
    return HashMap::new();
}

type EventValue = Value;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(namespace))]
#[serde(tag = "_type")]
pub enum GraphEvents {
    AddNode(EventValue),
    RemoveNode(EventValue),
    RenameNode(EventValue),
    ChangeNode(EventValue),
    AddConnection(EventValue),
    RemoveConnection(EventValue),
    ChangeConnection(EventValue),
    AddInitial(EventValue),
    RemoveInitial(EventValue),
    ChangeProperties(EventValue),
    AddGroup(EventValue),
    RemoveGroup(EventValue),
    RenameGroup(EventValue),
    ChangeGroup(EventValue),
    AddInport(EventValue),
    RemoveInport(EventValue),
    RenameInport(EventValue),
    ChangeInport(EventValue),
    AddOutport(EventValue),
    RemoveOutport(EventValue),
    RenameOutport(EventValue),
    ChangeOutport(EventValue),
    StartTransaction(EventValue),
    EndTransaction(EventValue),
    Transaction(EventValue),
    #[default]
    None,
}

#[derive(Debug, Clone)]
pub enum GraphError {
    NodeNotFound(String),
    DuplicateNode(String),
    InvalidConnection { from: String, to: String },
    CycleDetected,
    InvalidOperation(String),
}

impl std::fmt::Display for GraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GraphError::NodeNotFound(id) => write!(f, "Node not found: {}", id),
            GraphError::DuplicateNode(id) => write!(f, "Node already exists: {}", id),
            GraphError::InvalidConnection { from, to } => {
                write!(f, "Invalid connection from {} to {}", from, to)
            }
            GraphError::CycleDetected => write!(f, "Cycle detected in graph"),
            GraphError::InvalidOperation(msg) => write!(f, "Invalid operation: {}", msg),
        }
    }
}

impl std::error::Error for GraphError {}

/// Second tier: Workspace-enhanced graph export with discovery metadata
#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceGraphExport {
    /// The core graph definition (first tier)
    #[serde(flatten)]
    pub graph: GraphExport,
    
    /// Workspace discovery metadata
    pub workspace_metadata: WorkspaceMetadata,
}

/// Metadata added during workspace discovery
#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMetadata {
    /// Discovered namespace based on file structure
    pub discovered_namespace: String,
    
    /// Original file path relative to workspace root
    pub source_path: String,
    
    /// File format detected
    pub source_format: WorkspaceFileFormat,
    
    /// Discovery timestamp
    pub discovered_at: String,
    
    /// File size in bytes
    pub file_size: u64,
    
    /// Last modified time of source file
    pub last_modified: Option<String>,
    
    /// Resolved dependencies after analysis
    pub resolved_dependencies: Vec<ResolvedDependency>,
    
    /// Auto-discovered connections to other graphs
    pub auto_connections: Vec<AutoDiscoveredConnection>,
    
    /// Interface compatibility analysis
    pub interface_analysis: InterfaceAnalysis,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub enum WorkspaceFileFormat {
    Json,
    Yaml,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct ResolvedDependency {
    /// The dependency graph name
    pub target_graph: String,
    
    /// Target graph's namespace
    pub target_namespace: String,
    
    /// Whether dependency was resolved successfully
    pub resolved: bool,
    
    /// Version constraint if specified
    pub version_constraint: Option<String>,
    
    /// Resolution status
    pub resolution_status: DependencyResolutionStatus,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub enum DependencyResolutionStatus {
    Resolved,
    NotFound,
    VersionMismatch,
    CircularDependency,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct AutoDiscoveredConnection {
    /// Unique identifier for this connection
    pub connection_id: String,
    
    /// Source graph name
    pub from_graph: String,
    
    /// Source graph namespace
    pub from_namespace: String,
    
    /// Source interface name
    pub from_interface: String,
    
    /// Target graph name
    pub to_graph: String,
    
    /// Target graph namespace
    pub to_namespace: String,
    
    /// Target interface name
    pub to_interface: String,
    
    /// Confidence score (0.0 to 1.0)
    pub confidence: f64,
    
    /// How this connection was discovered
    pub discovery_method: DiscoveryMethod,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub enum DiscoveryMethod {
    ExplicitDeclaration,
    InterfaceMatching,
    DataTypeCompatibility,
    NamingConvention,
    DependencyAnalysis,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct InterfaceAnalysis {
    /// Number of provided interfaces
    pub provided_count: usize,
    
    /// Number of required interfaces
    pub required_count: usize,
    
    /// Compatibility scores with other graphs
    pub compatibility_scores: HashMap<String, f64>,
    
    /// Interface type mismatches found
    pub type_mismatches: Vec<InterfaceTypeMismatch>,
    
    /// Unused provided interfaces
    pub unused_provided: Vec<String>,
    
    /// Unsatisfied required interfaces
    pub unsatisfied_required: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct InterfaceTypeMismatch {
    pub provided_interface: String,
    pub provided_type: Option<String>,
    pub required_interface: String,
    pub required_type: Option<String>,
    pub target_graph: String,
    pub severity: MismatchSeverity,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub enum MismatchSeverity {
    Warning,
    Error,
    Critical,
}

impl WorkspaceGraphExport {
    /// Create a WorkspaceGraphExport from a base GraphExport
    pub fn from_graph_export(graph: GraphExport, workspace_metadata: WorkspaceMetadata) -> Self {
        WorkspaceGraphExport {
            graph,
            workspace_metadata,
        }
    }
    
    /// Extract the base GraphExport
    pub fn into_graph_export(self) -> GraphExport {
        self.graph
    }
    
    /// Get a reference to the base GraphExport
    pub fn graph_export(&self) -> &GraphExport {
        &self.graph
    }
    
    /// Get a mutable reference to the base GraphExport
    pub fn graph_export_mut(&mut self) -> &mut GraphExport {
        &mut self.graph
    }
    
    /// Get the graph name
    pub fn graph_name(&self) -> Option<&str> {
        self.graph.properties.get("name").and_then(|v| v.as_str())
    }
    
    /// Get the discovered namespace
    pub fn namespace(&self) -> &str {
        &self.workspace_metadata.discovered_namespace
    }
    
    /// Check if this graph has unresolved dependencies
    pub fn has_unresolved_dependencies(&self) -> bool {
        self.workspace_metadata.resolved_dependencies.iter()
            .any(|dep| !dep.resolved)
    }
    
    /// Get all auto-discovered connections with confidence above threshold
    pub fn get_confident_auto_connections(&self, threshold: f64) -> Vec<&AutoDiscoveredConnection> {
        self.workspace_metadata.auto_connections.iter()
            .filter(|conn| conn.confidence >= threshold)
            .collect()
    }
    
    /// Check interface compatibility with another graph
    pub fn is_compatible_with(&self, other_graph_name: &str) -> Option<f64> {
        self.workspace_metadata.interface_analysis.compatibility_scores
            .get(other_graph_name)
            .copied()
    }
}

impl Default for WorkspaceMetadata {
    fn default() -> Self {
        WorkspaceMetadata {
            discovered_namespace: "default".to_string(),
            source_path: "unknown".to_string(),
            source_format: WorkspaceFileFormat::Json,
            discovered_at: chrono::Utc::now().to_rfc3339(),
            file_size: 0,
            last_modified: None,
            resolved_dependencies: Vec::new(),
            auto_connections: Vec::new(),
            interface_analysis: InterfaceAnalysis::default(),
        }
    }
}

impl Default for InterfaceAnalysis {
    fn default() -> Self {
        InterfaceAnalysis {
            provided_count: 0,
            required_count: 0,
            compatibility_scores: HashMap::new(),
            type_mismatches: Vec::new(),
            unused_provided: Vec::new(),
            unsatisfied_required: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct FlowValidation {
    pub cycles: Vec<Vec<String>>,
    pub orphaned_nodes: Vec<String>,
    pub port_mismatches: Vec<PortMismatch>,
}

#[derive(Debug)]
pub struct DataFlowPath {
    pub nodes: Vec<String>,
    pub transforms: Vec<DataTransform>,
}

#[derive(Debug)]
pub struct DataTransform {
    pub node: String,
    pub operation: String,
    pub input_type: String,
    pub output_type: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionPath {
    pub nodes: Vec<String>,
    pub estimated_time: f32,
    pub resource_requirements: HashMap<String, f32>,
}

#[derive(Debug, Default)]
pub struct ParallelismAnalysis {
    pub parallel_branches: Vec<Subgraph>,
    pub pipeline_stages: Vec<PipelineStage>,
    pub max_parallelism: usize,
}

#[derive(Debug)]
pub enum Bottleneck {
    HighDegree(String),
    SequentialChain(Vec<String>),
}

#[derive(Debug, Default)]
pub struct Subgraph {
    pub nodes: Vec<String>,
    pub internal_connections: Vec<GraphConnection>,
    pub entry_points: Vec<String>,
    pub exit_points: Vec<String>,
}

#[derive(Debug)]
pub struct PipelineStage {
    pub level: usize,
    pub nodes: Vec<String>,
}

/// Analysis results for a subgraph
#[derive(Debug, Clone)]
pub struct SubgraphAnalysis {
    pub node_count: usize,
    pub connection_count: usize,
    pub entry_points: Vec<String>,
    pub exit_points: Vec<String>,
    pub is_cyclic: bool,
    pub max_depth: usize,
    pub branching_factor: f64,
}

/// Cycle analysis result
#[derive(Debug)]
pub struct CycleAnalysis {
    pub total_cycles: usize,
    pub cycle_lengths: Vec<usize>,
    pub nodes_in_cycles: HashSet<String>,
    pub longest_cycle: Option<Vec<String>>,
    pub shortest_cycle: Option<Vec<String>>,
}

/// Detailed analysis of orphaned nodes
#[derive(Debug)]
pub struct OrphanedNodeAnalysis {
    pub total_orphaned: usize,
    pub completely_isolated: Vec<String>,
    pub unreachable: Vec<String>,
    pub disconnected_groups: Vec<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct PortMismatch {
    pub from_node: String,
    pub from_port: String,
    pub from_type: PortType,
    pub to_node: String,
    pub to_port: String,
    pub to_type: PortType,
    pub reason: String,
}

impl std::fmt::Display for PortMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Port type mismatch: {}:{} ({:?}) -> {}:{} ({:?}): {}",
            self.from_node,
            self.from_port,
            self.from_type,
            self.to_node,
            self.to_port,
            self.to_type,
            self.reason
        )
    }
}


#[derive(Clone, Debug)]
pub struct NodePosition {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct NodeDimensions {
    pub width: f32,
    pub height: f32,
    pub anchor: AnchorPoint,
}

#[derive(Clone, Debug)]
pub struct AnchorPoint {
    pub x: f32,  // Relative to node's left edge (0.0 to 1.0)
    pub y: f32,  // Relative to node's top edge (0.0 to 1.0)
}


/// Optimization suggestions for graph execution
#[derive(Clone)]
pub enum OptimizationSuggestion {
    ParallelizableChain { nodes: Vec<String> },
    RedundantNode { node: String, reason: String },
    ResourceBottleneck { resource: String, severity: f64 },
    DataTypeOptimization { from: String, to: String, suggestion: String },
}


/// Enhanced analysis result including performance predictions
#[derive(Default)]
pub struct EnhancedGraphAnalysis {
    pub parallelism: ParallelismAnalysis,
    pub estimated_execution_time: f64,
    pub resource_requirements: HashMap<String, f64>,
    pub optimization_suggestions: Vec<OptimizationSuggestion>,
    pub performance_bottlenecks: Vec<Bottleneck>,
}
