use anyhow::{Result, Context};
use microsandbox::{PythonSandbox, NodeSandbox, BaseSandbox};
use tokio::net::TcpListener;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::Mutex as AsyncMutex;
use std::collections::HashMap;
use tracing::{info, debug};

mod extractor;

use reflow_network::websocket_rpc::types::{RpcRequest, RpcResponse, RpcNotification, RpcError};
use reflow_network::script_discovery::types::ScriptRuntime;
use uuid::Uuid;

/// MicroSandbox-based runtime server for script actors
/// This server runs the WebSocket RPC interface and executes scripts
/// inside MicroSandbox for secure isolation
pub struct MicroSandboxRuntime {
    /// Python sandbox for Python actors
    python_sandbox: Arc<AsyncMutex<Option<PythonSandbox>>>,
    /// Node sandbox for JavaScript actors  
    node_sandbox: Arc<AsyncMutex<Option<NodeSandbox>>>,
    /// Loaded actor scripts
    loaded_actors: Arc<RwLock<HashMap<String, LoadedActor>>>,
    /// Port to listen on
    port: u16,
    /// Host to bind to
    host: String,
}

/// Represents a loaded actor script
struct LoadedActor {
    /// Actor name/ID
    name: String,
    /// Script content
    source: String,
    /// Runtime type (Python/JavaScript)
    runtime: ScriptRuntime,
    /// Actor metadata
    metadata: Value,
}

impl MicroSandboxRuntime {
    /// Create a new MicroSandbox runtime server
    pub async fn new(host: String, port: u16) -> Result<Self> {
        Ok(Self {
            python_sandbox: Arc::new(AsyncMutex::new(None)),
            node_sandbox: Arc::new(AsyncMutex::new(None)),
            loaded_actors: Arc::new(RwLock::new(HashMap::new())),
            port,
            host,
        })
    }
    
    /// Initialize sandboxes on demand
    async fn ensure_python_sandbox(&self) -> Result<()> {
        let mut sandbox_guard = self.python_sandbox.lock().await;
        if sandbox_guard.is_none() {
            info!("Initializing Python sandbox");
            // Set MicroSandbox server URL if not already set
            // Try common environment variable names
            if std::env::var("MSB_SERVER_URL").is_err() {
                std::env::set_var("MSB_SERVER_URL", "http://localhost:8080");
            }
            if std::env::var("MICROSANDBOX_SERVER_URL").is_err() {
                std::env::set_var("MICROSANDBOX_SERVER_URL", "http://localhost:8080");
            }
            if std::env::var("MSB_URL").is_err() {
                std::env::set_var("MSB_URL", "http://localhost:8080");
            }
            let mut sandbox = PythonSandbox::create("reflow-python")
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create Python sandbox: {}", e))?;
            sandbox.start(None).await
                .map_err(|e| anyhow::anyhow!("Failed to start Python sandbox: {}", e))?;
            *sandbox_guard = Some(sandbox);
        }
        Ok(())
    }
    
    async fn ensure_node_sandbox(&self) -> Result<()> {
        let mut sandbox_guard = self.node_sandbox.lock().await;
        if sandbox_guard.is_none() {
            info!("Initializing Node sandbox");
            // Set MicroSandbox server URL if not already set
            // Try common environment variable names
            if std::env::var("MSB_SERVER_URL").is_err() {
                std::env::set_var("MSB_SERVER_URL", "http://localhost:8080");
            }
            if std::env::var("MICROSANDBOX_SERVER_URL").is_err() {
                std::env::set_var("MICROSANDBOX_SERVER_URL", "http://localhost:8080");
            }
            if std::env::var("MSB_URL").is_err() {
                std::env::set_var("MSB_URL", "http://localhost:8080");
            }
            let mut sandbox = NodeSandbox::create("reflow-node")
                .await
                .map_err(|e| anyhow::anyhow!("Failed to create Node sandbox: {}", e))?;
            sandbox.start(None).await
                .map_err(|e| anyhow::anyhow!("Failed to start Node sandbox: {}", e))?;
            *sandbox_guard = Some(sandbox);
        }
        Ok(())
    }
    
