use super::{Message, ScriptConfig, ScriptEngine};
use anyhow::Result;
use flume::{Receiver, Sender};
use futures::executor::block_on;
use parking_lot::{Mutex, RwLock};
use quickjs_runtime::{
    builder::QuickJsRuntimeBuilder,
    facades::QuickJsRuntimeFacade,
    jsutils::{JsError, Script},
    quickjs_utils::objects::create_object,
    values::{JsValueConvertable, JsValueFacade},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{mpsc, Arc},
};

pub struct JavaScriptEngine {
    pub worker: Option<WorkerBackend>,
}

pub enum WorkerBackend {
    WebWorker(WebWorkerHandle),
    QuickJs(QuickJsContext),
}

#[derive(Clone)]
struct WebWorkerHandle {
    tx: Sender<WorkerMessage>,
}

struct QuickJsContext {
    ctx: Arc<Mutex<QuickJsRuntimeFacade>>,
    rx: Receiver<WorkerMessage>,
}

impl QuickJsContext {
    pub fn new() -> Result<Self> {
        let ctx = Arc::new(Mutex::new(QuickJsRuntimeBuilder::new().build()));
        let (tx, rx) = flume::unbounded();
        Ok(Self { ctx, rx })
    }
}

#[derive(Serialize, Deserialize)]
enum WorkerMessage {
    Execute { source: String },
    Result { value: Message },
    Error { message: String },
    Terminate,
}

impl JavaScriptEngine {
    fn setup_web_worker_handlers(&mut self, rx: Receiver<WorkerMessage>) {
        if let Some(WorkerBackend::WebWorker(handle)) = &self.worker {
            let tx = handle.tx.clone();
            std::thread::spawn(move || {
                for msg in rx {
                    match msg {
                        WorkerMessage::Execute { source } => {
                            // Parse the source as JSON first, then convert to Message
                            match serde_json::from_str::<serde_json::Value>(&source) {
                                Ok(json_value) => {
                                    if let Ok(message) =
                                        serde_json::from_value::<Message>(json_value)
                                    {
                                        tx.send(WorkerMessage::Result { value: message }).unwrap();
                                    } else {
                                        tx.send(WorkerMessage::Error {
                                            message: format!(
                                                "Failed to convert JSON to Message: {}",
                                                source
                                            ),
                                        })
                                        .unwrap();
                                    }
                                }
                                Err(e) => {
                                    tx.send(WorkerMessage::Error {
                                        message: format!("Failed to parse JSON: {}", e),
                                    })
                                    .unwrap();
                                }
                            }
                        }
                        WorkerMessage::Terminate => break,
                        _ => {}
                    }
                }
            });
        }
    }

    fn setup_quickjs_handlers(&mut self, rx: Receiver<WorkerMessage>) {
        if let Some(WorkerBackend::QuickJs(ctx)) = &self.worker {
            let ctx_clone = ctx.ctx.clone();
            std::thread::spawn(move || {
                for msg in rx {
                    match msg {
                        WorkerMessage::Execute { source } => {
                            let _ = ctx_clone.lock().eval(None, Script::new("*.js", &source));
                        }
                        WorkerMessage::Terminate => break,
                        _ => {}
                    }
                }
            });
        }
    }
}

#[async_trait::async_trait]
impl ScriptEngine for JavaScriptEngine {
    async fn init(&mut self, config: &ScriptConfig) -> Result<()> {
        let use_web_worker = cfg!(target_arch = "wasm32") || cfg!(feature = "webworker");

        if !use_web_worker {
            let ctx = Arc::new(Mutex::new(QuickJsRuntimeBuilder::new().build()));
            let (tx, rx) = flume::unbounded();
            self.worker = Some(WorkerBackend::QuickJs(QuickJsContext { ctx, rx }));
        } else {
            let (tx, rx) = flume::unbounded();
            self.worker = Some(WorkerBackend::WebWorker(WebWorkerHandle { tx }));
        }

        let source_code = &config.source;
        let entry_point = &config.entry_point;

        match &mut self.worker {
            Some(WorkerBackend::QuickJs(ctx)) => {
                ctx.ctx.lock().eval(None, Script::new("*.js", source_code))
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
            }
            Some(WorkerBackend::WebWorker(handle)) => {
                handle.tx.send_async(WorkerMessage::Execute {
                    source: source_code.clone(),
                }).await?;
            }
            None => return Err(anyhow::anyhow!("No worker initialized")),
        }

        let (worker_tx, worker_rx) = flume::unbounded();
        if use_web_worker {
            self.setup_web_worker_handlers(worker_rx);
        } else {
            self.setup_quickjs_handlers(worker_rx);
        }

        if let Some(backend) = &self.worker {
            match backend {
                WorkerBackend::WebWorker(handle) => {
                    handle.tx.send_async(WorkerMessage::Execute {
                        source: format!(
                            "globalThis.onmessage = {{ handleEvent: e => {{ {} }} }}",
                            config.source
                        ),
                    }).await?;
                }
                WorkerBackend::QuickJs(ctx) => {
                    ctx.ctx
                        .lock()
                        .eval(None, Script::new("*.js", &config.source))
                        .await?;
                }
            }
        }

        Ok(())
    }

