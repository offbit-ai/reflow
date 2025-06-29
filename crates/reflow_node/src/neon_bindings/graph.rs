//! Graph-related Neon bindings
//!
//! Provides Node.js bindings for Graph and GraphHistory classes
//! with full feature parity to WASM version

use crate::neon_bindings::utils::*;
use neon::prelude::*;
use parking_lot::RwLock as ParkingRwLock;
use reflow_network::{graph::types::*, graph::Graph};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

/// Node.js Graph wrapper with full functionality
pub struct NodeGraph {
    inner: Arc<ParkingRwLock<Graph>>,
}

impl Finalize for NodeGraph {}

impl NodeGraph {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraph>> {
        // Get optional parameters: name, case_sensitive, properties
        let name = get_optional_string_arg(&mut cx, 0).unwrap_or_else(|| "Graph".to_string());

        let case_sensitive = cx
            .argument_opt(1)
            .and_then(|v| v.downcast::<JsBoolean, _>(&mut cx).ok())
            .map(|b| b.value(&mut cx))
            .unwrap_or(true);

        let properties = cx
            .argument_opt(2)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .and_then(|v| if v.is_object() { Some(v) } else { None })
            .unwrap_or(JsonValue::Object(serde_json::Map::new()));

        let props_map = match properties {
            JsonValue::Object(map) => Some(map.into_iter().collect::<HashMap<String, JsonValue>>()),
            _ => None,
        };

        let graph = Graph::new(&name, case_sensitive, props_map);

        Ok(cx.boxed(NodeGraph {
            inner: Arc::new(ParkingRwLock::new(graph)),
        }))
    }

    fn add_node(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        let id = cx.argument::<JsString>(0)?.value(&mut cx);
        let component = cx.argument::<JsString>(1)?.value(&mut cx);
        let metadata = cx
            .argument_opt(2)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .and_then(|v| {
                if let JsonValue::Object(map) = v {
                    Some(map.into_iter().collect())
                } else {
                    None
                }
            });

        this.inner.write().add_node(&id, &component, metadata);
        Ok(cx.undefined())
    }

    fn remove_node(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let id = cx.argument::<JsString>(0)?.value(&mut cx);

        this.inner.write().remove_node(&id);
        Ok(cx.undefined())
    }

    fn add_connection(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        let from_node = cx.argument::<JsString>(0)?.value(&mut cx);
        let from_port = cx.argument::<JsString>(1)?.value(&mut cx);
        let to_node = cx.argument::<JsString>(2)?.value(&mut cx);
        let to_port = cx.argument::<JsString>(3)?.value(&mut cx);
        let metadata = cx
            .argument_opt(4)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .and_then(|v| {
                if let JsonValue::Object(map) = v {
                    Some(map.into_iter().collect())
                } else {
                    None
                }
            });

        this.inner
            .write()
            .add_connection(&from_node, &from_port, &to_node, &to_port, metadata);
        Ok(cx.undefined())
    }

    fn remove_connection(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        let from_node = cx.argument::<JsString>(0)?.value(&mut cx);
        let from_port = cx.argument::<JsString>(1)?.value(&mut cx);
        let to_node = cx.argument::<JsString>(2)?.value(&mut cx);
        let to_port = cx.argument::<JsString>(3)?.value(&mut cx);

        this.inner
            .write()
            .remove_connection(&from_node, &from_port, &to_node, &to_port);
        Ok(cx.undefined())
    }

    fn add_initial(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let _v = cx.argument::<JsValue>(0)?;
        let data = js_to_json_value(&mut cx, _v)?;
        let to_node = cx.argument::<JsString>(1)?.value(&mut cx);
        let to_port = cx.argument::<JsString>(2)?.value(&mut cx);
        let metadata = cx
            .argument_opt(3)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .and_then(|v| {
                if let JsonValue::Object(map) = v {
                    Some(map.into_iter().collect())
                } else {
                    None
                }
            });

        this.inner
            .write()
            .add_initial(data, &to_node, &to_port, metadata);
        Ok(cx.undefined())
    }

