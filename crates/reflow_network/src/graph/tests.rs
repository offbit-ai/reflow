use super::*;
use serde_json::json;
use std::collections::HashMap;

#[test]
fn test_graph_basic_operations() {
    let mut graph = Graph::new("test_graph", true, None);
    
    // Test node operations
    graph.add_node("node1", "TestComponent", None);
    assert!(graph.get_node("node1").is_some());
    
    let metadata = HashMap::from([
        ("x".to_string(), json!(100)),
        ("y".to_string(), json!(200)),
    ]);
    graph.add_node("node2", "TestComponent2", Some(metadata.clone()));
    
    let node2 = graph.get_node("node2").unwrap();
    assert_eq!(node2.metadata.as_ref().unwrap().get("x").unwrap(), &json!(100));
    
    // Test connection operations
    graph.add_connection("node1", "out", "node2", "in", None);
    let conn = graph.get_connection("node1", "out", "node2", "in").unwrap();
    assert_eq!(conn.from.node_id, "node1");
    assert_eq!(conn.to.node_id, "node2");
    
    // Test port operations
    graph.add_inport("input1", "node1", "in", PortType::Any, None);
    graph.add_outport("output1", "node2", "out", PortType::Any, None);
    
    assert!(graph.inports.contains_key("input1"));
    assert!(graph.outports.contains_key("output1"));
    
    // Test initial values
    graph.add_initial(json!("test_data"), "node1", "in", None);
    let iip = graph.initializers.iter().find(|i| i.to.node_id == "node1").unwrap();
    assert_eq!(iip.data, json!("test_data"));
}

#[test]
fn test_graph_history() {
    let mut graph = Graph::new("test_graph", true, None);
    let mut history = GraphHistory::new(graph.event_channel.1.clone());
    
    // Test node addition with history
    let add_node_cmd = AddNodeCommand::new(
        "node1".to_string(),
        "TestComponent".to_string(),
        None,
    );
    history.execute(Box::new(add_node_cmd), &mut graph).unwrap();
    assert!(graph.get_node("node1").is_some());
    
    // Test undo
    history.undo(&mut graph).unwrap();
    assert!(graph.get_node("node1").is_none());
    
    // Test redo
    history.redo(&mut graph).unwrap();
    assert!(graph.get_node("node1").is_some());
    
    // Test transaction
    let mut transaction = history.begin_transaction();
    
    transaction.add_command(Box::new(AddNodeCommand::new(
        "node2".to_string(),
        "TestComponent2".to_string(),
        None,
    )));
    
    transaction.add_command(Box::new(AddConnectionCommand::new(
        "node1".to_string(),
        "out".to_string(),
        "node2".to_string(),
        "in".to_string(),
        None,
    )));
    
    history.commit_transaction(transaction, &mut graph).unwrap();
    
    assert!(graph.get_node("node2").is_some());
    assert!(graph.get_connection("node1", "out", "node2", "in").is_some());
    
    // Test transaction undo
    history.undo(&mut graph).unwrap();
    assert!(graph.get_node("node2").is_none());
    assert!(graph.get_connection("node1", "out", "node2", "in").is_none());
}

#[test]
fn test_graph_metadata() {
    let mut graph = Graph::new("test_graph", true, None);
    
    // Test node metadata
    let node_metadata = HashMap::from([
        ("x".to_string(), json!(100)),
        ("y".to_string(), json!(200)),
    ]);
    
    graph.add_node("node1", "TestComponent", Some(node_metadata.clone()));
    graph.set_node_metadata("node1", HashMap::from([
        ("x".to_string(), json!(300)),
    ]));
    
    let node = graph.get_node("node1").unwrap();
    assert_eq!(node.metadata.as_ref().unwrap().get("x").unwrap(), &json!(300));
    assert_eq!(node.metadata.as_ref().unwrap().get("y").unwrap(), &json!(200));
    
    // Test group metadata
    let group_metadata = HashMap::from([
        ("color".to_string(), json!("#ff0000")),
    ]);
    
    graph.add_group("group1", vec!["node1".to_string()], Some(group_metadata));
    graph.set_group_metadata("group1", HashMap::from([
        ("color".to_string(), json!("#00ff00")),
    ]));
    
    let group = graph.groups.iter().find(|g| g.id == "group1").unwrap();
    assert_eq!(group.metadata.as_ref().unwrap().get("color").unwrap(), &json!("#00ff00"));
}

