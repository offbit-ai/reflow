//! Graph-related Neon bindings
//!
//! Provides Node.js bindings for Graph and GraphHistory classes
//! with full feature parity to WASM version

use crate::neon_bindings::utils::*;
use neon::prelude::*;
use parking_lot::RwLock as ParkingRwLock;
use reflow_network::graph;
use reflow_network::graph::history::{Command, CompositeCommand, GraphHistory};
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
    fn js_new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraph>> {
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
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;

        let id = cx.argument::<JsString>(1)?.value(&mut cx);
        let component = cx.argument::<JsString>(2)?.value(&mut cx);
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

        this.inner.write().add_node(&id, &component, metadata);
        Ok(cx.undefined())
    }

    fn remove_node(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let id = cx.argument::<JsString>(1)?.value(&mut cx);

        this.inner.write().remove_node(&id);
        Ok(cx.undefined())
    }

    fn add_connection(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;

        let from_node = cx.argument::<JsString>(1)?.value(&mut cx);
        let from_port = cx.argument::<JsString>(2)?.value(&mut cx);
        let to_node = cx.argument::<JsString>(3)?.value(&mut cx);
        let to_port = cx.argument::<JsString>(4)?.value(&mut cx);
        let metadata = cx
            .argument_opt(5)
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
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;

        let from_node = cx.argument::<JsString>(1)?.value(&mut cx);
        let from_port = cx.argument::<JsString>(1)?.value(&mut cx);
        let to_node = cx.argument::<JsString>(3)?.value(&mut cx);
        let to_port = cx.argument::<JsString>(4)?.value(&mut cx);

        this.inner
            .write()
            .remove_connection(&from_node, &from_port, &to_node, &to_port);
        Ok(cx.undefined())
    }

    fn add_initial(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let _v = cx.argument::<JsValue>(1)?;
        let data = js_to_json_value(&mut cx, _v)?;
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
            .add_initial(data, &to_node, &to_port, metadata);
        Ok(cx.undefined())
    }

    fn remove_initial(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;

        let to_node = cx.argument::<JsString>(1)?.value(&mut cx);
        let to_port = cx.argument::<JsString>(2)?.value(&mut cx);

        this.inner.write().remove_initial(&to_node, &to_port);
        Ok(cx.undefined())
    }

    fn add_inport(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;

        let public_port = cx.argument::<JsString>(1)?.value(&mut cx);
        let node_id = cx.argument::<JsString>(2)?.value(&mut cx);
        let node_port = cx.argument::<JsString>(3)?.value(&mut cx);
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

        // Use Flow as default port type for compatibility
        this.inner
            .write()
            .add_inport(&public_port, &node_id, &node_port, PortType::Flow, metadata);
        Ok(cx.undefined())
    }

    fn remove_inport(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let public_port = cx.argument::<JsString>(1)?.value(&mut cx);

        this.inner.write().remove_inport(&public_port);
        Ok(cx.undefined())
    }

    fn add_outport(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;

        let public_port = cx.argument::<JsString>(1)?.value(&mut cx);
        let node_id = cx.argument::<JsString>(2)?.value(&mut cx);
        let node_port = cx.argument::<JsString>(3)?.value(&mut cx);
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
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let public_port = cx.argument::<JsString>(1)?.value(&mut cx);

        this.inner.write().remove_outport(&public_port);
        Ok(cx.undefined())
    }

    fn add_group(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let name = cx.argument::<JsString>(1)?.value(&mut cx);
        let nodes = cx.argument::<JsArray>(2)?;
        let metadata = cx
            .argument_opt(3)
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
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let name = cx.argument::<JsString>(1)?.value(&mut cx);

        this.inner.write().remove_group(&name);
        Ok(cx.undefined())
    }

    fn set_properties(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let _v = cx.argument::<JsValue>(1)?;
        let properties = js_to_json_value(&mut cx, _v)?;

        if let JsonValue::Object(map) = properties {
            let props_map = map.into_iter().collect::<HashMap<String, JsonValue>>();
            this.inner.write().set_properties(props_map);
        }

        Ok(cx.undefined())
    }

    fn get_properties<'a>(mut cx: FunctionContext<'a>) -> JsResult<'a, JsValue> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;

        // Use export to access properties since they're private
        let props = this.inner.read().get_properties();
        let props_json = JsonValue::Object(props.into_iter().collect());

        let v = json_value_to_js(&mut cx, &props_json);

        v
    }

    fn export(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let graph_export = this.inner.read().export();

        // Convert GraphExport to JSON and then to JS
        let json_value = match serde_json::to_value(&graph_export) {
            Ok(v) => v,
            Err(e) => return cx.throw_error(format!("Failed to serialize graph: {}", e)),
        };

        json_value_to_js(&mut cx, &json_value)
    }

    fn import(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let _v = cx.argument::<JsValue>(1)?;
        let graph_data = js_to_json_value(&mut cx, _v)?;

        let graph_export: GraphExport = match serde_json::from_value(graph_data) {
            Ok(v) => v,
            Err(e) => return cx.throw_error(format!("Failed to deserialize graph: {}", e)),
        };

        *this.inner.write() = Graph::load(graph_export, None);
        Ok(cx.undefined())
    }

    fn to_json(mut cx: FunctionContext) -> JsResult<JsString> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
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
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let cloned = this.inner.clone();

        Ok(cx.boxed(NodeGraph { inner: cloned }))
    }

    fn get_nodes(mut cx: FunctionContext) -> JsResult<JsArray> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
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
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
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

    fn get_initializers(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let graph = this.inner.read();

        match serde_json::to_value(&graph.initializers) {
            Ok(json_value) => json_value_to_js(&mut cx, &json_value),
            Err(e) => cx.throw_error(format!("Failed to serialize initializers: {}", e)),
        }
    }

    fn set_property(mut cx: FunctionContext) -> JsResult<JsUndefined> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let key = cx.argument::<JsString>(1)?.value(&mut cx);
        let value = cx.argument::<JsValue>(1)?;

        let value_json = js_to_json_value(&mut cx, value)?;
        this.inner.write().properties.insert(key, value_json);
        Ok(cx.undefined())
    }

    fn get_property(mut cx: FunctionContext) -> JsResult<JsValue> {
        let this = cx.argument::<JsBox<NodeGraph>>(0)?;
        let key = cx.argument::<JsString>(1)?.value(&mut cx);

        let graph = this.inner.read();
        match graph.properties.get(&key) {
            Some(value) => json_value_to_js(&mut cx, value),
            None => Ok(cx.null().upcast()),
        }
    }
}

