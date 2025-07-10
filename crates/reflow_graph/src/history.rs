use flume::{Receiver, Sender};
use futures::future::Shared;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use tsify::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsValue;
#[cfg(target_arch = "wasm32")]
use web_sys::IdbDatabase;

#[cfg(target_arch = "wasm32")]
use gloo_utils::format::JsValueSerdeExt;

use crate::{types::*, Graph};
use std::sync::{Arc, Mutex};

/// Command trait defining the interface for all graph operations
pub trait Command {
    fn execute(&self, graph: &mut Graph) -> Result<(), String>;
    fn undo(&self, graph: &mut Graph) -> Result<(), String>;
}

/// Composite command for grouping multiple commands into a single transaction
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct CompositeCommand {
    commands: Vec<Box<dyn Command>>,
    description: String,
}

impl CompositeCommand {
    pub fn new(description: &str) -> Self {
        Self {
            commands: Vec::new(),
            description: description.to_string(),
        }
    }

    pub fn add_command(&mut self, command: Box<dyn Command>) {
        self.commands.push(command);
    }
}

impl Command for CompositeCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        for command in &self.commands {
            command.execute(graph)?;
        }
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        for command in self.commands.iter().rev() {
            command.undo(graph)?;
        }
        Ok(())
    }
}

/// History manager for tracking and managing undo/redo operations
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct GraphHistory {
    undo_stack: Vec<Box<dyn Command>>,
    redo_stack: Vec<Box<dyn Command>>,
    max_history: Option<usize>,
    event_receiver: Receiver<GraphEvents>,
}

