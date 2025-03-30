use std::{collections::HashMap, sync::Arc};
use anyhow::Result;
use parking_lot::{Mutex, RwLock};
use reflow_network::{actor::{Actor, ActorBehavior, ActorPayload, ActorState, MemoryState, Port}, message::Message};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptConfig {
    pub runtime: ScriptRuntime,
    pub source: String,
    pub entry_point: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScriptRuntime {
    JavaScript,
    Python,
    Extism,
}

/// Base trait for script runtimes
#[async_trait::async_trait]
pub trait ScriptEngine: Send + Sync {
    async fn init(&mut self, config: &ScriptConfig) -> Result<()>;
    async fn call(&self, context: &crate::context::ScriptContext) -> Result<Message>;
    async fn cleanup(&mut self) -> Result<()>;
}

/// JavaScript runtime implementation
pub mod javascript;

/// Python runtime implementation using PyO3
// pub mod python;

/// Extism plugin runtime implementation
pub mod extism;

/// Context for script execution
pub mod context;

/// Script actor that wraps a script engine
pub struct ScriptActor {
    config: ScriptConfig,
    engine: Arc<Mutex<dyn ScriptEngine>>
}

impl ScriptActor {
    pub fn new(config: ScriptConfig) -> Self {
        let engine: Arc<Mutex<dyn ScriptEngine>> = match config.runtime {
            ScriptRuntime::JavaScript => Arc::new(Mutex::new(javascript::JavaScriptEngine {
                worker: None
            })),
            ScriptRuntime::Python => unimplemented!(),
            // ScriptRuntime::Python => Arc::new(Mutex::new(python::PythonEngine {
            //     interpreter: Arc::new(RwLock::new(None)),
            // })),
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
        Box::new(move |payload: ActorPayload, state: Arc<Mutex<dyn ActorState>>, outports: Port| {
            // Create a future that processes all inputs asynchronously
            let future = async move {
                let mut results = HashMap::new();
                for (port, msg) in payload {
                    // Create a context with the port name, input message, state, and outports
                    let mut inputs = HashMap::new();
                    inputs.insert(port.clone(), msg);
                    
                    let context = context::ScriptContext::new(
                        port.clone(),
                        inputs,
                        state.clone(),
                        outports.clone()
                    );
                    
                    // Use await to call the engine asynchronously
                    if let Ok(output) = engine.lock().call(&context).await {
                        results.insert(port, output);
                    }
                }
                Ok(results)
            };
            
            // Use tokio to block on the future
            match tokio::runtime::Handle::try_current() {
                Ok(handle) => handle.block_on(future),
                Err(_) => {
                    // Create a new runtime if we're not in a tokio context
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(future)
                }
            }
        })
    }

    fn get_outports(&self) -> Port {
        let (sender, receiver) = flume::unbounded();
        (sender, receiver)
    }

    fn get_inports(&self) -> Port {
        let (sender, receiver) = flume::unbounded();
        (sender, receiver)
    }

    fn create_process(&self) -> std::pin::Pin<Box<dyn futures::Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let state:Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = self.get_outports();
        Box::pin(async move {
            let (_, receiver) = inports;
            while let Ok(payload) = receiver.recv_async().await {
               
                if let Ok(result) = behavior(payload, state.clone(), outports.clone()) {
                    let _ = outports.0.send_async(result).await;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_javascript_actor() {
        // Initialize the JavaScript engine first
        let mut engine = javascript::JavaScriptEngine { worker: None };
        
        // Create a JavaScript script config
        let config = ScriptConfig {
            runtime: ScriptRuntime::JavaScript,
            source: r#"function process(args, context) { return args[0]; }"#.to_string(),
            entry_point: "process".to_string(),
        };
        
        // Initialize the engine with the config asynchronously
        engine.init(&config).await.expect("Failed to initialize engine");
        
        // Create the script actor with the initialized engine
        let actor = ScriptActor {
            config: config.clone(),
            engine: Arc::new(Mutex::new(engine))
        };
        
        // Get behavior function
        let behavior = actor.get_behavior();
        
        // Create state and ports
        let state: Arc<Mutex<dyn ActorState>> = Arc::new(Mutex::new(MemoryState::default()));
        let outports = actor.get_outports();
        
        // Create a test payload with the correct port name
        let mut payload = HashMap::new();
        payload.insert("process".to_string(), Message::String("test".to_string()));
        
        // Call the behavior function
        let result = behavior(payload, state, outports.clone());
        
        // Verify the result
        assert!(result.is_ok());
        if let Ok(output) = result {
            assert!(output.contains_key("process"), "Output does not contain 'process' key: {:?}", output);
            assert_eq!(output.get("process"), Some(&Message::String("test".to_string())));
        }
    }

    #[test]
    fn test_python_actor() {
        // Test Python script actor - currently unimplemented
        // This test is a placeholder for when Python support is added
        // The implementation would follow a similar pattern to the JavaScript test
    }

    #[test]
    fn test_extism_actor() {
        // Test Extism plugin actor - currently unimplemented
        // This test is a placeholder for when Extism support is fully implemented
        // The implementation would follow a similar pattern to the JavaScript test
    }
}