// pub struct NodeCompositeCommand {
//     inner: Box<CompositeCommand>,
// }

// impl Finalize for NodeCompositeCommand {}

// impl NodeCompositeCommand {
//     fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeCompositeCommand>> {
//         let name_js = cx.argument::<JsValue>(0)?;
//         let name = name_js
//             .downcast::<JsString, _>(&mut cx)
//             .map(|s| s.value(&mut cx))
//             .unwrap_or_else(|_| "CompositeCommand".to_string());
//         let command = CompositeCommand::new(&name);
//         Ok(cx.boxed(NodeCompositeCommand { inner: Box::new(command) }))
//     }
// }

// pub struct NodeGraphHistory {
//     inner: Arc<ParkingRwLock<GraphHistory>>,
// }

// impl Finalize for NodeGraphHistory {}

// impl NodeGraphHistory {
//     fn new(mut cx: FunctionContext) -> JsResult<JsBox<NodeGraphHistory>> {
//         let limit = get_optional_number_arg(&mut cx, 0)
//             .map(|n| n as usize)
//             .unwrap_or(50);

//         let history = Graph::with_history_and_limit(limit).1;

//         Ok(cx.boxed(NodeGraphHistory {
//             inner: Arc::new(ParkingRwLock::new(history)),
//         }))
//     }

//     fn with_graph_and_limit(mut cx: FunctionContext) -> JsResult<JsArray> {
//         let limit = get_optional_number_arg(&mut cx, 0)
//             .map(|n| n as usize)
//             .unwrap_or(50);

//         let (graph, history) = Graph::with_history_and_limit(limit);

//         let result = cx.empty_array();

//         let node_graph = cx.boxed(NodeGraph {
//             inner: Arc::new(ParkingRwLock::new(graph)),
//         });
//         result.set(&mut cx, 0, node_graph)?;

//         let node_history = cx.boxed(NodeGraphHistory {
//             inner: Arc::new(ParkingRwLock::new(history)),
//         });
//         result.set(&mut cx, 1, node_history)?;

//         Ok(result)
//     }

//     fn commit_transaction(mut cx: FunctionContext) -> JsResult<JsUndefined> {
//         let this = cx.this::<JsBox<NodeGraphHistory>>()?;
//         let command_js = cx.argument::<JsValue>(0)?;
//         let graph_js = cx.argument::<JsValue>(1)?;

//         // let graph_json = js_to_json_value(&mut cx, graph_js)?;
//         // let graph_export: GraphExport = match serde_json::from_value(graph_json) {
//         //     Ok(export) => export,
//         //     Err(e) => return cx.throw_error(format!("Failed to parse graph: {}", e))
//         // };

//         let graph = graph_js
//             .downcast::<JsBox<NodeGraph>, _>(&mut cx)
//             .map_err(|_| -> JsResult<JsUndefined> {
//                 cx.throw_error("Graph must be an instance of NodeGraph")
//             })
//             .expect("Failed to downcast graph");

