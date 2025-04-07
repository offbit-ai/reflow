use std::{collections::HashMap, sync::Arc};
use anyhow::Result;
use parking_lot::Mutex;
use reflow_network::{actor::{ActorState, Port}, message::Message};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// ScriptContext provides a unified interface for script engines to interact with
/// the actor system. It encapsulates input/output ports and state management.
#[derive(Clone)]
pub struct ScriptContext {
    /// The method/port name being called
    pub method: String,
    /// Input data from ports
    pub inputs: HashMap<String, Message>,
    /// State storage for the script
    pub state: Arc<Mutex<dyn ActorState>>,
    /// Output port for sending results
    pub outports: Port,
}

/// Serializable representation of the context for passing to script engines
#[derive(Serialize, Deserialize, Debug)]
pub struct SerializableContext {
    /// The method/port name being called
    pub method: String,
    /// Input data from ports
    pub inputs: HashMap<String, Value>,
    /// Current state data
    pub state: HashMap<String, Value>,
}

impl ScriptContext {
    /// Create a new script context
    pub fn new(method: String, inputs: HashMap<String, Message>, state: Arc<Mutex<dyn ActorState>>, outports: Port) -> Self {
        Self {
            method,
            inputs,
            state,
            outports,
        }
    }
    
    /// Get a specific input by name
    pub fn get_input(&self, name: &str) -> Option<&Message> {
        self.inputs.get(name)
    }
    
    /// Send a message to an output port
    pub fn send_output(&self, port: &str, message: Message) -> Result<()> {
        let mut results = HashMap::new();
        results.insert(port.to_string(), message);
        self.outports.0.send(results)?;
        Ok(())
    }
    
    /// Get state value by key
    pub fn get_state(&self, key: &str) -> Option<Value> {
        if let Some(memory_state) = self.state.lock().as_any().downcast_ref::<reflow_network::actor::MemoryState>() {
            memory_state.get(key).cloned()
        } else {
            None
        }
    }
    
    /// Set state value by key
    pub fn set_state(&self, key: &str, value: Value) -> Result<()> {
        if let Some(memory_state) = self.state.lock().as_mut_any().downcast_mut::<reflow_network::actor::MemoryState>() {
            memory_state.insert(key, value);
            Ok(())
        } else {
            Err(anyhow::anyhow!("Unable to access state"))
        }
    }
    
    /// Convert to a serializable representation for passing to scripts
    pub fn to_serializable(&self) -> Result<SerializableContext> {
        let mut state_map = HashMap::new();
        
        if let Some(memory_state) = self.state.lock().as_any().downcast_ref::<reflow_network::actor::MemoryState>() {
            for (key, value) in &memory_state.0 {
                state_map.insert(key.clone(), value.clone());
            }
        }
        
        let mut inputs_map = HashMap::new();
        for (key, message) in &self.inputs {
            inputs_map.insert(key.clone(), serde_json::to_value(message)?);
        }
        
        Ok(SerializableContext {
            method: self.method.clone(),
            inputs: inputs_map,
            state: state_map,
        })
    }
}