#[test]
fn test_graph_port_operations() {
    let mut graph = Graph::new("test_graph", true, None);
    graph.add_node("node1", "TestComponent", None);
    
    // Test inport operations
    graph.add_inport("input1", "node1", "in", PortType::Any, None);
    assert!(graph.inports.contains_key("input1"));
    
    graph.rename_inport("input1", "new_input");
    assert!(!graph.inports.contains_key("input1"));
    assert!(graph.inports.contains_key("new_input"));
    
    // Test outport operations
    graph.add_outport("output1", "node1", "out", PortType::Any, None);
    assert!(graph.outports.contains_key("output1"));
    
    graph.rename_outport("output1", "new_output");
    assert!(!graph.outports.contains_key("output1"));
    assert!(graph.outports.contains_key("new_output"));
    
    // Test port metadata
    let port_metadata = HashMap::from([
        ("x".to_string(), json!(100)),
        ("y".to_string(), json!(200)),
    ]);
    
    graph.set_inport_metadata("new_input", port_metadata.clone());
    let inport = graph.inports.get("new_input").unwrap();
    assert_eq!(inport.metadata.as_ref().unwrap().get("x").unwrap(), &json!(100));
    
    graph.set_outport_metadata("new_output", port_metadata);
    let outport = graph.outports.get("new_output").unwrap();
    assert_eq!(outport.metadata.as_ref().unwrap().get("y").unwrap(), &json!(200));
}

#[test]
fn test_graph_error_handling() {
    let mut graph = Graph::new("test_graph", true, None);
    
    // Test duplicate node
    graph.add_node("node1", "TestComponent", None);
    graph.add_node("node1", "TestComponent", None);
    assert!(!graph.graph_errors.is_empty());
    assert!(matches!(graph.graph_errors[0], GraphError::DuplicateNode(_)));
    
    // Test invalid connection
    graph.add_connection("nonexistent", "out", "node1", "in", None);
    assert!(graph.get_connection("nonexistent", "out", "node1", "in").is_none());
    
    // Test invalid node removal
    graph.remove_node("nonexistent");
    assert!(graph.get_node("nonexistent").is_none());
}

#[test]
fn test_graph_serialization() {
    let mut graph = Graph::new("test_graph", true, None);
    
    // Add some nodes and connections
    graph.add_node("node1", "TestComponent", None);
    graph.add_node("node2", "TestComponent2", None);
    graph.add_connection("node1", "out", "node2", "in", None);
    graph.add_initial(json!("test_data"), "node1", "in", None);
    
    // Export the graph
    let exported = graph.export();
    
    // Create a new graph from the export
    let imported = Graph::load(exported, None);
    
    // Verify the imported graph
    assert!(imported.get_node("node1").is_some());
    assert!(imported.get_node("node2").is_some());
    assert!(imported.get_connection("node1", "out", "node2", "in").is_some());
    assert!(imported.initializers.iter().any(|i| i.data == json!("test_data")));
}

