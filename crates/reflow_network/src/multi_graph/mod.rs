use crate::graph::{
    Graph,
    types::{GraphConnection, GraphEdge, GraphExport, GraphGroup, GraphNode},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fmt::Debug,
};

use anyhow::Result;

pub mod workspace;

// WASM bindings module (only for WASM target)
#[cfg(target_arch = "wasm32")]
pub mod wasm_bindings;

pub type GenerationError = anyhow::Error;

// Graph source management using existing types
#[derive(Debug)]
pub enum GraphSource {
    JsonFile(String),         // Path to JSON file
    JsonContent(String),      // JSON string content
    GraphExport(GraphExport), // Direct GraphExport instance
    NetworkApi(String),       // API endpoint returning GraphExport
                              // Dynamic(Box<dyn GraphGenerator>),           // Runtime graph generation
}

impl Clone for GraphSource {
    fn clone(&self) -> Self {
        match self {
            Self::JsonFile(arg0) => Self::JsonFile(arg0.clone()),
            Self::JsonContent(arg0) => Self::JsonContent(arg0.clone()),
            Self::GraphExport(arg0) => Self::GraphExport(arg0.clone()),
            Self::NetworkApi(arg0) => Self::NetworkApi(arg0.clone()),
            // Self::Dynamic(arg0) => Self::Dynamic(*arg0),
        }
    }
}

pub trait GraphGenerator: Send + Sync + Debug {
    fn generate(&self) -> Result<GraphExport, GenerationError>;
}

// Re-export the two-tier system types
pub use crate::graph::types::{
    WorkspaceGraphExport, WorkspaceMetadata, WorkspaceFileFormat,
    ResolvedDependency, AutoDiscoveredConnection, InterfaceAnalysis,
    DependencyResolutionStatus, DiscoveryMethod, InterfaceTypeMismatch,
    MismatchSeverity,
    // First tier types (already extended GraphExport)
    GraphDependency, ExternalConnection, InterfaceDefinition
};



// Metadata enhancement for existing GraphExport
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct GraphMetadata {
    pub namespace: Option<String>,   // Preferred namespace
    pub version: Option<String>,     // Graph version
    pub dependencies: Vec<String>,   // Other graph names this depends on
    pub exports: Vec<String>,        // Process names to expose publicly
    pub tags: Vec<String>,           // Classification tags
    pub description: Option<String>, // Human readable description
}

