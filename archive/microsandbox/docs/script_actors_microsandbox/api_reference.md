# Script Actor API Reference

## Table of Contents

1. [Python API](#python-api)
2. [JavaScript API](#javascript-api)
3. [Rust Integration API](#rust-integration-api)
4. [Configuration API](#configuration-api)
5. [Message Types](#message-types)
6. [State Operations](#state-operations)

## Python API

### @actor Decorator

```python
@actor(
    name: str = None,
    inports: List[str] = None,
    outports: List[str] = None,
    state: type = None,
    await_all_inports: bool = False,
    version: str = "1.0.0",
    description: str = "",
    tags: List[str] = None,
    category: str = None,
    config_schema: Dict = None,
    memory: str = "256MB",
    timeout: int = 30,
    state_backend: str = "redis"
)
```

#### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `name` | `str` | Function name | Actor component name |
| `inports` | `List[str]` | `[]` | Required input ports |
| `outports` | `List[str]` | `[]` | Expected output ports |
| `state` | `type` | `None` | State class for type hints |
| `await_all_inports` | `bool` | `False` | Wait for all inputs before processing |
| `version` | `str` | `"1.0.0"` | Actor version |
| `description` | `str` | Docstring | Actor description |
| `tags` | `List[str]` | `[]` | Categorization tags |
| `category` | `str` | `None` | Actor category |
| `config_schema` | `Dict` | `None` | JSON Schema for configuration |
| `memory` | `str` | `"256MB"` | Memory limit |
| `timeout` | `int` | `30` | Execution timeout in seconds |
| `state_backend` | `str` | `"redis"` | State storage backend |

#### Example

```python
from reflow import actor, ActorContext, Message
from typing import Dict
import numpy as np

@actor(
    name="DataProcessor",
    inports=["data", "config"],
    outports=["processed", "metrics", "error"],
    tags=["data", "processing"],
    memory="1GB",
    timeout=60
)
async def process_data(context: ActorContext) -> Dict[str, Message]:
    """
    Processes input data with configurable operations.
    
    Args:
        context: Actor execution context
        
    Returns:
        Dictionary of output messages
    """
    payload = context.get_payload()
    state = context.get_state()
    config = context.get_config()
    
    # Get input data
    data = payload.get("data")
    if not data:
        return {"error": Message.error("No data provided")}
    
    # Process data
    operation = config.get("operation", "mean")
    values = data.unwrap()
    
    if operation == "mean":
        result = np.mean(values)
    elif operation == "sum":
        result = np.sum(values)
    else:
        result = values
    
    # Update state
    count = await state.increment("process_count")
    
    # Return outputs
    return {
        "processed": Message.array([result]),
        "metrics": Message.object({
            "operation": operation,
            "count": count,
            "timestamp": context.timestamp()
        })
    }
```

### ActorContext Class

```python
class ActorContext:
    """Context provided to actor process function."""
    
    @property
    def payload(self) -> Dict[str, Message]:
        """Input messages keyed by port name."""
    
    @property
    def config(self) -> Dict[str, Any]:
        """Actor configuration."""
    
    @property
    def state(self) -> ActorState:
        """Persistent state backend."""
    
    def get_payload(self) -> Dict[str, Message]:
        """Get all input messages."""
    
    def get_config(self) -> Dict[str, Any]:
        """Get actor configuration."""
    
    def get_state(self) -> ActorState:
        """Get state backend."""
    
    def has_input(self, port: str) -> bool:
        """Check if input port has data."""
    
    def get_input(self, port: str) -> Optional[Message]:
        """Get message from specific port."""
    
    def timestamp(self) -> int:
        """Get current timestamp in milliseconds."""
    
    def actor_id(self) -> str:
        """Get unique actor instance ID."""
```

### Message Class

```python
class Message:
    """Reflow message type."""
    
    @staticmethod
    def integer(value: int) -> Message:
        """Create integer message."""
    
    @staticmethod
    def float(value: float) -> Message:
        """Create float message."""
    
    @staticmethod
    def string(value: str) -> Message:
        """Create string message."""
    
    @staticmethod
    def boolean(value: bool) -> Message:
        """Create boolean message."""
    
    @staticmethod
    def array(value: List[Any]) -> Message:
        """Create array message."""
    
    @staticmethod
    def object(value: Dict[str, Any]) -> Message:
        """Create object message."""
    
    @staticmethod
    def error(message: str, code: str = None) -> Message:
        """Create error message."""
    
    @staticmethod
    def stream(generator) -> Message:
        """Create stream message."""
    
    def unwrap(self) -> Any:
        """Extract raw value from message."""
    
    @property
    def type(self) -> str:
        """Get message type name."""
    
    def is_error(self) -> bool:
        """Check if message is an error."""
```

### ActorState Class

```python
class ActorState:
    """Persistent state interface."""
    
    async def get(self, key: str, default: Any = None) -> Any:
        """Get value from state."""
    
    async def set(self, key: str, value: Any) -> None:
        """Set value in state."""
    
    async def delete(self, key: str) -> bool:
        """Delete key from state."""
    
    async def increment(self, key: str, amount: int = 1) -> int:
        """Atomically increment counter."""
    
    async def decrement(self, key: str, amount: int = 1) -> int:
        """Atomically decrement counter."""
    
    async def push(self, key: str, value: Any) -> int:
        """Push value to list."""
    
    async def pop(self, key: str) -> Optional[Any]:
        """Pop value from list."""
    
    async def extend(self, key: str, values: List[Any]) -> int:
        """Extend list with multiple values."""
    
    async def expire(self, key: str, seconds: int) -> bool:
        """Set expiration time for key."""
    
    async def ttl(self, key: str) -> int:
        """Get remaining TTL in seconds."""
    
    async def exists(self, key: str) -> bool:
        """Check if key exists."""
    
    async def keys(self, pattern: str = "*") -> List[str]:
        """List keys matching pattern."""
```

## JavaScript API

### @actor Decorator

```javascript
@actor({
    name: string,
    inports: string[],
    outports: string[],
    state: class,
    awaitAllInports: boolean,
    version: string,
    description: string,
    tags: string[],
    category: string,
    configSchema: object,
    memory: string,
    timeout: number,
    stateBackend: string
})
```

#### Example

```javascript
const { actor, ActorContext, Message } = require('reflow');

@actor({
    name: "StreamProcessor",
    inports: ["stream", "config"],
    outports: ["processed", "stats"],
    tags: ["streaming", "analytics"],
    memory: "512MB",
    timeout: 30
})
async function processStream(context) {
    /**
     * Processes streaming data with configurable aggregation.
     */
    const payload = context.getPayload();
    const state = context.getState();
    const config = context.getConfig();
    
    // Get stream data
    const streamData = payload.get("stream");
    if (!streamData) {
        return { error: Message.error("No stream data") };
    }
    
    // Get or initialize buffer
    let buffer = await state.get("buffer", []);
    const windowSize = config.windowSize || 10;
    
    // Add to buffer
    buffer.push(streamData.unwrap());
    if (buffer.length > windowSize) {
        buffer = buffer.slice(-windowSize);
    }
    await state.set("buffer", buffer);
    
    // Calculate statistics
    const sum = buffer.reduce((a, b) => a + b, 0);
    const avg = sum / buffer.length;
    const max = Math.max(...buffer);
    const min = Math.min(...buffer);
    
    // Update counters
    const totalProcessed = await state.increment("total_processed");
    
    return {
        processed: Message.float(avg),
        stats: Message.object({
            average: avg,
            sum: sum,
            max: max,
            min: min,
            count: buffer.length,
            total: totalProcessed
        })
    };
}

module.exports = processStream;
```

### ActorContext Class (JavaScript)

```javascript
class ActorContext {
    /**
     * Get input payload
     * @returns {Map<string, Message>} Input messages
     */
    getPayload(): Map<string, Message>
    
    /**
     * Get actor configuration
     * @returns {object} Configuration object
     */
    getConfig(): object
    
    /**
     * Get state backend
     * @returns {ActorState} State interface
     */
    getState(): ActorState
    
    /**
     * Check if input port has data
     * @param {string} port - Port name
     * @returns {boolean} True if port has data
     */
    hasInput(port: string): boolean
    
    /**
     * Get message from specific port
     * @param {string} port - Port name
     * @returns {Message|null} Message or null
     */
    getInput(port: string): Message | null
    
    /**
     * Get current timestamp
     * @returns {number} Timestamp in milliseconds
     */
    timestamp(): number
    
    /**
     * Get actor instance ID
     * @returns {string} Unique actor ID
     */
    actorId(): string
}
```

### Message Class (JavaScript)

```javascript
class Message {
    /**
     * Create integer message
     * @param {number} value - Integer value
     * @returns {Message} Integer message
     */
    static integer(value: number): Message
    
    /**
     * Create float message
     * @param {number} value - Float value
     * @returns {Message} Float message
     */
    static float(value: number): Message
    
    /**
     * Create string message
     * @param {string} value - String value
     * @returns {Message} String message
     */
    static string(value: string): Message
    
    /**
     * Create boolean message
     * @param {boolean} value - Boolean value
     * @returns {Message} Boolean message
     */
    static boolean(value: boolean): Message
    
    /**
     * Create array message
     * @param {Array} value - Array value
     * @returns {Message} Array message
     */
    static array(value: any[]): Message
    
    /**
     * Create object message
     * @param {object} value - Object value
     * @returns {Message} Object message
     */
    static object(value: object): Message
    
    /**
     * Create error message
     * @param {string} message - Error message
     * @param {string} [code] - Error code
     * @returns {Message} Error message
     */
    static error(message: string, code?: string): Message
    
    /**
     * Extract raw value
     * @returns {any} Raw value
     */
    unwrap(): any
    
    /**
     * Get message type
     * @returns {string} Type name
     */
    get type(): string
    
    /**
     * Check if error
     * @returns {boolean} True if error
     */
    isError(): boolean
}
```

## Rust Integration API

### ScriptActorDiscovery

```rust
pub struct ScriptActorDiscovery {
    config: ScriptDiscoveryConfig,
}

impl ScriptActorDiscovery {
    /// Create new discovery instance
    pub fn new(config: ScriptDiscoveryConfig) -> Self
    
    /// Discover all script actors in workspace
    pub async fn discover_actors(&self) -> Result<Vec<DiscoveredScriptActor>>
    
    /// Validate discovered actors
    pub async fn validate_actors(&self, actors: &[DiscoveredScriptActor]) -> Result<()>
}

pub struct ScriptDiscoveryConfig {
    pub root_path: PathBuf,
    pub patterns: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub validate_metadata: bool,
    pub auto_register: bool,
}
```

### ComponentRegistry

```rust
pub struct ComponentRegistry {
    // Internal fields
}

impl ComponentRegistry {
    /// Register script actor factory
    pub fn register_script_actor(
        &mut self, 
        name: &str, 
        factory: ScriptActorFactory
    ) -> Result<()>
    
    /// Create actor instance
    pub async fn create_instance(&self, name: &str) -> Result<Box<dyn Actor>>
    
    /// Get all registered components
    pub fn list_components(&self) -> Vec<ComponentInfo>
    
    /// Check if component exists
    pub fn has_component(&self, name: &str) -> bool
}
```

### ScriptActorFactory

```rust
pub struct ScriptActorFactory {
    metadata: DiscoveredScriptActor,
    websocket_url: String,
    redis_url: String,
}

#[async_trait]
impl ActorFactory for ScriptActorFactory {
    /// Create new actor instance
    async fn create_instance(&self) -> Result<Box<dyn Actor>>
    
    /// Get actor metadata
    fn metadata(&self) -> &DiscoveredScriptActor
}
```

### WebSocketRpcClient

```rust
pub struct WebSocketRpcClient {
    // Internal fields
}

impl WebSocketRpcClient {
    /// Connect to WebSocket server
    pub async fn connect(url: &str) -> Result<Self>
    
    /// Make RPC call
    pub async fn call(&self, method: &str, params: Value) -> Result<Value>
    
    /// Close connection
    pub async fn close(&mut self) -> Result<()>
}
```

## Configuration API

### Workspace Configuration

```yaml
workspace:
  # Script discovery settings
  discover_scripts: boolean
  script_patterns: string[]
  excluded_paths: string[]
  
  # Sandbox configuration
  sandbox:
    python:
      image: string
      packages: string[]
      memory: string
      cpu: number
    javascript:
      image: string
      packages: string[]
      memory: string
      cpu: number
    pool:
      preload_count: number
      max_size: number
      idle_timeout: number
  
  # State backend
  redis:
    url: string
    namespace: string
    ttl: number
    max_connections: number
  
  # WebSocket settings
  websocket:
    port: number
    max_frame_size: string
    ping_interval: number
    compression: boolean
```

### Actor Configuration Schema

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "operation": {
      "type": "string",
      "enum": ["mean", "sum", "max", "min"],
      "default": "mean"
    },
    "windowSize": {
      "type": "integer",
      "minimum": 1,
      "maximum": 1000,
      "default": 10
    },
    "threshold": {
      "type": "number",
      "default": 0.5
    }
  }
}
```

## Message Types

### Type Definitions

| Type | Description | Example |
|------|-------------|---------|
| `Integer` | 64-bit integer | `42` |
| `Float` | 64-bit float | `3.14` |
| `String` | UTF-8 string | `"hello"` |
| `Boolean` | Boolean value | `true` |
| `Array` | Ordered list | `[1, 2, 3]` |
| `Object` | Key-value map | `{"key": "value"}` |
| `Stream` | Async stream | Generator |
| `Error` | Error with message | `{"error": "msg"}` |
| `Any` | Any type | Dynamic |
| `Option` | Optional value | `Some(T)` or `None` |

### Type Compatibility

```python
# Type checking in Python
if message.type == "integer":
    value = message.unwrap()
    # value is guaranteed to be int

# Type conversion
integer_msg = Message.integer(42)
float_msg = Message.float(3.14)
string_msg = Message.string(str(integer_msg.unwrap()))
```

## State Operations

### Basic Operations

| Operation | Description | Example |
|-----------|-------------|---------|
| `get(key)` | Get value | `await state.get("count")` |
| `set(key, value)` | Set value | `await state.set("count", 10)` |
| `delete(key)` | Delete key | `await state.delete("temp")` |
| `exists(key)` | Check existence | `await state.exists("count")` |

### Atomic Operations

| Operation | Description | Example |
|-----------|-------------|---------|
| `increment(key, n)` | Atomic increment | `await state.increment("counter", 1)` |
| `decrement(key, n)` | Atomic decrement | `await state.decrement("counter", 1)` |

### List Operations

| Operation | Description | Example |
|-----------|-------------|---------|
| `push(key, value)` | Append to list | `await state.push("queue", item)` |
| `pop(key)` | Remove from list | `await state.pop("queue")` |
| `extend(key, values)` | Append multiple | `await state.extend("queue", items)` |

### TTL Operations

| Operation | Description | Example |
|-----------|-------------|---------|
| `expire(key, seconds)` | Set expiration | `await state.expire("session", 3600)` |
| `ttl(key)` | Get remaining TTL | `await state.ttl("session")` |

## Error Handling

### Python Error Handling

```python
@actor(name="SafeProcessor", inports=["data"], outports=["result", "error"])
async def process_safely(context: ActorContext):
    try:
        data = context.payload.get("data")
        if not data:
            return {"error": Message.error("Missing data", "MISSING_INPUT")}
        
        result = process_data(data.unwrap())
        return {"result": Message.object(result)}
        
    except ValueError as e:
        return {"error": Message.error(str(e), "VALIDATION_ERROR")}
    except Exception as e:
        return {"error": Message.error(f"Unexpected error: {e}", "INTERNAL_ERROR")}
```

### JavaScript Error Handling

```javascript
@actor({name: "SafeProcessor", inports: ["data"], outports: ["result", "error"]})
async function processSafely(context) {
    try {
        const data = context.payload.get("data");
        if (!data) {
            return { error: Message.error("Missing data", "MISSING_INPUT") };
        }
        
        const result = await processData(data.unwrap());
        return { result: Message.object(result) };
        
    } catch (error) {
        if (error instanceof ValidationError) {
            return { error: Message.error(error.message, "VALIDATION_ERROR") };
        }
        return { error: Message.error(`Unexpected: ${error}`, "INTERNAL_ERROR") };
    }
}
```

## Best Practices

1. **Always validate inputs** before processing
2. **Use appropriate message types** for outputs
3. **Handle errors gracefully** with error ports
4. **Document actor behavior** in docstrings
5. **Set reasonable timeouts** for long operations
6. **Use state sparingly** for performance
7. **Test actors independently** before integration
8. **Version actors** for compatibility
9. **Use tags and categories** for organization
10. **Monitor resource usage** in production