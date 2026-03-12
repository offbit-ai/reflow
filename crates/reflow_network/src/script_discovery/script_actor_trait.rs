use super::types::{PortDefinition, ScriptActorMetadata};
use anyhow::Result;
use async_trait::async_trait;
use reflow_actor::message::Message;
use serde_json::Value;
use std::collections::HashMap;

/// Trait for simple script-based actors that need bridging to the full Actor trait
/// This is a simpler interface for scripts that:
/// - Process messages synchronously (return outputs)
/// - Don't need direct access to outports for async sending
/// - Are typically embedded or simple scripts
///
/// For full async capabilities, implement Actor directly (like WebSocketScriptActor does)
#[async_trait]
pub trait ScriptActor: Send + Sync + 'static {
    /// Process a message and return outputs
    /// Simple synchronous processing - input to output
    async fn process(
        &mut self,
        inputs: HashMap<String, Message>,
    ) -> Result<HashMap<String, Message>>;

    /// Get input port definitions
    fn get_inports(&self) -> Vec<PortDefinition>;

    /// Get output port definitions
    fn get_outports(&self) -> Vec<PortDefinition>;

    /// Get actor metadata
    fn get_metadata(&self) -> ScriptActorMetadata;

    /// Initialize the actor (called once before first message)
    async fn initialize(&mut self) -> Result<()> {
        Ok(())
    }

    /// Cleanup the actor (called when shutting down)
    async fn cleanup(&mut self) -> Result<()> {
        Ok(())
    }

    /// Get current state (for debugging/inspection)
    async fn get_state(&self) -> Result<Value> {
        Ok(Value::Null)
    }

    /// Set state (for restoration)
    async fn set_state(&mut self, _state: Value) -> Result<()> {
        Ok(())
    }
}