    /// Load an actor script into the sandbox
    pub async fn load_actor(&self, file_path: &str) -> Result<()> {
        info!("Loading actor from: {}", file_path);
        
        // Read the script file
        let source = tokio::fs::read_to_string(file_path)
            .await
            .context("Failed to read actor file")?;
        
        // Determine runtime from extension
        let runtime = if file_path.ends_with(".py") {
            ScriptRuntime::Python
        } else if file_path.ends_with(".js") || file_path.ends_with(".mjs") {
            ScriptRuntime::JavaScript
        } else {
            return Err(anyhow::anyhow!("Unsupported script type"));
        };
        
        // Extract metadata from the script
        let metadata = self.extract_metadata(&source, runtime)?;
        let actor_name = metadata["component"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        
        // Store the loaded actor
        let actor = LoadedActor {
            name: actor_name.clone(),
            source,
            runtime,
            metadata,
        };
        
        self.loaded_actors.write().insert(actor_name.clone(), actor);
        info!("Loaded actor: {}", actor_name);
        
        Ok(())
    }
    
    /// Extract metadata from script source
    fn extract_metadata(&self, source: &str, runtime: ScriptRuntime) -> Result<Value> {
        extractor::extract_metadata(source, runtime)
    }
    
    /// Process an actor execution request
    async fn process_actor(&self, params: Value, connection_id: &str) -> Result<Value> {
        let actor_id = params["actor_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing actor_id"))?;
        
        // Get the loaded actor (clone to release lock before await)
        let (actor_source, actor_runtime) = {
            let actors = self.loaded_actors.read();
            let actor = actors
                .get(actor_id)
                .ok_or_else(|| anyhow::anyhow!("Actor not found: {}", actor_id))?;
            (actor.source.clone(), actor.runtime.clone())
        };
        
        // Prepare the execution context with connection ID for callbacks
        let context = json!({
            "payload": params["payload"],
            "config": params["config"],
            "state": params["state"],
            "actor_id": actor_id,
            "timestamp": params["timestamp"],
            "__connection_id": connection_id,
            "__rpc_url": format!("ws://{}:{}", self.host, self.port)
        });
        
        // Execute in sandbox based on runtime
        let result = match actor_runtime {
            ScriptRuntime::Python => {
                self.execute_python_actor(&actor_source, context, connection_id).await?
            }
            ScriptRuntime::JavaScript => {
                self.execute_javascript_actor(&actor_source, context, connection_id).await?
            }
            _ => {
                return Err(anyhow::anyhow!("Unsupported runtime: {:?}", actor_runtime));
            }
        };
        
        Ok(result)
    }
    