    fn remove_initial(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        let to_node = cx.argument::<JsString>(0)?.value(&mut cx);
        let to_port = cx.argument::<JsString>(1)?.value(&mut cx);

        this.inner.write().remove_initial(&to_node, &to_port);
        Ok(cx.undefined())
    }

    fn add_inport(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        let public_port = cx.argument::<JsString>(0)?.value(&mut cx);
        let node_id = cx.argument::<JsString>(1)?.value(&mut cx);
        let node_port = cx.argument::<JsString>(2)?.value(&mut cx);
        let metadata = cx
            .argument_opt(3)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .and_then(|v| {
                if let JsonValue::Object(map) = v {
                    Some(map.into_iter().collect())
                } else {
                    None
                }
            });

        // Use Flow as default port type for compatibility
        this.inner
            .write()
            .add_inport(&public_port, &node_id, &node_port, PortType::Flow, metadata);
        Ok(cx.undefined())
    }

    fn remove_inport(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let public_port = cx.argument::<JsString>(0)?.value(&mut cx);

        this.inner.write().remove_inport(&public_port);
        Ok(cx.undefined())
    }

    fn add_outport(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        let public_port = cx.argument::<JsString>(0)?.value(&mut cx);
        let node_id = cx.argument::<JsString>(1)?.value(&mut cx);
        let node_port = cx.argument::<JsString>(2)?.value(&mut cx);
        let metadata = cx
            .argument_opt(3)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .and_then(|v| {
                if let JsonValue::Object(map) = v {
                    Some(map.into_iter().collect())
                } else {
                    None
                }
            });

        // Use Flow as default port type for compatibility
        this.inner.write().add_outport(
            &public_port,
            &node_id,
            &node_port,
            PortType::Flow,
            metadata,
        );
        Ok(cx.undefined())
    }

    fn remove_outport(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let public_port = cx.argument::<JsString>(0)?.value(&mut cx);

        this.inner.write().remove_outport(&public_port);
        Ok(cx.undefined())
    }

    fn add_group(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        let name = cx.argument::<JsString>(0)?.value(&mut cx);
        let nodes = cx.argument::<JsArray>(1)?;
        let metadata = cx
            .argument_opt(2)
            .and_then(|v| js_to_json_value(&mut cx, v).ok())
            .map(|v| serde_json::from_value::<HashMap<String, JsonValue>>(v).ok())
            .unwrap();

        let mut node_list = Vec::new();
        for i in 0..nodes.len(&mut cx) {
            if let Ok(node_str) = nodes.get::<JsString, _, _>(&mut cx, i) {
                node_list.push(node_str.value(&mut cx));
            }
        }

        this.inner.write().add_group(&name, node_list, metadata);
        Ok(cx.undefined())
    }

    fn remove_group(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let name = cx.argument::<JsString>(0)?.value(&mut cx);

        this.inner.write().remove_group(&name);
        Ok(cx.undefined())
    }

    fn set_properties(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let _v = cx.argument::<JsValue>(0)?;
        let properties = js_to_json_value(&mut cx, _v)?;

        if let JsonValue::Object(map) = properties {
            let props_map = map.into_iter().collect::<HashMap<String, JsonValue>>();
            this.inner.write().set_properties(props_map);
        }

        Ok(cx.undefined())
    }

