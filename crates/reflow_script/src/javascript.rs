use crate::context::ScriptContext;

use super::{Message, ScriptConfig, ScriptEngine};
use anyhow::Result;
use deno_runtime::deno_core::{serde_v8, v8};
use flume::{Receiver, Sender};
use futures::executor::block_on;
use parking_lot::Mutex;
use reflow_js::JavascriptRuntime;
use serde_json::{Value, json};
use std::{collections::HashMap, fmt::Debug, sync::Arc};

#[derive(Clone)]
pub struct JavaScriptEngine {
    pub worker: Option<WebWorkerHandle>,
    pub worker_tx: Option<Sender<WorkerMessage>>,
    pub(crate) runtime: Option<Arc<Mutex<JavascriptRuntime>>>,
    pub(crate) context_senders: HashMap<String, Sender<HashMap<String, Message>>>,
}

impl JavaScriptEngine {
    pub fn new() -> Self {
        Self {
            worker: None,
            worker_tx: None,
            runtime: None,
            context_senders: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub struct WebWorkerHandle {
    tx: Sender<WorkerMessage>,
}

#[derive(Clone)]
pub enum WorkerMessage {
    Execute {
        source: String,
        response_id: Option<String>,
    },
    ExecuteWithContext(ScriptContext, Option<String>),
    Result {
        value: Message,
        response_id: Option<String>,
    },
    Error {
        message: String,
        response_id: Option<String>,
    },
    Terminate {
        response_id: Option<String>,
    },
}

impl Debug for WorkerMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Execute {
                source,
                response_id,
            } => f
                .debug_struct("Execute")
                .field("source", source)
                .field("response_id", response_id)
                .finish(),
            Self::ExecuteWithContext(arg0, response_id) => f
                .debug_tuple("ExecuteWithContext")
                .field(&arg0.to_serializable().unwrap())
                .field(response_id)
                .finish(),
            Self::Result { value, response_id } => f
                .debug_struct("Result")
                .field("value", value)
                .field("response_id", response_id)
                .finish(),
            Self::Error {
                message,
                response_id,
            } => f
                .debug_struct("Error")
                .field("message", message)
                .field("response_id", response_id)
                .finish(),
            Self::Terminate { response_id } => f
                .debug_struct("Terminate")
                .field("response_id", response_id)
                .finish(),
        }
    }
}

impl JavaScriptEngine {
    fn setup_web_worker_handlers(&mut self, rx: Receiver<WorkerMessage>) {
        if let Some(handle) = &self.worker {
            let tx = handle.tx.clone();
            std::thread::spawn(move || {
                for msg in rx {
                    match msg {
                        WorkerMessage::Execute {
                            source,
                            response_id,
                        } => {
                            // Parse the source as JSON first, then convert to Message
                            match serde_json::from_str::<serde_json::Value>(&source) {
                                Ok(json_value) => {
                                    if let Ok(message) =
                                        serde_json::from_value::<Message>(json_value)
                                    {
                                        tx.send(WorkerMessage::Result {
                                            value: message,
                                            response_id,
                                        })
                                        .unwrap();
                                    } else {
                                        tx.send(WorkerMessage::Error {
                                            message: format!(
                                                "Failed to convert JSON to Message: {}",
                                                source
                                            ),
                                            response_id,
                                        })
                                        .unwrap();
                                    }
                                }
                                Err(e) => {
                                    tx.send(WorkerMessage::Error {
                                        message: format!("Failed to parse JSON: {}", e),
                                        response_id,
                                    })
                                    .unwrap();
                                }
                            }
                        }
                        WorkerMessage::Terminate { .. } => break,
                        _ => {}
                    }
                }
            });
        }
    }
}

unsafe impl Send for JavaScriptEngine{}

#[async_trait::async_trait]
impl ScriptEngine for JavaScriptEngine {
    async fn init(&mut self, config: &ScriptConfig) -> Result<()> {
        let use_web_worker = cfg!(target_arch = "wasm32") || cfg!(feature = "webworker");

        if config.packages.is_some() {
            return Err(anyhow::anyhow!("JavaScript engine does not currently support packages"));
        }

        if !use_web_worker {
            let runtime = Arc::new(Mutex::new(JavascriptRuntime::new()?));
            // Initialize the worker with the source code
            let runtime_clone = runtime.clone();
            let source_clone = config.source.clone();
            {
                let mut runtime = runtime_clone.lock();
                let _ = block_on(runtime.execute("worker", &source_clone));
            }
            self.runtime = Some(runtime);
        } else {
            let (tx, rx) = flume::unbounded();
            self.worker = Some(WebWorkerHandle { tx });

            // Set up the WebWorker handler
            self.setup_web_worker_handlers(rx);

            // Initialize the worker with the source code
            self.worker_tx = Some(self.worker.as_ref().unwrap().tx.clone());
            self.worker_tx
                .as_ref()
                .unwrap()
                .send_async(WorkerMessage::Execute {
                    source: config.source.clone(),
                    response_id: None,
                })
                .await?;
        }

        Ok(())
    }