    /// Execute a Python actor in the sandbox
    async fn execute_python_actor(&self, source: &str, context: Value, _connection_id: &str) -> Result<Value> {
        debug!("Executing Python actor in sandbox");
        
        // Ensure sandbox is initialized
        self.ensure_python_sandbox().await?;
        
        // Create the execution script that wraps the actor with RPC callback support
        let wrapper_script = format!(r#"
import json
import sys
import asyncio
import websockets

# Actor decorator implementation
def actor(metadata):
    """Decorator to attach metadata to an actor function"""
    def decorator(func):
        func.__actor_metadata__ = metadata
        return func
    return decorator

# Actor source code
{}

# Execution context
context_json = '{}'
context = json.loads(context_json)

# Global for RPC communication
_connection_id = context.get('__connection_id')
_rpc_url = context.get('__rpc_url')

# Create actor context object with output sending capability
class ActorContext:
    def __init__(self, data):
        self.payload = data.get('payload', {{}})
        self.config = data.get('config', {{}})
        self.state = data.get('state', {{}})
        self.actor_id = data.get('actor_id', '')
        self.timestamp = data.get('timestamp', 0)
        self._outputs = {{}}
    
    def get_input(self, port, default=None):
        """Get input from port - Message converts to direct JSON values"""
        # Message already converts to plain JSON values:
        # Integer(42) -> 42, String("hello") -> "hello", etc.
        return self.payload.get(port, default)
    
    async def send_output(self, port, value):
        """Send output to a specific port via WebSocket RPC"""
        if _connection_id and _rpc_url:
            try:
                async with websockets.connect(_rpc_url) as websocket:
                    notification = {{
                        "jsonrpc": "2.0",
                        "method": "output",
                        "params": {{
                            "connection_id": _connection_id,
                            "actor_id": self.actor_id,
                            "port": port,
                            "data": value  # Send raw value, Message::from(Value) will handle conversion
                        }}
                    }}
                    await websocket.send(json.dumps(notification))
            except Exception as e:
                print(f"Failed to send output: {{e}}", file=sys.stderr)
        
        # Store raw value for final result
        self._outputs[port] = value
    
    def send_output_sync(self, port, value):
        """Synchronous version for non-async contexts"""
        asyncio.run(self.send_output(port, value))
        
    def get_outputs(self):
        """Get all outputs for final result"""
        return self._outputs

# Create context and execute
actor_context = ActorContext(context)

# Find and execute the actor function
async def run_actor():
    # Find the actor function decorated with @actor
    for name, obj in globals().items():
        if callable(obj) and hasattr(obj, '__actor_metadata__'):
            # Execute the actor
            result = await obj(actor_context) if asyncio.iscoroutinefunction(obj) else obj(actor_context)
            # Merge any returned outputs with sent outputs
            if isinstance(result, dict):
                actor_context._outputs.update(result)
            return actor_context._outputs
    
    raise Exception("No actor function found - ensure your function is decorated with @actor")

# Run and output result
try:
    result = asyncio.run(run_actor())
    print(json.dumps({{"outputs": result if isinstance(result, dict) else {{"output": result}}}}))
except Exception as e:
    print(json.dumps({{"error": str(e)}}), file=sys.stderr)
    sys.exit(1)
"#, source, context.to_string());
        
        // Execute in Python sandbox
        let sandbox_guard = self.python_sandbox.lock().await;
        let sandbox = sandbox_guard.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Python sandbox not initialized"))?;
        
        let execution = sandbox
            .run(&wrapper_script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to execute Python actor: {}", e))?;
        
        // Get the output
        let output = execution
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get Python execution output: {}", e))?;
        
        // Parse the output
        let result: Value = serde_json::from_str(&output)
            .context("Failed to parse actor output")?;
        
        Ok(result)
    }
    
    /// Execute a JavaScript actor in the sandbox
    async fn execute_javascript_actor(&self, source: &str, context: Value, _connection_id: &str) -> Result<Value> {
        debug!("Executing JavaScript actor in sandbox");
        
        // Ensure sandbox is initialized
        self.ensure_node_sandbox().await?;
        
        // Create the execution script that wraps the actor with RPC callback support
        let wrapper_script = format!(r#"
const WebSocket = require('ws');

// Actor source code
{}

// Execution context
const context = {};

// Global for RPC communication
const _connectionId = context.__connection_id;
const _rpcUrl = context.__rpc_url;

// Create actor context object with output sending capability
class ActorContext {{
    constructor(data) {{
        this.payload = data.payload || {{}};
        this.config = data.config || {{}};
        this.state = data.state || {{}};
        this.actor_id = data.actor_id || '';
        this.timestamp = data.timestamp || 0;
        this._outputs = {{}};
    }}
    
    getInput(port, defaultValue = null) {{
        // Get input from port - Message converts to direct JSON values
        // Message already converts to plain JSON values:
        // Integer(42) -> 42, String("hello") -> "hello", etc.
        return this.payload[port] !== undefined ? this.payload[port] : defaultValue;
    }}
    
    async sendOutput(port, value) {{
        // Send output to a specific port via WebSocket RPC
        if (_connectionId && _rpcUrl) {{
            try {{
                const ws = new WebSocket(_rpcUrl);
                await new Promise((resolve, reject) => {{
                    ws.on('open', () => {{
                        const notification = {{
                            jsonrpc: "2.0",
                            method: "output",
                            params: {{
                                connection_id: _connectionId,
                                actor_id: this.actor_id,
                                port: port,
                                data: value  // Send raw value, Message::from(Value) will handle conversion
                            }}
                        }};
                        ws.send(JSON.stringify(notification));
                        ws.close();
                        resolve();
                    }});
                    ws.on('error', reject);
                }});
            }} catch (e) {{
                console.error(`Failed to send output: ${{e}}`);
            }}
        }}
        
        // Store raw value for final result
        this._outputs[port] = value;
    }}
    
    sendOutputSync(port, value) {{
        // For synchronous contexts, queue for later sending
        this._outputs[port] = value;
    }}
    
    getOutputs() {{
        return this._outputs;
    }}
}}

// Create context and execute
const actorContext = new ActorContext(context);

// Find and execute the actor function
(async () => {{
    try {{
        // Look for exported actor function or decorated function
        let result;
        if (typeof process !== 'undefined' && process.__actor__) {{
            result = await process.__actor__(actorContext);
        }} else if (typeof processData === 'function') {{
            result = await processData(actorContext);
        }} else {{
            throw new Error('No actor function found');
        }}
        
        // Merge any returned outputs with sent outputs
        if (typeof result === 'object' && result !== null) {{
            Object.assign(actorContext._outputs, result);
        }}
        
        // Output final result
        const output = {{
            outputs: actorContext._outputs
        }};
        console.log(JSON.stringify(output));
    }} catch (e) {{
        console.error(JSON.stringify({{ error: e.message }}));
        process.exit(1);
    }}
}})();
"#, source, context.to_string());
        
        // Execute in Node sandbox
        let sandbox_guard = self.node_sandbox.lock().await;
        let sandbox = sandbox_guard.as_ref()
            .ok_or_else(|| anyhow::anyhow!("Node sandbox not initialized"))?;
        
        let execution = sandbox
            .run(&wrapper_script)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to execute JavaScript actor: {}", e))?;
        
        // Get the output
        let output = execution
            .output()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get JavaScript execution output: {}", e))?;
        
        // Parse the output
        let result: Value = serde_json::from_str(&output)
            .context("Failed to parse actor output")?;
        
        Ok(result)
    }
    
    /// Handle RPC request
    async fn handle_rpc_request(&self, request: RpcRequest, connection_id: &str) -> RpcResponse {
        debug!("Handling RPC request: method={}", request.method);
        
        let result = match request.method.as_str() {
            "process" => {
                // Process actor execution
                match self.process_actor(request.params, connection_id).await {
                    Ok(result) => result,
                    Err(e) => {
                        return RpcResponse {
                            jsonrpc: "2.0".to_string(),
                            id: request.id,
                            result: None,
                            error: Some(RpcError {
                                code: -32603,
                                message: e.to_string(),
                                data: None,
                            }),
                        };
                    }
                }
            }
            "load" => {
                // Load a new actor
                if let Some(file_path) = request.params["file_path"].as_str() {
                    match self.load_actor(file_path).await {
                        Ok(_) => json!({"success": true}),
                        Err(e) => json!({"success": false, "error": e.to_string()}),
                    }
                } else {
                    json!({"success": false, "error": "Missing file_path"})
                }
            }
            "list" => {
                // List loaded actors
                let actors = self.loaded_actors.read();
                json!({
                    "actors": actors.keys().cloned().collect::<Vec<_>>()
                })
            }
            _ => {
                // Unknown method
                return RpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(RpcError {
                        code: -32601,
                        message: format!("Method not found: {}", request.method),
                        data: None,
                    }),
                };
            }
        };
        
        RpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(result),
            error: None,
        }
    }
    
    /// Start the WebSocket RPC server
    pub async fn start(self: Arc<Self>) -> Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        info!("Starting MicroSandbox Runtime Server on {}", addr);
        
        let listener = TcpListener::bind(&addr).await?;
        info!("Server listening on ws://{}", addr);
        
        while let Ok((stream, _)) = listener.accept().await {
            let runtime = self.clone();
            tokio::spawn(async move {
                if let Ok(ws_stream) = accept_async(stream).await {
                    runtime.handle_connection(ws_stream).await;
                }
            });
        }
        
        Ok(())
    }
    