impl GraphHistory {
    pub fn new(event_receiver: Receiver<GraphEvents>) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history: None,
            event_receiver,
        }
    }

    pub fn with_max_history(event_receiver: Receiver<GraphEvents>, max_history: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_history: Some(max_history),
            event_receiver,
        }
    }

    // Process any pending events from the event channel
    pub fn process_events(&mut self, graph: &mut Graph) -> Result<(), String> {
        if let Ok(event) = graph.event_channel.1.try_recv() {
            let command = self.create_command_from_event(event)?;
            self.execute(command, graph)?;
        }

        Ok(())
    }

    fn create_command_from_event(&self, event: GraphEvents) -> Result<Box<dyn Command>, String> {
        match event {
            GraphEvents::AddNode(value) => {
                let node: GraphNode = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse AddNode event: {}", e))?;
                Ok(Box::new(AddNodeCommand::new(
                    node.id,
                    node.component,
                    node.metadata,
                )))
            }
            GraphEvents::RemoveNode(value) => {
                let node: GraphNode = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse RemoveNode event: {}", e))?;
                Ok(Box::new(RemoveNodeCommand {
                    node,
                    connections: Vec::new(), // These will be handled by separate events
                    initializers: Vec::new(),
                    inports: Vec::new(),
                    outports: Vec::new(),
                    groups: Vec::new(),
                }))
            }
            GraphEvents::RenameNode(value) => {
                #[derive(Deserialize)]
                struct RenameData {
                    old: String,
                    new: String,
                }
                let data: RenameData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse RenameNode event: {}", e))?;
                Ok(Box::new(RenameNodeCommand::new(data.old, data.new)))
            }
            GraphEvents::AddConnection(value) => {
                let conn: GraphConnection = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse AddConnection event: {}", e))?;
                Ok(Box::new(AddConnectionCommand::new(
                    conn.from.node_id,
                    conn.from.port_id,
                    conn.to.node_id,
                    conn.to.port_id,
                    conn.metadata,
                )))
            }
            GraphEvents::RemoveConnection(value) => {
                let conn: GraphConnection = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse RemoveConnection event: {}", e))?;
                Ok(Box::new(RemoveConnectionCommand { connection: conn }))
            }
            GraphEvents::AddInitial(value) => {
                let iip: GraphIIP = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse AddInitial event: {}", e))?;
                Ok(Box::new(AddInitialCommand::new(
                    iip.data,
                    iip.to.node_id,
                    iip.to.port_id,
                    iip.metadata,
                )))
            }
            GraphEvents::RemoveInitial(value) => {
                let iip: GraphIIP = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse RemoveInitial event: {}", e))?;
                Ok(Box::new(RemoveInitialCommand { iip }))
            }
            GraphEvents::AddGroup(value) => {
                let group: GraphGroup = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse AddGroup event: {}", e))?;
                Ok(Box::new(AddGroupCommand::new(
                    group.id,
                    group.nodes,
                    group.metadata,
                )))
            }
            GraphEvents::RemoveGroup(value) => {
                let group: GraphGroup = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse RemoveGroup event: {}", e))?;
                Ok(Box::new(RemoveGroupCommand { group }))
            }
            GraphEvents::AddInport(value) => {
                #[derive(Deserialize)]
                struct InportData {
                    id: String,
                    port: GraphEdge,
                }
                let data: InportData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse AddInport event: {}", e))?;
                Ok(Box::new(AddInportCommand::new(
                    data.id,
                    data.port.node_id,
                    data.port.port_name,
                    data.port.port_type,
                    data.port.metadata,
                )))
            }
            GraphEvents::RemoveInport(value) => {
                #[derive(Deserialize)]
                struct InportData {
                    id: String,
                    port: GraphEdge,
                }
                let data: InportData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse RemoveInport event: {}", e))?;
                Ok(Box::new(RemoveInportCommand {
                    port_id: data.id,
                    edge: data.port,
                }))
            }
            GraphEvents::AddOutport(value) => {
                #[derive(Deserialize)]
                struct OutportData {
                    id: String,
                    port: GraphEdge,
                }
                let data: OutportData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse AddOutport event: {}", e))?;
                Ok(Box::new(AddOutportCommand::new(
                    data.id,
                    data.port.node_id,
                    data.port.port_name,
                    data.port.port_type,
                    data.port.metadata,
                )))
            }
            GraphEvents::RemoveOutport(value) => {
                #[derive(Deserialize)]
                struct OutportData {
                    id: String,
                    port: GraphEdge,
                }
                let data: OutportData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse RemoveOutport event: {}", e))?;
                Ok(Box::new(RemoveOutportCommand {
                    port_id: data.id,
                    edge: data.port,
                }))
            }
            GraphEvents::ChangeProperties(value) => {
                #[derive(Deserialize)]
                struct ChangeData {
                    new: HashMap<String, Value>,
                    before: HashMap<String, Value>,
                }
                let data: ChangeData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse ChangeProperties event: {}", e))?;
                Ok(Box::new(SetPropertiesCommand {
                    old_properties: data.before,
                    new_properties: data.new,
                }))
            }
            GraphEvents::ChangeNode(value) => {
                #[derive(Deserialize)]
                struct ChangeData {
                    node: GraphNode,
                    old_metadata: Option<HashMap<String, Value>>,
                    new_metadata: HashMap<String, Value>,
                }
                let data: ChangeData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse ChangeNode event: {}", e))?;
                Ok(Box::new(SetNodeMetadataCommand {
                    node_id: data.node.id,
                    old_metadata: data.old_metadata,
                    new_metadata: data.new_metadata,
                }))
            }
            GraphEvents::ChangeGroup(value) => {
                #[derive(Deserialize)]
                struct ChangeData {
                    group: GraphGroup,
                    old_metadata: Option<HashMap<String, Value>>,
                    new_metadata: HashMap<String, Value>,
                }
                let data: ChangeData = serde_json::from_value(value)
                    .map_err(|e| format!("Failed to parse ChangeGroup event: {}", e))?;
                Ok(Box::new(SetGroupMetadataCommand {
                    group_id: data.group.id,
                    old_metadata: data.old_metadata,
                    new_metadata: data.new_metadata,
                }))
            }
            _ => Err("Unsupported event type".to_string()),
        }
    }

    pub fn execute(&mut self, command: Box<dyn Command>, graph: &mut Graph) -> Result<(), String> {
        command.execute(graph)?;
        self.undo_stack.push(command);
        if let Some(max) = self.max_history {
            while self.undo_stack.len() > max {
                self.undo_stack.remove(0);
            }
        }
        self.redo_stack.clear();
        Ok(())
    }

    pub fn undo(&mut self, graph: &mut Graph) -> Result<(), String> {
        if let Some(command) = self.undo_stack.pop() {
            command.undo(graph)?;
            self.redo_stack.push(command);
            Ok(())
        } else {
            Err("No operations to undo".to_string())
        }
    }

    pub fn redo(&mut self, graph: &mut Graph) -> Result<(), String> {
        if let Some(command) = self.redo_stack.pop() {
            command.execute(graph)?;
            self.undo_stack.push(command);
            Ok(())
        } else {
            Err("No operations to redo".to_string())
        }
    }

    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn begin_transaction(&self) -> CompositeCommand {
        CompositeCommand::new("Transaction")
    }

    pub fn commit_transaction(
        &mut self,
        transaction: CompositeCommand,
        graph: &mut Graph,
    ) -> Result<(), String> {
        self.execute(Box::new(transaction), graph)
    }
}

// Node Operations
pub struct AddNodeCommand {
    id: String,
    component: String,
    metadata: Option<HashMap<String, Value>>,
}

impl AddNodeCommand {
    pub fn new(id: String, component: String, metadata: Option<HashMap<String, Value>>) -> Self {
        Self {
            id,
            component,
            metadata,
        }
    }
}

impl Command for AddNodeCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_node(&self.id, &self.component, self.metadata.clone());
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_node(&self.id);
        Ok(())
    }
}