//         let command = command_js
//             .downcast::<JsBox<NodeCompositeCommand>, _>(&mut cx)
//             .map_err(|_|-> JsResult<JsUndefined> {
//                 cx.throw_error("Command must be an instance of CompositeCommand")
//             })
//             .expect("Failed to downcast command");

//          let comm =    Box::leak(command.inner); // Leak the command to avoid double free
//         this.inner
//             .write()
//             .execute(Box::new(*comm), &mut graph.inner.write());
//         Ok(cx.undefined())
//     }

//     fn undo(mut cx: FunctionContext) -> JsResult<JsValue> {
//         let this = cx.this::<JsBox<NodeGraphHistory>>()?;

//         match this.inner.write().undo() {
//             Some(graph_export) => match serde_json::to_value(&graph_export) {
//                 Ok(json_value) => json_value_to_js(&mut cx, &json_value),
//                 Err(e) => cx.throw_error(format!("Failed to serialize graph: {}", e)),
//             },
//             None => Ok(cx.null().upcast()),
//         }
//     }

//     fn redo(mut cx: FunctionContext) -> JsResult<JsValue> {
//         let this = cx.this::<JsBox<NodeGraphHistory>>()?;

//         match this.inner.write().redo() {
//             Some(graph_export) => match serde_json::to_value(&graph_export) {
//                 Ok(json_value) => json_value_to_js(&mut cx, &json_value),
//                 Err(e) => cx.throw_error(format!("Failed to serialize graph: {}", e)),
//             },
//             None => Ok(cx.null().upcast()),
//         }
//     }

//     fn can_undo(mut cx: FunctionContext) -> JsResult<JsBoolean> {
//         let this = cx.this::<JsBox<NodeGraphHistory>>()?;
//         Ok(cx.boolean(this.inner.read().can_undo()))
//     }

//     fn can_redo(mut cx: FunctionContext) -> JsResult<JsBoolean> {
//         let this = cx.this::<JsBox<NodeGraphHistory>>()?;
//         Ok(cx.boolean(this.inner.read().can_redo()))
//     }

//     fn clear(mut cx: FunctionContext) -> JsResult<JsUndefined> {
//         let this = cx.this::<JsBox<NodeGraphHistory>>()?;
//         this.inner.write().clear();
//         Ok(cx.undefined())
//     }

//     fn size(mut cx: FunctionContext) -> JsResult<JsNumber> {
//         let this = cx.this::<JsBox<NodeGraphHistory>>()?;
//         Ok(cx.number(this.inner.read().size() as f64))
//     }
// }

