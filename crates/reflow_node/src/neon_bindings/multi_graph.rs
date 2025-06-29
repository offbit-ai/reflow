//! Multi-graph related Neon bindings
//! 
//! Provides Node.js bindings for multi-graph functionality with WASM API parity
//! Simplified implementation without complex async handling for initial release

use neon::prelude::*;
use reflow_network::{
    multi_graph::{
        GraphLoader, GraphValidator, GraphNormalizer, GraphMetadata, GraphComposer,
        GraphComposition, GraphSource, GraphNamespaceManager, NamespaceConflictPolicy,
    },
    graph::types::GraphExport,
};
use crate::neon_bindings::utils::*;
use crate::runtime;
use std::collections::HashMap;
use serde_json::Value as JsonValue;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ===== GraphLoader =====

pub struct NodeGraphLoader {
    inner: GraphLoader,
}

impl Finalize for NodeGraphLoader {}

impl NodeGraphLoader {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphLoader>> {
        let loader = GraphLoader::new();
        Ok(cx.boxed(NodeGraphLoader { inner: loader }))
    }

    fn load_from_json(mut cx: FunctionContext) -> JsResult<JsValue> {
        let _this = cx.this::<JsBox<NodeGraphLoader>>()?;
        let json_content = cx.argument::<JsString>(0)?.value(&mut cx);
        
        // Execute synchronously for now
        let result = runtime::execute_async(|| async move {
            let source = GraphSource::JsonContent(json_content);
            let loader = GraphLoader::new();
            loader.load_graph(source).await
        });
        
        match result {
            Ok(graph_export) => {
                match serde_json::to_value(&graph_export) {
                    Ok(json_value) => json_value_to_js(&mut cx, &json_value),
                    Err(e) => cx.throw_error(format!("Failed to serialize graph: {}", e))
                }
            }
            Err(e) => cx.throw_error(format!("Failed to load graph: {}", e))
        }
    }

    fn load_from_file(mut cx: FunctionContext) -> JsResult<JsValue> {
        let _this = cx.this::<JsBox<NodeGraphLoader>>()?;
        let file_path = cx.argument::<JsString>(0)?.value(&mut cx);
        
        let result = runtime::execute_async(|| async move {
            let source = GraphSource::JsonFile(file_path);
            let loader = GraphLoader::new();
            loader.load_graph(source).await
        });
        
        match result {
            Ok(graph_export) => {
                match serde_json::to_value(&graph_export) {
                    Ok(json_value) => json_value_to_js(&mut cx, &json_value),
                    Err(e) => cx.throw_error(format!("Failed to serialize graph: {}", e))
                }
            }
            Err(e) => cx.throw_error(format!("Failed to load graph: {}", e))
        }
    }

    fn load_from_url(mut cx: FunctionContext) -> JsResult<JsValue> {
        let _this = cx.this::<JsBox<NodeGraphLoader>>()?;
        let url = cx.argument::<JsString>(0)?.value(&mut cx);
        
        let result = runtime::execute_async(|| async move {
            let source = GraphSource::NetworkApi(url);
            let loader = GraphLoader::new();
            loader.load_graph(source).await
        });
        
        match result {
            Ok(graph_export) => {
                match serde_json::to_value(&graph_export) {
                    Ok(json_value) => json_value_to_js(&mut cx, &json_value),
                    Err(e) => cx.throw_error(format!("Failed to serialize graph: {}", e))
                }
            }
            Err(e) => cx.throw_error(format!("Failed to load graph: {}", e))
        }
    }