    async fn call(&self, context: &crate::context::ScriptContext) -> Result<Message> {
        // Get the method name from the context
        let method = &context.method;

        // Get the input arguments from the context
        // For now, we'll just use the first input message if available
        let args: Vec<_> = context
            .inputs
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        let args_json = serde_json::to_value(&args)?;

        // Create an enhanced context object with output function
        let context_clone = context.clone();
        let mut context_obj = serde_json::to_value(context.to_serializable()?)?
            .as_object()
            .unwrap()
            .clone();

      

        let context_json = serde_json::to_value(context_obj)?;

        // Format the JavaScript call with both method and context
        let source = format!("{}({}, {})", method, args_json, context_json);

        match &self.worker {
            Some(WorkerBackend::WebWorker(handle)) => {
                // For WebWorker, we need to send the method call as a JSON string
                let call_json = json!({
                    "method": method,
                    "args": args,
                   // "context": serializable_context
                });

                handle.tx.send_async(WorkerMessage::Execute {
                    source: serde_json::to_string(&call_json)?,
                }).await?;

                // In a real implementation, we would wait for a response here
                // For now, just return an empty object as in the original code
                Ok(Message::Any(json!({}).into()))
            }
            Some(WorkerBackend::QuickJs(ctx)) => {
                let value_args = args
                    .iter()
                    .map(|arg| (arg.0.clone(), arg.1.clone().into()))
                    .collect::<Vec<(String, Value)>>();

                // Convert the args to a vector of JsValueFacade objects
                let js_args = vec![
                    json!(HashMap::<String, Value>::from_iter(value_args)).to_js_value_facade()
                ];

                // Register the send_output function in the QuickJS runtime
                let context_clone = context.clone();
                let ctx_lock = ctx.ctx.lock();
                
                // Create an async wrapper function for send_output
                ctx_lock.set_function(
                    &["Context"],
                    "send_output",
                    move |_q_ctx, args: Vec<JsValueFacade>| {
                        // Extract port and message
                        let port = args[0].get_str();
              
                        // Convert the second argument to a Value
                        let msg_value_future = args[1].to_serde_value();
                        let msg_value = match futures::executor::block_on(msg_value_future) {
                            Ok(v) => v,
                            Err(e) => return Err(JsError::new(
                                "Conversion Error".to_string(),
                                format!("Failed to convert JavaScript value to JSON: {}", e),
                                "send_output".to_string(),
                            )),
                        };

                        // Send the output through the context
                        let result = context_clone
                            .send_output(port, msg_value.into())
                            .map_err(|e| format!("Failed to send output: {}", e));

                        if result.is_err() {
                            return Err(JsError::new(
                                "Runtime Error".to_string(),
                                result.err().unwrap(),
                                "send_output".to_string(),
                            ));
                        } else {
                            Ok(JsValueFacade::Null)
                        }
                    },
                )?;

                // Use invoke_function with await instead of block_on
                let js_result = ctx_lock.invoke_function(None, &[], method, js_args).await
                    .map_err(|e| anyhow::anyhow!("QuickJS error: {:?}", e))?;
                
                let result = js_result.to_serde_value().await?;
                Ok(result.into())
            }
            None => Err(anyhow::anyhow!("Worker not initialized")),
        }
    }

