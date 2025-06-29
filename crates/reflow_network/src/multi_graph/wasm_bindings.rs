//! WASM bindings for multi_graph functionality
//! Provides JavaScript-accessible classes under the multi_graph namespace

use wasm_bindgen::prelude::*;
use serde_wasm_bindgen::{from_value, to_value};
use std::collections::HashMap;

use crate::graph::types::GraphExport;
use super::{
    GraphLoader, GraphValidator, GraphNormalizer, GraphMetadata, GraphComposer,
    GraphComposition, GraphSource, GraphNamespaceManager, NamespaceConflictPolicy,
    LoadError, ValidationError, NamespaceError, CompositionError,
    workspace::{WorkspaceDiscovery, WorkspaceConfig, WorkspaceComposition, NamespaceStrategy, DependencyResolutionStrategy}
};

// Helper function to convert Rust errors to JsValue
fn error_to_js(error: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&format!("Error: {}", error))
}

// ===== Multi-graph namespace =====

/// Namespace for multi-graph functionality
#[wasm_bindgen(js_name = "multi_graph")]
pub struct MultiGraphNamespace;

#[wasm_bindgen(js_class = "multi_graph")]
impl MultiGraphNamespace {
    /// Create a new GraphLoader instance
    #[wasm_bindgen(js_name = GraphLoader)]
    pub fn graph_loader() -> WasmGraphLoader {
        WasmGraphLoader {
            inner: super::GraphLoader::new(),
        }
    }

    /// Create a new GraphValidator instance  
    #[wasm_bindgen(js_name = GraphValidator)]
    pub fn graph_validator() -> WasmGraphValidator {
        WasmGraphValidator {
            inner: super::GraphValidator::new(),
        }
    }

    /// Create a new GraphNormalizer instance
    #[wasm_bindgen(js_name = GraphNormalizer)]
    pub fn graph_normalizer() -> WasmGraphNormalizer {
        WasmGraphNormalizer {
            inner: super::GraphNormalizer::new(),
        }
    }

    /// Create a new GraphComposer instance
    #[wasm_bindgen(js_name = GraphComposer)]
    pub fn graph_composer() -> WasmGraphComposer {
        WasmGraphComposer {
            inner: super::GraphComposer::new(),
        }
    }

    /// Create a new NamespaceManager instance
    #[wasm_bindgen(js_name = NamespaceManager)]
    pub fn namespace_manager() -> WasmNamespaceManager {
        WasmNamespaceManager {
            inner: super::GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve),
        }
    }

    /// Create a new BrowserWorkspace instance
    #[wasm_bindgen(js_name = BrowserWorkspace)]
    pub fn browser_workspace() -> WasmBrowserWorkspace {
        WasmBrowserWorkspace {}
    }

    /// Create GraphMetadata from a graph export
    #[wasm_bindgen(js_name = GraphMetadata)]
    pub fn graph_metadata_from_export(graph_js: &JsValue) -> Result<WasmGraphMetadata, JsValue> {
        let graph_export: GraphExport = from_value(graph_js.clone()).map_err(error_to_js)?;
        let metadata = super::GraphMetadata::from_graph_export(&graph_export);
        
        Ok(WasmGraphMetadata { inner: metadata })
    }
}

// ===== Core Graph Operations =====

#[wasm_bindgen]
pub struct WasmGraphLoader {
    inner: super::GraphLoader,
}

#[wasm_bindgen]
impl WasmGraphLoader {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGraphLoader {
        WasmGraphLoader {
            inner: super::GraphLoader::new(),
        }
    }

    /// Load a graph from JSON string content
    #[wasm_bindgen]
    pub async fn load_from_json(&self, json_content: &str) -> Result<JsValue, JsValue> {
        let source = GraphSource::JsonContent(json_content.to_string());
        
        match self.inner.load_graph(source).await {
            Ok(graph_export) => to_value(&graph_export).map_err(error_to_js),
            Err(e) => Err(error_to_js(e)),
        }
    }

    /// Load a graph from a URL using fetch API
    #[wasm_bindgen]
    pub async fn load_from_url(&self, url: &str) -> Result<JsValue, JsValue> {
        let source = GraphSource::NetworkApi(url.to_string());
        
        match self.inner.load_graph(source).await {
            Ok(graph_export) => to_value(&graph_export).map_err(error_to_js),
            Err(e) => Err(error_to_js(e)),
        }
    }