    /// Handle WebSocket connection
    async fn handle_connection(&self, ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) {
        let connection_id = Uuid::new_v4().to_string();
        info!("Client connected: {}", connection_id);
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        // Handle incoming messages
        while let Some(msg) = ws_receiver.next().await {
            if let Ok(msg) = msg {
                if let Message::Text(text) = msg {
                    debug!("Received message: {}", text);
                    // Try to parse as RPC request
                    if let Ok(request) = serde_json::from_str::<RpcRequest>(&text) {
                        // Handle request
                        let response = self.handle_rpc_request(request, &connection_id).await;
                        
                        // Send response directly
                        let response_text = serde_json::to_string(&response).unwrap();
                        debug!("Sending response: {}", response_text);
                        if ws_sender.send(Message::text(response_text)).await.is_err() {
                            break;
                        }
                    }
                    // Try to parse as RPC notification (for output messages from scripts)
                    else if let Ok(notification) = serde_json::from_str::<RpcNotification>(&text) {
                        if notification.method == "output" {
                            // For now, just log it - in a full implementation this would forward to other connections
                            debug!("Received output notification: {:?}", notification);
                        }
                    }
                }
            }
        }
        
        info!("Client disconnected: {}", connection_id);
    }
}

/// Main entry point for the runtime server
pub async fn run_microsandbox_runtime(host: String, port: u16, actors: Vec<String>) -> Result<()> {
    let runtime = Arc::new(MicroSandboxRuntime::new(host, port).await?);
    
    // Preload actors
    for actor_path in actors {
        runtime.load_actor(&actor_path).await?;
    }
    
    // Start server
    runtime.start().await
}

/// Cleanup sandboxes on shutdown
impl Drop for MicroSandboxRuntime {
    fn drop(&mut self) {
        // Note: We can't easily clean up async resources in Drop
        // The sandboxes will be cleaned up when the process exits
        // For proper cleanup, call a shutdown method explicitly
    }
}

impl MicroSandboxRuntime {
    /// Explicit shutdown method for proper cleanup
    pub async fn shutdown(&self) -> Result<()> {
        // Cleanup Python sandbox
        if let Some(mut sandbox) = self.python_sandbox.lock().await.take() {
            sandbox.stop().await
                .map_err(|e| anyhow::anyhow!("Failed to stop Python sandbox: {}", e))?;
        }
        
        // Cleanup Node sandbox
        if let Some(mut sandbox) = self.node_sandbox.lock().await.take() {
            sandbox.stop().await
                .map_err(|e| anyhow::anyhow!("Failed to stop Node sandbox: {}", e))?;
        }
        
        Ok(())
    }
}