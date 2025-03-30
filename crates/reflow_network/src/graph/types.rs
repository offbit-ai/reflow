use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::collections::HashSet;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub component: String,
    #[cfg_attr(target_arch = "wasm32", tsify(type = "Map<string, any> | undefined"))]
    pub metadata: Option<HashMap<String, Value>>,
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