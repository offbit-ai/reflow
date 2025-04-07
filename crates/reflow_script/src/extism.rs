use std::{collections::HashMap, sync::Arc};
use anyhow::Result;
use parking_lot::RwLock;
use ::extism::*;
use super::{ScriptConfig, ScriptEngine, Message};

pub struct ExtismEngine {
    pub (crate) plugin: Arc<RwLock<Option<Plugin>>>,
}

#[async_trait::async_trait]
impl ScriptEngine for ExtismEngine {
    async fn init(&mut self, config: &ScriptConfig) -> Result<()> {
        // Initialize Extism plugin
        todo!()
    }

    async fn call(&mut self, context: &crate::context::ScriptContext) -> Result<HashMap<String, Message>> {
        // Execute plugin function with context
        // This will allow access to inputs, state, and output ports
        todo!()
    }

    async fn cleanup(&mut self) -> Result<()> {
        // Cleanup plugin
        todo!()
    }
}