pub struct RemoveNodeCommand {
    node: GraphNode,
    connections: Vec<GraphConnection>,
    initializers: Vec<GraphIIP>,
    inports: Vec<(String, GraphEdge)>,
    outports: Vec<(String, GraphEdge)>,
    groups: Vec<(String, Vec<String>)>,
}

impl RemoveNodeCommand {
    pub fn new(graph: &Graph, node_id: &str) -> Option<Self> {
        let node = graph.get_node(node_id)?.clone();

        // Store connections
        let connections: Vec<GraphConnection> = graph
            .connections
            .iter()
            .filter(|conn| conn.from.node_id == node_id || conn.to.node_id == node_id)
            .cloned()
            .collect();

        // Store initializers
        let initializers: Vec<GraphIIP> = graph
            .initializers
            .iter()
            .filter(|iip| iip.to.node_id == node_id)
            .cloned()
            .collect();

        // Store inports
        let inports: Vec<(String, GraphEdge)> = graph
            .inports
            .iter()
            .filter(|(_, edge)| edge.node_id == node_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Store outports
        let outports: Vec<(String, GraphEdge)> = graph
            .outports
            .iter()
            .filter(|(_, edge)| edge.node_id == node_id)
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Store groups
        let groups: Vec<(String, Vec<String>)> = graph
            .groups
            .iter()
            .filter(|group| group.nodes.contains(&node_id.to_string()))
            .map(|group| (group.id.clone(), group.nodes.clone()))
            .collect();

        Some(Self {
            node,
            connections,
            initializers,
            inports,
            outports,
            groups,
        })
    }
}

impl Command for RemoveNodeCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_node(&self.node.id);
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        // Restore node
        graph.add_node(
            &self.node.id,
            &self.node.component,
            self.node.metadata.clone(),
        );

        // Restore connections
        for conn in &self.connections {
            graph.add_connection(
                &conn.from.node_id,
                &conn.from.port_id,
                &conn.to.node_id,
                &conn.to.port_id,
                conn.metadata.clone(),
            );
        }

        // Restore initializers
        for iip in &self.initializers {
            graph.add_initial(
                iip.data.clone(),
                &iip.to.node_id,
                &iip.to.port_id,
                iip.metadata.clone(),
            );
        }

        // Restore inports
        for (port_id, edge) in &self.inports {
            graph.add_inport(
                port_id,
                &edge.node_id,
                &edge.port_name,
                edge.port_type.clone(),
                edge.metadata.clone(),
            );
        }

        // Restore outports
        for (port_id, edge) in &self.outports {
            graph.add_outport(
                port_id,
                &edge.node_id,
                &edge.port_name,
                edge.port_type.clone(),
                edge.metadata.clone(),
            );
        }

        // Restore groups
        for (group_id, nodes) in &self.groups {
            if let Some(group) = graph.groups.iter().find(|g| &g.id == group_id) {
                let metadata = group.metadata.clone();
                graph.add_group(group_id, nodes.clone(), metadata);
            }
        }

        Ok(())
    }
}

pub struct RenameNodeCommand {
    old_id: String,
    new_id: String,
}

impl RenameNodeCommand {
    pub fn new(old_id: String, new_id: String) -> Self {
        Self { old_id, new_id }
    }
}

impl Command for RenameNodeCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.rename_node(&self.old_id, &self.new_id);
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.rename_node(&self.new_id, &self.old_id);
        Ok(())
    }
}

// Connection Operations
pub struct AddConnectionCommand {
    from_node: String,
    from_port: String,
    to_node: String,
    to_port: String,
    metadata: Option<HashMap<String, Value>>,
}

impl AddConnectionCommand {
    pub fn new(
        from_node: String,
        from_port: String,
        to_node: String,
        to_port: String,
        metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            from_node,
            from_port,
            to_node,
            to_port,
            metadata,
        }
    }
}

impl Command for AddConnectionCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_connection(
            &self.from_node,
            &self.from_port,
            &self.to_node,
            &self.to_port,
            self.metadata.clone(),
        );
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_connection(
            &self.from_node,
            &self.from_port,
            &self.to_node,
            &self.to_port,
        );
        if let Some(indices) = graph.connection_indices.get_mut(&(self.from_node.clone(), self.to_node.clone())) {
            indices.clear();
        }
        if let Some(indices) = graph.connection_port_indices.get_mut(&(
            (self.from_node.clone(), self.from_port.clone()),
            (self.to_node.clone(), self.to_port.clone()),
        )) {
            indices.clear();
        }
        Ok(())
    }
}

pub struct RemoveConnectionCommand {
    connection: GraphConnection,
}