    /// Load multiple graphs from an array of sources
    #[wasm_bindgen]
    pub async fn load_multiple(&self, sources: js_sys::Array) -> Result<js_sys::Array, JsValue> {
        let mut graph_sources = Vec::new();
        
        for i in 0..sources.length() {
            let source_obj = sources.get(i);
            let source_info: SourceInfo = from_value(source_obj).map_err(error_to_js)?;
            
            let source = match source_info.source_type.as_str() {
                "json" => GraphSource::JsonContent(source_info.content),
                "url" => GraphSource::NetworkApi(source_info.content),
                _ => return Err(JsValue::from_str("Invalid source type. Use 'json' or 'url'.")),
            };
            
            graph_sources.push(source);
        }

        match self.inner.load_multiple_graphs(graph_sources).await {
            Ok(graphs) => {
                let result = js_sys::Array::new();
                for graph in graphs {
                    let js_graph = to_value(&graph).map_err(error_to_js)?;
                    result.push(&js_graph);
                }
                Ok(result)
            }
            Err(e) => Err(error_to_js(e)),
        }
    }
}

#[derive(serde::Deserialize)]
struct SourceInfo {
    source_type: String, // "json" or "url"
    content: String,     // JSON string or URL
}

#[wasm_bindgen]
pub struct WasmGraphValidator {
    inner: super::GraphValidator,
}

#[wasm_bindgen]
impl WasmGraphValidator {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGraphValidator {
        WasmGraphValidator {
            inner: super::GraphValidator::new(),
        }
    }

    /// Validate a graph export structure
    #[wasm_bindgen]
    pub fn validate(&self, graph_js: &JsValue) -> Result<(), JsValue> {
        let graph_export: GraphExport = from_value(graph_js.clone()).map_err(error_to_js)?;
        
        match self.inner.validate(&graph_export) {
            Ok(()) => Ok(()),
            Err(e) => Err(error_to_js(e)),
        }
    }
}

#[wasm_bindgen]
pub struct WasmGraphNormalizer {
    inner: super::GraphNormalizer,
}

#[wasm_bindgen]
impl WasmGraphNormalizer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGraphNormalizer {
        WasmGraphNormalizer {
            inner: super::GraphNormalizer::new(),
        }
    }

    /// Normalize a graph export (ensures consistent format)
    #[wasm_bindgen]
    pub fn normalize(&self, graph_js: &JsValue) -> Result<JsValue, JsValue> {
        let mut graph_export: GraphExport = from_value(graph_js.clone()).map_err(error_to_js)?;
        
        match self.inner.normalize(&mut graph_export) {
            Ok(()) => to_value(&graph_export).map_err(error_to_js),
            Err(e) => Err(error_to_js(e)),
        }
    }
}

#[wasm_bindgen]
pub struct WasmGraphMetadata {
    inner: super::GraphMetadata,
}

#[wasm_bindgen]
impl WasmGraphMetadata {
    /// Create metadata from a graph export
    #[wasm_bindgen]
    pub fn from_graph_export(graph_js: &JsValue) -> Result<WasmGraphMetadata, JsValue> {
        let graph_export: GraphExport = from_value(graph_js.clone()).map_err(error_to_js)?;
        let metadata = super::GraphMetadata::from_graph_export(&graph_export);
        
        Ok(WasmGraphMetadata { inner: metadata })
    }

    /// Get the namespace of this graph
    #[wasm_bindgen(getter)]
    pub fn namespace(&self) -> Option<String> {
        self.inner.namespace.clone()
    }

    /// Set the namespace of this graph
    #[wasm_bindgen(setter)]
    pub fn set_namespace(&mut self, namespace: Option<String>) {
        self.inner.namespace = namespace;
    }

    /// Get the version of this graph
    #[wasm_bindgen(getter)]
    pub fn version(&self) -> Option<String> {
        self.inner.version.clone()
    }

    /// Set the version of this graph
    #[wasm_bindgen(setter)]
    pub fn set_version(&mut self, version: Option<String>) {
        self.inner.version = version;
    }

    /// Get dependencies as a JavaScript array
    #[wasm_bindgen]
    pub fn dependencies(&self) -> js_sys::Array {
        let array = js_sys::Array::new();
        for dep in &self.inner.dependencies {
            array.push(&JsValue::from_str(dep));
        }
        array
    }

    /// Set dependencies from a JavaScript array
    #[wasm_bindgen]
    pub fn set_dependencies(&mut self, deps: js_sys::Array) {
        let mut dependencies = Vec::new();
        for i in 0..deps.length() {
            if let Some(dep_str) = deps.get(i).as_string() {
                dependencies.push(dep_str);
            }
        }
        self.inner.dependencies = dependencies;
    }

