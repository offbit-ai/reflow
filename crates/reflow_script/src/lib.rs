use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use reflow_network::{
    actor::{Actor, ActorBehavior, ActorPayload, ActorState, MemoryState, Port},
    message::Message,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptConfig {
    pub environment: ScriptEnvironment,
    pub runtime: ScriptRuntime,
    pub source: String,
    pub packages: Option<Vec<String>>,
    pub entry_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScriptRuntime {
    #[cfg(feature = "deno")]
    JavaScript,

    #[cfg(feature = "python")]
    Python,
    #[cfg(feature = "extism")]
    Extism,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ScriptEnvironment {
    REMOTE,
    #[default]
    SYSTEM,
    BROWSER,
}

/// Base trait for script runtimes
#[async_trait::async_trait]
pub trait ScriptEngine: Send + Sync {
    async fn init(&mut self, config: &ScriptConfig) -> Result<()>;
    async fn call(
        &mut self,
        context: &crate::context::ScriptContext,
    ) -> Result<HashMap<String, Message>>;
    async fn cleanup(&mut self) -> Result<()>;
}

#[cfg(feature = "deno")]
/// JavaScript runtime implementation
pub mod javascript;

#[cfg(feature = "python")]
/// Python runtime implementation using PyO3
pub mod python;

#[cfg(feature = "extism")]
/// Extism plugin runtime implementation
pub mod extism;

/// Context for script execution
pub mod context;

/// Script actor that wraps a script engine
pub struct ScriptActor {
    config: ScriptConfig,
    engine: Arc<Mutex<dyn ScriptEngine>>,
}

impl ScriptActor {
    pub fn new(config: ScriptConfig) -> Self {
        let engine: Arc<Mutex<dyn ScriptEngine>> = match config.runtime {
            #[cfg(feature = "deno")]
            ScriptRuntime::JavaScript => Arc::new(Mutex::new(javascript::JavaScriptEngine::new())),

            #[cfg(feature = "python")]
            ScriptRuntime::Python => {
                let use_shared_env =
                    std::env::var("USE_SHARED_ENV").unwrap_or("false".to_string()) == "true";
                Arc::new(Mutex::new(python::PythonEngine::new(
                    matches!(config.environment, ScriptEnvironment::REMOTE),
                    use_shared_env,
                )))
            }
            #[cfg(feature = "extism")]
            ScriptRuntime::Extism => Arc::new(Mutex::new(extism::ExtismEngine {
                plugin: Arc::new(RwLock::new(None)),
            })),
        };

        Self { config, engine }
    }
}

impl Actor for ScriptActor {
    fn get_behavior(&self) -> ActorBehavior {
        let engine = self.engine.clone();
        let entry_point = self.config.entry_point.clone();

        Box::new(
            move |payload: ActorPayload, state: Arc<Mutex<dyn ActorState>>, outports: Port| {
                let engine = engine.clone();
                let entry_point = entry_point.clone();
                let payload = payload.clone();

                // Create the context
                let context =
                    context::ScriptContext::new(entry_point, payload, state, outports.clone());

                // Return a future that owns all its data
                Box::pin(async move {
                    // Spawn a new task to handle the engine call
                    let result = tokio::task::spawn_blocking(move || {
                        // This runs in a separate thread where blocking is fine
                        let mut engine_guard = engine.lock();

                        // We need to block on the future since we're in a blocking context
                        futures::executor::block_on(engine_guard.call(&context))
                    })
                    .await??; // Double ? to handle both JoinError and the Result from call

                    Ok(result)
                })
            },
        )
    }

    fn get_outports(&self) -> Port {
        let (sender, receiver) = flume::unbounded();
        (sender, receiver)
    }

    fn get_inports(&self) -> Port {
        let (sender, receiver) = flume::unbounded();
        (sender, receiver)
    }

    fn create_process(
        &self,
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = self.get_outports();
        Box::pin(async move {
            let (_, receiver) = inports;
            while let Ok(payload) = receiver.recv_async().await {
                let result = behavior(payload, state.clone(), outports.clone()).await;

                if result.is_err() {
                    outports
                        .0
                        .send_async(HashMap::from_iter([(
                            "error".to_string(),
                            Message::Error(result.err().unwrap().to_string()),
                        )]))
                        .await
                        .unwrap();
                    return;
                }
                outports.0.send_async(result.unwrap()).await.unwrap();
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "deno")]
    #[tokio::test]
    async fn test_javascript_actor() {
        // Initialize the JavaScript engine first
        let mut engine = javascript::JavaScriptEngine::new();

        // Create a JavaScript script config
        let config = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::JavaScript,
            source: r#"function process(inputs, context) { return inputs.packet.data; }"#
                .to_string(),
            entry_point: "process".to_string(),
            packages: None,
        };

        // Initialize the engine with the config
        let _ = engine
            .init(&config)
            .await
            .expect("Failed to initialize engine");

        // Create the script actor with the initialized engine
        let actor = ScriptActor {
            config: config.clone(),
            engine: Arc::new(Mutex::new(engine)),
        };

        // Get behavior function
        let behavior = actor.get_behavior();

        // Create state and ports
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = actor.get_outports();

        // Create a test payload with the correct port name
        let mut payload = HashMap::new();
        payload.insert("packet".to_string(), Message::String("test".to_string()));

        // Call the behavior function
        let result = behavior(payload, state, outports.clone()).await;

        // Verify the result
        assert!(result.is_ok());
        if let Ok(output) = result {
            assert!(
                output.contains_key("out"),
                "Output does not contain 'process' key: {:?}",
                output
            );
            assert_eq!(
                output.get("out"),
                Some(&Message::String("test".to_string()))
            );
        }
    }

    #[cfg(feature = "python")]
    #[tokio::test]
    async fn test_python_actor() -> Result<()>  {
        use std::vec;

        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .finish();
        tracing::subscriber::set_global_default(subscriber)?;

        // Initialize the python engine first
        let mut engine = python::PythonEngine::new(false, true);
        // Create a python script config
        let config = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Python,
            source: r#"
import numpy as np
inputs=Context.get_inputs()
__return_value=np.array(inputs.get("packet").data).sum()
"#
            .to_string(),
            entry_point: uuid::Uuid::new_v4().to_string(),
            packages: Some(vec!["numpy".to_string()]),
        };

        // Initialize the engine with the config
        let _ = engine
            .init(&config)
            .await
            .expect("Failed to initialize engine");
        // Create the script actor with the initialized engine
        let actor = ScriptActor {
            config: config.clone(),
            engine: Arc::new(Mutex::new(engine)),
        };
     
        // Get behavior function
        let behavior = actor.get_behavior();

        // Create state and ports
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = actor.get_outports();
        // Create a test payload with the correct port name
        let mut payload = HashMap::new();
        use serde_json::json;
        use tracing::Level;
        use tracing_subscriber::FmtSubscriber;
        payload.insert(
            "packet".to_string(),
            Message::Array(vec![json!(1).into(), json!(2).into(), json!(3).into()]),
        );
        // Call the behavior function
        let result = behavior(payload, state, outports.clone()).await;
        // Verify the result
        assert!(result.is_ok());
        if let Ok(output) = result {
            assert!(
                output.contains_key("out"),
                "Output does not contain 'process' key: {:?}",
                output
            );
            assert_eq!(output.get("out"), Some(&Message::Integer(6)));
        }

        Ok(())
    }

    #[cfg(feature = "extism")]
    #[test]
    fn test_extism_actor() {
        // Test Extism plugin actor - currently unimplemented
        // This test is a placeholder for when Extism support is fully implemented
        // The implementation would follow a similar pattern to the JavaScript test
    }
}