    async fn call(
        &mut self,
        context: &crate::context::ScriptContext,
    ) -> Result<HashMap<String, Message>> {
        // Get the method name from the context
        let method = &context.method;

        // Convert inputs to a JSON value
        let args: HashMap<String, Value> = context
            .inputs
            .iter()
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or(Value::Null)))
            .collect();

        if let Some(handle) = &self.worker {
            // For WebWorker, we need to send the method call as a JSON string
            let call_json = json!({
                "method": method,
                "args": args,
            });

            handle
                .tx
                .send_async(WorkerMessage::Execute {
                    source: serde_json::to_string(&call_json)?,
                    response_id: None,
                })
                .await?;

            // In a real implementation, we would wait for a response here
            // For now, just return an empty object as in the original code
            return Ok(HashMap::from_iter([(
                "out".to_string(),
                Message::Optional(None),
            )]));
        }

        let context_id = uuid::Uuid::new_v4().to_string();
        let (outports_sender, _) = context.outports.clone();
        self.context_senders
            .insert(context_id.clone(), outports_sender);
        if let Some(runtime) = self.runtime.clone() {
            let mut runtime = runtime.lock();

            let (outports_send, _) = context.outports.clone();
            {
                let mut worker = runtime.worker.write();
                let scope = &mut worker.js_runtime.handle_scope();

                let num = scope.get_number_of_data_slots();
                scope.set_data(num, Box::into_raw(Box::new(outports_send)) as *mut _);
            }

            let send_output_fn = {
                fn callback(
                    scope: &mut v8::HandleScope,
                    args: v8::FunctionCallbackArguments,
                    mut rv: v8::ReturnValue,
                ) {
                    let error_key = v8::String::new(scope, "error").unwrap();
                    let error_wrapper = v8::Object::new(scope);

                    let sender = unsafe {
                        let slot = scope.get_number_of_data_slots();
                        let data = scope.get_data(slot) as *mut Sender<HashMap<String, Message>>;
                        let sender = data.read();
                        sender
                    };

                    if args.length() < 2 {
                        let error_msg =
                            v8::String::new(scope, "Invalid number of arguments.").unwrap();
                        error_wrapper.set(scope, error_key.into(), error_msg.into());
                        rv.set(error_wrapper.into());
                        return;
                    }

                    let port = args.get(0).to_rust_string_lossy(scope);

                    // Convert the message to a JSON value
                    let message = args.get(1).clone();

                    let message_value = serde_v8::from_v8(scope, message).unwrap();
                    let message_value: Message =
                        serde_json::from_value::<serde_json::Value>(message_value)
                            .unwrap()
                            .into();

                    let mut results = HashMap::new();
                    results.insert(port.to_string(), message_value);
                    if let Err(e) = sender.send(results) {
                        let error_msg =
                            v8::String::new(scope, &format!("Failed to send output: {}", e))
                                .unwrap();
                        error_wrapper.set(scope, error_key.into(), error_msg.into());
                        rv.set(error_wrapper.into());
                        return;
                    }

                    rv.set_undefined();
                }

                let mut worker = runtime.worker.write();
                let scope = &mut worker.js_runtime.handle_scope();

                let func = v8::FunctionTemplate::new(scope, callback);
                let func: v8::Local<v8::Value> = func.get_function(scope).unwrap().into();
                v8::Global::new(scope, func)
            };
            let _context_obj = runtime.create_object();
            let _context_obj =
                runtime.object_set_property(_context_obj, "send_output", send_output_fn.into());
            let context_obj = {
                let mut worker = runtime.worker.write();
                let scope = &mut worker.js_runtime.handle_scope();
                let v: v8::Local<v8::Value> = v8::Local::new(scope, _context_obj).into();
                let gv = v8::Global::new(scope, v);
                gv
            };
            let inputs = runtime
                .convert_value_from_rust(json!(context.inputs))
                .expect("Failed to convert input to javascript value");

            let result = runtime.call_function(&context.method, &[inputs, context_obj]);
            // Clean up the context sender
            self.context_senders.remove(&context_id);
            match result {
                Ok(res) => {
                    let mut results = HashMap::new();
                    let value = runtime.convert_value_to_rust::<serde_json::Value>(res);
                    if value.is_object() {
                        let obj = value.as_object().unwrap();
                        for (k, v) in obj.iter() {
                            let v: Message = serde_json::from_value::<serde_json::Value>(v.clone())
                                .unwrap()
                                .into();
                            results.insert(k.to_string(), v);
                        }
                        return Ok(results);
                    } else if value.is_null() {
                        results.insert("out".to_string(), Message::Flow); // Assuming Flow is the default behavior
                        return Ok(results);
                    }
                    let value: Message = serde_json::from_value::<serde_json::Value>(value)
                       .unwrap()
                       .into();
                    results.insert("out".to_string(), value);
                    return Ok(results);
                }
                Err(err) => {
                    return Err(anyhow::anyhow!(
                        "Failed to call function: {:?}",
                        err.to_string()
                    ));
                }
            }
        }

        Err(anyhow::anyhow!("Function call fialed"))
    }

    async fn cleanup(&mut self) -> Result<()> {
        if let Some(tx) = &self.worker_tx {
            // Send terminate message - using sync send instead of async
            // since we want this to be delivered immediately
            let _ = tx.send(WorkerMessage::Terminate { response_id: None });

            // Give the std::thread time to properly clean up
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Explicitly drop the worker which contains the isolate
        self.worker = None;
        self.worker_tx = None;

        Ok(())
    }
}

