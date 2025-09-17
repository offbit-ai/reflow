//! Zeal to Graph Converter
//!
//! Converts Zeal workflow format to Reflow's Graph format using the Graph API.

use reflow_graph::{Graph, types::GraphExport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use anyhow::{Result, anyhow};

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
    let main_graph = zeal_workflow.graphs
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
    
    // Add nodes using the Graph API
    for zeal_node in &main_graph.nodes {
        let mut node_metadata = HashMap::new();
        
        // Add position
        node_metadata.insert("x".to_string(), json!(zeal_node.position.x));
        node_metadata.insert("y".to_string(), json!(zeal_node.position.y));
        
        // Add visual properties
        node_metadata.insert("title".to_string(), json!(zeal_node.title));
        if let Some(subtitle) = &zeal_node.subtitle {
            node_metadata.insert("subtitle".to_string(), json!(subtitle));
        }
        node_metadata.insert("icon".to_string(), json!(zeal_node.icon));
        node_metadata.insert("variant".to_string(), json!(zeal_node.variant));
        node_metadata.insert("shape".to_string(), json!(zeal_node.shape));
        if let Some(size) = &zeal_node.size {
            node_metadata.insert("size".to_string(), json!(size));
        }
        
        // Add properties (template configuration)
        node_metadata.insert("properties".to_string(), json!(zeal_node.properties));
        
        // Add property values (user-provided values)
        if let Some(property_values) = &zeal_node.property_values {
            node_metadata.insert("propertyValues".to_string(), json!(property_values));
        }
        
        // Add port definitions
        node_metadata.insert("ports".to_string(), json!(zeal_node.ports));
        
        // Add environment variables if any
        if let Some(env_vars) = &zeal_node.required_env_vars {
            node_metadata.insert("required_env_vars".to_string(), json!(env_vars));
        }
        
        // The component is either the template_id or the node_type
        let component = zeal_node.template_id
            .as_ref()
            .unwrap_or(&zeal_node.node_type)
            .clone();
        
        // Add node to graph
        graph.add_node(&zeal_node.id, &component, Some(node_metadata));
    }
    
    // Add connections using the Graph API
    for zeal_conn in &main_graph.connections {
        graph.add_connection(
            &zeal_conn.from.node_id,
            &zeal_conn.from.port_id, 
            &zeal_conn.to.node_id,
            &zeal_conn.to.port_id,
            zeal_conn.metadata.clone()
        );
    }
    
    // Add initial packets for nodes without incoming connections that have property values
    for zeal_node in &main_graph.nodes {
        // Check if this node has any incoming connections
        let has_incoming = main_graph.connections.iter().any(|conn| 
            conn.to.node_id == zeal_node.id
        );
        
        if !has_incoming {
            // Create initial packet if the node has property values
            if let Some(property_values) = &zeal_node.property_values {
                if !property_values.is_empty() {
                    // Find an input port
                    if let Some(input_port) = zeal_node.ports.iter().find(|p| p.port_type == "input") {
                        graph.add_initial(
                            json!(property_values),
                            &zeal_node.id,
                            &input_port.id,
                            None
                        );
                    }
                }
            }
        }
    }
    
    // Add groups using the Graph API
    for zeal_group in &main_graph.groups {
        graph.add_group(&zeal_group.id, zeal_group.nodes.clone(), zeal_group.metadata.clone());
    }
    
    Ok(graph)
}

/// Convert Graph to GraphExport for server usage
pub fn convert_zeal_to_graph_export(zeal_workflow: &ZealWorkflow) -> Result<GraphExport> {
    let graph = convert_zeal_to_graph(zeal_workflow)?;
    Ok(graph.export())
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
            graphs: vec![
                ZealGraph {
                    id: "main".to_string(),
                    name: "Main Graph".to_string(),
                    namespace: "test".to_string(),
                    is_main: true,
                    nodes: vec![
                        ZealNode {
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
                        }
                    ],
                    connections: vec![],
                    groups: vec![],
                }
            ],
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