impl RemoveConnectionCommand {
    pub fn new(
        graph: &Graph,
        from_node: &str,
        from_port: &str,
        to_node: &str,
        to_port: &str,
    ) -> Option<Self> {
        let connection = graph.get_connection(from_node, from_port, to_node, to_port)?;
        Some(Self { connection })
    }
}

impl Command for RemoveConnectionCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_connection(
            &self.connection.from.node_id,
            &self.connection.from.port_id,
            &self.connection.to.node_id,
            &self.connection.to.port_id,
        );
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_connection(
            &self.connection.from.node_id,
            &self.connection.from.port_id,
            &self.connection.to.node_id,
            &self.connection.to.port_id,
            self.connection.metadata.clone(),
        );
        Ok(())
    }
}

// Group Operations
pub struct AddGroupCommand {
    group_id: String,
    nodes: Vec<String>,
    metadata: Option<HashMap<String, Value>>,
}

impl AddGroupCommand {
    pub fn new(
        group_id: String,
        nodes: Vec<String>,
        metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            group_id,
            nodes,
            metadata,
        }
    }
}

impl Command for AddGroupCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_group(&self.group_id, self.nodes.clone(), self.metadata.clone());
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_group(&self.group_id);
        Ok(())
    }
}

pub struct RemoveGroupCommand {
    group: GraphGroup,
}

impl RemoveGroupCommand {
    pub fn new(graph: &Graph, group_id: &str) -> Option<Self> {
        let group = graph.groups.iter().find(|g| g.id == group_id)?.clone();
        Some(Self { group })
    }
}

impl Command for RemoveGroupCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_group(&self.group.id);
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_group(
            &self.group.id,
            self.group.nodes.clone(),
            self.group.metadata.clone(),
        );
        Ok(())
    }
}

// IIP Operations
pub struct AddInitialCommand {
    data: Value,
    node: String,
    port: String,
    metadata: Option<HashMap<String, Value>>,
}

impl AddInitialCommand {
    pub fn new(
        data: Value,
        node: String,
        port: String,
        metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            data,
            node,
            port,
            metadata,
        }
    }
}

impl Command for AddInitialCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_initial(
            self.data.clone(),
            &self.node,
            &self.port,
            self.metadata.clone(),
        );
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_initial(&self.node, &self.port);
        Ok(())
    }
}

pub struct RemoveInitialCommand {
    iip: GraphIIP,
}

impl RemoveInitialCommand {
    pub fn new(graph: &Graph, node: &str, port: &str) -> Option<Self> {
        let iip = graph
            .initializers
            .iter()
            .find(|iip| iip.to.node_id == node && iip.to.port_id == port)?
            .clone();
        Some(Self { iip })
    }
}

impl Command for RemoveInitialCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_initial(&self.iip.to.node_id, &self.iip.to.port_id);
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_initial(
            self.iip.data.clone(),
            &self.iip.to.node_id,
            &self.iip.to.port_id,
            self.iip.metadata.clone(),
        );
        Ok(())
    }
}

pub struct AddInportCommand {
    port_id: String,
    node_id: String,
    port_key: String,
    port_type: PortType,
    metadata: Option<HashMap<String, Value>>,
}

impl AddInportCommand {
    pub fn new(
        port_id: String,
        node_id: String,
        port_key: String,
        port_type: PortType,
        metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            port_id,
            node_id,
            port_key,
            port_type,
            metadata,
        }
    }
}

impl Command for AddInportCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_inport(
            &self.port_id,
            &self.node_id,
            &self.port_key,
            self.port_type.clone(),
            self.metadata.clone(),
        );
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_inport(&self.port_id);
        Ok(())
    }
}

pub struct RemoveInportCommand {
    port_id: String,
    edge: GraphEdge,
}

impl RemoveInportCommand {
    pub fn new(graph: &Graph, port_id: &str) -> Option<Self> {
        let edge = graph.inports.get(port_id)?.clone();
        Some(Self {
            port_id: port_id.to_string(),
            edge,
        })
    }
}

impl Command for RemoveInportCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_inport(&self.port_id);
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_inport(
            &self.port_id,
            &self.edge.node_id,
            &self.edge.port_name,
            self.edge.port_type.clone(),
            self.edge.metadata.clone(),
        );
        Ok(())
    }
}

pub struct AddOutportCommand {
    port_id: String,
    node_id: String,
    port_key: String,
    port_type: PortType,
    metadata: Option<HashMap<String, Value>>,
}

impl AddOutportCommand {
    pub fn new(
        port_id: String,
        node_id: String,
        port_key: String,
        port_type: PortType,
        metadata: Option<HashMap<String, Value>>,
    ) -> Self {
        Self {
            port_id,
            node_id,
            port_key,
            port_type,
            metadata,
        }
    }
}

