use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use reflow_actor::{
    Actor, ActorBehavior, ActorConfig, ActorContext, ActorLoad, ActorPayload, ActorState, MemoryState, Port,
    message::Message,
};
use reflow_tracing_protocol::client::TracingIntegration;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptConfig {
    pub environment: ScriptEnvironment,
    pub runtime: ScriptRuntime,
    pub source: Vec<u8>,
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

/// Database connection pool (only available with database features)
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub mod db_manager;
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub mod db_pool;

/// Database actor for executing database operations (only available with database features)
#[cfg(any(feature = "sqlite", feature = "postgres"))]
pub mod db_actor;

/// Script actor that wraps a script engine
pub struct ScriptActor {
    config: ScriptConfig,
    engine: Arc<Mutex<dyn ScriptEngine>>,
    inports_channel: Port,
    outports_channel: Port,
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
            ScriptRuntime::Extism => Arc::new(Mutex::new(extism::ExtismEngine::new())),
        };

        // Initialize the engine with the config
        {
            let mut engine_guard = engine.lock();
            let config_clone = config.clone();
            // Use block_on to run the async init
            futures::executor::block_on(async {
                engine_guard.init(&config_clone).await.expect("Failed to initialize script engine");
            });
        }

        Self {
            config,
            engine,
            inports_channel: flume::unbounded(),
            outports_channel: flume::unbounded(),
        }
    }
}

impl Actor for ScriptActor {
    fn get_behavior(&self) -> ActorBehavior {
        let engine = self.engine.clone();
        let entry_point = self.config.entry_point.clone();

        Box::new(
            move |context:ActorContext| {
                let engine = engine.clone();
                let entry_point = entry_point.clone();
                let payload = context.get_payload();

                // Create the context
                let context =
                    context::ScriptContext::new(entry_point, payload.clone(), context.get_state(), context.get_outports(), context.get_config().clone());

                // Return a future that owns all its data
                Box::pin(async move {
                    let mut engine_guard = engine.lock();

                    // We need to block on the future since we're in a blocking context
                    let result = futures::executor::block_on(engine_guard.call(&context))?;

                    Ok(result)
                })
            },
        )
    }

    fn get_outports(&self) -> Port {
        self.outports_channel.clone()
    }

    fn get_inports(&self) -> Port {
        self.inports_channel.clone()
    }

    fn create_process(
        &self,
        actor_config: ActorConfig,
        tracing_integration: Option<TracingIntegration>
    ) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = self.get_outports();
        Box::pin(async move {
            while let Ok(payload) = inports.1.recv_async().await {
                let context = ActorContext::new(
                    payload,
                    outports.clone(),
                    state.clone(),
                    actor_config.clone(),
                    Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
                );
                let result = behavior(context).await;

                if result.is_err() {
                    outports
                        .0
                        .send_async(HashMap::from_iter([(
                            "error".to_string(),
                            Message::error(result.err().unwrap().to_string()),
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
                .as_bytes()
                .to_vec(),
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
            inports_channel: flume::unbounded(),
            outports_channel: flume::unbounded(),
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
    async fn test_python_actor() -> Result<()> {
        use reflow_actor::{ActorContext, ActorLoad};
        use serde_json::json;
        use std::vec;
        use tracing::Level;
        use tracing_subscriber::FmtSubscriber;

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
            .as_bytes()
            .to_vec(),
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
            inports_channel: flume::unbounded(),
            outports_channel: flume::unbounded(),
        };

        // Get behavior function
        let behavior = actor.get_behavior();

        // Create state and ports
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = actor.get_outports();
        // Create a test payload with the correct port name
        let mut payload = HashMap::new();

        payload.insert(
            "packet".to_string(),
            Message::array(vec![json!(1).into(), json!(2).into(), json!(3).into()]),
        );

        let actor_config = ActorConfig::default();

        let context = ActorContext::new(
            payload,
            outports.clone(),
            state.clone(),
            actor_config.clone(),
            Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
        );
        // Call the behavior function
        let result = behavior(context).await;
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
    #[tokio::test]
    async fn test_extism_actor() {
        use reflow_actor::types::GraphNode;
        
        // Create an extism script config using the counter_actor example
        let config = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source: include_bytes!("../../../crates/reflow_wasm/examples/counter_actor/target/wasm32-unknown-unknown/release/counter_actor.wasm").to_vec(),
            entry_point: "process".to_string(),
            packages: None,
        };
        
        // Create the script actor
        let actor = ScriptActor::new(config);
        
        // Create actor config with initial counter value
        let mut metadata = HashMap::new();
        metadata.insert("initial_value".to_string(), serde_json::json!(10));
        
        let actor_config = ActorConfig::from_node(GraphNode {
            id: "test_counter".to_string(),
            component: "CounterActor".to_string(),
            metadata: Some(metadata),
            ..Default::default()
        }).unwrap();
        
        // Start the actor process
        let process = actor.create_process(actor_config, None);
        let _handle = tokio::spawn(process);
        
        // Get ports
        let inports = actor.get_inports();
        let outports = actor.get_outports();
        
        // Test increment operation
        let mut payload = HashMap::new();
        payload.insert("increment".to_string(), Message::Flow);
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        // Verify the result
        assert!(result.contains_key("count"), "Result should contain 'count' key");
        assert_eq!(result["count"], Message::Integer(11)); // 10 + 1 = 11
        assert!(result.contains_key("changed"), "Result should contain 'changed' key");
        assert_eq!(result["changed"], Message::Boolean(true));
        
        // Test decrement operation
        let mut payload = HashMap::new();
        payload.insert("decrement".to_string(), Message::Flow);
        
        inports.0.send_async(payload).await.unwrap();
        let result = outports.1.recv_async().await.unwrap();
        
        assert_eq!(result["count"], Message::Integer(10)); // 11 - 1 = 10
        assert_eq!(result["changed"], Message::Boolean(true));
    }
}