    fn load_multiple(mut cx: FunctionContext) -> JsResult<JsValue> {
        let _this = cx.this::<JsBox<NodeGraphLoader>>()?;
        let sources_array = cx.argument::<JsArray>(0)?;
        
        // Convert JS sources to Rust sources
        let mut graph_sources = Vec::new();
        for i in 0..sources_array.len(&mut cx) {
            let source_obj = sources_array.get::<JsObject, _, _>(&mut cx, i)?;
            
            let source_type_val = source_obj.get::<JsValue, _, _>(&mut cx, "type")?;
            let source_type = source_type_val.downcast::<JsString, _>(&mut cx).expect("Expecting to cast JS Value to String").value(&mut cx);
            
            let content_val = source_obj.get::<JsValue, _, _>(&mut cx, "content")?;
            let content = content_val.downcast::<JsString, _>(&mut cx).expect("Expecting to cast JS Value to String").value(&mut cx);
            
            let source = match source_type.as_str() {
                "json" => GraphSource::JsonContent(content),
                "file" => GraphSource::JsonFile(content),
                "url" => GraphSource::NetworkApi(content),
                _ => return cx.throw_error("Invalid source type. Use 'json', 'file', or 'url'."),
            };
            
            graph_sources.push(source);
        }
        
        let result = runtime::execute_async(|| async move {
            let loader = GraphLoader::new();
            loader.load_multiple_graphs(graph_sources).await
        });
        
        match result {
            Ok(graphs) => {
                let js_array = JsArray::new(&mut cx, graphs.len());
                for (i, graph) in graphs.iter().enumerate() {
                    match serde_json::to_value(graph) {
                        Ok(json_value) => {
                            let js_graph = json_value_to_js(&mut cx, &json_value)?;
                            js_array.set(&mut cx, i as u32, js_graph)?;
                        }
                        Err(e) => return cx.throw_error(format!("Failed to serialize graph {}: {}", i, e))
                    }
                }
                Ok(js_array.upcast())
            }
            Err(e) => cx.throw_error(format!("Failed to load graphs: {}", e))
        }
    }
}

// ===== GraphValidator =====

pub struct NodeGraphValidator {
    inner: GraphValidator,
}

impl Finalize for NodeGraphValidator {}

impl NodeGraphValidator {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphValidator>> {
        let validator = GraphValidator::new();
        Ok(cx.boxed(NodeGraphValidator { inner: validator }))
    }

    fn validate(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraphValidator>>()?;
        let graph_js = cx.argument::<JsValue>(0)?;
        
        let graph_json = js_to_json_value(&mut cx, graph_js)?;
        let graph_export: GraphExport = match serde_json::from_value(graph_json) {
            Ok(export) => export,
            Err(e) => return cx.throw_error(format!("Failed to deserialize graph: {}", e))
        };
        
        match this.inner.validate(&graph_export) {
            Ok(()) => Ok(cx.undefined()),
            Err(e) => cx.throw_error(format!("Validation failed: {}", e)),
        }
    }
}

// ===== GraphNormalizer =====

pub struct NodeGraphNormalizer {
    inner: GraphNormalizer,
}

impl Finalize for NodeGraphNormalizer {}

impl NodeGraphNormalizer {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphNormalizer>> {
        let normalizer = GraphNormalizer::new();
        Ok(cx.boxed(NodeGraphNormalizer { inner: normalizer }))
    }

    fn normalize(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<NodeGraphNormalizer>>()?;
        let graph_js = cx.argument::<JsValue>(0)?;
        
        let graph_json = js_to_json_value(&mut cx, graph_js)?;
        let mut graph_export: GraphExport = match serde_json::from_value(graph_json) {
            Ok(export) => export,
            Err(e) => return cx.throw_error(format!("Failed to deserialize graph: {}", e))
        };
        
        match this.inner.normalize(&mut graph_export) {
            Ok(()) => {
                match serde_json::to_value(&graph_export) {
                    Ok(normalized_json) => json_value_to_js(&mut cx, &normalized_json),
                    Err(e) => cx.throw_error(format!("Failed to serialize normalized graph: {}", e))
                }
            }
            Err(e) => cx.throw_error(format!("Normalization failed: {}", e)),
        }
    }
}

// ===== GraphMetadata =====

pub struct NodeGraphMetadata {
    inner: Arc<Mutex<GraphMetadata>>,
}

impl Finalize for NodeGraphMetadata {}

impl NodeGraphMetadata {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphMetadata>> {
        let metadata = GraphMetadata {
            namespace: None,
            version: None,
            dependencies: Vec::new(),
            exports: Vec::new(),
            tags: Vec::new(),
            description: None,
        };
        Ok(cx.boxed(NodeGraphMetadata { 
            inner: Arc::new(Mutex::new(metadata))
        }))
    }