impl Command for AddOutportCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_outport(
            &self.port_id,
            &self.node_id,
            &self.port_key,
            self.port_type.clone(),
            self.metadata.clone(),
        );
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_outport(&self.port_id);
        Ok(())
    }
}

pub struct RemoveOutportCommand {
    port_id: String,
    edge: GraphEdge,
}

impl RemoveOutportCommand {
    pub fn new(graph: &Graph, port_id: &str) -> Option<Self> {
        let edge = graph.outports.get(port_id)?.clone();
        Some(Self {
            port_id: port_id.to_string(),
            edge,
        })
    }
}

impl Command for RemoveOutportCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.remove_outport(&self.port_id);
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.add_outport(
            &self.port_id,
            &self.edge.node_id,
            &self.edge.port_name,
            self.edge.port_type.clone(),
            self.edge.metadata.clone(),
        );
        Ok(())
    }
}

// Metadata Operations
pub struct SetNodeMetadataCommand {
    node_id: String,
    old_metadata: Option<HashMap<String, Value>>,
    new_metadata: HashMap<String, Value>,
}

impl SetNodeMetadataCommand {
    pub fn new(graph: &Graph, node_id: &str, new_metadata: HashMap<String, Value>) -> Option<Self> {
        let old_metadata = graph.get_node(node_id)?.metadata.clone();
        Some(Self {
            node_id: node_id.to_string(),
            old_metadata,
            new_metadata,
        })
    }
}

impl Command for SetNodeMetadataCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.set_node_metadata(&self.node_id, self.new_metadata.clone());
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        if let Some(metadata) = &self.old_metadata {
            graph.set_node_metadata(&self.node_id, metadata.clone());
        }
        Ok(())
    }
}

pub struct SetGroupMetadataCommand {
    group_id: String,
    old_metadata: Option<HashMap<String, Value>>,
    new_metadata: HashMap<String, Value>,
}

impl SetGroupMetadataCommand {
    pub fn new(
        graph: &Graph,
        group_id: &str,
        new_metadata: HashMap<String, Value>,
    ) -> Option<Self> {
        let old_metadata = graph
            .groups
            .iter()
            .find(|g| g.id == group_id)?
            .metadata
            .clone();
        Some(Self {
            group_id: group_id.to_string(),
            old_metadata,
            new_metadata,
        })
    }
}

impl Command for SetGroupMetadataCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.set_group_metadata(&self.group_id, self.new_metadata.clone());
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        if let Some(metadata) = &self.old_metadata {
            graph.set_group_metadata(&self.group_id, metadata.clone());
        }
        Ok(())
    }
}

// Add new command for properties changes
pub struct SetPropertiesCommand {
    old_properties: HashMap<String, Value>,
    new_properties: HashMap<String, Value>,
}

impl Command for SetPropertiesCommand {
    fn execute(&self, graph: &mut Graph) -> Result<(), String> {
        graph.set_properties(self.new_properties.clone());
        Ok(())
    }

    fn undo(&self, graph: &mut Graph) -> Result<(), String> {
        graph.set_properties(self.old_properties.clone());
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
#[cfg_attr(target_arch = "wasm32", derive(Tsify))]
#[cfg_attr(target_arch = "wasm32", tsify(into_wasm_abi))]
#[cfg_attr(target_arch = "wasm32", tsify(from_wasm_abi))]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub undo_size: usize,
    pub redo_size: usize,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_class = GraphHistory)]
impl GraphHistory {
    #[wasm_bindgen(constructor)]
    pub fn _new(graph: &Graph) -> Self {
        GraphHistory::new(graph.event_channel.1.clone())
    }

    #[wasm_bindgen(js_name = withMaxHistory)]
    pub fn _with_max_history(graph: &Graph, max_history: usize) -> Self {
        GraphHistory::with_max_history(graph.event_channel.1.clone(), max_history)
    }

    /// Process any pending events from the graph
    #[wasm_bindgen(js_name = processEvents)]
    pub fn _process_events(&mut self, graph: &mut Graph) -> Result<(), JsValue> {
        self.process_events(graph)
            .map_err(|e| JsValue::from_str(&e))
    }

    /// Undo the last operation
    #[wasm_bindgen(js_name = undo)]
    pub fn _undo(&mut self, graph: &mut Graph) -> Result<(), JsValue> {
        self.undo(graph).map_err(|e| JsValue::from_str(&e))
    }

    /// Redo the last undone operation
    #[wasm_bindgen(js_name = redo)]
    pub fn _redo(&mut self, graph: &mut Graph) -> Result<(), JsValue> {
        self.redo(graph).map_err(|e| JsValue::from_str(&e))
    }

    /// Clear the history
    #[wasm_bindgen(js_name = clear)]
    pub fn _clear(&mut self) {
        self.clear();
    }