/// Create Graph constructor function with all methods
pub fn create_graph(mut cx: FunctionContext) -> JsResult<JsFunction> {
    let constructor = JsFunction::new(&mut cx, |cx| NodeGraph::js_new(cx))?;

    // Add static methods
    let load_fn = JsFunction::new(&mut cx, NodeGraph::import)?;
    constructor.set(&mut cx, "load", load_fn)?;

    // let with_history_fn = JsFunction::new(&mut cx, NodeGraphHistory::with_graph_and_limit)?;
    // constructor.set(&mut cx, "withHistoryAndLimit", with_history_fn)?;

    // Add instance methods to prototype
    let prototype_result = constructor.get::<JsObject, _, _>(&mut cx, "prototype");

    let prototype = match prototype_result {
        Ok(proto) => {
            println!("✅ Got existing prototype");
            proto
        }
        Err(_) => {
            println!("❌ No existing prototype, creating new one");
            // Create a new prototype object
            let new_proto = cx.empty_object();
            constructor.set(&mut cx, "prototype", new_proto)?;
            constructor.get::<JsObject, _, _>(&mut cx, "prototype")?
        }
    };

    // let prototype = constructor.upcast::<JsObject>();

    // Node operations
    let add_node_fn = JsFunction::new(&mut cx, NodeGraph::add_node)?;
    prototype.set(&mut cx, "addNode", add_node_fn)?;

    let remove_node_fn = JsFunction::new(&mut cx, NodeGraph::remove_node)?;
    prototype.set(&mut cx, "removeNode", remove_node_fn)?;

    // Connection operations
    let add_connection_fn = JsFunction::new(&mut cx, NodeGraph::add_connection)?;
    prototype.set(&mut cx, "addConnection", add_connection_fn)?;

    let remove_connection_fn = JsFunction::new(&mut cx, NodeGraph::remove_connection)?;
    prototype.set(&mut cx, "removeConnection", remove_connection_fn)?;

    // Initial packet operations
    let add_initial_fn = JsFunction::new(&mut cx, NodeGraph::add_initial)?;
    prototype.set(&mut cx, "addInitial", add_initial_fn)?;

    let remove_initial_fn = JsFunction::new(&mut cx, NodeGraph::remove_initial)?;
    prototype.set(&mut cx, "removeInitial", remove_initial_fn)?;

    // Port operations
    let add_inport_fn = JsFunction::new(&mut cx, NodeGraph::add_inport)?;
    prototype.set(&mut cx, "addInport", add_inport_fn)?;

    let remove_inport_fn = JsFunction::new(&mut cx, NodeGraph::remove_inport)?;
    prototype.set(&mut cx, "removeInport", remove_inport_fn)?;

    let add_outport_fn = JsFunction::new(&mut cx, NodeGraph::add_outport)?;
    prototype.set(&mut cx, "addOutport", add_outport_fn)?;

    let remove_outport_fn = JsFunction::new(&mut cx, NodeGraph::remove_outport)?;
    prototype.set(&mut cx, "removeOutport", remove_outport_fn)?;

    // Group operations
    let add_group_fn = JsFunction::new(&mut cx, NodeGraph::add_group)?;
    prototype.set(&mut cx, "addGroup", add_group_fn)?;

    let remove_group_fn = JsFunction::new(&mut cx, NodeGraph::remove_group)?;
    prototype.set(&mut cx, "removeGroup", remove_group_fn)?;

    // Graph operations
    let export_fn = JsFunction::new(&mut cx, NodeGraph::export)?;
    prototype.set(&mut cx, "export", export_fn)?;

    let clone_fn = JsFunction::new(&mut cx, NodeGraph::clone_graph)?;
    prototype.set(&mut cx, "clone", clone_fn)?;

    // Property operations
    let set_property_fn = JsFunction::new(&mut cx, NodeGraph::set_property)?;
    prototype.set(&mut cx, "setProperty", set_property_fn)?;

    let set_properties_fn = JsFunction::new(&mut cx, NodeGraph::set_properties)?;
    prototype.set(&mut cx, "setProperties", set_properties_fn)?;

    let get_property_fn = JsFunction::new(&mut cx, NodeGraph::get_property)?;
    prototype.set(&mut cx, "getProperty", get_property_fn)?;

    let get_properties_fn = JsFunction::new(&mut cx, NodeGraph::get_properties)?;
    prototype.set(&mut cx, "getProperties", get_properties_fn)?;

    // Serialization
    let to_json_fn = JsFunction::new(&mut cx, NodeGraph::to_json)?;
    prototype.set(&mut cx, "toJSON", to_json_fn)?;

    // Getters for graph components
    let get_nodes_fn = JsFunction::new(&mut cx, NodeGraph::get_nodes)?;
    prototype.set(&mut cx, "getNodes", get_nodes_fn)?;

    let get_connections_fn = JsFunction::new(&mut cx, NodeGraph::get_connections)?;
    prototype.set(&mut cx, "getConnections", get_connections_fn)?;

    let get_initializers_fn = JsFunction::new(&mut cx, NodeGraph::get_initializers)?;
    prototype.set(&mut cx, "getInitializers", get_initializers_fn)?;

    Ok(constructor)
}

// /// Create GraphHistory constructor function with all methods
// pub fn create_graph_history(mut cx: FunctionContext) -> JsResult<JsFunction> {
//     let constructor = JsFunction::new(&mut cx, NodeGraphHistory::new)?;

//     // Add static methods
//     let with_graph_fn = JsFunction::new(&mut cx, NodeGraphHistory::with_graph_and_limit)?;
//     constructor.set(&mut cx, "withGraphAndLimit", with_graph_fn)?;

//     // Add instance methods to prototype
//     let prototype = constructor.get::<JsObject, _, _>(&mut cx, "prototype")?;

//     let push_fn = JsFunction::new(&mut cx, NodeGraphHistory::push)?;
//     prototype.set(&mut cx, "push", push_fn)?;

//     let undo_fn = JsFunction::new(&mut cx, NodeGraphHistory::undo)?;
//     prototype.set(&mut cx, "undo", undo_fn)?;

//     let redo_fn = JsFunction::new(&mut cx, NodeGraphHistory::redo)?;
//     prototype.set(&mut cx, "redo", redo_fn)?;

//     let can_undo_fn = JsFunction::new(&mut cx, NodeGraphHistory::can_undo)?;
//     prototype.set(&mut cx, "canUndo", can_undo_fn)?;

//     let can_redo_fn = JsFunction::new(&mut cx, NodeGraphHistory::can_redo)?;
//     prototype.set(&mut cx, "canRedo", can_redo_fn)?;

//     let clear_fn = JsFunction::new(&mut cx, NodeGraphHistory::clear)?;
//     prototype.set(&mut cx, "clear", clear_fn)?;

//     let size_fn = JsFunction::new(&mut cx, NodeGraphHistory::size)?;
//     prototype.set(&mut cx, "size", size_fn)?;

//     Ok(constructor)
// }