    /// Get exports as a JavaScript array
    #[wasm_bindgen]
    pub fn exports(&self) -> js_sys::Array {
        let array = js_sys::Array::new();
        for exp in &self.inner.exports {
            array.push(&JsValue::from_str(exp));
        }
        array
    }

    /// Set exports from a JavaScript array
    #[wasm_bindgen]
    pub fn set_exports(&mut self, exports: js_sys::Array) {
        let mut exp_list = Vec::new();
        for i in 0..exports.length() {
            if let Some(exp_str) = exports.get(i).as_string() {
                exp_list.push(exp_str);
            }
        }
        self.inner.exports = exp_list;
    }

    /// Inject this metadata into a graph export
    #[wasm_bindgen]
    pub fn inject_into_graph_export(&self, graph_js: &JsValue) -> Result<JsValue, JsValue> {
        let mut graph_export: GraphExport = from_value(graph_js.clone()).map_err(error_to_js)?;
        self.inner.inject_into_graph_export(&mut graph_export);
        to_value(&graph_export).map_err(error_to_js)
    }
}

// ===== Graph Composition =====

#[wasm_bindgen]
pub struct WasmGraphComposer {
    inner: super::GraphComposer,
}

#[wasm_bindgen]
impl WasmGraphComposer {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmGraphComposer {
        WasmGraphComposer {
            inner: super::GraphComposer::new(),
        }
    }

    /// Compose multiple graphs into a single graph
    #[wasm_bindgen]
    pub async fn compose_graphs(&mut self, composition_js: &JsValue) -> Result<JsValue, JsValue> {
        let composition: CompositionConfig = from_value(composition_js.clone()).map_err(error_to_js)?;
        
        // Convert JS composition to Rust GraphComposition
        let mut sources = Vec::new();
        for source_info in composition.sources {
            let source = match source_info.source_type.as_str() {
                "json" => GraphSource::JsonContent(source_info.content),
                "url" => GraphSource::NetworkApi(source_info.content),
                "graph" => {
                    // Direct graph export
                    let graph_export: GraphExport = serde_json::from_str(&source_info.content)
                        .map_err(|e| JsValue::from_str(&format!("Invalid graph JSON: {}", e)))?;
                    GraphSource::GraphExport(graph_export)
                }
                _ => return Err(JsValue::from_str("Invalid source type. Use 'json', 'url', or 'graph'.")),
            };
            sources.push(source);
        }

        let graph_composition = GraphComposition {
            sources,
            connections: Vec::new(), // TODO: Convert from JS
            shared_resources: Vec::new(), // TODO: Convert from JS
            properties: HashMap::new(),
            case_sensitive: Some(false),
            metadata: None,
        };

        match self.inner.compose_graphs(graph_composition).await {
            Ok(graph) => {
                // Convert Graph to GraphExport for JS consumption
                let graph_export = graph.export();
                to_value(&graph_export).map_err(error_to_js)
            }
            Err(e) => Err(error_to_js(e)),
        }
    }
}

#[derive(serde::Deserialize)]
struct CompositionConfig {
    sources: Vec<SourceInfo>,
    // TODO: Add connections and shared_resources
}

// ===== Namespace Management =====

#[wasm_bindgen]
pub struct WasmNamespaceManager {
    inner: super::GraphNamespaceManager,
}

#[wasm_bindgen]
impl WasmNamespaceManager {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmNamespaceManager {
        WasmNamespaceManager {
            inner: super::GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve),
        }
    }

    /// Register a graph and return its assigned namespace
    #[wasm_bindgen]
    pub fn register_graph(&mut self, graph_js: &JsValue) -> Result<String, JsValue> {
        let graph_export: GraphExport = from_value(graph_js.clone()).map_err(error_to_js)?;
        
        match self.inner.register_graph(&graph_export) {
            Ok(namespace) => Ok(namespace),
            Err(e) => Err(error_to_js(e)),
        }
    }

    /// Resolve a process path to get process information
    #[wasm_bindgen]
    pub fn resolve_process_path(&self, path: &str) -> Result<JsValue, JsValue> {
        match self.inner.resolve_process_path(path) {
            Ok(process_ref) => {
                let info = ProcessInfo {
                    qualified_name: process_ref.qualified_name.clone(),
                    local_name: process_ref.local_name.clone(),
                    component: process_ref.component.clone(),
                    visibility: match process_ref.visibility {
                        super::ProcessVisibility::Private => "private".to_string(),
                        super::ProcessVisibility::Shared => "shared".to_string(),
                        super::ProcessVisibility::Public => "public".to_string(),
                    },
                };
                to_value(&info).map_err(error_to_js)
            }
            Err(e) => Err(error_to_js(e)),
        }
    }
}