    fn from_graph_export(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphMetadata>> {
        let graph_js = cx.argument::<JsValue>(0)?;
        
        let graph_json = js_to_json_value(&mut cx, graph_js)?;
        let graph_export: GraphExport = match serde_json::from_value(graph_json) {
            Ok(export) => export,
            Err(e) => return cx.throw_error(format!("Failed to deserialize graph: {}", e))
        };
        
        let metadata = GraphMetadata::from_graph_export(&graph_export);
        Ok(cx.boxed(NodeGraphMetadata { 
            inner: Arc::new(Mutex::new(metadata))
        }))
    }

    fn get_namespace(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let inner = this.inner.lock().unwrap();
        match &inner.namespace {
            Some(ns) => Ok(cx.string(ns).upcast()),
            None => Ok(cx.null().upcast()),
        }
    }

    fn set_namespace(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let namespace = cx.argument_opt(0)
            .and_then(|v| v.downcast::<JsString, _>(&mut cx).ok())
            .map(|s| s.value(&mut cx));
        
        let mut inner = this.inner.lock().unwrap();
        inner.namespace = namespace;
        Ok(cx.undefined())
    }

    fn get_version(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let inner = this.inner.lock().unwrap();
        match &inner.version {
            Some(v) => Ok(cx.string(v).upcast()),
            None => Ok(cx.null().upcast()),
        }
    }

    fn set_version(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let version = cx.argument_opt(0)
            .and_then(|v| v.downcast::<JsString, _>(&mut cx).ok())
            .map(|s| s.value(&mut cx));
        
        let mut inner = this.inner.lock().unwrap();
        inner.version = version;
        Ok(cx.undefined())
    }

    fn get_dependencies(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let inner = this.inner.lock().unwrap();
        string_vec_to_js_array(&mut cx, inner.dependencies.as_slice())
    }

    fn set_dependencies(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let deps_array = cx.argument::<JsArray>(0)?;
        let dependencies = js_array_to_string_vec(&mut cx, deps_array)?;
        
        let mut inner = this.inner.lock().unwrap();
        inner.dependencies = dependencies;
        Ok(cx.undefined())
    }

    fn get_exports(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let inner = this.inner.lock().unwrap();
        string_vec_to_js_array(&mut cx, &inner.exports)
    }

    fn set_exports(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let exports_array = cx.argument::<JsArray>(0)?;
        let exports = js_array_to_string_vec(&mut cx, exports_array)?;
        
        let mut inner = this.inner.lock().unwrap();
        inner.exports = exports;
        Ok(cx.undefined())
    }

    fn inject_into_graph_export(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<NodeGraphMetadata>>()?;
        let graph_js = cx.argument::<JsValue>(0)?;
        
        let graph_json = js_to_json_value(&mut cx, graph_js)?;
        let mut graph_export: GraphExport = match serde_json::from_value(graph_json) {
            Ok(export) => export,
            Err(e) => return cx.throw_error(format!("Failed to deserialize graph: {}", e))
        };
        
        {
            let inner = this.inner.lock().unwrap();
            inner.inject_into_graph_export(&mut graph_export);
        }
        
        match serde_json::to_value(&graph_export) {
            Ok(updated_json) => json_value_to_js(&mut cx, &updated_json),
            Err(e) => cx.throw_error(format!("Failed to serialize graph: {}", e))
        }
    }
}

// ===== GraphComposer =====

pub struct NodeGraphComposer {
    inner: GraphComposer,
}

impl Finalize for NodeGraphComposer {}

impl NodeGraphComposer {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphComposer>> {
        let composer = GraphComposer::new();
        Ok(cx.boxed(NodeGraphComposer { inner: composer }))
    }

    fn compose_graphs(mut cx: FunctionContext) -> JsResult<JsValue> {
        let _this = cx.this::<JsBox<NodeGraphComposer>>()?;
        let _composition_js = cx.argument::<JsValue>(0)?;
        
        // For now, create a basic composition
        let graph_composition = GraphComposition {
            sources: Vec::new(),
            connections: Vec::new(),
            shared_resources: Vec::new(),
            properties: HashMap::new(),
            case_sensitive: Some(false),
            metadata: None,
        };
        
        let result = runtime::execute_async(|| async move {
            let mut composer = GraphComposer::new();
            composer.compose_graphs(graph_composition).await
        });
        
        match result {
            Ok(graph) => {
                let graph_export = graph.export();
                match serde_json::to_value(&graph_export) {
                    Ok(json_value) => json_value_to_js(&mut cx, &json_value),
                    Err(e) => cx.throw_error(format!("Failed to serialize composed graph: {}", e))
                }
            }
            Err(e) => cx.throw_error(format!("Failed to compose graphs: {}", e))
        }
    }
}

// ===== NamespaceManager =====

pub struct NodeNamespaceManager {
    inner: Arc<Mutex<GraphNamespaceManager>>,
}

impl Finalize for NodeNamespaceManager {}

impl NodeNamespaceManager {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeNamespaceManager>> {
        let manager = GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve);
        Ok(cx.boxed(NodeNamespaceManager { 
            inner: Arc::new(Mutex::new(manager))
        }))
    }