impl Drop for JavaScriptEngine {
    fn drop(&mut self) {
        // Force synchronous cleanup on drop
        if self.worker.is_some() {
            if let Some(tx) = &self.worker_tx {
                let _ = tx.send(WorkerMessage::Terminate { response_id: None });
                // Give a little time for shutdown
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            self.worker = None;
            self.worker_tx = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ScriptEnvironment, ScriptRuntime};
    use anyhow::anyhow;
    use serde_json::json;

    fn test_config(source: &str) -> ScriptConfig {
        ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::JavaScript,
            source: source.to_string(),
            entry_point: "main".to_string(),
            packages: None,
        }
    }

    #[tokio::test]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_simple_call_function() -> anyhow::Result<()> {
        use std::collections::HashMap;

        use reflow_network::actor::Port;

        let mut engine = JavaScriptEngine::new();
        let config = test_config(
            r#"
            function add(inputs, context) { 
                context.send_output("data", inputs.arg1.data + inputs.arg2.data);
                return inputs.arg1.data + inputs.arg2.data; 
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
        assert_eq!(result.get("out"), Some(&Message::Integer(3)));

        // Test send_output
        let res = outports.1.recv();
        assert!(res.is_ok());
        let result = res.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result.get("data").unwrap(), &Message::Integer(3));

        engine.cleanup().await?;
        drop(engine);
        Ok(())
    }

    #[tokio::test]
    #[cfg(not(target_arch = "wasm32"))]
    async fn test_call_function() -> anyhow::Result<()> {
        use std::collections::HashMap;

        use reflow_network::actor::Port;

        let mut engine = JavaScriptEngine::new();
        let config = test_config(
            r#"
            function add(inputs, context) { 
                return inputs.arg1.data + inputs.arg2.data; 
            }
            function concat(inputs, context) { 
                return inputs.arg1.data + inputs.arg2.data; 
            }
            function getObject(args, context) { 
                return { a: 1, b: 'test' }; 
            }
            function getArray(args, context) { 
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
        assert_eq!(result.get("out").unwrap(), &Message::Integer(3));

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
        assert_eq!(result.get("out").unwrap(), &Message::String("foobar".into()));

        // Test object serialization
        let inputs = HashMap::new();
        let context = crate::context::ScriptContext::new(
            "getObject".to_string(),
            inputs,
            state.clone(),
            outports.clone(),
        );
        let result = engine.call(&context).await?;
        let expected = HashMap::from_iter([
            ("a".to_string(), Message::Integer(1)),
            ("b".to_string(), Message::String("test".into())),
        ]);
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
        let expected:Message = json!(vec![1, 2, 3]).into();
        assert_eq!(result.get("out").unwrap(), &expected);

        engine.cleanup().await?;
        drop(engine);
        Ok(())
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn test_web_worker_message_passing() -> anyhow::Result<()> {
        let mut engine = JavaScriptEngine {
            worker: None,
            worker_tx: None,
        };
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
            Message::Float(42.0),
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
        let mut engine = JavaScriptEngine::new();
        let config = test_config(
            r#"
            function process(inputs, context) { 
               // Use the Context.send_output method
               context.send_output("value", { "data": "output-test" });
               
               // Return a result
               return { "success": true };
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

        // Call the function and check the result
        let result = engine.call(&context).await?;
        let expected = HashMap::from_iter([
            ("success".to_string(), Message::Boolean(true)),
        ]);

        assert_eq!(result, expected, "Result doesn't match expected value");

        // Check if we received the output message
        if let Ok(output) = receiver.try_recv() {
            assert!(
                output.contains_key("value"),
                "Output does not contain 'value' key"
            );
            if let Some(message) = output.get("value") {
                let expected_output = Message::Object(json!({"data": "output-test"}).into());
                assert_eq!(
                    message, &expected_output,
                    "Output message doesn't match expected value"
                );
            }
        }

        Ok(())
    }
}