#[cfg(target_arch = "wasm32")]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;
    
    wasm_bindgen_test_configure!(run_in_browser);
    
    #[wasm_bindgen_test]
    fn test_wasm_graph_operations() {
        let mut graph = Graph::_new("test_graph", true, JsValue::NULL);
        
        // Test node operations
        graph._add_node("node1", "TestComponent", JsValue::NULL);
        assert!(graph.get_node("node1").is_some());
        
        // Test connection operations
        graph._add_node("node2", "TestComponent2", JsValue::NULL);
        graph._add_connection("node1", "out", "node2", "in", JsValue::NULL);
        assert!(graph.get_connection("node1", "out", "node2", "in").is_some());
    }
    
    #[wasm_bindgen_test]
    fn test_wasm_history_operations() {
        let mut graph = Graph::_new("test_graph", true, JsValue::NULL);
        let mut history = GraphHistory::_new(&graph);
        
        graph._add_node("node1", "TestComponent", JsValue::NULL);
        history.process_events(&mut graph).unwrap();
        
        let state = history._get_state();
        assert!(state.can_undo);
        assert!(!state.can_redo);
    }

    #[wasm_bindgen_test]
    fn test_wasm_port_operations() {
        let mut graph = Graph::_new("test_graph", true, JsValue::NULL);
        graph._add_node("node1", "TestComponent", JsValue::NULL);

        // Test inport operations
        graph._add_inport("input1", "node1", "in", PortType::Any, JsValue::NULL);
        assert!(graph.inports.contains_key("input1"));

        // Test outport operations
        graph._add_outport("output1", "node1", "out", PortType::Any, JsValue::NULL);
        assert!(graph.outports.contains_key("output1"));

        // Test port metadata
        let metadata = js_sys::Object::new();
        js_sys::Reflect::set(&metadata, &"x".into(), &100.into()).unwrap();
        graph._set_inport_metadata("input1", metadata.into());
        
        let inport = graph.inports.get("input1").unwrap();
        assert_eq!(inport.metadata.as_ref().unwrap().get("x").unwrap(), &json!(100));
    }

    #[wasm_bindgen_test]
    fn test_wasm_graph_export_import() {
        let mut graph = Graph::_new("test_graph", true, JsValue::NULL);
        
        // Setup test graph
        graph._add_node("node1", "TestComponent", JsValue::NULL);
        graph._add_node("node2", "TestComponent2", JsValue::NULL);
        graph._add_connection("node1", "out", "node2", "in", JsValue::NULL);
        
        // Test export
        let exported = graph._export();
        assert!(matches!(exported, GraphExport{..}));
        
        // Test import
        let imported = Graph::_load(exported, JsValue::NULL);
        assert!(imported.get_node("node1").is_some());
        assert!(imported.get_node("node2").is_some());
        assert!(imported.get_connection("node1", "out", "node2", "in").is_some());
    }

    #[wasm_bindgen_test]
    fn test_wasm_graph_groups() {
        let mut graph = Graph::_new("test_graph", true, JsValue::NULL);
        
        // Add nodes
        graph._add_node("node1", "TestComponent", JsValue::NULL);
        graph._add_node("node2", "TestComponent2", JsValue::NULL);
        
        // Create group
        let nodes = vec!["node1".to_string(), "node2".to_string()];
        graph._add_group("group1", nodes, JsValue::NULL);
        
        // Verify group
        let group = graph.groups.iter().find(|g| g.id == "group1").unwrap();
        assert_eq!(group.nodes.len(), 2);
        assert!(group.nodes.contains(&"node1".to_string()));
        assert!(group.nodes.contains(&"node2".to_string()));
    }

    #[wasm_bindgen_test]
    fn test_wasm_calculate_layout() {
        let mut graph = Graph::_new("test_graph", true, JsValue::NULL);
        
        // Add some nodes with metadata
        let node1_metadata = js_sys::Object::new();
        let dimensions1 = js_sys::Object::new();
        js_sys::Reflect::set(&dimensions1, &"width".into(), &100.0.into()).unwrap();
        js_sys::Reflect::set(&dimensions1, &"height".into(), &50.0.into()).unwrap();
        js_sys::Reflect::set(&node1_metadata, &"dimensions".into(), &dimensions1).unwrap();
        
        graph._add_node("node1", "TestComponent", node1_metadata.into());
        graph._add_node("node2", "TestComponent", JsValue::NULL);
        graph._add_connection("node1", "out", "node2", "in", JsValue::NULL);
        
        // Calculate layout
        let layout = graph._calculate_layout();
        
        // Verify the result
        assert!(js_sys::Reflect::has(&layout, &"node1".into()).unwrap());
        assert!(js_sys::Reflect::has(&layout, &"node2".into()).unwrap());
        
        // Check node1 position
        let node1_pos = js_sys::Reflect::get(&layout, &"node1".into()).unwrap();
        assert!(js_sys::Reflect::has(&node1_pos, &"x".into()).unwrap());
        assert!(js_sys::Reflect::has(&node1_pos, &"y".into()).unwrap());
        
        // Check node2 position
        let node2_pos = js_sys::Reflect::get(&layout, &"node2".into()).unwrap();
        assert!(js_sys::Reflect::has(&node2_pos, &"x".into()).unwrap());
        assert!(js_sys::Reflect::has(&node2_pos, &"y".into()).unwrap());
    }
}