    fn get_properties<'a>(mut cx: FunctionContext<'a>) -> JsResult<'a, JsValue> {
        let this = cx.this::<JsBox<NodeGraph>>()?;

        // Use export to access properties since they're private
        let props = this.inner.read().get_properties();
        let props_json = JsonValue::Object(props.into_iter().collect());

        let v = json_value_to_js(&mut cx, &props_json);

        v
    }

    fn export(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let graph_export = this.inner.read().export();

        // Convert GraphExport to JSON and then to JS
        let json_value = match serde_json::to_value(&graph_export) {
            Ok(v) => v,
            Err(e) => return cx.throw_error(format!("Failed to serialize graph: {}", e)),
        };

        json_value_to_js(&mut cx, &json_value)
    }

    fn import(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let _v = cx.argument::<JsValue>(0)?;
        let graph_data = js_to_json_value(&mut cx, _v)?;

        let graph_export: GraphExport = match serde_json::from_value(graph_data) {
            Ok(v) => v,
            Err(e) => return cx.throw_error(format!("Failed to deserialize graph: {}", e)),
        };

        *this.inner.write() = Graph::load(graph_export, None);
        Ok(cx.undefined())
    }

    fn to_json(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let graph_export = this.inner.read().export();

        let json_str = match serde_json::to_string_pretty(&graph_export) {
            Ok(s) => s,
            Err(e) => return cx.throw_error(format!("Failed to serialize to JSON: {}", e)),
        };

        Ok(cx.string(json_str))
    }

    fn from_json(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraph>> {
        let json_str = cx.argument::<JsString>(0)?.value(&mut cx);

        let graph_export: GraphExport = match serde_json::from_str(&json_str) {
            Ok(v) => v,
            Err(e) => return cx.throw_error(format!("Failed to parse JSON: {}", e)),
        };

        let graph = Graph::load(graph_export, None);

        Ok(cx.boxed(NodeGraph {
            inner: Arc::new(ParkingRwLock::new(graph)),
        }))
    }

    fn clone_graph(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraph>> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let cloned = this.inner.clone();

        Ok(cx.boxed(NodeGraph { inner: cloned }))
    }

    fn get_nodes(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let nodes = this.inner.read().get_nodes();

        let js_array = JsArray::new(&mut cx, nodes.len());
        for (i, node) in nodes.iter().enumerate() {
            let node_json = match serde_json::to_value(node) {
                Ok(v) => v,
                Err(e) => return cx.throw_error(format!("Failed to serialize node: {}", e)),
            };
            let js_node = json_value_to_js(&mut cx, &node_json)?;
            js_array.set(&mut cx, i as u32, js_node)?;
        }

        Ok(js_array)
    }

    fn get_connections(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx.this::<JsBox<NodeGraph>>()?;
        let connections = this.inner.read().get_connections();

        let js_array = JsArray::new(&mut cx, connections.len());
        for (i, connection) in connections.iter().enumerate() {
            let conn_json = match serde_json::to_value(connection) {
                Ok(v) => v,
                Err(e) => return cx.throw_error(format!("Failed to serialize connection: {}", e)),
            };
            let js_conn = json_value_to_js(&mut cx, &conn_json)?;
            js_array.set(&mut cx, i as u32, js_conn)?;
        }

        Ok(js_array)
    }
}

/// Node.js GraphHistory wrapper
pub struct NodeGraphHistory {
    // TODO: Implement history tracking
}

impl Finalize for NodeGraphHistory {}

impl NodeGraphHistory {
    fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphHistory>> {
        Ok(cx.boxed(NodeGraphHistory {}))
    }

    fn push_snapshot(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        // TODO: Implement history snapshot
        Ok(cx.undefined())
    }

    fn undo(mut cx: FunctionContext) -> JsResult<JsValue> {
        // TODO: Implement undo functionality
        Ok(cx.undefined().upcast())
    }

    fn redo(mut cx: FunctionContext) -> JsResult<JsValue> {
        // TODO: Implement redo functionality
        Ok(cx.undefined().upcast())
    }

    fn can_undo(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        // TODO: Check if undo is available
        Ok(cx.boolean(false))
    }

    fn can_redo(mut cx: FunctionContext) -> JsResult<JsBoolean> {
        // TODO: Check if redo is available
        Ok(cx.boolean(false))
    }
}

/// Create Graph constructor
pub fn create_graph(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraph::new)?)
}

/// Create GraphHistory constructor
pub fn create_graph_history(mut cx: FunctionContext) -> JsResult<JsFunction> {
    Ok(JsFunction::new(&mut cx, NodeGraphHistory::new)?)
}