    /// Get the current history state
    #[wasm_bindgen(js_name = getState)]
    pub fn _get_state(&self) -> HistoryState {
        HistoryState {
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            undo_size: self.undo_stack.len(),
            redo_size: self.redo_stack.len(),
        }
    }

    /// Begin a new transaction for grouping operations
    #[wasm_bindgen(js_name = beginTransaction)]
    pub fn _begin_transaction(&self) -> CompositeCommand {
        CompositeCommand::new("Transaction")
    }

    /// Commit a transaction to the history
    #[wasm_bindgen(js_name = commitTransaction)]
    pub fn _commit_transaction(
        &mut self,
        transaction: CompositeCommand,
        graph: &mut Graph,
    ) -> Result<(), JsValue> {
        self.commit_transaction(transaction, graph)
            .map_err(|e| JsValue::from_str(&e))
    }

    #[wasm_bindgen(js_name = createStorageManager)]
    pub fn create_storage_manager(db_name: &str, store_name: &str) -> StorageManager {
        StorageManager::new(db_name, store_name)
    }

    #[wasm_bindgen(js_name = loadFromSnapshot)]
    pub fn load_from_snapshot(
        snapshot_str: &str,
        graph: &mut Graph,
    ) -> Result<GraphHistory, JsValue> {
        let snapshot: HistorySnapshot = serde_json::from_str(snapshot_str)
            .map_err(|e| JsValue::from_str(&format!("Failed to parse snapshot: {}", e)))?;

        // Load graph state
        *graph = Graph::load(snapshot.graph_state, None);

        // Create new history with the same configuration
        let mut history = if let Some(max) = snapshot.max_history {
            GraphHistory::with_max_history(graph.event_channel.1.clone(), max)
        } else {
            GraphHistory::new(graph.event_channel.1.clone())
        };

        Ok(history)
    }
}

// // WASM wrapper for CompositeCommand
// #[cfg(target_arch = "wasm32")]
// #[wasm_bindgen(js_class = CompositeCommand)]
// pub struct CompositeCommandJS {
//     command: CompositeCommand,
// }

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
interface HistoryState {
    canUndo: boolean;
    canRedo: boolean;
    undoSize: number;
    redoSize: number;
}

interface GraphHistory {
    new(graph: Graph): GraphHistory;
    withMaxHistory(graph: Graph, maxHistory: number): GraphHistory;
    processEvents(graph: Graph): void;
    undo(graph: Graph): void;
    redo(graph: Graph): void;
    clear(): void;
    getState(): HistoryState;
    beginTransaction(): CompositeCommand;
    commitTransaction(transaction: CompositeCommand, graph: Graph): void;
}

interface CompositeCommand {
}

interface Graph {
    withHistory(): [Graph, GraphHistory];
    withHistoryAndLimit(maxHistory: number): [Graph, GraphHistory];
}
"#;