#[derive(serde::Serialize)]
struct ProcessInfo {
    qualified_name: String,
    local_name: String,
    component: String,
    visibility: String,
}

// ===== Browser Workspace Management =====

#[wasm_bindgen]
pub struct WasmBrowserWorkspace {
    // For browser-specific workspace management
}

#[wasm_bindgen]
impl WasmBrowserWorkspace {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmBrowserWorkspace {
        WasmBrowserWorkspace {}
    }

    /// Create a workspace composition from a collection of graphs
    #[wasm_bindgen]
    pub fn from_graph_collection(&self, graphs: js_sys::Array) -> Result<JsValue, JsValue> {
        let mut graph_exports = Vec::new();
        
        for i in 0..graphs.length() {
            let graph_js = graphs.get(i);
            let graph_export: GraphExport = from_value(graph_js).map_err(error_to_js)?;
            graph_exports.push(graph_export);
        }

        // Create a simple workspace composition
        let workspace_info = WorkspaceInfo {
            graphs: graph_exports,
            namespaces: HashMap::new(), // TODO: Build namespaces
        };

        to_value(&workspace_info).map_err(error_to_js)
    }

    /// Save workspace to browser storage (localStorage)
    #[wasm_bindgen]
    pub async fn save_to_browser_storage(&self, workspace_name: &str, workspace_js: &JsValue) -> Result<(), JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
        let storage = window.local_storage()
            .map_err(|_| JsValue::from_str("Failed to access localStorage"))?
            .ok_or_else(|| JsValue::from_str("localStorage not available"))?;

        let workspace_json = js_sys::JSON::stringify(workspace_js)
            .map_err(|_| JsValue::from_str("Failed to stringify workspace"))?;
        
        let key = format!("reflow_workspace_{}", workspace_name);
        storage.set_item(&key, &workspace_json.as_string().unwrap())
            .map_err(|_| JsValue::from_str("Failed to save to localStorage"))?;

        Ok(())
    }

    /// Load workspace from browser storage (localStorage)
    #[wasm_bindgen]
    pub async fn load_from_browser_storage(&self, workspace_name: &str) -> Result<JsValue, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
        let storage = window.local_storage()
            .map_err(|_| JsValue::from_str("Failed to access localStorage"))?
            .ok_or_else(|| JsValue::from_str("localStorage not available"))?;

        let key = format!("reflow_workspace_{}", workspace_name);
        let workspace_json = storage.get_item(&key)
            .map_err(|_| JsValue::from_str("Failed to access localStorage"))?
            .ok_or_else(|| JsValue::from_str("Workspace not found"))?;

        js_sys::JSON::parse(&workspace_json)
            .map_err(|_| JsValue::from_str("Failed to parse workspace JSON"))
    }

    /// List available workspaces in browser storage
    #[wasm_bindgen]
    pub fn list_workspaces(&self) -> Result<js_sys::Array, JsValue> {
        let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window object"))?;
        let storage = window.local_storage()
            .map_err(|_| JsValue::from_str("Failed to access localStorage"))?
            .ok_or_else(|| JsValue::from_str("localStorage not available"))?;

        let length = storage.length()
            .map_err(|_| JsValue::from_str("Failed to get storage length"))?;

        let workspaces = js_sys::Array::new();
        let prefix = "reflow_workspace_";

        for i in 0..length {
            if let Ok(Some(key)) = storage.key(i) {
                if key.starts_with(prefix) {
                    let workspace_name = &key[prefix.len()..];
                    workspaces.push(&JsValue::from_str(workspace_name));
                }
            }
        }

        Ok(workspaces)
    }
}

#[derive(serde::Serialize)]
struct WorkspaceInfo {
    graphs: Vec<GraphExport>,
    namespaces: HashMap<String, Vec<String>>,
}

// ===== Error Types =====

#[wasm_bindgen]
pub struct WasmLoadError {
    message: String,
}

#[wasm_bindgen]
impl WasmLoadError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

#[wasm_bindgen]
pub struct WasmValidationError {
    message: String,
}

#[wasm_bindgen]
impl WasmValidationError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

#[wasm_bindgen]
pub struct WasmNamespaceError {
    message: String,
}

#[wasm_bindgen]
impl WasmNamespaceError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}

#[wasm_bindgen]
pub struct WasmCompositionError {
    message: String,
}

#[wasm_bindgen]
impl WasmCompositionError {
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }
}