impl GraphMetadata {
    /// Extract metadata from GraphExport.properties
    pub fn from_graph_export(export: &GraphExport) -> Self {
        let props = &export.properties;

        GraphMetadata {
            namespace: props
                .get("namespace")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            version: props
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            dependencies: props
                .get("dependencies")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            exports: props
                .get("exports")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            tags: props
                .get("tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            description: props
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
        }
    }

    /// Inject metadata into GraphExport.properties
    pub fn inject_into_graph_export(&self, export: &mut GraphExport) {
        if let Some(namespace) = &self.namespace {
            export.properties.insert(
                "namespace".to_string(),
                serde_json::Value::String(namespace.clone()),
            );
        }
        if let Some(version) = &self.version {
            export.properties.insert(
                "version".to_string(),
                serde_json::Value::String(version.clone()),
            );
        }
        if !self.dependencies.is_empty() {
            export.properties.insert(
                "dependencies".to_string(),
                serde_json::Value::Array(
                    self.dependencies
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        if !self.exports.is_empty() {
            export.properties.insert(
                "exports".to_string(),
                serde_json::Value::Array(
                    self.exports
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        if !self.tags.is_empty() {
            export.properties.insert(
                "tags".to_string(),
                serde_json::Value::Array(
                    self.tags
                        .iter()
                        .map(|s| serde_json::Value::String(s.clone()))
                        .collect(),
                ),
            );
        }
        if let Some(description) = &self.description {
            export.properties.insert(
                "description".to_string(),
                serde_json::Value::String(description.clone()),
            );
        }
    }
}

// Graph loader working with existing types
#[derive(Serialize, Deserialize)]
pub struct GraphLoader {
    validator: GraphValidator,
    normalizer: GraphNormalizer,
}

impl GraphLoader {
    pub fn new() -> Self {
        GraphLoader {
            validator: GraphValidator::new(),
            normalizer: GraphNormalizer::new(),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn load_graph(&self, source: GraphSource) -> Result<GraphExport, LoadError> {
        let mut graph_export = match source {
            GraphSource::JsonFile(path) => {
                let content = tokio::fs::read_to_string(&path).await?;
                serde_json::from_str::<GraphExport>(&content)?
            }
            GraphSource::JsonContent(content) => serde_json::from_str::<GraphExport>(&content)?,
            GraphSource::GraphExport(export) => export,
            GraphSource::NetworkApi(url) => {
                let response = reqwest::get(&url).await?;
                response.json::<GraphExport>().await?
            }
            // GraphSource::Dynamic(generator) => {
            //     generator.generate()?
            // },
        };

        // Validate the graph
        self.validator.validate(&graph_export)?;

        // Normalize the graph (ensure consistent format)
        self.normalizer.normalize(&mut graph_export)?;

        Ok(graph_export)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn load_graph(&self, source: GraphSource) -> Result<GraphExport, LoadError> {
        let mut graph_export = match source {
            GraphSource::JsonFile(_path) => {
                return Err(LoadError::IoError(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "File system access not supported in WASM target. Use JsonContent or GraphExport instead."
                )));
            }
            GraphSource::JsonContent(content) => serde_json::from_str::<GraphExport>(&content)?,
            GraphSource::GraphExport(export) => export,
            GraphSource::NetworkApi(url) => {
                // Use web-sys fetch for WASM
                return self.load_graph_from_url_wasm(&url).await;
            }
            // GraphSource::Dynamic(generator) => {
            //     generator.generate()?
            // },
        };

        // Validate the graph
        self.validator.validate(&graph_export)?;

        // Normalize the graph (ensure consistent format)
        self.normalizer.normalize(&mut graph_export)?;

        Ok(graph_export)
    }

    #[cfg(target_arch = "wasm32")]
    async fn load_graph_from_url_wasm(&self, url: &str) -> Result<GraphExport, LoadError> {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::{Request, RequestInit, RequestMode, Response};

        let mut opts = RequestInit::new();
        opts.set_method("GET");
        opts.set_mode(RequestMode::Cors);

        let request = Request::new_with_str_and_init(url, &opts)
            .map_err(|e| LoadError::HttpError(format!("Failed to create request: {:?}", e)))?;

        let window = web_sys::window().ok_or_else(|| {
            LoadError::HttpError("Failed to get window object".to_string())
        })?;

        let resp_value = JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| LoadError::HttpError(format!("Network error: {:?}", e)))?;

        let resp: Response = resp_value.dyn_into()
            .map_err(|e| LoadError::HttpError(format!("Invalid response: {:?}", e)))?;

        let json = JsFuture::from(resp.json()
            .map_err(|e| LoadError::HttpError(format!("Failed to get JSON: {:?}", e)))?)
            .await
            .map_err(|e| LoadError::HttpError(format!("Failed to parse JSON: {:?}", e)))?;

        let graph_export: GraphExport = serde_wasm_bindgen::from_value(json)
            .map_err(|e| LoadError::JsonError(serde_json::Error::io(
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("WASM JSON parse error: {:?}", e))
            )))?;

        Ok(graph_export)
    }

    pub async fn load_multiple_graphs(
        &self,
        sources: Vec<GraphSource>,
    ) -> Result<Vec<GraphExport>, LoadError> {
        let mut graphs = Vec::new();

        for source in sources {
            let graph = self.load_graph(source).await?;
            graphs.push(graph);
        }

        Ok(graphs)
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphValidator {
    // Validation rules for GraphExport
}

impl GraphValidator {
    pub fn new() -> Self {
        GraphValidator {}
    }

    pub fn validate(&self, graph: &GraphExport) -> Result<(), ValidationError> {
        // Validate required properties
        if !graph.properties.contains_key("name") {
            return Err(ValidationError::MissingProperty("name".to_string()));
        }

        // Validate processes exist for all connections
        for connection in &graph.connections {
            if let Some(data) = &connection.data {
                // This is an IIP (Initial Information Packet)
                if !graph.processes.contains_key(&connection.to.node_id) {
                    return Err(ValidationError::InvalidConnection(format!(
                        "IIP target process '{}' not found",
                        connection.to.node_id
                    )));
                }
            } else {
                // Regular connection
                if !graph.processes.contains_key(&connection.from.node_id) {
                    return Err(ValidationError::InvalidConnection(format!(
                        "Source process '{}' not found",
                        connection.from.node_id
                    )));
                }
                if !graph.processes.contains_key(&connection.to.node_id) {
                    return Err(ValidationError::InvalidConnection(format!(
                        "Target process '{}' not found",
                        connection.to.node_id
                    )));
                }
            }
        }

        // Validate inports/outports reference valid processes
        for (_, inport) in &graph.inports {
            if !graph.processes.contains_key(&inport.node_id) {
                return Err(ValidationError::InvalidPort(format!(
                    "Inport references non-existent process '{}'",
                    inport.node_id
                )));
            }
        }

        for (_, outport) in &graph.outports {
            if !graph.processes.contains_key(&outport.node_id) {
                return Err(ValidationError::InvalidPort(format!(
                    "Outport references non-existent process '{}'",
                    outport.node_id
                )));
            }
        }

        // Validate groups reference valid processes
        for group in &graph.groups {
            for node_id in &group.nodes {
                if !graph.processes.contains_key(node_id) {
                    return Err(ValidationError::InvalidGroup(format!(
                        "Group '{}' references non-existent process '{}'",
                        group.id, node_id
                    )));
                }
            }
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
pub struct GraphNormalizer;

impl GraphNormalizer {
    pub fn new() -> Self {
        GraphNormalizer
    }

    pub fn normalize(&self, graph: &mut GraphExport) -> Result<()> {
        // Ensure name property exists
        if !graph.properties.contains_key("name") {
            graph.properties.insert(
                "name".to_string(),
                serde_json::Value::String("unnamed_graph".to_string()),
            );
        }

        // Normalize connection metadata
        for connection in &mut graph.connections {
            if connection.metadata.is_none() {
                connection.metadata = Some(HashMap::new());
            }
        }

        // Normalize process metadata
        for (_, process) in &mut graph.processes {
            if process.metadata.is_none() {
                process.metadata = Some(HashMap::new());
            }
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("HTTP error: {0}")]
    HttpError(String),
    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationError),
    #[error("Generation error: {0}")]
    GenerationError(#[from] GenerationError),
}

#[cfg(not(target_arch = "wasm32"))]
impl From<reqwest::Error> for LoadError {
    fn from(err: reqwest::Error) -> Self {
        LoadError::HttpError(err.to_string())
    }
}


#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Missing required property: {0}")]
    MissingProperty(String),
    #[error("Invalid connection: {0}")]
    InvalidConnection(String),
    #[error("Invalid port: {0}")]
    InvalidPort(String),
    #[error("Invalid group: {0}")]
    InvalidGroup(String),
}

// Namespace manager for graph composition using existing types
pub struct GraphNamespaceManager {
    namespace_mappings: HashMap<String, String>, // graph_name -> namespace
    process_registry: HashMap<String, ProcessNamespace>, // namespace -> processes
    conflict_resolution: NamespaceConflictPolicy,
    reserved_namespaces: HashSet<String>,
}

#[derive(Debug, Clone)]
pub struct ProcessNamespace {
    pub namespace_path: String, // e.g., "data_flow", "ml_pipeline"
    pub graph_name: String,     // Source graph name
    pub processes: HashMap<String, ProcessReference>, // process_name -> reference
    pub inports: HashMap<String, PortReference>, // inport_name -> reference
    pub outports: HashMap<String, PortReference>, // outport_name -> reference
    pub groups: HashMap<String, GroupReference>, // group_name -> reference
}

#[derive(Debug, Clone)]
pub struct ProcessReference {
    pub qualified_name: String, // Full path: "data_flow/collector"
    pub local_name: String,     // Name within graph: "collector"
    pub component: String,      // Actor component type
    pub metadata: Option<HashMap<String, Value>>, // Process metadata
    pub visibility: ProcessVisibility,
}

#[derive(Debug, Clone)]
pub enum ProcessVisibility {
    Private, // Only accessible within same graph
    Shared,  // Accessible to other graphs in composition
    Public,  // Exposed as external interface
}

#[derive(Debug, Clone)]
pub struct PortReference {
    pub qualified_name: String, // Full path with port
    pub graph_edge: GraphEdge,  // Original edge definition
    pub port_type: PortType,    // Input or Output
}

#[derive(Debug, Clone)]
pub enum PortType {
    Input,
    Output,
}

#[derive(Debug, Clone)]
pub struct GroupReference {
    pub qualified_name: String,       // Full path: "data_flow/processors"
    pub local_name: String,           // Name within graph: "processors"
    pub qualified_nodes: Vec<String>, // Fully qualified node names
    pub metadata: Option<HashMap<String, Value>>,
}

impl GraphNamespaceManager {
    pub fn new(policy: NamespaceConflictPolicy) -> Self {
        GraphNamespaceManager {
            namespace_mappings: HashMap::new(),
            process_registry: HashMap::new(),
            conflict_resolution: policy,
            reserved_namespaces: HashSet::from([
                "system".to_string(),
                "shared".to_string(),
                "global".to_string(),
            ]),
        }
    }

    pub fn register_graph(&mut self, graph: &GraphExport) -> Result<String, NamespaceError> {
        let metadata = GraphMetadata::from_graph_export(graph);

        // Determine namespace for this graph
        let namespace = self.determine_namespace(graph, &metadata)?;

        // Check for conflicts
        let graph_name = self.extract_graph_name(graph);
        if let Some(existing) = self.namespace_mappings.get(&graph_name).cloned() {
            if &existing != &namespace {
                return self.handle_namespace_conflict(&graph_name, &existing, &namespace);
            }
        }

        // Register processes in namespace
        let process_namespace = self.create_process_namespace(graph, &namespace, &metadata)?;

        self.namespace_mappings
            .insert(graph_name, namespace.clone());
        self.process_registry
            .insert(namespace.clone(), process_namespace);

        Ok(namespace)
    }

    fn handle_namespace_conflict(&mut self, graph_name:&str, existing: &str, namespace: &str) -> Result<String, NamespaceError> {
        unimplemented!("Resolve namespace conflict");
    }

    fn extract_graph_name(&self, graph: &GraphExport) -> String {
        graph
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed_graph")
            .to_string()
    }

    fn determine_namespace(
        &self,
        graph: &GraphExport,
        metadata: &GraphMetadata,
    ) -> Result<String, NamespaceError> {
        // Priority order:
        // 1. Explicit namespace in metadata
        // 2. Graph name (if no conflicts)
        // 3. Graph name with version suffix
        // 4. Generated unique namespace

        if let Some(requested_namespace) = &metadata.namespace {
            if self.is_namespace_available(requested_namespace) {
                return Ok(requested_namespace.clone());
            }
        }

        // Try graph name
        let base_name = self.sanitize_namespace_name(&self.extract_graph_name(graph));
        if self.is_namespace_available(&base_name) {
            return Ok(base_name);
        }

        // Try with version
        if let Some(version) = &metadata.version {
            let versioned_name = format!("{}_{}", base_name, version);
            if self.is_namespace_available(&versioned_name) {
                return Ok(versioned_name);
            }
        }

        // Generate unique namespace
        for i in 1..=999 {
            let candidate = format!("{}_{}", base_name, i);
            if self.is_namespace_available(&candidate) {
                return Ok(candidate);
            }
        }

        Err(NamespaceError::NoAvailableNamespace(base_name))
    }

    fn create_process_namespace(
        &self,
        graph: &GraphExport,
        namespace: &str,
        metadata: &GraphMetadata,
    ) -> Result<ProcessNamespace, NamespaceError> {
        let mut processes = HashMap::new();
        let mut inports = HashMap::new();
        let mut outports = HashMap::new();
        let mut groups = HashMap::new();

        // Register processes
        for (process_name, process_def) in &graph.processes {
            let qualified_name = format!("{}/{}", namespace, process_name);

            // Determine visibility
            let visibility = if metadata.exports.contains(process_name) {
                ProcessVisibility::Public
            } else if process_def
                .metadata
                .as_ref()
                .and_then(|m| m.get("shared"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                ProcessVisibility::Shared
            } else {
                ProcessVisibility::Private
            };

            let process_ref = ProcessReference {
                qualified_name: qualified_name.clone(),
                local_name: process_name.clone(),
                component: process_def.component.clone(),
                metadata: process_def.metadata.clone(),
                visibility,
            };

            processes.insert(process_name.clone(), process_ref);
        }

        // Register inports
        for (inport_name, graph_edge) in &graph.inports {
            let qualified_name = format!("{}/{}", namespace, inport_name);
            let port_ref = PortReference {
                qualified_name,
                graph_edge: graph_edge.clone(),
                port_type: PortType::Input,
            };
            inports.insert(inport_name.clone(), port_ref);
        }

        // Register outports
        for (outport_name, graph_edge) in &graph.outports {
            let qualified_name = format!("{}/{}", namespace, outport_name);
            let port_ref = PortReference {
                qualified_name,
                graph_edge: graph_edge.clone(),
                port_type: PortType::Output,
            };
            outports.insert(outport_name.clone(), port_ref);
        }

        // Register groups
        for group in &graph.groups {
            let qualified_name = format!("{}/{}", namespace, group.id);
            let qualified_nodes: Vec<String> = group
                .nodes
                .iter()
                .map(|node| format!("{}/{}", namespace, node))
                .collect();

            let group_ref = GroupReference {
                qualified_name,
                local_name: group.id.clone(),
                qualified_nodes,
                metadata: group.metadata.clone(),
            };
            groups.insert(group.id.clone(), group_ref);
        }

        Ok(ProcessNamespace {
            namespace_path: namespace.to_string(),
            graph_name: self.extract_graph_name(graph),
            processes,
            inports,
            outports,
            groups,
        })
    }

    pub fn resolve_process_path(&self, path: &str) -> Result<&ProcessReference, NamespaceError> {
        let parts: Vec<&str> = path.split('/').collect();

        if parts.len() != 2 {
            return Err(NamespaceError::InvalidPath(path.to_string()));
        }

        let namespace = parts[0];
        let process_name = parts[1];

        if let Some(process_namespace) = self.process_registry.get(namespace) {
            if let Some(process_ref) = process_namespace.processes.get(process_name) {
                return Ok(process_ref);
            }
        }

        Err(NamespaceError::ProcessNotFound(path.to_string()))
    }

    fn sanitize_namespace_name(&self, name: &str) -> String {
        name.chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }

    fn is_namespace_available(&self, namespace: &str) -> bool {
        !self.reserved_namespaces.contains(namespace)
            && !self.process_registry.contains_key(namespace)
    }
}

#[derive(Debug, Clone)]
pub enum NamespaceConflictPolicy {
    AutoResolve,   // Automatically generate alternative namespace
    VersionSuffix, // Add version suffix to conflicting namespaces
    Fail,          // Fail composition on namespace conflicts
}

#[derive(Debug, thiserror::Error)]
pub enum NamespaceError {
    #[error("No available namespace for graph: {0}")]
    NoAvailableNamespace(String),
    #[error("Invalid path format: {0}")]
    InvalidPath(String),
    #[error("Process not found: {0}")]
    ProcessNotFound(String),
    #[error("Namespace conflict: {0}")]
    NamespaceConflict(String),
}

// Main graph composition system working with existing Graph types
pub struct GraphComposer {
    loader: GraphLoader,
    namespace_manager: GraphNamespaceManager,
    dependency_resolver: DependencyResolver,
}

impl GraphComposer {
    pub fn new() -> Self {
        GraphComposer {
            loader: GraphLoader::new(),
            namespace_manager: GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve),
            dependency_resolver: DependencyResolver::new(),
        }
    }

    pub async fn compose_graphs(
        &mut self,
        composition: GraphComposition,
    ) -> Result<Graph, CompositionError> {
        // 1. Load all graphs
        let mut graph_exports = Vec::new();
        for source in &composition.sources {
            let graph_export = self.loader.load_graph(source.clone()).await?;
            graph_exports.push(graph_export);
        }

        // 2. Resolve dependencies
        let ordered_graphs = self
            .dependency_resolver
            .resolve_dependencies(&graph_exports)?;

        // 3. Register graphs in namespaces
        let mut namespace_assignments = HashMap::new();
        for graph in &ordered_graphs {
            let namespace = self.namespace_manager.register_graph(graph)?;
            let graph_name = graph
                .properties
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unnamed")
                .to_string();
            namespace_assignments.insert(graph_name, namespace);
        }

        // 4. Create composed graph
        let composed_graph_export = self.create_composed_graph_export(
            &ordered_graphs,
            &namespace_assignments,
            &composition,
        )?;

        // 5. Convert to Graph instance
        let composed_graph = Graph::load(composed_graph_export, composition.metadata);

        Ok(composed_graph)
    }

    fn create_composed_graph_export(
        &self,
        graphs: &[GraphExport],
        namespace_assignments: &HashMap<String, String>,
        composition: &GraphComposition,
    ) -> Result<GraphExport, CompositionError> {
        let mut composed = GraphExport {
            case_sensitive: composition.case_sensitive.unwrap_or(false),
            properties: composition.properties.clone(),
            inports: HashMap::new(),
            outports: HashMap::new(),
            groups: Vec::new(),
            processes: HashMap::new(),
            connections: Vec::new(),
            graph_dependencies: Vec::new(),
            external_connections: Vec::new(),
            provided_interfaces: HashMap::new(),
            required_interfaces: HashMap::new(),
        };

        // Add processes from all graphs with namespace prefixes
        for graph in graphs {
            let graph_name = graph
                .properties
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unnamed")
                .to_string();

            let namespace = namespace_assignments
                .get(&graph_name)
                .ok_or_else(|| CompositionError::NamespaceNotFound(graph_name.clone()))?;

            // Add namespaced processes
            for (process_name, process_def) in &graph.processes {
                let qualified_name = format!("{}/{}", namespace, process_name);
                composed
                    .processes
                    .insert(qualified_name, process_def.clone());
            }

            // Add namespaced connections
            for connection in &graph.connections {
                let mut namespaced_connection = connection.clone();

                // Namespace the connection endpoints
                if connection.data.is_none() {
                    // Regular connection - namespace both from and to
                    namespaced_connection.from.node_id =
                        format!("{}/{}", namespace, connection.from.node_id);
                }
                namespaced_connection.to.node_id =
                    format!("{}/{}", namespace, connection.to.node_id);

                composed.connections.push(namespaced_connection);
            }

            // Add namespaced groups
            for group in &graph.groups {
                let mut namespaced_group = group.clone();
                namespaced_group.id = format!("{}/{}", namespace, group.id);
                namespaced_group.nodes = group
                    .nodes
                    .iter()
                    .map(|node| format!("{}/{}", namespace, node))
                    .collect();
                composed.groups.push(namespaced_group);
            }

            // Add external inports/outports with namespace prefixes
            for (inport_name, graph_edge) in &graph.inports {
                let qualified_inport_name = format!("{}/{}", namespace, inport_name);
                let mut namespaced_edge = graph_edge.clone();
                namespaced_edge.node_id = format!("{}/{}", namespace, graph_edge.node_id);
                composed
                    .inports
                    .insert(qualified_inport_name, namespaced_edge);
            }

            for (outport_name, graph_edge) in &graph.outports {
                let qualified_outport_name = format!("{}/{}", namespace, outport_name);
                let mut namespaced_edge = graph_edge.clone();
                namespaced_edge.node_id = format!("{}/{}", namespace, graph_edge.node_id);
                composed
                    .outports
                    .insert(qualified_outport_name, namespaced_edge);
            }
        }

        // Add cross-graph connections from composition
        for connection in &composition.connections {
            let graph_connection = GraphConnection {
                from: GraphEdge {
                    node_id: connection.from.process.clone(),
                    port_id: connection.from.port.clone(),
                    index: connection.from.index,
                    ..Default::default()
                },
                to: GraphEdge {
                    node_id: connection.to.process.clone(),
                    port_id: connection.to.port.clone(),
                    index: connection.to.index,
                    ..Default::default()
                },
                metadata: connection.metadata.clone(),
                data: None, // Cross-graph connections are not IIPs
            };
            composed.connections.push(graph_connection);
        }

        // Add shared resources as processes
        for shared_resource in &composition.shared_resources {
            #[cfg(not(target_arch = "wasm32"))]
            let id = uuid::Uuid::new_v4().to_string();
            
            #[cfg(target_arch = "wasm32")]
            let id = {
                use js_sys::Math;
                // Generate a simple UUID-like string for WASM
                format!("shared-{}-{}", Math::random(), shared_resource.name)
            };
            
            let shared_process = GraphNode {
                id,
                component: shared_resource.component.clone(),
                metadata: shared_resource.metadata.clone(),
            };
            composed
                .processes
                .insert(shared_resource.name.clone(), shared_process);
        }

        Ok(composed)
    }
}

// Composition configuration using existing graph concepts
#[derive(Debug, Clone)]
pub struct GraphComposition {
    pub sources: Vec<GraphSource>,                // Graphs to compose
    pub connections: Vec<CompositionConnection>,  // Cross-graph connections
    pub shared_resources: Vec<SharedResource>,    // Shared processes
    pub properties: HashMap<String, Value>,       // Composed graph properties
    pub case_sensitive: Option<bool>,             // Case sensitivity for composed graph
    pub metadata: Option<HashMap<String, Value>>, // Additional metadata
}

#[derive(Debug, Clone)]
pub struct CompositionConnection {
    pub from: CompositionEndpoint,
    pub to: CompositionEndpoint,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone)]
pub struct CompositionEndpoint {
    pub process: String,      // Qualified process name: "namespace/process"
    pub port: String,         // Port name
    pub index: Option<usize>, // Optional array index
}

#[derive(Debug, Clone)]
pub struct SharedResource {
    pub name: String,                             // Process name in composed graph
    pub component: String,                        // Actor 
    pub metadata: Option<HashMap<String, Value>>, // Process metadata
}

// Dependency resolution for graphs
pub struct DependencyResolver;

impl DependencyResolver {
    pub fn new() -> Self {
        DependencyResolver
    }

    pub fn resolve_dependencies(
        &self,
        graphs: &[GraphExport],
    ) -> Result<Vec<GraphExport>, DependencyError> {
        // Simple topological sort based on dependencies in properties
        let mut ordered = Vec::new();
        let mut remaining: Vec<_> = graphs.iter().collect();
        let mut added_names = HashSet::new();

        while !remaining.is_empty() {
            let mut progress = false;
            let mut i = 0;

            while i < remaining.len() {
                let graph = remaining[i];
                let dependencies = graph
                    .properties
                    .get("dependencies")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default();

                // Check if all dependencies are satisfied
                let satisfied = dependencies.iter().all(|dep| added_names.contains(*dep));

                if satisfied {
                    ordered.push(graph.clone());
                    let graph_name = graph
                        .properties
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unnamed")
                        .to_string();
                    added_names.insert(graph_name);
                    remaining.remove(i);
                    progress = true;
                } else {
                    i += 1;
                }
            }

            if !progress {
                return Err(DependencyError::CircularDependency);
            }
        }

        Ok(ordered)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CompositionError {
    #[error("Load error: {0}")]
    LoadError(#[from] LoadError),
    #[error("Namespace error: {0}")]
    NamespaceError(#[from] NamespaceError),
    #[error("Dependency error: {0}")]
    DependencyError(#[from] DependencyError),
    #[error("Namespace not found: {0}")]
    NamespaceNotFound(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DependencyError {
    #[error("Circular dependency detected")]
    CircularDependency,
    #[error("Missing dependency: {0}")]
    MissingDependency(String),
}