    async fn cleanup(&mut self) -> Result<()> {
        match &mut self.worker {
            Some(WorkerBackend::WebWorker(handle)) => {
                handle.tx.send_async(WorkerMessage::Terminate).await?;
            }
            Some(WorkerBackend::QuickJs(ctx)) => {
                // Drop the context asynchronously if possible
                ctx.ctx.lock().drop_context("main");
            }
            None => {}
        }
        self.worker = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ScriptRuntime;
    use anyhow::anyhow;
    use serde_json::json;

    fn test_config(source: &str) -> ScriptConfig {
        ScriptConfig {
            runtime: ScriptRuntime::JavaScript,
            source: source.to_string(),
            entry_point: "main".to_string(),
        }
    }

    #[tokio::test]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_call_function() -> anyhow::Result<()> {
        use std::collections::HashMap;

        use reflow_network::actor::Port;

        let mut engine = JavaScriptEngine { worker: None };
        let config = test_config(
            r#"
            function add(inputs) { 
                return inputs.arg1 + inputs.arg2; 
            }
            function concat(inputs) { 
                return inputs.arg1 + inputs.arg2; 
            }
            function getObject(args) { 
                return { a: 1, b: 'test' }; 
            }
            function getArray(args) { 
                return [1, 2, 3]; 
            }
        "#,
        );
        engine.init(&config).await?;

        // Create state and ports for context
        let state = Arc::new(Mutex::new(reflow_network::actor::MemoryState::default()));
        let (sender, receiver) = flume::unbounded();
        let outports: Port = (sender, receiver);

        // Test numeric operations
        let mut inputs = HashMap::new();
        inputs.insert("arg1".to_string(), Message::Integer(1));
        inputs.insert("arg2".to_string(), Message::Integer(2));
        let context = crate::context::ScriptContext::new(
            "add".to_string(),
            inputs,
            state.clone(),
            outports.clone(),
        );
        let result = engine.call(&context).await?;
        assert_eq!(result, Message::Integer(3));

        // Test string operations
        let mut inputs = HashMap::new();
        inputs.insert("arg1".to_string(), Message::String("foo".into()));
        inputs.insert("arg2".to_string(), Message::String("bar".into()));
        let context = crate::context::ScriptContext::new(
            "concat".to_string(),
            inputs,
            state.clone(),
            outports.clone(),
        );
        let result = engine.call(&context).await?;
        assert_eq!(result, Message::String("foobar".into()));

        // Test object serialization
        let inputs = HashMap::new();
        let context = crate::context::ScriptContext::new(
            "getObject".to_string(),
            inputs,
            state.clone(),
            outports.clone(),
        );
        let result = engine.call(&context).await?;
        let expected = Message::Any(json!({ "a": 1, "b": "test" }).into());
        assert_eq!(result, expected);

        // Test array serialization
        let inputs = HashMap::new();
        let context = crate::context::ScriptContext::new(
            "getArray".to_string(),
            inputs,
            state.clone(),
            outports.clone(),
        );
        let result = engine.call(&context).await?;
        let expected = Message::Any(json!([1, 2, 3]).into());
        assert_eq!(result, expected);

        if let Some(WorkerBackend::QuickJs(_)) = &engine.worker {
            Ok(())
        } else {
            Err(anyhow!("Wrong backend initialized"))
        }
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_web_worker_message_passing() -> anyhow::Result<()> {
        let mut engine = JavaScriptEngine { worker: None };
        let config = test_config(
            r#"
                function onmessage(args, context) {
                    return {
                        type: "message",
                        value: args[0]
                    };
                }
            "#,
        );
        engine.init(&config)?;

        // Create state and ports for context
        let state = Arc::new(Mutex::new(reflow_network::actor::MemoryState::default()));
        let (sender, receiver) = flume::unbounded();
        let outports: Port = (sender, receiver);

        // Test round-trip message preservation
        let test_messages = vec![
            Message::Number(42.into()),
            Message::String("test".into()),
            Message::Any(json!({ "nested": [1, "two"] }).into()),
        ];

        for msg in test_messages {
            // Create context with the message as input
            let mut inputs = HashMap::new();
            inputs.insert("onmessage".to_string(), msg.clone());

            let context = crate::context::ScriptContext::new(
                "onmessage".to_string(),
                inputs,
                state.clone(),
                outports.clone(),
            );

            let result = engine.call(&context)?;
            assert_eq!(
                result,
                Message::Any(
                    json!({
                        "type": "message",
                        "value": msg
                    })
                    .into()
                )
            );
        }

        Ok()
    }

    #[tokio::test]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_context_send_output() -> anyhow::Result<()> {
        use reflow_network::actor::Port;
        let mut engine = JavaScriptEngine { worker: None };
        let config = test_config(
            r#"
            function process(args) { 
               Context.send_output("value", { "data": "output-test"})
            }
        "#,
        );
        engine.init(&config).await?;
        // Create state and ports for context
        let state = Arc::new(Mutex::new(reflow_network::actor::MemoryState::default()));
        let (sender, receiver) = flume::unbounded();
        let outports: Port = (sender, receiver.clone());

        // Create empty inputs for the test
        let inputs = HashMap::new();

        // Create a test context
        let context = crate::context::ScriptContext::new(
            "process".to_string(),
            inputs,
            state.clone(),
            outports.clone(),
        );

        // Directly call the send_output method with await
        let test_message = Message::Object(json!({"data": "output-test"}).into()).into();

        engine.call(&context).await?;

        // Check if we received the message on the output port
        if let Ok(output) = receiver.recv_async().await {
            assert!(
                output.contains_key("value"),
                "Output does not contain 'value' key"
            );
            if let Some(message) = output.get("value") {
                assert_eq!(message, &test_message, "Messages don't match");
            } else {
                panic!("No message found for 'test-output' key");
            }
        } else {
            panic!("No message received on output port");
        }

        Ok(())
    }
}
