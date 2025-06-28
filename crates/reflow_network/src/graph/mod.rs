pub mod history;
pub mod journal;
pub mod tests;
pub mod types;

use std::collections::{HashMap, HashSet, VecDeque};

// use foreach::ForEach;
#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;
use history::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use types::*;
#[cfg(target_arch = "wasm32")]
use web_sys::js_sys::Function;

/// This class represents an abstract FBP graph containing nodes
/// connected to each other with edges.
/// These graphs can be used for visualization and sketching, but
/// also are the way to start an FBP network.
#[derive(Clone)]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Graph {
    pub(crate) name: String,
    pub(crate) nodes: HashMap<String, GraphNode>,
    pub(crate) connections: Vec<GraphConnection>,
    // Indexed connections for faster connection lookups
    pub(crate) connection_indices: HashMap<(String, String), Vec<usize>>,
    /// Key is ((from_node, from_port), (to_node, to_port)) -> Vec<connection_indices>
    pub(crate) connection_port_indices: HashMap<((String, String), (String, String)), Vec<usize>>,
    pub(crate) initializers: Vec<GraphIIP>,
    // Indexed initializers for faster initializer lookups
    pub(crate) initializer_indices: HashMap<String, Vec<usize>>,
    pub(crate) groups: Vec<GraphGroup>,
    // Indexed node groups for faster group membership lookups
    pub(crate) node_groups: HashMap<String, HashSet<String>>,
    pub(crate) inports: HashMap<String, GraphEdge>,
    pub(crate) outports: HashMap<String, GraphEdge>,
    pub(crate) properties: HashMap<String, Value>,
    pub(crate) case_sensitive: bool,
    pub(crate) event_channel: (flume::Sender<GraphEvents>, flume::Receiver<GraphEvents>),
    // Cached adjacency lists for faster traversal
    pub(crate) adjacency_lists: HashMap<String, Vec<String>>,

    pub(crate) graph_errors: Vec<GraphError>,
}

impl Default for Graph {
    fn default() -> Self {
        Self {
            name: Default::default(),
            nodes: Default::default(),
            connections: Default::default(),
            initializers: Default::default(),
            groups: Default::default(),
            inports: Default::default(),
            outports: Default::default(),
            properties: Default::default(),
            case_sensitive: Default::default(),
            event_channel: flume::bounded(1000),
            connection_indices: HashMap::new(),
            connection_port_indices: HashMap::new(),
            initializer_indices: HashMap::new(),
            node_groups: HashMap::new(),
            adjacency_lists: HashMap::new(),
            graph_errors: Vec::new(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_class = Graph)]
impl Graph {
    #[wasm_bindgen(constructor)]
    pub fn _new(name: &str, case_sensitive: bool, properties: JsValue) -> Graph {
        let mut is_property = true;
        if properties.is_null() || properties.is_undefined() {
            is_property = false;
        }

        let _meta = if is_property {
            Some(
                properties
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected Graph properties to be type of Map<string, any>"),
            )
        } else {
            None
        };

        Self::new(name, case_sensitive, _meta)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setProperties))]
    /// This method allows changing properties of the graph.
    pub fn _set_properties(&mut self, properties: JsValue) {
        self.set_properties(
            properties
                .into_serde::<HashMap<String, Value>>()
                .expect("Expected graph properties to be type of Map<string, any>"),
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = getProperties))]
    /// This method allows getting properties of the graph.
    pub fn _get_properties(&mut self) -> JsValue {
        JsValue::from_serde(&serde_json::json!(self.properties)).unwrap_or_default()
    }

    /// Nodes objects can be retrieved from the graph by their ID:
    /// ```
    /// const node = myGraph.getNode('actor_id');
    /// ```
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = getNode))]
    pub fn _get_node(&self, key: &str) -> Option<GraphNode> {
        self.get_node(key).cloned()
    }

    /// Adding a node to the graph
    /// Nodes are identified by an ID unique to the graph. Additionally,
    /// a node may contain information on what FBP component it is and
    /// possibly display coordinates.
    /// ```
    /// const metadata = {x: 91, y: 154};
    /// myGraph.addNode("node_id", "ReadFile", metadata);
    /// ```
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addNode))]
    pub fn _add_node(&mut self, id: &str, process: &str, metadata: JsValue) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        // web_sys::console::log_1(&format!("[AddNode] is_meta: {}", is_meta).as_str().into());

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected Node metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        // web_sys::console::log_1(&format!("[AddNode] meta: {:?}", _meta).as_str().into());
        self.add_node(id, process, _meta);
    }
    /// Add inport associated with a node in the Graph
    /// ```
    /// myGraph.addInport("my_inport", "my_node", "in", false);
    /// // With metadata `{x: 10, y:10}`
    /// myGraph.addInport("my_inport2", "my_node2", "in", false, {x: 10, y:10});
    /// ```
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addInport))]
    pub fn _add_inport(
        &mut self,
        port_id: &str,
        node_id: &str,
        port_key: &str,
        port_type: PortType,
        metadata: JsValue,
    ) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected inport metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        self.add_inport(port_id, node_id, port_key, port_type, _meta);
    }

    /// Add outport associated with a node in the Graph
    /// ```
    /// myGraph.addOutport("my_outport", "my_node", "out", false);
    /// // With metadata `{x: 10, y:10}`
    /// myGraph.addOutport("my_outport2", "my_node2", "out", false, {x: 10, y:10});
    /// ```
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addOutport))]
    pub fn _add_outport(
        &mut self,
        port_id: &str,
        node_id: &str,
        port_key: &str,
        port_type: PortType,
        metadata: JsValue,
    ) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected outport metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        self.add_outport(port_id, node_id, port_key, port_type, _meta);
    }

    /// Adding Initial Information Packets
    ///
    /// Initial Information Packets (IIPs) can be used for sending data
    /// to specified node inports without a sending node instance.
    ///
    /// IIPs are especially useful for sending configuration information
    /// to components at FBP network start-up time. This could include
    /// filenames to read, or network ports to listen to.
    ///
    /// ```
    /// myGraph.addInitial("somefile.txt", "Read", "source");
    /// myGraph.addInitialIndex("somefile.txt", "Read", "source", 2);
    /// ```
    /// If inports are defined on the graph, IIPs can be applied calling
    /// the `addGraphInitial` or `addGraphInitialIndex` methods.
    /// ```
    /// myGraph.addGraphInitial("somefile.txt", "file");
    ///	myGraph.addGraphInitialIndex("somefile.txt", "file", 2);
    /// ```
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addInitial))]
    pub fn _add_initial(&mut self, data: JsValue, node: &str, port: &str, metadata: JsValue) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        if data.is_null() || data.is_undefined() {
            web_sys::console::error_1(&"Initial data must be provided".into());
            return;
        }

        self.add_initial(
            data.into_serde::<Value>().unwrap_or_default(),
            node,
            port,
            _meta,
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addInitialIndex))]
    pub fn _add_initial_index(
        &mut self,
        data: JsValue,
        node: &str,
        port: &str,
        index: usize,
        metadata: JsValue,
    ) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected  metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        if data.is_null() || data.is_undefined() {
            web_sys::console::error_1(&"Initial data must be provided".into());
            return;
        }

        self.add_initial_index(
            data.into_serde::<Value>().unwrap_or_default(),
            node,
            port,
            index,
            _meta,
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addGraphInitial))]
    pub fn _add_graph_initial(&mut self, data: JsValue, node: &str, metadata: JsValue) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        if data.is_null() || data.is_undefined() {
            web_sys::console::error_1(&"Initial data must be provided".into());
            return;
        }

        self.add_graph_initial(data.into_serde::<Value>().unwrap_or_default(), node, _meta);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addGraphInitialIndex))]
    pub fn _add_graph_initial_index(
        &mut self,
        data: JsValue,
        node: &str,
        index: usize,
        metadata: JsValue,
    ) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        if data.is_null() || data.is_undefined() {
            web_sys::console::error_1(&"Initial data must be provided".into());
            return;
        }

        self.add_graph_initial_index(
            data.into_serde::<Value>().unwrap_or_default(),
            node,
            index,
            _meta,
        );
    }

    /// Connecting nodes
    ///
    /// Nodes can be connected by adding connection between a node's outport
    ///	and another node's inport:
    /// ```no_run
    /// myGraph.addConnection("Read", "out", "Display", "in");
    /// myGraph.addConnectionIndex("Read", "out", null, "Display", "in", 2);
    /// ```
    /// Adding a connection will emit the `AddConnection` event.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addConnection))]
    pub fn _add_connection(
        &mut self,
        out_node: &str,
        out_port: &str,
        in_node: &str,
        in_port: &str,
        metadata: JsValue,
    ) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected connection metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        self.add_connection(out_node, out_port, in_node, in_port, _meta);
    }

    /// Grouping nodes in a graph
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = addGroup))]
    pub fn _add_group(&mut self, group: &str, nodes: Vec<String>, metadata: JsValue) {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected group metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        self.add_group(group, nodes, _meta);
    }

    /// Remove inport from Graph
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = removeInport))]
    pub fn _remove_inport(&mut self, port_id: &str) {
        self.remove_inport(port_id);
    }

    /// Remove outport from Graph
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = removeOutport))]
    pub fn _remove_outport(&mut self, port_id: &str) {
        self.remove_outport(port_id);
    }

    /// Removing Initial Information Packets
    ///
    /// IIPs can be removed by calling the `remove_initial` method.
    /// ```
    /// myGraph.removeInitial("Read", "source");
    /// ```
    /// If the IIP was applied via the `addGraphInitial` or
    /// `addGraphInitial_index` functions, it can be removed using
    /// the `removeGraphInitial` method.
    /// ```
    /// myGraph.removeGraphInitial("file");
    /// ```
    /// Remove an IIP will emit a `remove_initial` event.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = removeInitial))]
    pub fn _remove_initial(&mut self, id: &str, port: &str) {
        self.remove_initial(id, port);
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = removeGraphInitial))]
    pub fn _remove_graph_initial(&mut self, id: &str) {
        self.remove_graph_initial(id);
    }

    /// Remove a group of nodes
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = removeGroup))]
    pub fn _remove_group(&mut self, group_id: &str) {
        self.remove_group(group_id);
    }

    /// Connection objects can be retrieved from the graph by the node and port IDs:
    /// ```
    /// connection = graph.getConnection("Read", "out", "Write", "in");
    /// ```
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = getConnection))]
    pub fn _get_connection(
        &self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
    ) -> Option<GraphConnection> {
        self.get_connection(node, port, node2, port2)
    }

    /// Disconnected nodes
    ///
    /// Connections between nodes can be removed by providing the
    ///	nodes and ports to disconnect.
    /// ```
    /// graph.removeConnection("Display", "out", "Foo", "in");
    /// ```
    /// Removing a connection will emit the `removeEdge` event.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = removeConnection))]
    pub fn _remove_connection(&mut self, node: &str, port: &str, node2: &str, port2: &str) {
        self.remove_connection(node, port, node2, port2);
    }

    /// Removing a node from the graph
    /// Existing nodes can be removed from a graph by their ID. This
    /// will remove the node and also remove all edges connected to it.
    /// ```
    /// graph.removeNode("Read");
    /// ```
    /// Once the node has been removed, the `remove_node` event will be
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = removeNode))]
    pub fn _remove_node(&mut self, id: &str) {
        self.remove_node(id);
    }

    /// Rename the identifier of an inport
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = renameInport))]
    pub fn _rename_inport(&mut self, old_id: &str, new_id: &str) {
        self.rename_inport(old_id, new_id);
    }

    /// Rename the identifier of an outport
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = renameOutport))]
    pub fn _rename_outport(&mut self, old_id: &str, new_id: &str) {
        self.rename_outport(old_id, new_id);
    }

    /// Renaming a node
    ///
    /// Nodes IDs can be changed by calling this method.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = renameNode))]
    pub fn _rename_node(&mut self, old_id: &str, new_id: &str) {
        self.rename_node(old_id, new_id);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setNodeMetadata))]
    pub fn _set_node_metadata(&mut self, id: &str, metadata: JsValue) {
        self.set_node_metadata(
            id,
            metadata
                .into_serde::<HashMap<String, Value>>()
                .expect("Expected metadata to be type of Map<string, any>"),
        );
    }

    /// Changing a connection's metadata
    ///
    /// Connection metadata can be set or changed by calling this method.
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setConnectionMetadata))]
    pub fn _set_connection_metadata(
        &mut self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
        metadata: JsValue,
    ) {
        self.set_connection_metadata(
            node,
            port,
            node2,
            port2,
            metadata
                .into_serde::<HashMap<String, Value>>()
                .expect("Expected metadata to be type of Map<string, any>"),
        );
    }
    /// Changing an inport's metadata
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setInportMetadata))]
    pub fn _set_inport_metadata(&mut self, port_id: &str, metadata: JsValue) {
        self.set_inport_metadata(
            port_id,
            metadata
                .into_serde::<HashMap<String, Value>>()
                .expect("Expected metadata to be type of Map<string, any>"),
        );
    }

    /// Changing an outport's metadata
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setOutportMetadata))]
    pub fn _set_outport_metadata(&mut self, port_id: &str, metadata: JsValue) {
        self.set_outport_metadata(
            port_id,
            metadata
                .into_serde::<HashMap<String, Value>>()
                .expect("Expected metadata to be type of Map<string, any>"),
        );
    }

    /// Changing a group's metadata
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = setGroupMetadata))]
    pub fn _set_group_metadata(&mut self, group_id: &str, metadata: JsValue) {
        self.set_group_metadata(
            group_id,
            metadata
                .into_serde::<HashMap<String, Value>>()
                .expect("Expected metadata to be type of Map<string, any>"),
        );
    }

    /// Export the graph
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = toJSON))]
    pub fn _export(&self) -> GraphExport {
        self.export()
    }

    /// Export the graph
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = load))]
    pub fn _load(obj: GraphExport, metadata: JsValue) -> Graph {
        let mut is_meta = true;
        if metadata.is_null() {
            is_meta = false;
        } else if metadata.is_undefined() {
            is_meta = false;
        }

        let _meta = if is_meta {
            Some(
                metadata
                    .into_serde::<HashMap<String, Value>>()
                    .expect("Expected graph metadata to be type of Map<string, any>"),
            )
        } else {
            None
        };

        let graph = Self::load(obj, _meta);
        graph
    }

    /// Subscribe to the graph's events
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
    pub fn subscribe(&self, callback: Function) {
        let receiver = self.event_channel.1.clone();

        let cl: Closure<dyn Fn()> = Closure::new(move || {
            if let Ok(event) = receiver.clone().try_recv() {
                let _ = callback.call1(
                    &JsValue::null(),
                    &JsValue::from_serde(&serde_json::json!(event.clone())).unwrap_or_default(),
                );
            }
        });
        if let Ok(set_interval) =
            web_sys::js_sys::Reflect::get(&web_sys::js_sys::global().into(), &"setInterval".into())
        {
            let _set_interval: web_sys::js_sys::Function = set_interval.into();
            if let Ok(_handle) = _set_interval.call2(
                &JsValue::null(),
                cl.as_ref().clone().as_ref().unchecked_ref(),
                &0.into(),
            ) {}
        }
        cl.forget();
    }

    #[wasm_bindgen(js_name = withHistory)]
    pub fn _with_history() -> web_sys::js_sys::Array {
        let (graph, history) = Self::with_history();
        let result = web_sys::js_sys::Array::new();
        result.push(&JsValue::from(graph));
        result.push(&JsValue::from(history));
        result
    }

    #[wasm_bindgen(js_name = withHistoryAndLimit)]
    pub fn _with_history_and_limit(max_history: usize) -> web_sys::js_sys::Array {
        let (graph, history) = Self::with_history_and_limit(max_history);
        let result = web_sys::js_sys::Array::new();
        result.push(&JsValue::from(graph));
        result.push(&JsValue::from(history));
        result
    }

    #[wasm_bindgen(js_name = addToGroup)]
    pub fn _add_to_group(&mut self, group_id: &str, node_id: &str) {
        self.add_to_group(group_id, node_id)
    }

    #[wasm_bindgen(js_name = removeFromGroup)]
    pub fn _remove_from_group(&mut self, group_id: &str, node_id: &str) {
        self.remove_from_group(group_id, node_id)
    }

    #[wasm_bindgen(js_name = "calculateLayout")]
    pub fn _calculate_layout(&self) -> JsValue {
        let layout = self.calculate_layout();
        let obj = js_sys::Object::new();

        for (node_id, position) in layout {
            let pos = js_sys::Object::new();
            js_sys::Reflect::set(&pos, &"x".into(), &position.x.into()).unwrap();
            js_sys::Reflect::set(&pos, &"y".into(), &position.y.into()).unwrap();
            js_sys::Reflect::set(&obj, &node_id.into(), &pos.into()).unwrap();
        }

        obj.into()
    }
}

