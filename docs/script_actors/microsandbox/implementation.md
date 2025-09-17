# MicroSandbox Implementation Guide

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Phase 1: Core Structure Modifications](#phase-1-core-structure-modifications)
3. [Phase 2: Script Discovery System](#phase-2-script-discovery-system)
4. [Phase 3: Component Registry](#phase-3-component-registry)
5. [Phase 4: WebSocket RPC](#phase-4-websocket-rpc)
6. [Phase 5: Redis State Backend](#phase-5-redis-state-backend)
7. [Phase 6: MicroSandbox Integration](#phase-6-microsandbox-integration)
8. [Phase 7: Testing](#phase-7-testing)
9. [Phase 8: Migration](#phase-8-migration)

## Prerequisites

### Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
# WebSocket
tokio-tungstenite = "0.20"
tungstenite = "0.20"

# Redis
redis = { version = "0.24", features = ["tokio-comp", "connection-manager"] }

# Shared Memory
shared_memory = "0.12"
memmap2 = "0.9"

# Serialization
bitcode = "0.6"
serde_json = "1.0"

# MicroSandbox (when available)
# microsandbox = "0.1"
```

### Environment Setup

```bash
# Install Redis
docker run -d -p 6379:6379 redis:7-alpine

# Create project structure
mkdir -p crates/reflow_network/src/{script_discovery,websocket_rpc,redis_state}
mkdir -p crates/reflow_network/{python,javascript}
```

## Phase 1: Core Structure Modifications

### Step 1.1: Add Script Runtime to GraphNode

```rust
// crates/reflow_graph/src/types.rs

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ScriptRuntime {
    Python,
    JavaScript,
}

// Modify existing GraphNode
impl GraphNode {
    // Add after existing fields
    #[serde(skip_serializing_if = "Option::is_none")]
    pub script_runtime: Option<ScriptRuntime>,
    
    // Add helper methods
    pub fn is_script_actor(&self) -> bool {
        self.script_runtime.is_some()
    }
    
    pub fn from_script_actor(actor: &DiscoveredScriptActor, id: String) -> Self {
        GraphNode {
            id,
            component: actor.component.clone(),
            metadata: Some(HashMap::from([
                ("runtime".to_string(), json!(actor.runtime)),
                ("version".to_string(), json!(actor.version)),
            ])),
            script_runtime: Some(actor.runtime.clone()),
        }
    }
}
```

### Step 1.2: Create Script Discovery Types

```rust
// crates/reflow_network/src/script_discovery/types.rs

use serde::{Serialize, Deserialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredScriptActor {
    pub component: String,
    pub description: String,
    pub file_path: PathBuf,
    pub runtime: ScriptRuntime,
    pub inports: Vec<PortDefinition>,
    pub outports: Vec<PortDefinition>,
    pub workspace_metadata: ScriptActorMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScriptActorMetadata {
    pub namespace: String,
    pub version: String,
    pub author: Option<String>,
    pub dependencies: Vec<String>,
    pub runtime_requirements: RuntimeRequirements,
    pub config_schema: Option<serde_json::Value>,
    pub source_hash: String,
    pub last_modified: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeRequirements {
    pub runtime_version: String,
    pub memory_limit: String,
    pub cpu_limit: Option<f32>,
    pub timeout: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDefinition {
    pub name: String,
    pub port_type: PortType,
    pub required: bool,
    pub description: String,
    pub default: Option<serde_json::Value>,
}
```

## Phase 2: Script Discovery System

### Step 2.1: Create Discovery Service

```rust
// crates/reflow_network/src/script_discovery/discovery.rs

use glob::glob;
use std::path::{Path, PathBuf};
use anyhow::Result;

pub struct ScriptActorDiscovery {
    config: ScriptDiscoveryConfig,
    metadata_extractor: MetadataExtractor,
}

impl ScriptActorDiscovery {
    pub fn new(config: ScriptDiscoveryConfig) -> Self {
        Self {
            config,
            metadata_extractor: MetadataExtractor::new(),
        }
    }
    
    pub async fn discover_actors(&self) -> Result<Vec<DiscoveredScriptActor>> {
        let mut actors = Vec::new();
        
        // Find all actor files
        for pattern in &self.config.patterns {
            let full_pattern = self.config.root_path.join(pattern);
            
            for entry in glob(&full_pattern.to_string_lossy())? {
                if let Ok(path) = entry {
                    if let Ok(actor) = self.process_file(&path).await {
                        actors.push(actor);
                    }
                }
            }
        }
        
        Ok(actors)
    }
    
    async fn process_file(&self, path: &Path) -> Result<DiscoveredScriptActor> {
        let runtime = self.determine_runtime(path)?;
        let metadata = self.extract_metadata(path, runtime).await?;
        
        Ok(DiscoveredScriptActor {
            component: metadata.component,
            description: metadata.description,
            file_path: path.to_path_buf(),
            runtime,
            inports: metadata.inports,
            outports: metadata.outports,
            workspace_metadata: self.build_workspace_metadata(path, &metadata)?,
        })
    }
    
    fn determine_runtime(&self, path: &Path) -> Result<ScriptRuntime> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("py") => Ok(ScriptRuntime::Python),
            Some("js") | Some("mjs") => Ok(ScriptRuntime::JavaScript),
            _ => Err(anyhow::anyhow!("Unknown script type")),
        }
    }
}
```

### Step 2.2: Create Metadata Extractor

```rust
// crates/reflow_network/src/script_discovery/extractor.rs

pub struct MetadataExtractor;

impl MetadataExtractor {
    pub async fn extract_python_metadata(&self, file: &Path) -> Result<ActorMetadata> {
        // Execute Python extraction script
        let script = include_str!("../../python/extract_metadata.py");
        
        let output = Command::new("python3")
            .arg("-c")
            .arg(script)
            .arg(file)
            .output()
            .await?;
        
        let metadata: ActorMetadata = serde_json::from_slice(&output.stdout)?;
        Ok(metadata)
    }
    
    pub async fn extract_javascript_metadata(&self, file: &Path) -> Result<ActorMetadata> {
        // Execute JavaScript extraction script
        let script = include_str!("../../javascript/extract_metadata.js");
        
        let output = Command::new("node")
            .arg("-e")
            .arg(script)
            .arg(file)
            .output()
            .await?;
        
        let metadata: ActorMetadata = serde_json::from_slice(&output.stdout)?;
        Ok(metadata)
    }
}
```

### Step 2.3: Python Metadata Extraction Script

```python
# crates/reflow_network/python/extract_metadata.py

import ast
import json
import sys
from pathlib import Path

def extract_actor_metadata(file_path):
    """Extract metadata from Python actor file."""
    with open(file_path, 'r') as f:
        tree = ast.parse(f.read())
    
    for node in ast.walk(tree):
        if isinstance(node, ast.FunctionDef):
            for decorator in node.decorator_list:
                if isinstance(decorator, ast.Call):
                    if getattr(decorator.func, 'id', None) == 'actor':
                        return parse_actor_decorator(decorator, node)
    
    return None

def parse_actor_decorator(decorator, func_node):
    """Parse @actor decorator arguments."""
    metadata = {
        'component': func_node.name,
        'description': ast.get_docstring(func_node) or '',
        'inports': [],
        'outports': [],
        'version': '1.0.0',
        'dependencies': []
    }
    
    for keyword in decorator.keywords:
        if keyword.arg == 'name':
            metadata['component'] = ast.literal_eval(keyword.value)
        elif keyword.arg == 'inports':
            ports = ast.literal_eval(keyword.value)
            metadata['inports'] = [
                {'name': p, 'type': 'any', 'required': True} 
                for p in ports
            ]
        elif keyword.arg == 'outports':
            ports = ast.literal_eval(keyword.value)
            metadata['outports'] = [
                {'name': p, 'type': 'any', 'required': False} 
                for p in ports
            ]
    
    return metadata

if __name__ == '__main__':
    metadata = extract_actor_metadata(sys.argv[1])
    print(json.dumps(metadata))
```

## Phase 3: Component Registry

### Step 3.1: Create Unified Component Registry

```rust
// crates/reflow_network/src/component_registry.rs

use std::collections::HashMap;
use std::any::Any;
use async_trait::async_trait;

pub struct ComponentRegistry {
    native_actors: HashMap<String, Box<dyn Actor>>,
    wasm_actors: HashMap<String, WasmActorFactory>,
    script_actors: HashMap<String, ScriptActorFactory>,
    component_index: HashMap<String, ComponentType>,
}

pub enum ComponentType {
    Native,
    Wasm,
    Script(ScriptRuntime),
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            native_actors: HashMap::new(),
            wasm_actors: HashMap::new(),
            script_actors: HashMap::new(),
            component_index: HashMap::new(),
        }
    }
    
    pub fn register_script_actor(
        &mut self, 
        name: &str, 
        factory: ScriptActorFactory
    ) -> Result<()> {
        let runtime = factory.metadata.runtime.clone();
        self.script_actors.insert(name.to_string(), factory);
        self.component_index.insert(
            name.to_string(), 
            ComponentType::Script(runtime)
        );
        Ok(())
    }
    
    pub async fn create_instance(&self, name: &str) -> Result<Box<dyn Actor>> {
        match self.component_index.get(name) {
            Some(ComponentType::Native) => {
                Ok(self.native_actors.get(name)
                    .ok_or_else(|| anyhow::anyhow!("Not found"))?
                    .clone())
            }
            Some(ComponentType::Wasm) => {
                self.wasm_actors.get(name)
                    .ok_or_else(|| anyhow::anyhow!("Not found"))?
                    .create_instance()
                    .await
            }
            Some(ComponentType::Script(_)) => {
                self.script_actors.get(name)
                    .ok_or_else(|| anyhow::anyhow!("Not found"))?
                    .create_instance()
                    .await
            }
            None => Err(anyhow::anyhow!("Component not found: {}", name))
        }
    }
}
```

### Step 3.2: Create Script Actor Factory

```rust
// crates/reflow_network/src/script_actor_factory.rs

pub struct ScriptActorFactory {
    metadata: DiscoveredScriptActor,
    websocket_url: String,
    redis_url: String,
}

#[async_trait]
impl ActorFactory for ScriptActorFactory {
    async fn create_instance(&self) -> Result<Box<dyn Actor>> {
        // Create WebSocket RPC client
        let rpc_client = WebSocketRpcClient::connect(&self.websocket_url).await?;
        
        // Create Redis state backend
        let redis_state = RedisActorState::new(
            &self.redis_url,
            &self.metadata.workspace_metadata.namespace,
            &self.metadata.component,
        ).await?;
        
        // Create script actor
        let actor = WebSocketScriptActor {
            metadata: self.metadata.clone(),
            rpc_client,
            redis_state,
        };
        
        Ok(Box::new(actor))
    }
}
```

## Phase 4: WebSocket RPC

### Step 4.1: Create WebSocket RPC Client

```rust
// crates/reflow_network/src/websocket_rpc/client.rs

use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio::sync::{Mutex, oneshot};
use std::collections::HashMap;
use uuid::Uuid;

pub struct WebSocketRpcClient {
    ws: Arc<Mutex<WebSocketStream<MaybeTlsStream<TcpStream>>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
}

impl WebSocketRpcClient {
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws_stream, _) = connect_async(url).await?;
        
        let client = Self {
            ws: Arc::new(Mutex::new(ws_stream)),
            pending: Arc::new(Mutex::new(HashMap::new())),
        };
        
        // Start message handler
        client.start_handler();
        
        Ok(client)
    }
    
    pub async fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = Uuid::new_v4().to_string();
        
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        
        // Create response channel
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        
        // Send message
        let msg = Message::text(message.to_string());
        self.ws.lock().await.send(msg).await?;
        
        // Wait for response
        let response = rx.await?;
        Ok(response)
    }
    
    fn start_handler(&self) {
        let ws = self.ws.clone();
        let pending = self.pending.clone();
        
        tokio::spawn(async move {
            while let Some(msg) = ws.lock().await.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Ok(response) = serde_json::from_str::<Value>(&text) {
                        if let Some(id) = response.get("id").and_then(|v| v.as_str()) {
                            if let Some(tx) = pending.lock().await.remove(id) {
                                let _ = tx.send(response.get("result").cloned()
                                    .unwrap_or(Value::Null));
                            }
                        }
                    }
                }
            }
        });
    }
}
```

### Step 4.2: Create WebSocket Script Actor

```rust
// crates/reflow_network/src/websocket_script_actor.rs

pub struct WebSocketScriptActor {
    metadata: DiscoveredScriptActor,
    rpc_client: WebSocketRpcClient,
    redis_state: RedisActorState,
}

#[async_trait]
impl Actor for WebSocketScriptActor {
    async fn process(&mut self, message: Message) -> Result<()> {
        // Create context for script
        let context = json!({
            "payload": self.convert_message_to_json(message),
            "config": self.metadata.config_schema,
            "state": {
                "namespace": self.redis_state.namespace,
                "actor_id": self.redis_state.actor_id,
            }
        });
        
        // Call script via RPC
        let result = self.rpc_client.call("process", context).await?;
        
        // Process output messages
        if let Some(outputs) = result.as_object() {
            for (port, value) in outputs {
                let message = self.convert_json_to_message(value)?;
                self.send_output(port, message).await?;
            }
        }
        
        Ok(())
    }
    
    fn convert_message_to_json(&self, message: Message) -> Value {
        // Convert Reflow Message to JSON
        match message {
            Message::Integer(i) => json!({"type": "integer", "value": i}),
            Message::String(s) => json!({"type": "string", "value": s}),
            Message::Array(a) => json!({"type": "array", "value": a}),
            Message::Object(o) => json!({"type": "object", "value": o}),
            _ => json!({"type": "any", "value": null}),
        }
    }
}
```

## Phase 5: Redis State Backend

### Step 5.1: Create Redis Actor State

```rust
// crates/reflow_network/src/redis_state/actor_state.rs

use redis::aio::Connection;
use redis::AsyncCommands;

pub struct RedisActorState {
    client: redis::Client,
    namespace: String,
    actor_id: String,
}

impl RedisActorState {
    pub async fn new(url: &str, namespace: &str, actor_id: &str) -> Result<Self> {
        let client = redis::Client::open(url)?;
        
        Ok(Self {
            client,
            namespace: namespace.to_string(),
            actor_id: actor_id.to_string(),
        })
    }
    
    fn make_key(&self, key: &str) -> String {
        format!("{}:{}:{}", self.namespace, self.actor_id, key)
    }
}

#[async_trait]
impl ActorState for RedisActorState {
    async fn get(&self, key: &str) -> Result<Option<Value>> {
        let mut conn = self.client.get_async_connection().await?;
        let redis_key = self.make_key(key);
        
        let value: Option<String> = conn.get(redis_key).await?;
        Ok(value.and_then(|v| serde_json::from_str(&v).ok()))
    }
    
    async fn set(&mut self, key: &str, value: Value) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let redis_key = self.make_key(key);
        
        conn.set(redis_key, serde_json::to_string(&value)?).await?;
        Ok(())
    }
    
    async fn increment(&mut self, key: &str, amount: i64) -> Result<i64> {
        let mut conn = self.client.get_async_connection().await?;
        let redis_key = self.make_key(key);
        
        Ok(conn.incr(redis_key, amount).await?)
    }
    
    async fn push(&mut self, key: &str, value: Value) -> Result<()> {
        let mut conn = self.client.get_async_connection().await?;
        let redis_key = self.make_key(key);
        
        conn.rpush(redis_key, serde_json::to_string(&value)?).await?;
        Ok(())
    }
    
    async fn pop(&mut self, key: &str) -> Result<Option<Value>> {
        let mut conn = self.client.get_async_connection().await?;
        let redis_key = self.make_key(key);
        
        let value: Option<String> = conn.lpop(redis_key).await?;
        Ok(value.and_then(|v| serde_json::from_str(&v).ok()))
    }
}
```

## Phase 6: MicroSandbox Integration

### Step 6.1: Sandbox Manager (Placeholder)

```rust
// crates/reflow_network/src/microsandbox/manager.rs

// This will be replaced with actual MicroSandbox when available
pub struct SandboxManager {
    python_cmd: String,
    js_cmd: String,
}

impl SandboxManager {
    pub fn new() -> Self {
        Self {
            python_cmd: "python3".to_string(),
            js_cmd: "node".to_string(),
        }
    }
    
    pub async fn execute_python(&self, script: &str, args: Vec<String>) -> Result<String> {
        let output = Command::new(&self.python_cmd)
            .arg("-c")
            .arg(script)
            .args(args)
            .output()
            .await?;
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
    
    pub async fn execute_javascript(&self, script: &str, args: Vec<String>) -> Result<String> {
        let output = Command::new(&self.js_cmd)
            .arg("-e")
            .arg(script)
            .args(args)
            .output()
            .await?;
        
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

// Future MicroSandbox integration
/*
use microsandbox::{Sandbox, SandboxConfig};

pub struct MicroSandboxManager {
    sandboxes: HashMap<String, Arc<Sandbox>>,
}

impl MicroSandboxManager {
    pub async fn get_or_create(&mut self, runtime: ScriptRuntime) -> Result<Arc<Sandbox>> {
        let config = match runtime {
            ScriptRuntime::Python => SandboxConfig {
                image: "python:3.11",
                memory: "512MB",
                ..Default::default()
            },
            ScriptRuntime::JavaScript => SandboxConfig {
                image: "node:20",
                memory: "256MB",
                ..Default::default()
            },
        };
        
        let sandbox = Arc::new(Sandbox::new(config).await?);
        Ok(sandbox)
    }
}
*/
```

## Phase 7: Testing

### Step 7.1: Unit Tests

```rust
// crates/reflow_network/src/script_discovery/tests.rs

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    
    #[tokio::test]
    async fn test_discover_python_actor() {
        let temp_dir = TempDir::new().unwrap();
        let actor_file = temp_dir.path().join("test.actor.py");
        
        fs::write(&actor_file, r#"
from reflow import actor, ActorContext, Message

@actor(
    name="TestActor",
    inports=["input"],
    outports=["output"]
)
async def process(context: ActorContext):
    '''Test actor for unit tests'''
    return {"output": Message.string("test")}
"#).unwrap();
        
        let config = ScriptDiscoveryConfig {
            root_path: temp_dir.path().to_path_buf(),
            patterns: vec!["**/*.actor.py".to_string()],
            ..Default::default()
        };
        
        let discovery = ScriptActorDiscovery::new(config);
        let actors = discovery.discover_actors().await.unwrap();
        
        assert_eq!(actors.len(), 1);
        assert_eq!(actors[0].component, "TestActor");
        assert_eq!(actors[0].runtime, ScriptRuntime::Python);
    }
    
    #[tokio::test]
    async fn test_websocket_rpc() {
        // Start mock WebSocket server
        let server = MockWebSocketServer::start(8080).await;
        
        let client = WebSocketRpcClient::connect("ws://localhost:8080").await.unwrap();
        
        let result = client.call("test", json!({"foo": "bar"})).await.unwrap();
        assert_eq!(result["status"], "ok");
    }
    
    #[tokio::test]
    async fn test_redis_state() {
        let state = RedisActorState::new(
            "redis://localhost:6379",
            "test",
            "actor1"
        ).await.unwrap();
        
        // Test set/get
        state.set("key1", json!({"value": 42})).await.unwrap();
        let value = state.get("key1").await.unwrap();
        assert_eq!(value, Some(json!({"value": 42})));
        
        // Test increment
        let count = state.increment("counter", 1).await.unwrap();
        assert_eq!(count, 1);
        
        let count = state.increment("counter", 5).await.unwrap();
        assert_eq!(count, 6);
    }
}
```

### Step 7.2: Integration Tests

```rust
// crates/reflow_network/tests/script_actor_integration.rs

#[tokio::test]
async fn test_end_to_end_script_actor() {
    // Start Redis
    let _redis = start_redis_container().await;
    
    // Create Python actor file
    let actor_code = r#"
from reflow import actor, ActorContext, Message

@actor(name="Adder", inports=["a", "b"], outports=["sum"])
async def add_numbers(context):
    a = context.payload.get("a").unwrap()
    b = context.payload.get("b").unwrap()
    return {"sum": Message.integer(a + b)}
"#;
    
    // Discover and register actor
    let discovery = ScriptActorDiscovery::new(config);
    let actors = discovery.discover_actors().await.unwrap();
    
    let mut registry = ComponentRegistry::new();
    for actor in actors {
        let factory = ScriptActorFactory::new(actor);
        registry.register_script_actor(&actor.component, factory).unwrap();
    }
    
    // Create and test actor instance
    let mut actor = registry.create_instance("Adder").await.unwrap();
    
    let input = Message::object(hashmap!{
        "a" => Message::integer(5),
        "b" => Message::integer(3),
    });
    
    actor.process(input).await.unwrap();
    
    // Verify output
    let output = actor.get_output("sum").await.unwrap();
    assert_eq!(output, Message::integer(8));
}
```

## Phase 8: Migration

### Step 8.1: Migration Guide

```markdown
# Migration from reflow_py/reflow_js to MicroSandbox

## For Python Actors

### Before (reflow_py):
```python
def process(inputs, state):
    data = inputs.get("data")
    count = state.get("count", 0)
    state["count"] = count + 1
    return {"output": data * 2}
```

### After (MicroSandbox):
```python
@actor(name="Processor", inports=["data"], outports=["output"])
async def process(context: ActorContext):
    data = context.payload.get("data")
    count = await context.state.increment("count")
    return {"output": Message.integer(data.unwrap() * 2)}
```

## For JavaScript Actors

### Before (reflow_js):
```javascript
function process(inputs, state) {
    const data = inputs.data;
    state.count = (state.count || 0) + 1;
    return { output: data * 2 };
}
```

### After (MicroSandbox):
```javascript
@actor({name: "Processor", inports: ["data"], outports: ["output"]})
async function process(context) {
    const data = context.payload.get("data");
    const count = await context.state.increment("count");
    return { output: Message.integer(data.unwrap() * 2) };
}
```
```

### Step 8.2: Compatibility Adapter

```rust
// crates/reflow_network/src/migration/adapter.rs

pub struct LegacyAdapter;

impl LegacyAdapter {
    pub async fn migrate_python_actor(old_path: &Path) -> Result<String> {
        let old_code = fs::read_to_string(old_path)?;
        
        // Parse old format
        let migrated = format!(r#"
from reflow import actor, ActorContext, Message

@actor(name="{name}", inports={inports}, outports={outports})
async def process(context: ActorContext):
    # Migrated from legacy format
    payload = context.get_payload()
    state = context.get_state()
    
{body}
"#, 
            name = extract_name(&old_code),
            inports = extract_inports(&old_code),
            outports = extract_outports(&old_code),
            body = migrate_body(&old_code)
        );
        
        Ok(migrated)
    }
}
```

## Testing Checklist

- [ ] Unit tests for discovery
- [ ] Unit tests for metadata extraction
- [ ] Unit tests for component registry
- [ ] WebSocket RPC tests
- [ ] Redis state tests
- [ ] Integration tests
- [ ] Performance benchmarks
- [ ] Migration tests
- [ ] Security tests

## Deployment

### Docker Compose Setup

```yaml
# docker-compose.yml
version: '3.8'

services:
  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
  
  websocket-rpc:
    build: ./websocket-rpc
    ports:
      - "8080:8080"
    environment:
      - REDIS_URL=redis://redis:6379
  
  reflow:
    build: .
    depends_on:
      - redis
      - websocket-rpc
    environment:
      - REDIS_URL=redis://redis:6379
      - WEBSOCKET_URL=ws://websocket-rpc:8080
    volumes:
      - ./workspace:/workspace
```

## Next Steps

1. Complete Phase 1-3 for basic functionality
2. Add WebSocket RPC for communication
3. Integrate Redis for state management
4. Add MicroSandbox when available
5. Write comprehensive tests
6. Create migration tools
7. Document and release