    fn register_graph(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx.this::<JsBox<NodeNamespaceManager>>()?;
        let graph_js = cx.argument::<JsValue>(0)?;
        
        let graph_json = js_to_json_value(&mut cx, graph_js)?;
        let graph_export: GraphExport = match serde_json::from_value(graph_json) {
            Ok(export) => export,
            Err(e) => return cx.throw_error(format!("Failed to deserialize graph: {}", e))
        };
        
        let mut inner = this.inner.lock().unwrap();
        match inner.register_graph(&graph_export) {
            Ok(namespace) => Ok(cx.string(&namespace)),
            Err(e) => cx.throw_error(format!("Failed to register graph: {}", e)),
        }
    }

    fn resolve_process_path(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<NodeNamespaceManager>>()?;
        let path = cx.argument::<JsString>(0)?.value(&mut cx);
        
        let inner = this.inner.lock().unwrap();
        match inner.resolve_process_path(&path) {
            Ok(process_ref) => {
                let js_obj = JsObject::new(&mut cx);
                
                let qualified_name = cx.string(&process_ref.qualified_name);
                js_obj.set(&mut cx, "qualified_name", qualified_name)?;
                
                let local_name = cx.string(&process_ref.local_name);
                js_obj.set(&mut cx, "local_name", local_name)?;
                
                let component = cx.string(&process_ref.component);
                js_obj.set(&mut cx, "component", component)?;
                
                let visibility = match process_ref.visibility {
                    reflow_network::multi_graph::ProcessVisibility::Private => "private",
                    reflow_network::multi_graph::ProcessVisibility::Shared => "shared",
                    reflow_network::multi_graph::ProcessVisibility::Public => "public",
                };
                let visibility_str = cx.string(visibility);
                js_obj.set(&mut cx, "visibility", visibility_str)?;
                
                Ok(js_obj.upcast())
            }
            Err(e) => cx.throw_error(format!("Failed to resolve process path: {}", e)),
        }
    }
}

// ===== Simplified Workspace =====

pub struct NodeWorkspace;

impl Finalize for NodeWorkspace {}

impl NodeWorkspace {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeWorkspace>> {
        Ok(cx.boxed(NodeWorkspace))
    }

    fn load_from_directory(mut cx: FunctionContext) -> JsResult<JsValue> {
        let _this = cx.this::<JsBox<NodeWorkspace>>()?;
        let _directory = cx.argument::<JsString>(0)?.value(&mut cx);
        
        let result = runtime::execute_async(|| async move {
            // TODO: Implement actual directory discovery
            // For now, return empty array
            let graphs: Vec<GraphExport> = Vec::new();
            Ok::<Vec<GraphExport>, anyhow::Error>(graphs)
        });
        
        match result {
            Ok(graphs) => {
                let js_array = JsArray::new(&mut cx, graphs.len());
                for (i, graph) in graphs.iter().enumerate() {
                    match serde_json::to_value(graph) {
                        Ok(json_value) => {
                            let js_graph = json_value_to_js(&mut cx, &json_value)?;
                            js_array.set(&mut cx, i as u32, js_graph)?;
                        }
                        Err(e) => return cx.throw_error(format!("Failed to serialize graph {}: {}", i, e))
                    }
                }
                Ok(js_array.upcast())
            }
            Err(e) => cx.throw_error(format!("Failed to load workspace: {}", e))
        }
    }

    fn save_graph(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let _graph_js = cx.argument::<JsValue>(0)?;
        let _file_path = cx.argument::<JsString>(1)?.value(&mut cx);
        
        // TODO: Implement graph saving
        // For now, just return success
        Ok(cx.undefined())
    }

    fn compose_graphs(mut cx: FunctionContext) -> JsResult<JsValue> {
        let _this = cx.this::<JsBox<NodeWorkspace>>()?;
        
        // Return empty composition object
        let js_obj = JsObject::new(&mut cx);
        Ok(js_obj.upcast())
    }
}

// ===== Export functions =====

pub fn create_graph_loader(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraphLoader::new)?)
}

pub fn create_graph_validator(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraphValidator::new)?)
}

pub fn create_graph_normalizer(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraphNormalizer::new)?)
}

pub fn create_graph_metadata(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraphMetadata::new)?)
}

pub fn create_graph_composer(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraphComposer::new)?)
}

pub fn create_namespace_manager(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeNamespaceManager::new)?)
}

pub fn create_workspace(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeWorkspace::new)?)
}
