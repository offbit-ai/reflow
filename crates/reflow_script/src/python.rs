use crate::ScriptRuntime;

use super::{Message, ScriptConfig, ScriptEngine};
use anyhow::Result;
use dashmap::DashMap;
use once_cell::sync::OnceCell;
use parking_lot::RwLock;
use tracing::info;
use std::{collections::HashMap, str::FromStr, sync::Arc};

#[derive(Clone)]
pub struct PythonEngine {
    pub(crate) sources: DashMap<uuid::Uuid, String>,
    pub(crate) use_shared_env: bool,
    pub(crate) use_remote: bool,
    pub(crate) messenger: DashMap<
        uuid::Uuid,
        (
            flume::Sender<serde_json::Value>,
            flume::Receiver<serde_json::Value>,
        ),
    >,
    pub(crate) runtimes: DashMap<uuid::Uuid, reflow_py::PythonRuntime>,
}

impl PythonEngine {
    pub fn new(use_remote: bool, use_shared_env: bool) -> Self {
        Self {
            use_remote,
            use_shared_env,
            messenger: DashMap::new(),
            sources: DashMap::new(),
            runtimes: DashMap::new(),
        }
    }
}

unsafe impl Send for PythonEngine {}

#[async_trait::async_trait]
impl ScriptEngine for PythonEngine {
    async fn init(&mut self, config: &ScriptConfig) -> Result<()> {
        if config.runtime != ScriptRuntime::Python {
            return Err(anyhow::anyhow!("Invalid script runtime"));
        }

        let session_id = uuid::Uuid::parse_str(&config.entry_point)?;
        let mut runtime = reflow_py::PythonRuntime::new(self.use_remote, self.use_shared_env);
        info!("Initializing python runtime");
        runtime
            .init(session_id, config.packages.clone(), None)
            .await?;
        info!("Initialized python runtime");
        self.runtimes.insert(session_id, runtime);
        self.sources.insert(session_id, config.source.clone());
        self.messenger.insert(session_id, flume::unbounded());
        Ok(())
    }
    async fn call(
        &mut self,
        context: &crate::context::ScriptContext,
    ) -> Result<HashMap<String, Message>> {
        let session_id = uuid::Uuid::parse_str(&context.method)?;
        if let Some(runtime) = self.runtimes.get(&session_id) {
            let code = self
                .sources
                .get(&session_id)
                .expect("Could not find source for python script")
                .clone();
            let inputs = context
                .inputs
                .iter()
                .map(|(k, v)| (k.clone(), v.clone().into()))
                .collect();

            let (message_tx, message_rx) = self.messenger.get_mut(&session_id).unwrap().clone();
            let result = runtime
                .execute(session_id, &code, inputs, None, Some(message_tx))
                .await?;

            let message_rx_clone = message_rx.clone();
            let outports_clone = context.outports.clone();
            // forward messages to the context
            tokio::task::spawn(async move {
                use futures::StreamExt;
                let (tx, _) = outports_clone;
                message_rx_clone
                    .stream()
                    .for_each(async |message: serde_json::Value| {
                        let mut result = HashMap::new();
                        if message.is_object() {
                            let message = message.as_object().unwrap();
                            result = message
                                .iter()
                                .map(|(k, v)| (k.clone(), Message::from(v.clone()).into()))
                                .collect();
                            let _ = tx.send_async(result).await;
                            return;
                        } else if message.is_null() {
                            result.insert("out".to_string(), Message::Flow);
                            let _ = tx.send_async(result).await;
                            return;
                        }

                        result.insert("out".to_string(), Message::from(message));
                        let _ = tx.send_async(result).await;
                    })
                    .await;
            });

            if !result.stderr.is_empty() {
                return Err(anyhow::anyhow!(
                    "Python script failed with error: {}",
                    result.stderr
                ));
            }

            let mut result_map = HashMap::new();
            if let Some(value) = result.result {
                if value.is_object() {
                    let object = value.as_object().unwrap();
                    for (k, v) in object {
                        result_map.insert(k.to_string(), Message::from(v.clone()));
                    }
                } else {
                    result_map.insert("out".to_string(), Message::from(value));
                }

                return Ok(result_map);
            }

            result_map.insert("out".to_string(), Message::Flow); // Assume it's a flow if no result is returned
            return Ok(result_map);
        }

        Err(anyhow::anyhow!("Could not run python script"))
    }
    async fn cleanup(&mut self) -> Result<()> {
        if !self.runtimes.is_empty() {
            let mut r = self.runtimes.iter_mut().next().unwrap();
            let _ = r.cleanup();
            drop(r);
        }
        self.runtimes.iter().for_each(|runtime| {
            drop(runtime);
        });

        Ok(())
    }
}
