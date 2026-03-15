//! Zeal to Graph Converter
//!
//! Converts Zeal workflow format to Reflow's Graph format using the Graph API.

use anyhow::{Result, anyhow};
use reflow_graph::{Graph, types::{GraphExport, PortType}};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Zeal workflow format structures
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealWorkflow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub graphs: Vec<ZealGraph>,
    pub metadata: ZealWorkflowMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealGraph {
    pub id: String,
    pub name: String,
    pub namespace: String,
    pub is_main: bool,
    pub nodes: Vec<ZealNode>,
    pub connections: Vec<ZealConnection>,
    pub groups: Vec<ZealNodeGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealNode {
    pub id: String,
    pub template_id: Option<String>,
    pub node_type: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: String,
    pub variant: String,
    pub shape: String,
    pub size: Option<String>,
    pub ports: Vec<ZealPort>,
    pub properties: HashMap<String, Value>,
    pub property_values: Option<HashMap<String, Value>>,
    pub required_env_vars: Option<Vec<String>>,
    pub position: ZealPosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealPort {
    pub id: String,
    pub label: String,
    pub port_type: String, // "input" | "output"
    pub data_type: Option<String>,
    pub required: bool,
    pub multiple: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealConnection {
    pub id: String,
    pub from: ZealConnectionEndpoint,
    pub to: ZealConnectionEndpoint,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealConnectionEndpoint {
    pub node_id: String,
    pub port_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealNodeGroup {
    pub id: String,
    pub name: String,
    pub nodes: Vec<String>,
    pub metadata: Option<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZealWorkflowMetadata {
    pub version: String,
    pub author: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub tags: Option<Vec<String>>,
}

/// Converts Zeal workflow to Graph using the Graph API
pub fn convert_zeal_to_graph(zeal_workflow: &ZealWorkflow) -> Result<Graph> {
    // Find the main graph
    let main_graph = zeal_workflow
        .graphs
        .iter()
        .find(|g| g.is_main)
        .or_else(|| zeal_workflow.graphs.first())
        .ok_or_else(|| anyhow!("No graphs found in Zeal workflow"))?;

    // Build properties from metadata
    let mut properties = HashMap::new();
    properties.insert("workflow_id".to_string(), json!(zeal_workflow.id));
    properties.insert("workflow_name".to_string(), json!(zeal_workflow.name));
    properties.insert("description".to_string(), json!(zeal_workflow.description));
    if let Some(author) = &zeal_workflow.metadata.author {
        properties.insert("author".to_string(), json!(author));
    }

    // Create a new Graph using the fluent API
    let mut graph = Graph::new(&main_graph.name, false, Some(properties));

    // Identify input nodes that should become IIPs instead of graph nodes.
    // These are Zeal's user-input widgets — they produce static values, not
    // actor computations. Their output connections become initial packets.
    let input_node_templates: std::collections::HashSet<&str> = [
        "tpl_text_input",
        "tpl_number_input",
        "tpl_range_input",
    ]
    .into_iter()
    .collect();

    // Graph I/O and proxy nodes are resolved at conversion time.
    // - graph_input/graph_output: skip as graph nodes (SubgraphActor handles
    //   boundary mapping via GraphExport.inports/outports)
    // - proxy_input/proxy_output: connections remapped to internal node ports
    let graph_io_templates: std::collections::HashSet<&str> =
        ["graph_input", "graph_output"].into_iter().collect();
    let proxy_templates: std::collections::HashSet<&str> =
        ["proxy_input", "proxy_output"].into_iter().collect();

    // Map proxy node ID → (target_node, target_port) for connection rewriting
    let mut proxy_rewrites: HashMap<String, (String, String)> = HashMap::new();

    let mut input_node_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    // First pass: collect input node IDs and proxy rewrites
    for zeal_node in &main_graph.nodes {
        let tpl = zeal_node
            .template_id
            .as_deref()
            .unwrap_or(&zeal_node.node_type);
        if input_node_templates.contains(tpl) {
            input_node_ids.insert(zeal_node.id.clone());
        }

        // Graph I/O nodes: skip as graph nodes — they define subgraph boundaries
        // which SubgraphActor handles via GraphExport.inports/outports
        if graph_io_templates.contains(tpl) {
            input_node_ids.insert(zeal_node.id.clone());
        }

        // Proxy nodes: resolve to target node/port for connection rewriting
        if proxy_templates.contains(tpl) {
            input_node_ids.insert(zeal_node.id.clone()); // skip as graph node

            let pv = zeal_node.property_values.as_ref();
            if tpl == "proxy_input" {
                // proxy_input → target node's input port
                let target_node = pv
                    .and_then(|p| p.get("targetNodeId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let target_port = pv
                    .and_then(|p| p.get("targetPortId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("data")
                    .to_string();
                proxy_rewrites.insert(zeal_node.id.clone(), (target_node, target_port));
            } else {
                // proxy_output → source node's output port
                let source_node = pv
                    .and_then(|p| p.get("sourceNodeId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let source_port = pv
                    .and_then(|p| p.get("sourcePortId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("data")
                    .to_string();
                proxy_rewrites.insert(zeal_node.id.clone(), (source_node, source_port));
            }
        }
    }

    // Add nodes using the Graph API (skip input nodes — they become IIPs)
    for zeal_node in &main_graph.nodes {
        if input_node_ids.contains(&zeal_node.id) {
            continue;
        }
        let mut node_metadata = HashMap::new();

        // Nest Zeal visual/layout properties to avoid collisions with actor config
        node_metadata.insert("zeal".to_string(), json!({
            "x": zeal_node.position.x,
            "y": zeal_node.position.y,
            "title": zeal_node.title,
            "subtitle": zeal_node.subtitle,
            "icon": zeal_node.icon,
            "variant": zeal_node.variant,
            "shape": zeal_node.shape,
            "size": zeal_node.size,
            "ports": zeal_node.ports,
            "required_env_vars": zeal_node.required_env_vars,
        }));

        // Merge property defaults + user overrides into flat config.
        // properties: Record<string, { defaultValue, type, ... }> → extract defaults
        // property_values: Record<string, any> → user overrides (take precedence)
        for (key, prop_def) in &zeal_node.properties {
            if let Some(default) = prop_def.get("defaultValue") {
                node_metadata.insert(key.clone(), default.clone());
            }
        }
        if let Some(property_values) = &zeal_node.property_values {
            for (key, value) in property_values {
                node_metadata.insert(key.clone(), value.clone());
            }
        }

        // The component is either the template_id or the node_type
        let component = zeal_node
            .template_id
            .as_ref()
            .unwrap_or(&zeal_node.node_type)
            .clone();

        // Add node to graph
        graph.add_node(&zeal_node.id, &component, Some(node_metadata));

        // Register ports on the node
        for port in &zeal_node.ports {
            let port_type = match port.data_type.as_deref() {
                Some("string") => PortType::String,
                Some("number" | "float") => PortType::Float,
                Some("integer") => PortType::Integer,
                Some("boolean") => PortType::Boolean,
                Some("object") => PortType::Object(String::new()),
                Some("bytes") => PortType::Bytes,
                Some("stream") => PortType::Stream,
                Some("array") => PortType::Array(Box::new(PortType::Any)),
                _ => PortType::Any,
            };

            match port.port_type.as_str() {
                "input" => {
                    graph.add_inport(&port.id, &zeal_node.id, &port.id, port_type, None);
                }
                "output" => {
                    graph.add_outport(&port.id, &zeal_node.id, &port.id, port_type, None);
                }
                _ => {}
            }
        }
    }

    // Add connections using the Graph API.
    // Connections FROM input nodes become IIPs on the target port.
    for zeal_conn in &main_graph.connections {
        if input_node_ids.contains(&zeal_conn.from.node_id) {
            // Find the input node to extract its value
            if let Some(input_node) = main_graph.nodes.iter().find(|n| n.id == zeal_conn.from.node_id) {
                let value = extract_input_node_value(input_node);
                graph.add_initial(
                    value,
                    &zeal_conn.to.node_id,
                    &zeal_conn.to.port_id,
                    None,
                );
            }
            continue;
        }

        // Skip connections TO input/proxy nodes
        if input_node_ids.contains(&zeal_conn.to.node_id) {
            // If target is a proxy_input, rewrite to the proxy's target
            if let Some((target_node, target_port)) =
                proxy_rewrites.get(&zeal_conn.to.node_id)
            {
                if !target_node.is_empty() {
                    graph.add_connection(
                        &zeal_conn.from.node_id,
                        &zeal_conn.from.port_id,
                        target_node,
                        target_port,
                        zeal_conn.metadata.clone(),
                    );
                }
            }
            continue;
        }

        // Rewrite source if it's a proxy_output
        let (from_node, from_port) =
            if let Some((source_node, source_port)) =
                proxy_rewrites.get(&zeal_conn.from.node_id)
            {
                if source_node.is_empty() {
                    continue;
                }
                (source_node.as_str(), source_port.as_str())
            } else {
                (
                    zeal_conn.from.node_id.as_str(),
                    zeal_conn.from.port_id.as_str(),
                )
            };

        graph.add_connection(
            from_node,
            from_port,
            &zeal_conn.to.node_id,
            &zeal_conn.to.port_id,
            zeal_conn.metadata.clone(),
        );
    }

    // Add initial packets for nodes without incoming connections that have property values.
    // Skip input nodes (already converted to IIPs above).
    for zeal_node in &main_graph.nodes {
        if input_node_ids.contains(&zeal_node.id) {
            continue;
        }

        let has_incoming = main_graph
            .connections
            .iter()
            .any(|conn| conn.to.node_id == zeal_node.id && !input_node_ids.contains(&conn.from.node_id));

        if !has_incoming {
            if let Some(property_values) = &zeal_node.property_values
                && !property_values.is_empty()
            {
                if let Some(input_port) = zeal_node.ports.iter().find(|p| p.port_type == "input") {
                    graph.add_initial(json!(property_values), &zeal_node.id, &input_port.id, None);
                }
            }
        }
    }

    // Add groups using the Graph API
    for zeal_group in &main_graph.groups {
        graph.add_group(
            &zeal_group.id,
            zeal_group.nodes.clone(),
            zeal_group.metadata.clone(),
        );
    }

    Ok(graph)
}

/// Convert Graph to GraphExport for server usage
pub fn convert_zeal_to_graph_export(zeal_workflow: &ZealWorkflow) -> Result<GraphExport> {
    let graph = convert_zeal_to_graph(zeal_workflow)?;
    Ok(graph.export())
}

/// Extract the user-provided value from a Zeal input node.
///
/// Input nodes store their value in `property_values.defaultValue` (or
/// `property_values.value` for range inputs). The template type determines
/// the Message type:
/// - `tpl_text_input` → string
/// - `tpl_number_input` → number
/// - `tpl_range_input` → number (clamped to min/max)
fn extract_input_node_value(node: &ZealNode) -> Value {
    let tpl = node
        .template_id
        .as_deref()
        .unwrap_or(&node.node_type);

    let pv = node.property_values.as_ref();

    match tpl {
        "tpl_text_input" => {
            pv.and_then(|pv| pv.get("defaultValue"))
                .cloned()
                .unwrap_or(json!(""))
        }
        "tpl_number_input" => {
            let val = pv
                .and_then(|pv| pv.get("defaultValue"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            json!(val)
        }
        "tpl_range_input" => {
            let val = pv
                .and_then(|pv| pv.get("value").or(pv.get("defaultValue")))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let min = node.properties.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let max = node.properties.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0);
            json!(val.clamp(min, max))
        }
        _ => {
            // Generic fallback: use property_values as-is
            pv.map(|pv| json!(pv)).unwrap_or(json!(null))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_simple_workflow() {
        let zeal_workflow = ZealWorkflow {
            id: "test-workflow".to_string(),
            name: "Test Workflow".to_string(),
            description: "A test workflow".to_string(),
            graphs: vec![ZealGraph {
                id: "main".to_string(),
                name: "Main Graph".to_string(),
                namespace: "test".to_string(),
                is_main: true,
                nodes: vec![ZealNode {
                    id: "node1".to_string(),
                    template_id: Some("tpl_http_request".to_string()),
                    node_type: "http".to_string(),
                    title: "HTTP Request".to_string(),
                    subtitle: None,
                    icon: "http".to_string(),
                    variant: "default".to_string(),
                    shape: "rectangle".to_string(),
                    size: None,
                    ports: vec![
                        ZealPort {
                            id: "trigger".to_string(),
                            label: "Trigger".to_string(),
                            port_type: "input".to_string(),
                            data_type: None,
                            required: false,
                            multiple: false,
                        },
                        ZealPort {
                            id: "response".to_string(),
                            label: "Response".to_string(),
                            port_type: "output".to_string(),
                            data_type: None,
                            required: false,
                            multiple: false,
                        },
                    ],
                    properties: HashMap::new(),
                    property_values: Some({
                        let mut pv = HashMap::new();
                        pv.insert("url".to_string(), json!("https://api.example.com"));
                        pv.insert("method".to_string(), json!("GET"));
                        pv
                    }),
                    required_env_vars: None,
                    position: ZealPosition { x: 100.0, y: 100.0 },
                }],
                connections: vec![],
                groups: vec![],
            }],
            metadata: ZealWorkflowMetadata {
                version: "1.0.0".to_string(),
                author: Some("Test Author".to_string()),
                created_at: None,
                updated_at: None,
                tags: None,
            },
        };

        let result = convert_zeal_to_graph_export(&zeal_workflow);
        assert!(result.is_ok());

        let graph_export = result.unwrap();
        assert_eq!(graph_export.processes.len(), 1);
        assert!(graph_export.processes.contains_key("node1"));

        let node = &graph_export.processes["node1"];
        assert_eq!(node.component, "tpl_http_request");
    }
}