#[derive(Serialize, Deserialize)]
pub(crate) struct HistorySnapshot {
    pub(crate) graph_state: GraphExport,
    pub(crate) undo_stack_size: usize,
    pub(crate) redo_stack_size: usize,
    pub(crate) max_history: Option<usize>,
    pub(crate) version: u32,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct StorageManager {
    db_name: String,
    store_name: String,
    db: Arc<Mutex<Option<IdbDatabase>>>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl StorageManager {
    #[wasm_bindgen(constructor)]
    pub fn new(db_name: &str, store_name: &str) -> Self {
        Self {
            db_name: db_name.to_string(),
            store_name: store_name.to_string(),
            db: Arc::new(Mutex::new(None)),
        }
    }

    // Initialize IndexedDB
    #[wasm_bindgen(js_name = initDatabase)]
    pub async fn init_database(&self) -> web_sys::js_sys::Promise {
        use wasm_bindgen::JsCast;
        use wasm_bindgen_futures::future_to_promise;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::IdbDatabase;
        use web_sys::IdbOpenDbRequest;
        use web_sys::IdbRequest;
        use web_sys::IdbVersionChangeEvent;

        let db_name = self.db_name.clone();
        let store_name = self.store_name.clone();
        let db_ref = self.db.clone();

        future_to_promise(async move {
            let indexed_db = get_indexed_db()?;

            let db_request: IdbOpenDbRequest = indexed_db.open_with_u32(&db_name, 1)?;

            let upgrade_promise = web_sys::js_sys::Promise::new(&mut |resolve, reject| {
                let store_name = store_name.clone();
                let onupgradeneeded = Closure::wrap(Box::new(move |event: IdbVersionChangeEvent| {
                    if let Some(db) = event
                        .target()
                        .and_then(|target| target.dyn_into::<IdbOpenDbRequest>().ok())
                        .and_then(|request| request.result().ok())
                        .and_then(|result| result.dyn_into::<IdbDatabase>().ok())
                    {
                        if let Err(e) = db.create_object_store(&store_name) {
                            web_sys::console::error_1(
                                &format!("Error creating store: {:?}", e).into(),
                            );
                        }
                    }
                })
                    as Box<dyn FnMut(IdbVersionChangeEvent)>);

                db_request.set_onupgradeneeded(Some(onupgradeneeded.as_ref().unchecked_ref()));
                onupgradeneeded.forget();

                let _reject = reject.clone();

                let db_ref = db_ref.clone();
                let onsuccess = Closure::wrap(Box::new(move |event: web_sys::Event| {
                    let target = event.target().unwrap();
                    let request: IdbOpenDbRequest = target.dyn_into().unwrap();
                    match request.result() {
                        Ok(result) => {
                            let db: IdbDatabase = result.dyn_into().unwrap();
                            *db_ref.lock().unwrap() = Some(db);
                            web_sys::console::log_1(&JsValue::from_str(
                                "PERSISTENT DB INITIALIZED",
                            ));
                            resolve.call0(&JsValue::NULL).unwrap();
                        }
                        Err(e) => {
                            _reject.call1(&JsValue::NULL, &e).unwrap();
                        }
                    }
                }) as Box<dyn FnMut(web_sys::Event)>);

                let _reject = reject.clone();

                let onerror = Closure::wrap(Box::new(move |event: web_sys::Event| {
                    let target = event.target().unwrap();
                    let request = target.dyn_into::<IdbRequest>().unwrap();
                    _reject
                        .call1(&JsValue::NULL, &request.error().unwrap().unwrap())
                        .unwrap();
                }) as Box<dyn FnMut(web_sys::Event)>);

                db_request.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
                db_request.set_onerror(Some(onerror.as_ref().unchecked_ref()));

                onsuccess.forget();
                onerror.forget();
            });

            JsFuture::from(upgrade_promise).await
        })
    }

    // Save to IndexedDB
    #[wasm_bindgen(js_name = saveToIndexedDB)]
    pub fn save_to_indexed_db(
        &self,
        key: &str,
        graph: &Graph,
        history: &GraphHistory,
    ) -> web_sys::js_sys::Promise {
        use wasm_bindgen_futures::future_to_promise;
        use wasm_bindgen_futures::JsFuture;

        let db = self.db.lock().unwrap().clone();
        let db_name = self.db_name.clone();
        let store_name = self.store_name.clone();
        let snapshot = HistorySnapshot {
            graph_state: graph.export(),
            undo_stack_size: history.undo_stack.len(),
            redo_stack_size: history.redo_stack.len(),
            max_history: history.max_history,
            version: 1,
        };

        let snapshot_str = serde_json::to_string(&snapshot).unwrap();

        let key = key.to_owned();
        future_to_promise(async move {
            let db = db.ok_or_else(|| JsValue::from_str("Database not initialized"))?;

            let transaction = db
                .transaction_with_str_and_mode(&store_name, web_sys::IdbTransactionMode::Readwrite)
                .map_err(|_| JsValue::from_str("Failed to create transaction"))?;

            let store = transaction
                .object_store(&store_name)
                .map_err(|_| JsValue::from_str("Failed to get object store"))?;

            let request = store
                .put_with_key(&JsValue::from_str(&snapshot_str), &JsValue::from_str(&key))
                .map_err(|_| JsValue::from_str("Failed to put data"))?;

            let promise = web_sys::js_sys::Promise::new(&mut |resolve, reject| {
                let _request = request.clone();
                let on_success = Closure::wrap(Box::new(move |event: web_sys::Event| {
                    let _resolve = resolve.clone();
                    let on_put_success = Closure::wrap(Box::new(move |_| {
                        _resolve.call0(&JsValue::NULL).unwrap();
                    })
                        as Box<dyn FnMut(JsValue)>);

                    _request.set_onsuccess(Some(on_put_success.as_ref().unchecked_ref()));
                    on_put_success.forget();
                })
                    as Box<dyn FnMut(web_sys::Event)>);

                let on_error = Closure::wrap(Box::new(move |err| {
                    reject.call1(&JsValue::NULL, &err).unwrap();
                }) as Box<dyn FnMut(JsValue)>);

                request.set_onsuccess(Some(on_success.as_ref().unchecked_ref()));
                request.set_onerror(Some(on_error.as_ref().unchecked_ref()));

                on_success.forget();
                on_error.forget();
            });

            JsFuture::from(promise).await
        })
    }

    // Load from IndexedDB
    #[wasm_bindgen(js_name = loadFromIndexedDB)]
    pub fn load_from_indexed_db(&self, key: &str) -> web_sys::js_sys::Promise {
        use wasm_bindgen_futures::future_to_promise;
        use wasm_bindgen_futures::JsFuture;

        let db = self.db.lock().unwrap().clone();
        let store_name = self.store_name.clone();
        let key = key.to_string();

        future_to_promise(async move {
            let db = db.ok_or_else(|| JsValue::from_str("Database not initialized"))?;

            let transaction = db
                .transaction_with_str_and_mode(&store_name, web_sys::IdbTransactionMode::Readwrite)
                .map_err(|_| JsValue::from_str("Failed to create transaction"))?;

            let store = transaction
                .object_store(&store_name)
                .map_err(|_| JsValue::from_str("Failed to get object store"))?;

            let request = store
                .get(&JsValue::from_str(&key))
                .map_err(|_| JsValue::from_str("Failed to get data"))?;

            let load_promise = web_sys::js_sys::Promise::new(&mut |resolve, reject| {
                let _reject = reject.clone();
                let _resolve = resolve.clone();
                let onsuccess = Closure::wrap(Box::new(move |event: web_sys::Event| {
                    let target = event.target().unwrap();
                    let request: web_sys::IdbRequest = target.dyn_into().unwrap();
                    match request.result() {
                        Ok(value) => _resolve.call1(&JsValue::NULL, &value).unwrap(),
                        Err(e) => _reject.call1(&JsValue::NULL, &e).unwrap(),
                    };
                }) as Box<dyn FnMut(_)>);

                let onerror = Closure::wrap(Box::new(move |err: JsValue| {
                    reject
                        .call1(&JsValue::NULL, &JsValue::from_str("Load failed"))
                        .unwrap();
                }) as Box<dyn FnMut(_)>);

                request.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
                request.set_onerror(Some(onerror.as_ref().unchecked_ref()));

                onsuccess.forget();
                onerror.forget();
            });

            JsFuture::from(load_promise).await
        })
    }

    // Save to LocalStorage (for smaller graphs)
    #[wasm_bindgen(js_name = saveToLocalStorage)]
    pub fn save_to_local_storage(
        &self,
        key: &str,
        graph: &Graph,
        history: &GraphHistory,
    ) -> Result<(), JsValue> {
        let snapshot = HistorySnapshot {
            graph_state: graph.export(),
            undo_stack_size: history.undo_stack.len(),
            redo_stack_size: history.redo_stack.len(),
            max_history: history.max_history,
            version: 1,
        };

        let storage = web_sys::window()
            .ok_or_else(|| JsValue::from_str("No window found"))?
            .local_storage()
            .map_err(|_| JsValue::from_str("Failed to access localStorage"))?
            .ok_or_else(|| JsValue::from_str("localStorage not available"))?;

        let storage_key = format!("{}_{}_{}", self.db_name, self.store_name, key);
        let snapshot_str = serde_json::to_string(&snapshot)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

        storage
            .set_item(&storage_key, &snapshot_str)
            .map_err(|_| JsValue::from_str("Failed to save to localStorage"))?;

        Ok(())
    }

    // Load from LocalStorage
    #[wasm_bindgen(js_name = loadFromLocalStorage)]
    pub fn load_from_local_storage(&self, key: &str) -> Result<JsValue, JsValue> {
        let storage = web_sys::window()
            .ok_or_else(|| JsValue::from_str("No window found"))?
            .local_storage()
            .map_err(|_| JsValue::from_str("Failed to access localStorage"))?
            .ok_or_else(|| JsValue::from_str("localStorage not available"))?;

        let storage_key = format!("{}_{}_{}", self.db_name, self.store_name, key);

        let snapshot_str = storage
            .get_item(&storage_key)
            .map_err(|_| JsValue::from_str("Failed to load from localStorage"))?
            .ok_or_else(|| JsValue::from_str("No data found"))?;

        Ok(JsValue::from_str(&snapshot_str))
    }
}

// Helper function to get the global scope (works in both window and worker contexts)
#[cfg(target_arch = "wasm32")]
fn get_global_scope() -> Result<JsValue, JsValue> {
    // Try worker scope first
    let global = web_sys::js_sys::global();

    // Check if we're in a worker context
    if web_sys::js_sys::Reflect::has(&global, &"WorkerGlobalScope".into())? {
        Ok(global.into())
    } else {
        // Fallback to window scope
        web_sys::window()
            .ok_or_else(|| JsValue::from_str("Neither WorkerGlobalScope nor Window found"))
            .map(|w| w.into())
    }
}

// Helper function to get IndexedDB factory
#[cfg(target_arch = "wasm32")]
fn get_indexed_db() -> Result<web_sys::IdbFactory, JsValue> {
    let global = get_global_scope()?;

    // Try to get indexedDB from the global scope
    if let Ok(idb) = web_sys::js_sys::Reflect::get(&global, &"indexedDB".into()) {
        Ok(idb.unchecked_into::<web_sys::IdbFactory>())
    } else {
        Err(JsValue::from_str("IndexedDB not available in this context"))
    }
}