impl Graph {
    pub fn get_port_name(&self, port: &str) -> String {
        if self.case_sensitive {
            return port.to_string();
        }
        port.to_lowercase()
    }

    pub fn new(
        name: &str,
        case_sensitive: bool,
        properties: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            name: name.to_string(),
            nodes: HashMap::new(),
            connections: Vec::new(),
            initializers: Vec::new(),
            groups: Vec::new(),
            inports: HashMap::new(),
            outports: HashMap::new(),
            properties: properties.unwrap_or_default(),
            case_sensitive,
            event_channel: flume::bounded(1000),
            connection_indices: HashMap::new(),
            initializer_indices: HashMap::new(),
            node_groups: HashMap::new(),
            connection_port_indices: HashMap::new(),
            adjacency_lists: HashMap::new(),
            graph_errors: Vec::new(),
        }
    }

    // Helper method for event emission
    fn emit_event(&self, event: GraphEvents) {
        if let Err(e) = self.event_channel.0.send(event) {
            eprintln!("Failed to emit graph event: {}", e);
        }
    }

    // Cache management
    fn rebuild_adjacency_lists(&mut self) {
        self.adjacency_lists.clear();

        // Initialize empty lists for all nodes
        for node_id in self.nodes.keys() {
            self.adjacency_lists.insert(node_id.clone(), Vec::new());
        }

        // Build adjacency lists from connections
        for conn in self.connections.iter() {
            if let Some(list) = self.adjacency_lists.get_mut(&conn.from.node_id) {
                list.push(conn.to.node_id.clone());
            }
        }
    }

    /// This method allows changing properties of the graph.
    pub fn set_properties(&mut self, properties: HashMap<String, Value>) -> &mut Self {
        let before = self.properties.clone();
        self.properties = properties;

        self.event_channel
            .0
            .send(GraphEvents::ChangeProperties(json!({
                "new": self.properties.clone(),
                "before": before
            })))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        self
    }

    /// Nodes objects can be retrieved from the graph by their ID:
    /// ```no_run
    /// let node = my_graph.get_node('actor_id').unwrap();
    /// ```
    pub fn get_node(&self, key: &str) -> Option<&GraphNode> {
        self.nodes.get(key)
    }
    pub fn get_node_mut(&mut self, key: &str) -> Option<&mut GraphNode> {
        self.nodes.get_mut(key)
    }

    /// Adding a node to the graph
    /// Nodes are identified by an ID unique to the graph. Additionally,
    /// a node may contain information on what FBP component it is and
    /// possibly display coordinates.
    /// ```no_run
    /// let mut metadata = Map::new();
    /// metadata.insert("x".to_string(), 91);
    /// metadata.insert("y".to_string(), 154);
    /// my_graph.add_node("Read", "ReadFile", Some(metadata));
    /// ```
    pub fn add_node(
        &mut self,
        id: &str,
        component: &str,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        let node = GraphNode {
            id: id.to_owned(),
            component: component.to_owned(),
            metadata,
        };

        if self.nodes.contains_key(id) {
            self.graph_errors
                .push(GraphError::DuplicateNode(id.to_owned()));
            return self;
        }

        self.nodes.insert(id.to_owned(), node.clone());

        // Update adjacency list cache
        self.adjacency_lists.insert(id.to_owned(), Vec::new());

        // Send add_node event
        self.event_channel
            .0
            .send(GraphEvents::AddNode(json!(node)))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());

        self
    }

    /// Add inport associated with a node in the Graph
    /// ```no_run
    /// my_graph.add_inport("my_inport", "my_node", "in", PortType::Any, None);
    /// ```
    pub fn add_inport(
        &mut self,
        port_id: &str,
        node_id: &str,
        port_key: &str,
        port_type: PortType,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        // Check that node exists
        if self.get_node(node_id).is_none() {
            return self;
        }

        let port_id = self.get_port_name(port_id);

        let val = GraphEdge {
            node_id: node_id.to_owned(),
            port_id: port_id.clone(),
            port_name: self.get_port_name(port_key),
            index: None,
            data: None,
            metadata,
            ..Default::default()
        };
        self.inports.insert(port_id.clone(), val.clone());

        // Send add_inport event
        self.event_channel
            .0
            .send(GraphEvents::AddInport(json!({
                "id": port_id,
                "port": val
            })))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());

        self
    }

    /// Add outport associated with a node in the Graph
    /// ```no_run
    /// my_graph.add_outport("my_outport", "my_node", "in", PortType::Any, None);
    /// ```
    pub fn add_outport(
        &mut self,
        port_id: &str,
        node_id: &str,
        port_key: &str,
        port_type: PortType,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        // Check that node exists
        if self.get_node(node_id).is_none() {
            return self;
        }

        let port_id = self.get_port_name(port_id);

        let val = GraphEdge {
            node_id: node_id.to_owned(),
            port_id: port_id.clone(),
            port_name: self.get_port_name(port_key),
            index: None,
            data: None,
            metadata,
            port_type,
            ..Default::default()
        };
        self.outports.insert(port_id.clone(), val.clone());

        // Send add_outport event
        self.event_channel
            .0
            .send(GraphEvents::AddOutport(json!(json!({
                "id": port_id,
                "port": val
            }))))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());

        self
    }

    /// Adding Initial Information Packets
    ///
    /// Initial Information Packets (IIPs) can be used for sending data
    /// to specified node inports without a sending node instance.
    ///
    /// IIPs are especially useful for sending configuration information
    /// to components at FBP network start-up time. This could include
    /// filenames to read, or network ports to listen to.
    ///
    /// ```no_run
    /// my_graph.add_initial("somefile.txt", "Read", "source", None);
    /// my_graph.add_initial_index("somefile.txt", "Read", "source", Some(2), None);
    /// ```
    /// If inports are defined on the graph, IIPs can be applied calling
    /// the `add_graph_initial` or `add_graph_initial_index` methods.
    /// ```no_run
    /// my_graph.add_graph_initial("somefile.txt", "file", None);
    ///	my_graph.add_graph_initial_index("somefile.txt", "file", Some(2), None);
    /// ```
    ///
    pub fn add_initial(
        &mut self,
        data: Value,
        node: &str,
        port: &str,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        if let Some(_node) = self.get_node(node) {
            let port_id = self.get_port_name(port);

            let mut initializer_idx = self.initializers.len();
            let to_port = if let Some(port) = self.inports.get_mut(&port_id) {
                port.index = Some(initializer_idx);
                port.clone()
            } else {
                GraphEdge {
                    port_id: port_id.to_owned(),
                    node_id: node.to_owned(),
                    index: Some(initializer_idx),
                    ..Default::default()
                }
            };
            let initializer = GraphIIP {
                to: to_port,
                data,
                metadata,
            };

            // self.initializers[initializer_idx] = initializer;
            self.initializers.push(initializer);
            initializer_idx = self.initializers.len() - 1;
            self.initializer_indices
                .entry(node.to_owned())
                .or_default()
                .push(initializer_idx);

            // Send add_initial event
            self.event_channel
                .0
                .send(GraphEvents::AddInitial(json!(
                    self.initializers[initializer_idx]
                )))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }

        self
    }

    pub fn add_initial_index(
        &mut self,
        data: Value,
        node: &str,
        port: &str,
        index: usize,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        if let Some(_node) = self.get_node(node) {
            let port_id = self.get_port_name(port);
            let mut initializer_idx = index;
            let to_port = if let Some(port) = self.inports.get_mut(&port_id) {
                port.index = Some(initializer_idx);
                port.clone()
            } else {
                GraphEdge {
                    port_id: port_id.to_owned(),
                    node_id: node.to_owned(),
                    index: Some(index),
                    ..Default::default()
                }
            };
            let initializer = GraphIIP {
                to: to_port,
                data,
                metadata,
            };

            self.initializers.push(initializer);
            initializer_idx = self.initializers.len() - 1;

            self.initializer_indices
                .entry(node.to_owned())
                .or_insert_with(Vec::new)
                .push(initializer_idx);

            // Send add_initial event
            self.event_channel
                .0
                .send(GraphEvents::AddInitial(json!(self.initializers[index])))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }

        self
    }

    pub fn add_graph_initial(
        &mut self,
        data: Value,
        node: &str,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        if let Some(inport) = self.inports.clone().get(node) {
            self.add_initial(data, &inport.node_id, &inport.port_id, metadata);
        }
        self
    }

    pub fn add_graph_initial_index(
        &mut self,
        data: Value,
        node: &str,
        index: usize,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        if let Some(inport) = self.inports.clone().get(node) {
            self.add_initial_index(data, &inport.node_id, &inport.port_id, index, metadata);
        }
        self
    }

    /// Connecting nodes
    ///
    /// Nodes can be connected by adding connection between a node's outport
    ///	and another node's inport:
    /// ```no_run
    /// my_graph.add_connection("Read", "out", "Display", "in", None);
    /// ```
    /// Adding a connection will emit the `AddConnection` event.
    pub fn add_connection(
        &mut self,
        out_node: &str,
        out_port: &str,
        in_node: &str,
        in_port: &str,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        let out_port_id = self.get_port_name(out_port);
        let in_port_id = self.get_port_name(in_port);

        // Check if nodes exist - O(1) lookups
        if !self.nodes.contains_key(out_node) || !self.nodes.contains_key(in_node) {
            return self;
        }

        // Check for existing connection using indices - O(1) lookup
        let connection_key = (out_node.to_owned(), in_node.to_owned());

        if let Some(indices) = self.connection_indices.get(&connection_key) {
            for &idx in indices {
                let conn = &self.connections[idx];
                if conn.from.port_id == out_port_id && conn.to.port_id == in_port_id {
                    return self;
                }
            }
        }

        let from_port = if let Some(port) = self.outports.get(&out_port_id) {
            port.clone()
        } else {
            GraphEdge {
                port_id: out_port_id.to_owned(),
                node_id: out_node.to_owned(),
                index: None,
                ..Default::default()
            }
        };

        let to_port = if let Some(port) = self.inports.get(&in_port_id) {
            port.clone()
        } else {
            GraphEdge {
                port_id: in_port_id.to_owned(),
                node_id: in_node.to_owned(),
                index: None,
                ..Default::default()
            }
        };

        let connection = GraphConnection {
            from: from_port,
            to: to_port,
            data: None,
            metadata,
        };

        // Add connection and update indices
        let connection_idx = self.connections.len();
        self.connections.push(connection.clone());

        self.connection_indices
            .entry(connection_key)
            .or_insert_with(Vec::new)
            .push(connection_idx);

        // Update adjacency list
        self.adjacency_lists
            .get_mut(out_node)
            .map(|list| list.push(in_node.to_owned()));

        self.event_channel
            .0
            .send(GraphEvents::AddConnection(json!(connection)))
            .expect("Failed to emit Graph event");

        self
    }

    /// Grouping nodes in a graph
    pub fn add_group(
        &mut self,
        group_id: &str,
        nodes: Vec<String>,
        metadata: Option<HashMap<String, Value>>,
    ) -> &mut Self {
        if self
            .groups
            .iter()
            .find(|group| group.id == group_id)
            .is_some()
        {
            return self;
        }
        let g = &GraphGroup {
            id: group_id.to_owned(),
            nodes,
            metadata,
        };

        self.groups.push(g.clone());

        self.event_channel
            .0
            .send(GraphEvents::AddGroup(json!(g)))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());

        self
    }

    /// Remove inport from Graph
    pub fn remove_inport(&mut self, port_id: &str) -> &mut Self {
        let port_name = self.get_port_name(port_id);

        if !self.inports.contains_key(&(port_name.clone())) {
            return self;
        }

        let inp = self.inports.clone();

        self.inports.remove(&(port_name.clone()));

        self.event_channel
            .0
            .send(GraphEvents::RemoveInport(json!({
                "id": port_name.clone(),
                "port": inp.get(&(port_name.clone()))
            })))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());

        self
    }

    /// Remove outport from Graph
    pub fn remove_outport(&mut self, port_id: &str) -> &mut Self {
        let port_name = self.get_port_name(port_id);

        if !self.outports.contains_key(&(port_name.clone())) {
            return self;
        }

        let inp = self.inports.clone();

        self.outports.remove(&(port_name.clone()));

        self.event_channel
            .0
            .send(GraphEvents::RemoveOutport(json!({
                "id": port_name.clone(),
                "port": inp.get(&(port_name.clone()))
            })))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());

        self
    }

    /// Removing Initial Information Packets
    ///
    /// IIPs can be removed by calling the `remove_initial` method.
    /// ```no_run
    /// my_graph.remove_initial("Read", "source");
    /// ```
    /// If the IIP was applied via the `add_graph_initial` or
    /// `add_graph_initial_index` functions, it can be removed using
    /// the `remove_graph_initial` method.
    /// ```no_run
    /// my_graph.remove_graph_initial("file");
    /// ```
    /// Remove an IIP will emit a `RemoveInitial` event.
    pub fn remove_initial(&mut self, id: &str, port: &str) -> &mut Self {
        let port_id = self.get_port_name(port);
        let inits = self.initializers.clone();
        let mut _initializers = Vec::new();
        for iip in inits {
            if iip.to.node_id.as_str() == id && iip.to.port_id == port_id {
                self.event_channel
                    .0
                    .send(GraphEvents::RemoveInitial(json!(iip)))
                    .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
            } else {
                _initializers.push(iip);
            }
        }
        self.initializers = _initializers;
        self
    }

    pub fn remove_graph_initial(&mut self, id: &str) -> &mut Self {
        if let Some(inport) = self.inports.clone().get(id) {
            self.remove_initial(&inport.node_id, &inport.port_id);
        }
        self
    }

    /// Remove a group of nodes
    pub fn remove_group(&mut self, group_id: &str) -> &mut Self {
        self.groups = self
            .groups
            .clone()
            .iter()
            .filter(|v| {
                if v.id == group_id.to_owned() {
                    self.event_channel
                        .0
                        .send(GraphEvents::RemoveGroup(json!(v)))
                        .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
                    return false;
                }
                return true;
            })
            .map(|v| v.clone())
            .collect();

        self
    }

    pub fn add_to_group(&mut self, group_id: &str, node_id: &str) {
        self.node_groups
            .entry(node_id.to_owned())
            .or_insert_with(HashSet::new)
            .insert(group_id.to_owned());
    }

    pub fn remove_from_group(&mut self, group_id: &str, node_id: &str) {
        if let Some(groups) = self.node_groups.get_mut(node_id) {
            groups.remove(group_id);
            if groups.is_empty() {
                self.node_groups.remove(node_id);
            }
        }
    }

    /// Connection objects can be retrieved from the graph by the node and port IDs:
    /// ```no_run
    /// connection = my_graph.get_connection("Read", "out", "Write", "in");
    /// ```
    pub fn get_connection(
        &self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
    ) -> Option<GraphConnection> {
        let out_port = self.get_port_name(port);
        let in_port = self.get_port_name(port2);

        // Use connection indices for O(1) lookup
        self.connection_indices
            .get(&(node.to_owned(), node2.to_owned()))
            .and_then(|indices| {
                indices
                    .iter()
                    .find(|&&idx| {
                        let conn = &self.connections[idx];
                        conn.from.port_id == out_port && conn.to.port_id == in_port
                    })
                    .map(|&idx| self.connections[idx].clone())
            })
    }

    pub fn get_connection_mut(
        &mut self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
    ) -> Option<&mut GraphConnection> {
        let out_port = self.get_port_name(port);
        let in_port = self.get_port_name(port2);

        // Use connection indices for O(1) lookup
        self.connection_indices
            .get_mut(&(node.to_owned(), node2.to_owned()))
            .and_then(|indices| {
                indices
                    .iter_mut()
                    .find(|&&mut idx| {
                        let conn = &self.connections[idx];
                        conn.from.port_id == out_port && conn.to.port_id == in_port
                    })
                    .map(|&mut idx| &mut self.connections[idx])
            })
    }

    /// Disconnected nodes
    ///
    /// Connections between nodes can be removed by providing the
    ///	nodes and ports to disconnect.
    /// ```no_run
    /// my_graph.remove_connection("Display", "out", "Foo", "in");
    /// ```
    /// Removing a connection will emit the `RemoveConnection` event.
    pub fn remove_connection(
        &mut self,
        from_node: &str,
        from_port: &str,
        to_node: &str,
        to_port: &str,
    ) -> &mut Self {
        let from_port = self.get_port_name(from_port);
        let to_port = self.get_port_name(to_port);

        // Create the connection key for our index
        let connection_key = (
            (from_node.to_owned(), from_port.clone()),
            (to_node.to_owned(), to_port.clone()),
        );

        // Find all matching connection indices
        if let Some(indices) = self.connection_port_indices.get(&connection_key) {
            // Clone indices since we'll be modifying the collections
            let indices: Vec<usize> = indices.clone();

            // Remove connections in reverse order to maintain remaining indices
            for &idx in indices.iter().rev() {
                if idx < self.connections.len() {
                    let connection = self.connections[idx].clone();

                    // Remove the connection
                    self.connections.remove(idx);

                    // Emit event
                    self.event_channel
                        .0
                        .send(GraphEvents::RemoveConnection(json!(connection)))
                        .expect("Failed to emit Graph event");
                }
            }

            // Remove the empty index entry
            self.connection_port_indices.remove(&connection_key);

            // Rebuild indices since we modified the connections
            self.rebuild_connection_indices();
        }

        self
    }

    /// Removing a node from the graph
    /// Existing nodes can be removed from a graph by their ID. This
    /// will remove the node and also remove all edges connected to it.
    /// ```no_run
    /// my_graph.remove_node("Read");
    /// ```
    /// Once the node has been removed, the `remove_node` event will be
    pub fn remove_node(&mut self, id: &str) -> &mut Self {
        if let Some(node) = self.nodes.remove(id) {
            self.remove_node_connections(id);

            // Remove initializers
            if let Some(indices) = self.initializer_indices.get(id) {
                for &idx in indices.iter().rev() {
                    let iip = self.initializers.remove(idx);
                    self.event_channel
                        .0
                        .send(GraphEvents::RemoveInitial(json!(iip)))
                        .expect("Failed to emit Graph event");
                }
            }
            self.initializer_indices.remove(id);

            // Remove from ports
            self.inports.retain(|_, edge| edge.node_id != id);
            self.outports.retain(|_, edge| edge.node_id != id);

            // Remove from groups using node_groups index
            if let Some(groups) = self.node_groups.remove(id) {
                for group_id in groups {
                    if let Some(pos) = self.groups.iter().position(|g| g.id == group_id) {
                        let mut group = self.groups[pos].clone();
                        group.nodes.retain(|n| n != id);

                        if group.nodes.is_empty() {
                            self.groups.remove(pos);
                            self.event_channel
                                .0
                                .send(GraphEvents::RemoveGroup(json!(group)))
                                .expect("Failed to emit Graph event");
                        } else {
                            self.groups[pos] = group;
                        }
                    }
                }
            }

            self.event_channel
                .0
                .send(GraphEvents::RemoveNode(json!(node)))
                .expect("Failed to emit Graph event");
        }

        self
    }

    /// Rename the identifier of an inport
    pub fn rename_inport(&mut self, old_port: &str, new_port: &str) -> &mut Self {
        let old_port_name = self.get_port_name(old_port);
        let new_port_name = self.get_port_name(new_port);
        if !self.inports.contains_key(&(old_port_name.clone())) {
            return self;
        }

        if new_port_name == old_port_name {
            return self;
        }

        if let Some(old_port) = self.inports.remove(&old_port_name) {
            self.inports.insert(new_port_name.clone(), old_port.clone());

            self.event_channel
                .0
                .send(GraphEvents::RenameInport(json!({
                    "old": old_port_name.clone(),
                    "new": new_port_name.clone()
                })))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }

        self
    }

    /// Rename the identifier of an outport
    pub fn rename_outport(&mut self, old_port: &str, new_port: &str) -> &mut Self {
        let old_port_name = self.get_port_name(old_port);
        let new_port_name = self.get_port_name(new_port);

        if !self.outports.contains_key(&(old_port_name.clone())) {
            return self;
        }

        if new_port_name == old_port_name {
            return self;
        }

        if let Some(old_port) = self.outports.remove(&old_port_name) {
            self.outports
                .insert(new_port_name.clone(), old_port.clone());

            self.event_channel
                .0
                .send(GraphEvents::RenameOutport(json!({
                    "old": old_port_name.clone(),
                    "new": new_port_name.clone()
                })))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }

        self
    }

    /// Renaming a node
    ///
    /// Nodes IDs can be changed by calling this method.
    pub fn rename_node(&mut self, old_id: &str, new_id: &str) -> &mut Self {
        if let Some(node) = self.get_node_mut(old_id) {
            (*node).id = new_id.to_owned();

            let _ = self.connections.iter_mut().for_each(|edge| {
                if edge.from.node_id == old_id.to_owned() {
                    (*edge).from.node_id = new_id.to_owned()
                }
                if edge.to.node_id == old_id.to_owned() {
                    (*edge).to.node_id = new_id.to_owned()
                }
            });

            let _ = self.initializers.iter_mut().for_each(|iip| {
                if iip.to.node_id == old_id.to_owned() {
                    iip.to.node_id = new_id.to_owned()
                }
            });

            let _ = self.inports.clone().keys().for_each(|port| {
                if let Some(private) = self.inports.get_mut(port) {
                    if private.node_id == old_id.to_owned() {
                        private.node_id = new_id.to_owned();
                    }
                }
            });
            let _ = self.outports.clone().keys().for_each(|port| {
                if let Some(private) = self.outports.get_mut(port) {
                    if private.node_id == old_id.to_owned() {
                        private.node_id = new_id.to_owned();
                    }
                }
            });

            let _ = self.groups.iter_mut().for_each(|group| {
                if let Some(index) = group
                    .nodes
                    .iter()
                    .position(|n| n.to_owned() == old_id.to_owned())
                {
                    group.nodes[index] = new_id.to_owned();
                }
            });

            self.event_channel
                .0
                .send(GraphEvents::RenameNode(json!({
                    "old": old_id,
                    "new": new_id,
                })))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }
        self
    }

    pub fn set_node_metadata(&mut self, id: &str, metadata: HashMap<String, Value>) -> &mut Self {
        if let Some(node) = self.get_node_mut(id) {
            let before = node.metadata.clone();

            if node.metadata.is_none() {
                (*node).metadata = Some(HashMap::new());
            }

            if metadata.keys().len() == 0 {
                (*node).metadata = Some(HashMap::new());
            }

            let _ = metadata.clone().keys().for_each(|item| {
                let meta = metadata.clone();
                let val = meta.get(item);

                if let Some(existing_meta) = node.metadata.as_mut() {
                    if let Some(val) = val {
                        (*existing_meta).insert(item.clone(), val.clone());
                    } else {
                        (*existing_meta).remove(item);
                    }
                }
            });
            let _node = node.clone();

            self.event_channel
                .0
                .send(GraphEvents::ChangeNode(json!({
                    "node": _node,
                    "old_metadata": before,
                    "new_metadata": metadata
                })))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }

        self
    }

    /// Changing a connection's metadata
    ///
    /// Connection metadata can be set or changed by calling this method.
    pub fn set_connection_metadata(
        &mut self,
        node: &str,
        port: &str,
        node2: &str,
        port2: &str,
        metadata: HashMap<String, Value>,
    ) -> &mut Self {
        let edge = self.get_connection_mut(node, port, node2, port2);
        if edge.is_none() {
            return self;
        }

        let edge = edge.unwrap();
        if edge.metadata.is_none() {
            (*edge).metadata = Some(HashMap::new());
        }
        let before = edge.metadata.clone();
        for item in metadata.clone().keys() {
            let val = metadata.get(item);
            if let Some(edge_metadata) = edge.metadata.as_mut() {
                if let Some(val) = val {
                    (*edge_metadata).insert(item.clone(), val.clone());
                } else {
                    (*edge_metadata).remove(item);
                }
            }
        }
        let _edge = edge.clone();

        self.event_channel
            .0
            .send(GraphEvents::ChangeConnection(json!({
                "edge": _edge,
                "old_metadata": before,
                "new_metadata": metadata
            })))
            .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        self
    }

    pub fn set_inport_metadata(
        &mut self,
        port_id: &str,
        metadata: HashMap<String, Value>,
    ) -> &mut Self {
        let port_name = self.get_port_name(port_id);
        if !self.inports.contains_key(&(port_name.clone())) {
            return self;
        }

        if let Some(p) = self.inports.get(&(port_name.clone())) {
            let mut p = p.clone();
            if p.metadata.is_none() {
                p.metadata = Some(HashMap::new());
            }

            let before = p.metadata.clone();

            metadata.clone().keys().for_each(|item| {
                let meta = metadata.clone();
                let val = meta.get(item);
                let mut existing_meta = p.metadata.clone();
                if let Some(existing_meta) = existing_meta.as_mut() {
                    if let Some(val) = val {
                        existing_meta.insert(item.clone(), val.clone());
                    } else {
                        existing_meta.remove(item);
                    }
                    p.metadata = Some(existing_meta.clone());
                    self.inports.insert(port_name.clone(), p.clone());
                } else {
                    // iter.next();
                    return;
                }
            });

            self.event_channel
                .0
                .send(GraphEvents::ChangeInport(json!({
                    "name": port_name.clone(),
                        "port": p.clone(),
                        "old_metadata": before,
                        "new_metadata": metadata
                })))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }

        self
    }

    pub fn set_outport_metadata(
        &mut self,
        port_id: &str,
        metadata: HashMap<String, Value>,
    ) -> &mut Self {
        let port_name = self.get_port_name(port_id);
        if !self.outports.contains_key(&(port_name.clone())) {
            return self;
        }

        if let Some(p) = self.outports.get(&(port_name.clone())) {
            let mut p = p.clone();
            if p.metadata.is_none() {
                p.metadata = Some(HashMap::new());
            }

            let before = p.metadata.clone();

            metadata.clone().keys().for_each(|item| {
                let meta = metadata.clone();
                let val = meta.get(item);
                let mut existing_meta = p.metadata.clone();
                if let Some(existing_meta) = existing_meta.as_mut() {
                    if let Some(val) = val {
                        existing_meta.insert(item.clone(), val.clone());
                    } else {
                        existing_meta.remove(item);
                    }
                    p.metadata = Some(existing_meta.clone());
                    self.outports.insert(port_name.clone(), p.clone());
                } else {
                    // iter.next();
                    return;
                }
            });

            self.event_channel
                .0
                .send(GraphEvents::ChangeOutport(json!({
                    "name": port_name.clone(),
                        "port": p.clone(),
                        "old_metadata": before,
                        "new_metadata": metadata
                })))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }

        self
    }

    pub fn set_group_metadata(
        &mut self,
        group_id: &str,
        metadata: HashMap<String, Value>,
    ) -> &mut Self {
        for (i, group) in self.groups.clone().iter_mut().enumerate() {
            if group.id != group_id.to_owned() {
                continue;
            }
            let before = group.metadata.clone();
            for item in metadata.clone().keys() {
                if let Some(meta) = group.metadata.as_mut() {
                    if let Some(val) = metadata.get(item) {
                        meta.insert(item.to_owned(), val.clone());
                    } else {
                        meta.remove(item);
                    }
                }
            }
            self.groups[i] = group.clone();

            self.event_channel
                .0
                .send(GraphEvents::ChangeGroup(json!({
                    "group": group.clone(),
                    "old_metadata": before,
                    "new_metadata": metadata
                })))
                .expect(format!("{:?}", "Expecetd to emit Graph event").as_str());
        }
        self
    }

    // Helper method to rebuild connection indices after bulk removals
    fn rebuild_connection_indices(&mut self) {
        self.connection_port_indices.clear();

        for (idx, conn) in self.connections.iter().enumerate() {
            let key = (
                (conn.from.node_id.clone(), conn.from.port_id.clone()),
                (conn.to.node_id.clone(), conn.to.port_id.clone()),
            );

            self.connection_port_indices
                .entry(key)
                .or_insert_with(Vec::new)
                .push(idx);
        }
    }

    /// Remove all connections for a node (useful for node deletion)
    pub fn remove_node_connections(&mut self, node_id: &str) -> &mut Self {
        // Find all connections involving this node
        let connections_to_remove: Vec<(usize, GraphConnection)> = self
            .connection_port_indices
            .iter()
            .filter(|(((from_node, _), (to_node, _)), _)| {
                from_node == node_id || to_node == node_id
            })
            .flat_map(|(_, indices)| {
                indices
                    .iter()
                    .map(|&idx| (idx, self.connections[idx].clone()))
            })
            .collect();

        // Remove connections in reverse order
        for (idx, connection) in connections_to_remove.iter().rev() {
            self.connections.remove(*idx);
            self.event_channel
                .0
                .send(GraphEvents::RemoveConnection(json!(connection)))
                .expect("Failed to emit Graph event");
        }

        // Rebuild indices after bulk removal
        self.rebuild_connection_indices();

        self
    }

    /// Get all connections for a specific port
    pub fn get_port_connections(&self, node_id: &str, port_id: &str) -> Vec<&GraphConnection> {
        let port_id = self.get_port_name(port_id);

        self.connection_port_indices
            .iter()
            .filter(|(((from_node, from_port), (to_node, to_port)), _)| {
                (from_node == node_id && from_port == &port_id)
                    || (to_node == node_id && to_port == &port_id)
            })
            .flat_map(|(_, indices)| indices.iter().map(|&idx| &self.connections[idx]))
            .collect()
    }

    /// Get all incoming connections for a node
    /// Returns a vector of (source_node_id, source_port_id, connection)
    pub fn get_incoming_connections(&self, node_id: &str) -> Vec<(&str, &str, &GraphConnection)> {
        self.connection_port_indices
            .iter()
            .filter(|(((_from_node, _from_port), (to_node, _to_port)), _)| to_node == node_id)
            .flat_map(|(_, indices)| {
                indices.iter().filter_map(|&idx| {
                    let conn = &self.connections[idx];
                    Some((&conn.from.node_id[..], &conn.from.port_id[..], conn))
                })
            })
            .collect()
    }

    /// Get all outgoing connections for a node
    /// Returns a vector of (target_node_id, target_port_id, connection)
    pub fn get_outgoing_connections(&self, node_id: &str) -> Vec<(&str, &str, &GraphConnection)> {
        self.connection_port_indices
            .iter()
            .filter(|(((from_node, _from_port), (_to_node, _to_port)), _)| from_node == node_id)
            .flat_map(|(_, indices)| {
                indices.iter().filter_map(|&idx| {
                    let conn = &self.connections[idx];
                    Some((&conn.to.node_id[..], &conn.to.port_id[..], conn))
                })
            })
            .collect()
    }

    /// Get incoming connections for a specific port
    pub fn get_incoming_connections_for_port(
        &self,
        node_id: &str,
        port_id: &str,
    ) -> Vec<(&str, &str, &GraphConnection)> {
        let port_id = self.get_port_name(port_id);

        self.connection_port_indices
            .iter()
            .filter(|(((_from_node, _from_port), (to_node, to_port)), _)| {
                to_node == node_id && to_port == &port_id
            })
            .flat_map(|(_, indices)| {
                indices.iter().filter_map(|&idx| {
                    let conn = &self.connections[idx];
                    Some((&conn.from.node_id[..], &conn.from.port_id[..], conn))
                })
            })
            .collect()
    }

    /// Get outgoing connections for a specific port
    pub fn get_outgoing_connections_for_port(
        &self,
        node_id: &str,
        port_id: &str,
    ) -> Vec<(&str, &str, &GraphConnection)> {
        let port_id = self.get_port_name(port_id);

        self.connection_port_indices
            .iter()
            .filter(|(((from_node, from_port), (_to_node, _to_port)), _)| {
                from_node == node_id && from_port == &port_id
            })
            .flat_map(|(_, indices)| {
                indices.iter().filter_map(|&idx| {
                    let conn = &self.connections[idx];
                    Some((&conn.to.node_id[..], &conn.to.port_id[..], conn))
                })
            })
            .collect()
    }

    /// Get all connected nodes (both incoming and outgoing)
    pub fn get_connected_nodes(&self, node_id: &str) -> HashSet<&str> {
        let mut connected_nodes = HashSet::new();

        // Add nodes with incoming connections
        self.get_incoming_connections(node_id)
            .iter()
            .for_each(|(source_node, _, _)| {
                connected_nodes.insert(*source_node);
            });

        // Add nodes with outgoing connections
        self.get_outgoing_connections(node_id)
            .iter()
            .for_each(|(target_node, _, _)| {
                connected_nodes.insert(*target_node);
            });

        connected_nodes
    }

    /// Get connection degree (incoming and outgoing) for a node
    pub fn get_connection_degree(&self, node_id: &str) -> (usize, usize) {
        let incoming = self.get_incoming_connections(node_id).len();
        let outgoing = self.get_outgoing_connections(node_id).len();
        (incoming, outgoing)
    }

    /// Get connection degree for a specific port
    pub fn get_port_connection_degree(&self, node_id: &str, port_id: &str) -> (usize, usize) {
        let incoming = self
            .get_incoming_connections_for_port(node_id, port_id)
            .len();
        let outgoing = self
            .get_outgoing_connections_for_port(node_id, port_id)
            .len();
        (incoming, outgoing)
    }

    /// Check if two nodes are directly connected
    pub fn are_nodes_connected(&self, node1: &str, node2: &str) -> bool {
        self.connection_port_indices
            .keys()
            .any(|((from_node, _), (to_node, _))| {
                (from_node == node1 && to_node == node2) || (from_node == node2 && to_node == node1)
            })
    }

    /// Check if specific ports are connected
    pub fn are_ports_connected(&self, node1: &str, port1: &str, node2: &str, port2: &str) -> bool {
        let port1 = self.get_port_name(port1);
        let port2 = self.get_port_name(port2);

        self.connection_port_indices.contains_key(&(
            (node1.to_owned(), port1.clone()),
            (node2.to_owned(), port2.clone()),
        )) || self
            .connection_port_indices
            .contains_key(&((node2.to_owned(), port2), (node1.to_owned(), port1)))
    }

    pub fn export(&self) -> GraphExport {
        let mut json = GraphExport {
            case_sensitive: self.case_sensitive,
            properties: HashMap::new(),
            inports: self.inports.clone(),
            outports: self.outports.clone(),
            groups: Vec::new(),
            processes: HashMap::new(),
            connections: Vec::new(),
            graph_dependencies: Vec::new(),
            external_connections: Vec::new(),
            provided_interfaces: HashMap::new(),
            required_interfaces: HashMap::new(),
        };

        json.properties = self.properties.clone();
        json.properties
            .insert("name".to_owned(), Value::from(self.name.to_owned()));

        for group in &self.groups {
            let mut group_data = group.clone();
            if let Some(metadata) = group.metadata.clone() {
                if !metadata.is_empty() {
                    group_data.metadata = Some(metadata);
                }
            }
            json.groups.push(group_data);
        }

        json.processes = self.nodes.clone();

        json.connections = self.connections.clone();

        for iip in self.initializers.clone() {
            json.connections.push(GraphConnection {
                from: GraphEdge::default(),
                to: iip.to,
                data: Some(iip.data.clone()),
                metadata: iip.metadata,
            });
        }

        json
    }

    pub fn load(json: GraphExport, metadata: Option<HashMap<String, Value>>) -> Graph {
        let mut graph = Graph::new(
            json.properties
                .get("name")
                .unwrap()
                .as_str()
                .unwrap_or_default(),
            json.case_sensitive,
            metadata,
        );

        graph.set_properties(HashMap::from_iter(
            json.properties.clone().into_iter().filter(|v| {
                if v.0 != "name" {
                    return true;
                }
                return false;
            }),
        ));

        json.processes.keys().for_each(|prop| {
            if let Some(def) = json.processes.clone().get(prop) {
                graph.add_node(prop.as_str(), &def.component, def.metadata.clone());
            }
        });

        json.connections.clone().into_iter().for_each(|conn| {
            if let Some(data) = conn.data {
                if conn.to.index.is_some() {
                    graph.add_initial_index(
                        data,
                        &conn.to.node_id,
                        &graph.get_port_name(&conn.to.port_id),
                        conn.to.index.unwrap_or_default(),
                        conn.metadata,
                    );
                } else {
                    graph.add_initial(
                        data,
                        &conn.to.node_id,
                        &graph.get_port_name(&&conn.to.port_id),
                        conn.metadata,
                    );
                }

                return;
            }

            // if conn.from.index.is_some() || conn.to.index.is_some() {
            //     graph.add_connection_index(
            //         &conn.from.node_id,
            //         &graph.get_port_name(&conn.from.port_id),
            //         conn.from.index,
            //         &conn.to.node_id,
            //         &graph.get_port_name(&conn.to.port_id),
            //         conn.to.index,
            //         conn.metadata,
            //     );
            //     // iter.next();
            //     return;
            // }
            graph.add_connection(
                &conn.from.node_id,
                &graph.get_port_name(&conn.from.port_id),
                &conn.to.node_id,
                &graph.get_port_name(&conn.to.port_id),
                conn.metadata,
            );
        });

        json.inports.clone().keys().for_each(|inport| {
            if let Some(pri) = json.inports.clone().get(inport) {
                graph.add_inport(
                    inport,
                    &pri.node_id,
                    &graph.get_port_name(&pri.port_name),
                    pri.port_type.clone(),
                    pri.metadata.clone(),
                );
            }
        });
        json.outports.clone().keys().for_each(|outport| {
            if let Some(pri) = json.outports.clone().get(outport) {
                graph.add_outport(
                    outport,
                    &pri.node_id,
                    &graph.get_port_name(&pri.port_name),
                    pri.port_type.clone(),
                    pri.metadata.clone(),
                );
            }
        });

        for group in json.groups.clone() {
            graph.add_group(&group.id, group.nodes, group.metadata);
        }

        graph
    }

    pub fn with_history() -> (Self, GraphHistory) {
        let graph = Self::default();
        let history = GraphHistory::new(graph.event_channel.1.clone());
        (graph, history)
    }

    pub fn with_history_and_limit(max_history: usize) -> (Self, GraphHistory) {
        let graph = Self::default();
        let history = GraphHistory::with_max_history(graph.event_channel.1.clone(), max_history);
        (graph, history)
    }

    pub fn traverse_depth_first<F>(&self, start: &str, mut visitor: F) -> Result<(), GraphError>
    where
        F: FnMut(&GraphNode),
    {
        if !self.nodes.contains_key(start) {
            return Err(GraphError::NodeNotFound(start.to_owned()));
        }

        let mut visited = HashSet::new();
        let mut stack = vec![start.to_owned()];

        while let Some(node_id) = stack.pop() {
            if visited.insert(node_id.clone()) {
                if let Some(node) = self.nodes.get(&node_id) {
                    visitor(node);
                }

                if let Some(neighbors) = self.adjacency_lists.get(&node_id) {
                    for neighbor in neighbors.iter().rev() {
                        if !visited.contains(neighbor) {
                            stack.push(neighbor.clone());
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn traverse_breadth_first<F>(&self, start: &str, mut visitor: F) -> Result<(), GraphError>
    where
        F: FnMut(&GraphNode),
    {
        if !self.nodes.contains_key(start) {
            return Err(GraphError::NodeNotFound(start.to_owned()));
        }

        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(start.to_owned());
        visited.insert(start.to_owned());

        while let Some(node_id) = queue.pop_front() {
            if let Some(node) = self.nodes.get(&node_id) {
                visitor(node);
            }

            if let Some(neighbors) = self.adjacency_lists.get(&node_id) {
                for neighbor in neighbors {
                    if visited.insert(neighbor.clone()) {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        Ok(())
    }

    /// Validates the entire flow graph
    pub fn validate_flow(&self) -> Result<FlowValidation, GraphError> {
        let mut validation = FlowValidation::default();

        // Check for cycles
        if let Some(cycle) = self.detect_cycles() {
            validation.cycles.push(cycle);
        }

        // Check for orphaned nodes
        self.find_orphaned_nodes()
            .into_iter()
            .for_each(|node| validation.orphaned_nodes.push(node));

        // Validate port compatibility
        self.validate_port_types()
            .into_iter()
            .for_each(|error| validation.port_mismatches.push(error));

        Ok(validation)
    }

    /// Traces data flow through the graph from a starting node
    pub fn trace_data_flow(&self, start_node: &str) -> Result<Vec<DataFlowPath>, GraphError> {
        let mut paths = Vec::new();
        let mut visited = HashSet::new();
        let mut current_path = Vec::new();

        self.trace_flow_recursive(start_node, &mut visited, &mut current_path, &mut paths)?;
        Ok(paths)
    }

    /// Finds all possible execution paths through the graph
    pub fn find_execution_paths(&self) -> Vec<ExecutionPath> {
        let mut paths = Vec::new();
        let start_nodes = self.find_initial_nodes();

        for start_node in start_nodes {
            self.trace_execution_path(&start_node, &mut paths);
        }

        paths
    }

    /// Analyzes parallel execution possibilities
    pub fn analyze_parallelism(&self) -> ParallelismAnalysis {
        let mut analysis = ParallelismAnalysis::default();
        // let mut visited = HashSet::new();

        // Find independent subgraphs that can run in parallel
        self.find_subgraphs()
            .into_iter()
            .for_each(|subgraph| analysis.parallel_branches.push(subgraph));

        // Identify pipeline stages
        self.identify_pipeline_stages()
            .into_iter()
            .for_each(|stage| analysis.pipeline_stages.push(stage));

        analysis
    }

    /// Detects bottlenecks in the flow
    pub fn detect_bottlenecks(&self) -> Vec<Bottleneck> {
        let mut bottlenecks = Vec::new();

        // Find nodes with high in/out degree
        self.find_high_degree_nodes()
            .into_iter()
            .for_each(|node| bottlenecks.push(Bottleneck::HighDegree(node)));

        // Find sequential chains that could be parallelized
        self.find_sequential_chains()
            .into_iter()
            .for_each(|chain| bottlenecks.push(Bottleneck::SequentialChain(chain)));

        bottlenecks
    }

    // Helper methods for the main traversal functions

    /// Finds nodes without any incoming connections
    fn find_initial_nodes(&self) -> Vec<String> {
        let nodes = self.nodes.keys().cloned().collect::<HashSet<_>>();
        let mut has_incoming = HashSet::new();

        for conn in &self.connections {
            has_incoming.insert(conn.to.node_id.clone());
        }

        nodes.difference(&has_incoming).cloned().collect()
    }

    /// Recursively traces flow through the graph
    fn trace_flow_recursive(
        &self,
        current_node: &str,
        visited: &mut HashSet<String>,
        current_path: &mut Vec<String>,
        paths: &mut Vec<DataFlowPath>,
    ) -> Result<(), GraphError> {
        if visited.contains(current_node) {
            return Ok(());
        }

        visited.insert(current_node.to_string());
        current_path.push(current_node.to_string());

        // Find all outgoing connections
        let next_nodes = self.get_outgoing_connections(current_node);

        if next_nodes.is_empty() {
            // We've reached an end node, record the path
            paths.push(DataFlowPath {
                nodes: current_path.clone(),
                transforms: self.analyze_path_transforms(current_path),
            });
        } else {
            // Continue tracing each branch
            for (next_node_id, _, _) in next_nodes {
                self.trace_flow_recursive(&next_node_id, visited, current_path, paths)?;
            }
        }

        current_path.pop();
        visited.remove(current_node);
        Ok(())
    }

    /// Traces all possible execution paths starting from a given node
    pub fn trace_execution_path(&self, start_node: &str, paths: &mut Vec<ExecutionPath>) {
        let mut visited = HashSet::new();
        let mut current_path = ExecutionPath {
            nodes: Vec::new(),
            estimated_time: 0.0,
            resource_requirements: HashMap::new(),
        };

        self.trace_path_recursive(
            start_node,
            &mut visited,
            &mut current_path,
            paths,
            0,                   // depth
            &mut HashSet::new(), // cycle detection
        );
    }

    fn trace_path_recursive(
        &self,
        current_node: &str,
        visited: &mut HashSet<String>,
        current_path: &mut ExecutionPath,
        all_paths: &mut Vec<ExecutionPath>,
        depth: usize,
        cycle_nodes: &mut HashSet<String>,
    ) {
        // Prevent infinite recursion
        if depth > self.nodes.len() * 2 {
            return;
        }

        // Check for cycles
        if cycle_nodes.contains(current_node) {
            // Mark this path as cyclic in metadata
            current_path
                .resource_requirements
                .insert("contains_cycle".to_string(), 1.0);
            return;
        }

        // Add current node to path
        cycle_nodes.insert(current_node.to_string());
        current_path.nodes.push(current_node.to_string());

        // Get node metadata for timing estimates
        if let Some(node) = self.nodes.get(current_node) {
            if let Some(metadata) = &node.metadata {
                // Add estimated execution time if available
                if let Some(time) = metadata.get("estimated_time") {
                    if let Some(time_val) = time.as_f64() {
                        current_path.estimated_time += time_val as f32;
                    }
                }

                // Add resource requirements if specified
                if let Some(resources) = metadata.get("resources") {
                    if let Some(obj) = resources.as_object() {
                        for (resource, value) in obj {
                            let requirement = value.as_f64().unwrap_or(0.0) as f32;
                            *current_path
                                .resource_requirements
                                .entry(resource.clone())
                                .or_insert(0.0) += requirement;
                        }
                    }
                }
            }
        }

        // Get outgoing connections grouped by port
        let mut port_connections: HashMap<String, Vec<&GraphConnection>> = HashMap::new();

        for (target_node, target_port, connection) in self.get_outgoing_connections(current_node) {
            port_connections
                .entry(connection.from.port_id.clone())
                .or_default()
                .push(connection);
        }

        // Handle different types of nodes based on their ports
        if port_connections.is_empty() {
            // End node - save the current path
            all_paths.push(current_path.clone());
        } else {
            // Check port types to determine execution behavior
            let mut is_branching = false;
            let mut is_parallel = false;

            for (port_id, connections) in &port_connections {
                if connections.len() > 1 {
                    // Check if this is a branching or parallel execution
                    if let Some(first_conn) = connections.first() {
                        match first_conn.from.port_type {
                            PortType::Flow => {
                                // Multiple flow outputs indicate branching
                                is_branching = true;
                            }
                            PortType::Event | PortType::Stream => {
                                // Events and streams can trigger parallel execution
                                is_parallel = true;
                            }
                            _ => {} // Other types don't affect execution flow
                        }
                    }
                }
            }

            // Handle different execution patterns
            if is_parallel {
                // For parallel execution, create paths for all combinations
                self.trace_parallel_paths(
                    &port_connections,
                    visited,
                    current_path,
                    all_paths,
                    depth,
                    cycle_nodes,
                );
            } else if is_branching {
                // For branching, create separate paths for each branch
                self.trace_branching_paths(
                    &port_connections,
                    visited,
                    current_path,
                    all_paths,
                    depth,
                    cycle_nodes,
                );
            } else {
                // For sequential execution, just follow the single path
                if let Some(connections) = port_connections.values().next() {
                    if let Some(connection) = connections.first() {
                        self.trace_path_recursive(
                            &connection.to.node_id,
                            visited,
                            current_path,
                            all_paths,
                            depth + 1,
                            cycle_nodes,
                        );
                    }
                }
            }
        }

        // Cleanup
        cycle_nodes.remove(current_node);
        current_path.nodes.pop();
    }

    fn trace_parallel_paths(
        &self,
        port_connections: &HashMap<String, Vec<&GraphConnection>>,
        visited: &mut HashSet<String>,
        current_path: &mut ExecutionPath,
        all_paths: &mut Vec<ExecutionPath>,
        depth: usize,
        cycle_nodes: &mut HashSet<String>,
    ) {
        // Keep track of parallel execution time (use maximum of parallel paths)
        let mut max_parallel_time: f32 = 0.0;
        let mut parallel_paths: Vec<ExecutionPath> = Vec::new();

        // Create paths for each parallel branch
        for connections in port_connections.values() {
            for connection in connections {
                let mut branch_path: ExecutionPath = current_path.clone();
                let mut branch_visited = visited.clone();
                let mut branch_cycles = cycle_nodes.clone();

                self.trace_path_recursive(
                    &connection.to.node_id,
                    &mut branch_visited,
                    &mut branch_path,
                    &mut parallel_paths,
                    depth + 1,
                    &mut branch_cycles,
                );

                max_parallel_time =
                    max_parallel_time.max(branch_path.estimated_time - current_path.estimated_time);
            }
        }

        // Adjust timing for parallel execution
        for mut path in parallel_paths {
            path.estimated_time = current_path.estimated_time + max_parallel_time;
            path.resource_requirements.insert(
                "parallel_branches".to_string(),
                port_connections.values().map(|v| v.len() as f32).sum(),
            );
            all_paths.push(path);
        }
    }

    fn trace_branching_paths(
        &self,
        port_connections: &HashMap<String, Vec<&GraphConnection>>,
        visited: &mut HashSet<String>,
        current_path: &mut ExecutionPath,
        all_paths: &mut Vec<ExecutionPath>,
        depth: usize,
        cycle_nodes: &mut HashSet<String>,
    ) {
        // Create a separate path for each branch
        for connections in port_connections.values() {
            for connection in connections {
                let mut branch_path = current_path.clone();
                let mut branch_visited = visited.clone();
                let mut branch_cycles = cycle_nodes.clone();

                self.trace_path_recursive(
                    &connection.to.node_id,
                    &mut branch_visited,
                    &mut branch_path,
                    all_paths,
                    depth + 1,
                    &mut branch_cycles,
                );
            }
        }
    }

    /// Finds independent subgraphs that can be executed in parallel
    fn find_subgraphs(&self) -> Vec<Subgraph> {
        let mut subgraphs = Vec::new();
        let mut visited = HashSet::new();

        for node in self.nodes.keys() {
            if !visited.contains(node) {
                let mut subgraph = Subgraph::default();
                self.explore_subgraph(node, &mut visited, &mut subgraph);

                // Only add non-empty subgraphs
                if !subgraph.nodes.is_empty() {
                    subgraphs.push(subgraph);
                }
            }
        }

        subgraphs
    }

    /// Finds the subgraph reachable from a specific node
    pub fn get_reachable_subgraph(&self, start_node: &str) -> Option<Subgraph> {
        let mut visited = HashSet::new();
        let mut subgraph = Subgraph {
            nodes: Vec::new(),
            internal_connections: Vec::new(),
            entry_points: Vec::new(),
            exit_points: Vec::new(),
        };

        if self.nodes.contains_key(start_node) {
            self.explore_subgraph(start_node, &mut visited, &mut subgraph);
            Some(subgraph)
        } else {
            None
        }
    }

    /// Explores and builds a subgraph starting from a given node
    fn explore_subgraph(
        &self,
        start_node: &str,
        visited: &mut HashSet<String>,
        subgraph: &mut Subgraph,
    ) {
        if visited.contains(start_node) {
            return;
        }

        // Mark this node as visited
        visited.insert(start_node.to_string());

        // Add node to subgraph
        if let Some(node) = self.get_node(start_node) {
            subgraph.nodes.push(start_node.to_string());

            // Check if this is an entry point (has no incoming connections)
            let incoming = self.get_incoming_connections(start_node);
            if incoming.is_empty() {
                subgraph.entry_points.push(start_node.to_string());
            }

            // Check if this is an exit point (has no outgoing connections)
            let outgoing = self.get_outgoing_connections(start_node);
            if outgoing.is_empty() {
                subgraph.exit_points.push(start_node.to_string());
            }

            // Add and explore incoming connections
            for (source_node, source_port, connection) in incoming {
                // Add connection to subgraph
                subgraph.internal_connections.push(connection.clone());

                // Recursively explore the source node if not visited
                if !visited.contains(source_node) {
                    self.explore_subgraph(source_node, visited, subgraph);
                }
            }

            // Add and explore outgoing connections
            for (target_node, target_port, connection) in outgoing {
                // Add connection to subgraph
                subgraph.internal_connections.push(connection.clone());

                // Recursively explore the target node if not visited
                if !visited.contains(target_node) {
                    self.explore_subgraph(target_node, visited, subgraph);
                }
            }
        }
    }

    /// Analyzes subgraph characteristics
    pub fn analyze_subgraph(&self, subgraph: &Subgraph) -> SubgraphAnalysis {
        let mut analysis = SubgraphAnalysis {
            node_count: subgraph.nodes.len(),
            connection_count: subgraph.internal_connections.len(),
            entry_points: subgraph.entry_points.clone(),
            exit_points: subgraph.exit_points.clone(),
            is_cyclic: false,
            max_depth: 0,
            branching_factor: 0.0,
        };

        // Check for cycles
        analysis.is_cyclic = self.has_cycles_in_subgraph(subgraph);

        // Calculate max depth
        analysis.max_depth = self.calculate_subgraph_depth(subgraph);

        // Calculate average branching factor
        let total_branches: f64 = subgraph
            .nodes
            .iter()
            .map(|node| self.get_outgoing_connections(node).len() as f64)
            .sum();

        analysis.branching_factor = if !subgraph.nodes.is_empty() {
            total_branches / subgraph.nodes.len() as f64
        } else {
            0.0
        };

        analysis
    }

    /// Helper method to detect cycles in a subgraph
    fn has_cycles_in_subgraph(&self, subgraph: &Subgraph) -> bool {
        let mut visited = HashSet::new();
        let mut stack = HashSet::new();

        for node in &subgraph.entry_points {
            if self.has_cycle_from_node(node, &mut visited, &mut stack, subgraph) {
                return true;
            }
        }

        false
    }

    /// Helper method for cycle detection using DFS
    fn has_cycle_from_node(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
        subgraph: &Subgraph,
    ) -> bool {
        if !visited.contains(node) {
            visited.insert(node.to_string());
            stack.insert(node.to_string());

            for (target, _, _) in self.get_outgoing_connections(node) {
                if subgraph.nodes.contains(&target.to_string()) {
                    if !visited.contains(target) {
                        if self.has_cycle_from_node(target, visited, stack, subgraph) {
                            return true;
                        }
                    } else if stack.contains(target) {
                        return true;
                    }
                }
            }
        }
        stack.remove(node);
        false
    }

    /// Calculate maximum depth of a subgraph
    fn calculate_subgraph_depth(&self, subgraph: &Subgraph) -> usize {
        let mut max_depth = 0;
        let mut visited = HashSet::new();

        for entry_point in &subgraph.entry_points {
            let depth = self.calculate_depth_from_node(entry_point, &mut visited, subgraph);
            max_depth = max_depth.max(depth);
        }

        max_depth
    }

    /// Helper method to calculate depth from a node
    fn calculate_depth_from_node(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        subgraph: &Subgraph,
    ) -> usize {
        if !visited.insert(node.to_string()) {
            return 0;
        }

        let mut max_child_depth = 0;
        for (target, _, _) in self.get_outgoing_connections(node) {
            if subgraph.nodes.contains(&target.to_string()) {
                let child_depth = self.calculate_depth_from_node(target, visited, subgraph);
                max_child_depth = max_child_depth.max(child_depth);
            }
        }

        max_child_depth + 1
    }

    /// Identifies stages in a processing pipeline
    fn identify_pipeline_stages(&self) -> Vec<PipelineStage> {
        let mut stages = Vec::new();
        let mut assigned = HashSet::new();
        let mut current_stage = 0;

        while assigned.len() < self.nodes.len() {
            let mut stage = PipelineStage {
                level: current_stage,
                nodes: Vec::new(),
            };

            // Find nodes whose dependencies are all in previous stages
            for (node_id, _) in &self.nodes {
                if assigned.contains(node_id) {
                    continue;
                }

                let dependencies = self.get_incoming_connections(node_id);
                if dependencies
                    .iter()
                    .all(|(dep, _, _)| assigned.contains(&(String::from(dep.to_owned()))))
                {
                    stage.nodes.push(node_id.clone());
                    assigned.insert(node_id.clone());
                }
            }

            if stage.nodes.is_empty() {
                break;
            }

            stages.push(stage);
            current_stage += 1;
        }

        stages
    }

    /// Detects cycles in the graph and returns the first cycle found, if any.
    /// Returns a vector representing the path of the cycle if found.
    pub fn detect_cycles(&self) -> Option<Vec<String>> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut cycle_path = Vec::new();

        // Start DFS from each unvisited node
        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) {
                let mut current_path = Vec::new();
                if self.detect_cycle_dfs(
                    node_id,
                    &mut visited,
                    &mut rec_stack,
                    &mut current_path,
                    &mut cycle_path,
                ) {
                    return Some(cycle_path);
                }
            }
        }

        None
    }

    /// Helper method for cycle detection using DFS with path tracking
    fn detect_cycle_dfs(
        &self,
        node: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        current_path: &mut Vec<String>,
        cycle_path: &mut Vec<String>,
    ) -> bool {
        // Add node to both sets
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        current_path.push(node.to_string());

        // Check all adjacent nodes
        let outgoing = self.get_outgoing_connections(node);
        for (next_node, _, _) in outgoing {
            // If not visited, recurse
            if !visited.contains(next_node) {
                if self.detect_cycle_dfs(next_node, visited, rec_stack, current_path, cycle_path) {
                    return true;
                }
            }
            // If in recursion stack, we found a cycle
            else if rec_stack.contains(next_node) {
                // Find the start of the cycle in current_path
                if let Some(cycle_start) = current_path.iter().position(|x| x == next_node) {
                    // Extract the cycle path
                    cycle_path.extend(current_path[cycle_start..].iter().cloned());
                    return true;
                }
            }
        }

        // Remove node from recursion stack and current path
        rec_stack.remove(node);
        current_path.pop();
        false
    }

    /// Detects all cycles in the graph
    pub fn detect_all_cycles(&self) -> Vec<Vec<String>> {
        let mut all_cycles = Vec::new();
        let mut visited_edges = HashSet::new();

        // Start from each node
        for start_node in self.nodes.keys() {
            self.find_cycles_from_node(
                start_node,
                &mut visited_edges,
                &mut Vec::new(),
                &mut all_cycles,
            );
        }

        // Remove duplicate cycles (same cycle detected from different starting points)
        self.normalize_cycles(&mut all_cycles);
        all_cycles
    }

    /// Finds all cycles starting from a specific node
    fn find_cycles_from_node(
        &self,
        current: &str,
        visited_edges: &mut HashSet<(String, String)>,
        current_path: &mut Vec<String>,
        all_cycles: &mut Vec<Vec<String>>,
    ) {
        // Add current node to path
        current_path.push(current.to_string());

        // Check each outgoing connection
        let outgoing = self.get_outgoing_connections(current);
        for (next_node, _, _) in outgoing {
            let edge = (current.to_string(), next_node.to_string());

            // Skip if we've already visited this edge
            if visited_edges.contains(&edge) {
                continue;
            }

            visited_edges.insert(edge);

            // Check if next_node is already in our path (cycle found)
            if let Some(cycle_start) = current_path.iter().position(|x| x == next_node) {
                let mut cycle = current_path[cycle_start..].to_vec();
                cycle.push(next_node.to_string());
                all_cycles.push(cycle);
            } else if current_path.len() < self.nodes.len() {
                // Recurse if path length is less than total nodes
                self.find_cycles_from_node(next_node, visited_edges, current_path, all_cycles);
            }
        }

        current_path.pop();
    }

    /// Normalizes cycles to remove duplicates and rotations
    fn normalize_cycles(&self, cycles: &mut Vec<Vec<String>>) {
        // Sort each cycle to create a canonical representation
        for cycle in cycles.iter_mut() {
            // Find minimum element position
            let min_pos = cycle
                .iter()
                .enumerate()
                .min_by_key(|(_, x)| *x)
                .map(|(i, _)| i)
                .unwrap_or(0);

            // Rotate to start with minimum element
            cycle.rotate_left(min_pos);
        }

        // Remove duplicates
        cycles.sort();
        cycles.dedup();
    }

    /// Checks if a specific node is part of any cycle
    pub fn is_node_in_cycle(&self, node_id: &str) -> bool {
        if let Some(cycles) = self.detect_cycles() {
            cycles.contains(&node_id.to_string())
        } else {
            false
        }
    }

    /// Gets cycle information for analysis
    pub fn analyze_cycles(&self) -> CycleAnalysis {
        let mut analysis = CycleAnalysis {
            total_cycles: 0,
            cycle_lengths: Vec::new(),
            nodes_in_cycles: HashSet::new(),
            longest_cycle: None,
            shortest_cycle: None,
        };

        let all_cycles = self.detect_all_cycles();
        analysis.total_cycles = all_cycles.len();

        for cycle in all_cycles {
            let length = cycle.len();
            analysis.cycle_lengths.push(length);

            // Update nodes in cycles
            for node in &cycle {
                analysis.nodes_in_cycles.insert(node.clone());
            }

            // Update longest cycle
            if analysis
                .longest_cycle
                .as_ref()
                .map_or(true, |c| length > c.len())
            {
                analysis.longest_cycle = Some(cycle.clone());
            }

            // Update shortest cycle
            if analysis
                .shortest_cycle
                .as_ref()
                .map_or(true, |c| length < c.len())
            {
                analysis.shortest_cycle = Some(cycle.clone());
            }
        }

        analysis
    }

    /// Finds all orphaned nodes in the graph.
    /// An orphaned node is one that has:
    /// 1. No incoming connections AND no outgoing connections (completely isolated)
    /// 2. No path to/from any entry point of the graph
    pub fn find_orphaned_nodes(&self) -> Vec<String> {
        let mut orphaned = Vec::new();
        let mut connected = HashSet::new();

        // First, identify all nodes that have any connections
        for node_id in self.nodes.keys() {
            let incoming = self.get_incoming_connections(node_id);
            let outgoing = self.get_outgoing_connections(node_id);

            // If node has no connections at all, it's definitely orphaned
            if incoming.is_empty() && outgoing.is_empty() {
                orphaned.push(node_id.clone());
                continue;
            }

            // Mark connected nodes
            if !incoming.is_empty() {
                connected.insert(node_id.clone());
                for (source, _, _) in incoming {
                    connected.insert(source.to_string());
                }
            }
            if !outgoing.is_empty() {
                connected.insert(node_id.clone());
                for (target, _, _) in outgoing {
                    connected.insert(target.to_string());
                }
            }
        }

        // Add nodes that aren't in the connected set
        for node_id in self.nodes.keys() {
            if !connected.contains(node_id) && !orphaned.contains(node_id) {
                orphaned.push(node_id.clone());
            }
        }

        // Now check for nodes that are connected but unreachable from entry points
        let reachable = self.find_reachable_nodes();

        // Add nodes that are connected but unreachable
        for node_id in self.nodes.keys() {
            if !reachable.contains(node_id) && !orphaned.contains(node_id) {
                orphaned.push(node_id.clone());
            }
        }

        orphaned
    }

    /// Finds all nodes reachable from entry points
    fn find_reachable_nodes(&self) -> HashSet<String> {
        let mut reachable = HashSet::new();
        let mut queue = VecDeque::new();

        // Start with entry points (nodes with no incoming connections or explicit graph inputs)
        for node_id in self.nodes.keys() {
            if self.get_incoming_connections(node_id).is_empty()
                || self.inports.values().any(|p| p.node_id == *node_id)
            {
                queue.push_back(node_id.clone());
                reachable.insert(node_id.clone());
            }
        }

        // Breadth-first search to find all reachable nodes
        while let Some(node_id) = queue.pop_front() {
            for (target, _, _) in self.get_outgoing_connections(&node_id) {
                if !reachable.contains(target) {
                    reachable.insert(target.to_string());
                    queue.push_back(target.to_string());
                }
            }
        }

        reachable
    }

    /// Gets detailed analysis of orphaned nodes
    pub fn analyze_orphaned_nodes(&self) -> OrphanedNodeAnalysis {
        let mut analysis = OrphanedNodeAnalysis {
            total_orphaned: 0,
            completely_isolated: Vec::new(),
            unreachable: Vec::new(),
            disconnected_groups: Vec::new(),
        };

        // Find completely isolated nodes (no connections at all)
        for node_id in self.nodes.keys() {
            let incoming = self.get_incoming_connections(node_id);
            let outgoing = self.get_outgoing_connections(node_id);

            if incoming.is_empty() && outgoing.is_empty() {
                analysis.completely_isolated.push(node_id.clone());
            }
        }

        // Find unreachable nodes (have connections but no path from entry points)
        let reachable = self.find_reachable_nodes();
        for node_id in self.nodes.keys() {
            if !reachable.contains(node_id) && !analysis.completely_isolated.contains(node_id) {
                analysis.unreachable.push(node_id.clone());
            }
        }

        // Find disconnected groups (connected components not connected to main graph)
        let mut visited = HashSet::new();
        for node_id in self.nodes.keys() {
            if !visited.contains(node_id) && !analysis.completely_isolated.contains(node_id) {
                let mut group = Vec::new();
                self.find_connected_component(node_id, &mut visited, &mut group);

                // If this group is disconnected from entry points
                if group.iter().all(|n| !reachable.contains(n)) {
                    analysis.disconnected_groups.push(group);
                }
            }
        }

        analysis.total_orphaned = analysis.completely_isolated.len()
            + analysis.unreachable.len()
            + analysis
                .disconnected_groups
                .iter()
                .map(|g| g.len())
                .sum::<usize>();

        analysis
    }

    /// Helper method to find connected components using DFS
    fn find_connected_component(
        &self,
        start: &str,
        visited: &mut HashSet<String>,
        component: &mut Vec<String>,
    ) {
        if visited.contains(start) {
            return;
        }

        visited.insert(start.to_string());
        component.push(start.to_string());

        // Check outgoing connections
        for (target, _, _) in self.get_outgoing_connections(start) {
            self.find_connected_component(target, visited, component);
        }

        // Check incoming connections
        for (source, _, _) in self.get_incoming_connections(start) {
            self.find_connected_component(source, visited, component);
        }
    }

    /// Validates all port types in the graph
    pub fn validate_port_types(&self) -> Vec<PortMismatch> {
        let mut mismatches = Vec::new();

        // Check all connections
        for connection in &self.connections {
            if let Some(mismatch) = self.validate_connection_types(&connection) {
                mismatches.push(mismatch);
            }
        }

        // Check all initializers
        for initializer in &self.initializers {
            if let Some(mismatch) = self.validate_initializer_types(&initializer) {
                mismatches.push(mismatch);
            }
        }

        mismatches
    }

    /// Validates types for a specific connection
    fn validate_connection_types(&self, connection: &GraphConnection) -> Option<PortMismatch> {
        let from_type = &connection.from.port_type;
        let to_type = &connection.to.port_type;

        // Check type compatibility
        if !self.are_types_compatible(from_type, to_type) {
            return Some(PortMismatch {
                from_node: connection.from.node_id.clone(),
                from_port: connection.from.port_id.clone(),
                from_type: from_type.clone(),
                to_node: connection.to.node_id.clone(),
                to_port: connection.to.port_id.clone(),
                to_type: to_type.clone(),
                reason: "Incompatible types".to_string(),
            });
        }

        None
    }

    /// Validates types for an initializer
    fn validate_initializer_types(&self, initializer: &GraphIIP) -> Option<PortMismatch> {
        let value_type = self.infer_value_type(&initializer.data);
        let port_type = &initializer.to.port_type;

        if !self.is_value_compatible_with_type(&value_type, port_type) {
            return Some(PortMismatch {
                from_node: "Initializer".to_string(),
                from_port: "value".to_string(),
                from_type: value_type,
                to_node: initializer.to.node_id.clone(),
                to_port: initializer.to.port_id.clone(),
                to_type: port_type.clone(),
                reason: "Initializer value type mismatch".to_string(),
            });
        }

        None
    }

    /// Checks if two port types are compatible
    fn are_types_compatible(&self, from_type: &PortType, to_type: &PortType) -> bool {
        match (from_type, to_type) {
            // Any type can be connected to Any
            (_, PortType::Any) | (PortType::Any, _) => true,

            // Same types are compatible
            (a, b) if a == b => true,

            // Array compatibility
            (PortType::Array(a), PortType::Array(b)) => self.are_types_compatible(a, b),

            // Option compatibility
            (PortType::Option(a), b) => self.are_types_compatible(a, b),
            (a, PortType::Option(b)) => self.are_types_compatible(a, b),

            // Generic type compatibility
            // (PortType::Generic(_), _) | (_, PortType::Generic(_)) => true,

            // Tuple compatibility
            // (PortType::Tuple(a), PortType::Tuple(b)) => {
            //     if a.len() != b.len() {
            //         return false;
            //     }
            //     a.iter()
            //         .zip(b.iter())
            //         .all(|(a, b)| self.are_types_compatible(a, b))
            // }

            // Number type compatibility
            (PortType::Integer, PortType::Float) => true,

            // Stream compatibility
            (PortType::Stream, _) | (_, PortType::Stream) => true,

            // Everything else is incompatible
            _ => false,
        }
    }

    /// Infers the type of a JSON value
    fn infer_value_type(&self, value: &Value) -> PortType {
        match value {
            Value::Bool(_) => PortType::Boolean,
            Value::Number(n) => {
                if n.is_i64() {
                    PortType::Integer
                } else {
                    PortType::Float
                }
            }
            Value::String(_) => PortType::String,
            Value::Array(arr) => {
                if let Some(first) = arr.first() {
                    PortType::Array(Box::new(self.infer_value_type(first)))
                } else {
                    PortType::Array(Box::new(PortType::Any))
                }
            }
            Value::Object(_) => PortType::Object("Dynamic".to_string()),
            Value::Null => PortType::Option(Box::new(PortType::Any)),
        }
    }

    /// Checks if a value is compatible with a port type
    fn is_value_compatible_with_type(&self, value_type: &PortType, port_type: &PortType) -> bool {
        self.are_types_compatible(value_type, port_type)
    }

    /// Finds nodes with unusually high number of connections (potential bottlenecks)
    pub fn find_high_degree_nodes(&self) -> Vec<String> {
        let mut high_degree_nodes = Vec::new();
        let mut degree_counts: Vec<(String, usize)> = Vec::new();

        // Calculate degrees for all nodes
        for node_id in self.nodes.keys() {
            let in_degree = self.get_incoming_connections(node_id).len();
            let out_degree = self.get_outgoing_connections(node_id).len();
            let total_degree = in_degree + out_degree;

            degree_counts.push((node_id.clone(), total_degree));
        }

        if degree_counts.is_empty() {
            return high_degree_nodes;
        }

        // Calculate mean and standard deviation
        let mean_degree = degree_counts
            .iter()
            .map(|(_, degree)| degree)
            .sum::<usize>() as f64
            / degree_counts.len() as f64;

        let variance = degree_counts
            .iter()
            .map(|(_, degree)| {
                let diff = *degree as f64 - mean_degree;
                diff * diff
            })
            .sum::<f64>()
            / degree_counts.len() as f64;

        let std_dev = variance.sqrt();

        // Threshold for high degree (mean + 2 standard deviations)
        let threshold = mean_degree + 2.0 * std_dev;

        // Find nodes above threshold
        for (node_id, degree) in degree_counts {
            if degree as f64 > threshold {
                high_degree_nodes.push(node_id);
            }
        }

        high_degree_nodes
    }

    /// Finds chains of nodes that could potentially be parallelized
    pub fn find_sequential_chains(&self) -> Vec<Vec<String>> {
        let mut chains = Vec::new();
        let mut visited = HashSet::new();

        // Function to check if a node is part of a linear chain
        let is_chain_node = |node_id: &str, graph: &Graph| -> bool {
            let in_conn = graph.get_incoming_connections(node_id);
            let out_conn = graph.get_outgoing_connections(node_id);

            // Check if node has exactly one input and one output
            in_conn.len() == 1 && out_conn.len() == 1 &&
                // And all ports are of type Flow (not Event or parallel types)
                in_conn.iter().all(|(_, _, conn)| matches!(conn.to.port_type, PortType::Flow)) &&
                out_conn.iter().all(|(_, _, conn)| matches!(conn.from.port_type, PortType::Flow))
        };

        // Find start of chains (nodes with one output but not part of visited chains)
        for node_id in self.nodes.keys() {
            if visited.contains(node_id) {
                continue;
            }

            let out_conn = self.get_outgoing_connections(node_id);
            if out_conn.len() == 1 {
                let mut current_chain = Vec::new();
                let mut current = node_id.clone();

                // Follow the chain
                while let Some((next_node, _, _)) = self.get_outgoing_connections(&current).first()
                {
                    current_chain.push(current.clone());
                    visited.insert(current.clone());

                    if !is_chain_node(next_node, self) {
                        if !current_chain.contains(&next_node.to_string()) {
                            current_chain.push(next_node.to_string());
                        }
                        break;
                    }

                    current = next_node.to_string();
                }

                // Only consider chains of 3 or more nodes as potential parallelization candidates
                if current_chain.len() >= 3 {
                    chains.push(current_chain);
                }
            }
        }

        chains
    }

    /// Analyzes transformations along a path
    pub fn analyze_path_transforms(&self, path: &[String]) -> Vec<DataTransform> {
        let mut transforms = Vec::new();

        for window in path.windows(2) {
            if let [from_node, to_node] = window {
                // Find the connection between these nodes
                if let Some(connections) = self.get_connections_between(from_node, to_node) {
                    for (from_port, to_port, connection) in connections {
                        let transform = DataTransform {
                            node: to_node.clone(),
                            operation: self.infer_operation(to_node, &connection),
                            input_type: self.get_port_type_string(&connection.from.port_type),
                            output_type: self.get_port_type_string(&connection.to.port_type),
                        };
                        transforms.push(transform);
                    }
                }
            }
        }

        transforms
    }

    /// Helper method to get all connections between two nodes
    fn get_connections_between(
        &self,
        from_node: &str,
        to_node: &str,
    ) -> Option<Vec<(String, String, &GraphConnection)>> {
        let mut connections = Vec::new();

        for (target, target_port, connection) in self.get_outgoing_connections(from_node) {
            if target == to_node {
                connections.push((
                    connection.from.port_id.clone(),
                    connection.to.port_id.clone(),
                    connection,
                ));
            }
        }

        if connections.is_empty() {
            None
        } else {
            Some(connections)
        }
    }

    /// Infers the operation type based on node and connection metadata
    fn infer_operation(&self, node_id: &str, connection: &GraphConnection) -> String {
        if let Some(node) = self.nodes.get(node_id) {
            // Check node metadata first
            if let Some(metadata) = &node.metadata {
                if let Some(op) = metadata.get("operation") {
                    if let Some(op_str) = op.as_str() {
                        return op_str.to_string();
                    }
                }
            }

            // // If no explicit operation, infer from component type
            // match node.component.as_str() {
            //     "Filter" => "filter".to_string(),
            //     "Map" => "transform".to_string(),
            //     "Reduce" => "reduce".to_string(),
            //     "Merge" => "merge".to_string(),
            //     "Split" => "split".to_string(),
            //     _ => "process".to_string(),
            // }
            if self.case_sensitive {
                return node.component.clone();
            }
            node.component.to_lowercase().trim().replace(" ", "_")
        } else {
            "unknown".to_string()
        }
    }

    /// Converts PortType to string representation
    fn get_port_type_string(&self, port_type: &PortType) -> String {
        match port_type {
            PortType::Flow => "flow".to_string(),
            PortType::Event => "event".to_string(),
            PortType::Boolean => "boolean".to_string(),
            PortType::Integer => "integer".to_string(),
            PortType::Float => "float".to_string(),
            PortType::String => "string".to_string(),
            PortType::Object(name) => format!("object<{}>", name),
            PortType::Array(inner) => format!("array<{}>", self.get_port_type_string(inner)),
            PortType::Stream => "stream".to_string(),
            PortType::Any => "any".to_string(),
            // PortType::Generic(name) => format!("generic<{}>", name),
            PortType::Option(inner) => format!("option<{}>", self.get_port_type_string(inner)),
            PortType::Encoded => format!("encoded"),
            // PortType::Tuple(types) => format!(
            //     "tuple<{}>",
            //     types
            //         .iter()
            //         .map(|t| self.get_port_type_string(t))
            //         .collect::<Vec<_>>()
            //         .join(", ")
            // ),
        }
    }

    /// Performs comprehensive analysis of the graph for runtime optimization
    pub fn analyze_for_runtime(&self) -> EnhancedGraphAnalysis {
        let mut analysis = EnhancedGraphAnalysis::default();

        // Analyze parallelism opportunities
        analysis.parallelism = self.analyze_parallelism();

        // Estimate execution time based on metadata and graph structure
        analysis.estimated_execution_time = self.estimate_execution_time();

        // Analyze resource requirements
        analysis.resource_requirements = self.analyze_resource_requirements();

        // Find optimization opportunities
        analysis.optimization_suggestions = self.find_optimization_opportunities();

        // Identify performance bottlenecks
        analysis.performance_bottlenecks = self.detect_bottlenecks();

        analysis
    }

    /// Estimates total execution time based on node metadata and graph structure
    fn estimate_execution_time(&self) -> f64 {
        let mut total_time = 0.0;
        let stages = self.identify_pipeline_stages();

        for stage in stages {
            let mut stage_time = 0.0;
            for node_id in stage.nodes {
                if let Some(node) = self.nodes.get(&node_id) {
                    if let Some(metadata) = &node.metadata {
                        if let Some(time) = metadata.get("estimated_time") {
                            if let Some(t) = time.as_f64() {
                                stage_time = if t > stage_time { t } else { stage_time };
                            }
                        }
                    }
                }
            }
            total_time += stage_time;
        }

        total_time
    }

    /// Analyzes resource requirements for the entire graph
    fn analyze_resource_requirements(&self) -> HashMap<String, f64> {
        let mut requirements = HashMap::new();

        for node in self.nodes.values() {
            if let Some(metadata) = &node.metadata {
                if let Some(resources) = metadata.get("resources") {
                    if let Some(obj) = resources.as_object() {
                        for (resource, value) in obj {
                            let requirement = value.as_f64().unwrap_or(0.0);
                            *requirements.entry(resource.clone()).or_insert(0.0) += requirement;
                        }
                    }
                }
            }
        }

        requirements
    }

    /// Finds potential optimization opportunities in the graph
    fn find_optimization_opportunities(&self) -> Vec<OptimizationSuggestion> {
        let mut suggestions = Vec::new();

        // Find parallelizable chains
        for chain in self.find_sequential_chains() {
            suggestions.push(OptimizationSuggestion::ParallelizableChain { nodes: chain });
        }

        // Check for redundant nodes
        for node_id in self.nodes.keys() {
            if self.is_node_redundant(node_id) {
                suggestions.push(OptimizationSuggestion::RedundantNode {
                    node: node_id.clone(),
                    reason: "Node output is unused".to_string(),
                });
            }
        }

        // Analyze resource bottlenecks
        for (resource, usage) in self.analyze_resource_requirements() {
            if usage > 0.8 {
                // 80% threshold
                suggestions.push(OptimizationSuggestion::ResourceBottleneck {
                    resource,
                    severity: usage,
                });
            }
        }

        suggestions
    }

    /// Checks if a node is redundant (output never used)
    fn is_node_redundant(&self, node_id: &str) -> bool {
        let outgoing = self.get_outgoing_connections(node_id);
        outgoing.is_empty() && !self.outports.values().any(|p| p.node_id == node_id)
    }

    /// Calculate automatic layout positions for nodes
    pub fn calculate_layout(&self) -> HashMap<String, NodePosition> {
        let mut positions = HashMap::new();
        let mut layer_assignments = HashMap::new();
        let mut layer_counts = HashMap::new();

        // Calculate spacing based on node dimensions
        let (horizontal_spacing, vertical_spacing) = self.calculate_spacing();

        // Find root nodes (nodes with no incoming connections)
        let root_nodes: Vec<String> = self
            .nodes
            .keys()
            .filter(|node_id| self.get_incoming_connections(node_id).is_empty())
            .cloned()
            .collect();

        // Use BFS to assign layers and calculate horizontal positions
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();

        // Start with root nodes in the first layer
        for node_id in root_nodes {
            queue.push_back((node_id.clone(), 0));
            layer_assignments.insert(node_id.clone(), 0);
            *layer_counts.entry(0).or_insert(0) += 1;
        }

        // Process nodes level by level
        while let Some((node_id, layer)) = queue.pop_front() {
            if !visited.insert(node_id.clone()) {
                continue;
            }

            let node_count = *layer_counts.get(&layer).unwrap_or(&1) as f32;

            // Calculate base position where we want the anchor point
            let base_x = horizontal_spacing * node_count;
            let base_y = vertical_spacing * layer as f32;

            // Get node dimensions and adjust position based on anchor point
            if let Some(dimensions) = self.get_node_layout_info(&node_id) {
                let position = self.calculate_anchored_position(base_x, base_y, &dimensions);
                positions.insert(node_id.clone(), position);
            } else {
                // Fallback to center-anchored positioning
                positions.insert(
                    node_id.clone(),
                    NodePosition {
                        x: base_x,
                        y: base_y,
                    },
                );
            }

            // Process outgoing connections
            for (next_node, _, _) in self.get_outgoing_connections(&node_id) {
                let next_layer = layer + 1;

                // Only process if not already assigned to a higher layer
                if layer_assignments
                    .get(next_node)
                    .map_or(true, |&l| l < next_layer)
                {
                    layer_assignments.insert(next_node.to_string(), next_layer);
                    *layer_counts.entry(next_layer).or_insert(0) += 1;
                    queue.push_back((next_node.to_string(), next_layer));
                }
            }
        }

        // Center nodes in each layer
        for layer in 0..=*layer_assignments.values().max().unwrap_or(&0) {
            let nodes_in_layer: Vec<_> = positions
                .iter()
                .filter(|(_, pos)| (pos.y / vertical_spacing) as i32 == layer)
                .map(|(id, _)| id.clone())
                .collect();

            // Calculate total width of layer including node dimensions
            let layer_width: f32 = nodes_in_layer
                .iter()
                .map(|node_id| {
                    self.get_node_layout_info(node_id)
                        .map(|d| d.width + 50.0)
                        .unwrap_or(horizontal_spacing)
                })
                .sum();

            let start_x = -layer_width / 2.0;
            let mut current_x = start_x;

            // Position nodes within layer
            for node_id in nodes_in_layer {
                if let Some(dimensions) = self.get_node_layout_info(&node_id) {
                    let position = self.calculate_anchored_position(
                        current_x + (dimensions.width * dimensions.anchor.x),
                        positions[&node_id].y,
                        &dimensions,
                    );
                    positions.insert(node_id.clone(), position);
                    current_x += dimensions.width + 50.0;
                } else {
                    if let Some(pos) = positions.get_mut(&node_id) {
                        pos.x = current_x + horizontal_spacing / 2.0;
                        current_x += horizontal_spacing;
                    }
                }
            }
        }

        // Minimize edge crossings while respecting node dimensions
        self.minimize_edge_crossings(&mut positions, &layer_assignments);

        positions
    }

    /// Minimize edge crossings by adjusting node positions within their layers
    fn minimize_edge_crossings(
        &self,
        positions: &mut HashMap<String, NodePosition>,
        layer_assignments: &HashMap<String, i32>,
    ) {
        let max_layer = *layer_assignments.values().max().unwrap_or(&0);

        // Iterate through layers bottom-up and top-down several times
        for _ in 0..3 {
            // Bottom-up pass
            for layer in (1..=max_layer).rev() {
                self.optimize_layer_positions(layer, positions, layer_assignments);
            }

            // Top-down pass
            for layer in 1..=max_layer {
                self.optimize_layer_positions(layer, positions, layer_assignments);
            }
        }
    }

    /// Optimize positions of nodes within a layer to minimize edge crossings
    fn optimize_layer_positions(
        &self,
        layer: i32,
        positions: &mut HashMap<String, NodePosition>,
        layer_assignments: &HashMap<String, i32>,
    ) {
        // Get nodes in current layer
        let layer_nodes: Vec<_> = layer_assignments
            .iter()
            .filter(|(_, l)| **l == layer)
            .map(|(n, _)| n.clone())
            .collect();

        // Calculate barycenter for each node based on connected nodes' positions
        let mut node_barycenters: Vec<(String, f32)> = layer_nodes
            .iter()
            .map(|node_id| {
                let incoming = self.get_incoming_connections(node_id);
                let outgoing = self.get_outgoing_connections(node_id);

                let connected_positions: Vec<f32> = incoming
                    .iter()
                    .chain(outgoing.iter())
                    .filter_map(|(connected_id, _, _)| {
                        positions.get(*connected_id).map(|pos| pos.x)
                    })
                    .collect();

                let barycenter = if !connected_positions.is_empty() {
                    connected_positions.iter().sum::<f32>() / connected_positions.len() as f32
                } else {
                    positions.get(node_id).map(|pos| pos.x).unwrap_or(0.0)
                };

                (node_id.clone(), barycenter)
            })
            .collect();

        // Sort nodes by their barycenter
        node_barycenters.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Reposition nodes based on sorting
        let mut current_x = positions
            .get(&node_barycenters[0].0)
            .map(|p| p.x)
            .unwrap_or(0.0);

        for (node_id, _) in node_barycenters {
            if let Some(dimensions) = self.get_node_layout_info(&node_id) {
                if let Some(pos) = positions.get_mut(&node_id) {
                    pos.x = current_x;
                    current_x += dimensions.width + 50.0;
                }
            } else if let Some(pos) = positions.get_mut(&node_id) {
                pos.x = current_x;
                current_x += 150.0; // Default spacing
            }
        }
    }

    /// Apply auto-layout to the graph and store positions in node metadata
    pub fn auto_layout(&mut self) -> Result<(), GraphError> {
        let positions = self.calculate_layout();

        // Update node metadata with positions
        for (node_id, position) in positions {
            let mut metadata = self
                .get_node(&node_id)
                .and_then(|node| node.metadata.clone())
                .unwrap_or_default();

            // Create or update position map in metadata
            let position_map = json!({
                "x": position.x,
                "y": position.y
            });

            metadata.insert("position".to_string(), position_map);
            self.set_node_metadata(&node_id, metadata);
        }

        Ok(())
    }

    /// Update position for a specific node
    pub fn set_node_position(&mut self, node_id: &str, x: f32, y: f32) -> Result<(), GraphError> {
        let mut metadata = self
            .get_node(node_id)
            .and_then(|node| node.metadata.clone())
            .unwrap_or_default();

        let position_map = json!({
            "x": x,
            "y": y
        });

        metadata.insert("position".to_string(), position_map);
        self.set_node_metadata(node_id, metadata);
        Ok(())
    }

    /// Get node dimensions and anchor from metadata
    fn get_node_layout_info(&self, node_id: &str) -> Option<NodeDimensions> {
        let node = self.get_node(node_id)?;
        let metadata = node.metadata.as_ref()?;
        let dimensions = metadata.get("dimensions")?.as_object()?;

        let width = dimensions.get("width")?.as_f64()? as f32;
        let height = dimensions.get("height")?.as_f64()? as f32;

        // Get anchor point or default to center (0.5, 0.5)
        let anchor = if let Some(anchor) = dimensions.get("anchor").and_then(|a| a.as_object()) {
            AnchorPoint {
                x: anchor.get("x").and_then(|x| x.as_f64()).unwrap_or(0.5) as f32,
                y: anchor.get("y").and_then(|y| y.as_f64()).unwrap_or(0.5) as f32,
            }
        } else {
            AnchorPoint { x: 0.5, y: 0.5 }
        };

        Some(NodeDimensions {
            width,
            height,
            anchor,
        })
    }

    /// Calculate spacing based on node dimensions
    fn calculate_spacing(&self) -> (f32, f32) {
        // Default spacing if no dimensions available
        let mut horizontal_spacing = 150.0;
        let mut vertical_spacing = 100.0;

        // Try to find a node with dimensions
        for node_id in self.nodes.keys() {
            if let Some(dimensions) = self.get_node_layout_info(node_id) {
                horizontal_spacing = dimensions.width + 50.0; // 50px padding
                vertical_spacing = dimensions.height + 50.0; // 50px padding
                break;
            }
        }

        (horizontal_spacing, vertical_spacing)
    }

    /// Calculate position considering anchor point
    fn calculate_anchored_position(
        &self,
        base_x: f32,
        base_y: f32,
        dimensions: &NodeDimensions,
    ) -> NodePosition {
        // The base_x and base_y are where we want the anchor point to be
        // We need to offset by the anchor's position within the node
        NodePosition {
            x: base_x - (dimensions.width * dimensions.anchor.x),
            y: base_y - (dimensions.height * dimensions.anchor.y),
        }
    }
}
