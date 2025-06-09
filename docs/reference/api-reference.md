# API Reference

Complete API reference for Reflow components and systems.

## Core APIs

### Graph API
- [Creating Graphs](../api/graph/creating-graphs.md) - Basic graph operations and management
- [Graph Analysis](../api/graph/analysis.md) - Validation and performance analysis
- [Graph Layout](../api/graph/layout.md) - Positioning and visualization
- [Advanced Features](../api/graph/advanced.md) - History, optimization, and extensions

### Actor API
- [Creating Actors](../api/actors/creating-actors.md) - Actor implementation and lifecycle
- [Message Passing](../architecture/message-passing.md) - Communication patterns
- [Actor Model](../architecture/actor-model.md) - Architectural concepts

### Network API
- [Network Management](../api/networking/network-management.md) - Network creation and control
- [Connection Handling](../api/networking/connections.md) - Connection management
- [Event System](../api/networking/events.md) - Network events and monitoring

### Messaging API  
- [Message Types](../api/messaging/message-types.md) - Supported message formats
- [Port Management](../api/messaging/ports.md) - Input/output port handling
- [Message Routing](../api/messaging/routing.md) - Message routing and delivery

## Runtime APIs

### JavaScript/Deno Runtime
- [Deno Runtime](../scripting/javascript/deno-runtime.md) - JavaScript execution environment
- [Module System](../scripting/javascript/modules.md) - Module loading and management
- [Permissions](../scripting/javascript/permissions.md) - Security and sandboxing

### Python Runtime
- [Python Runtime](../scripting/python/python-runtime.md) - Python execution environment
- [Package Management](../scripting/python/packages.md) - Python package handling
- [Virtual Environments](../scripting/python/environments.md) - Isolation and dependencies

### WebAssembly Runtime
- [WASM Runtime](../scripting/wasm/wasm-runtime.md) - WebAssembly execution
- [Module Loading](../scripting/wasm/modules.md) - WASM module management
- [Memory Management](../scripting/wasm/memory.md) - Memory allocation and cleanup

## Component APIs

### Standard Library
- [Component Library](../components/standard-library.md) - Built-in components
- [Custom Components](../components/custom-components.md) - Creating custom components
- [Component Lifecycle](../components/lifecycle.md) - Component management

### Data Operations
- [Data Transformation](../components/data-operations.md) - Data processing components
- [Validation](../components/validation.md) - Data validation components
- [Aggregation](../components/aggregation.md) - Data aggregation operations

### Flow Control
- [Conditional Logic](../components/flow-control.md) - Branching and conditions
- [Loops and Iteration](../components/loops.md) - Iterative processing
- [Error Handling](../components/error-handling.md) - Error management

## Configuration Reference

### Runtime Configuration
```toml
[runtime]
thread_pool_size = 8      # Number of worker threads
log_level = "info"        # Logging level: trace, debug, info, warn, error
hot_reload = false        # Enable hot reloading in development

[memory]
max_heap_size = "1GB"     # Maximum heap size
gc_frequency = 100        # Garbage collection frequency
```

### Network Configuration
```toml
[network]
max_connections = 1000    # Maximum concurrent connections
timeout_ms = 5000        # Connection timeout in milliseconds
buffer_size = 8192       # Message buffer size

[websocket]
enable = true            # Enable WebSocket support
port = 8080             # WebSocket port
max_frame_size = 65536  # Maximum frame size
```

### Script Configuration
```toml
[scripts.deno]
enable = true
allow_net = false        # Network access permission
allow_read = true        # File read permission
allow_write = false      # File write permission

[scripts.python]
enable = true
virtual_env = "venv"     # Virtual environment path
requirements = "requirements.txt"

[scripts.wasm]
enable = true
max_memory = "64MB"      # Maximum WASM memory
stack_size = "1MB"       # Stack size
```

## Error Codes

### Runtime Errors
- `E001` - Actor initialization failed
- `E002` - Message routing error
- `E003` - Network connection failed
- `E004` - Script execution error
- `E005` - Memory allocation failed

### Graph Errors
- `G001` - Invalid graph structure
- `G002` - Cycle detected in graph
- `G003` - Port type mismatch
- `G004` - Orphaned node detected
- `G005` - Invalid connection

### Component Errors
- `C001` - Component not found
- `C002` - Invalid component configuration
- `C003` - Component lifecycle error
- `C004` - Port compatibility error
- `C005` - Component execution timeout

## Type Definitions

### Core Types

```rust
// Graph types
pub struct Graph {
    pub name: String,
    pub directed: bool,
    pub metadata: HashMap<String, Value>,
}

pub struct GraphNode {
    pub id: String,
    pub component: String,
    pub metadata: HashMap<String, Value>,
}

pub struct GraphConnection {
    pub from_node: String,
    pub from_port: String,
    pub to_node: String,
    pub to_port: String,
    pub metadata: Option<HashMap<String, Value>>,
}

// Message types
pub enum Message {
    Null,
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Array(Vec<Message>),
    Object(HashMap<String, Message>),
    Binary(Vec<u8>),
}

// Actor types
pub trait Actor: Send + Sync {
    fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError>;
    fn get_input_ports(&self) -> Vec<PortDefinition>;
    fn get_output_ports(&self) -> Vec<PortDefinition>;
}
```

### Configuration Types

```rust
pub struct RuntimeConfig {
    pub thread_pool_size: usize,
    pub log_level: String,
    pub hot_reload: bool,
}

pub struct NetworkConfig {
    pub max_connections: usize,
    pub timeout_ms: u64,
    pub buffer_size: usize,
}

pub struct ScriptConfig {
    pub runtime: ScriptRuntime,
    pub source: String,
    pub entry_point: String,
    pub permissions: ScriptPermissions,
}
```

## WebAssembly Exports

### Graph Management
```javascript
// Create and manage graphs
const graph = new Graph("MyGraph", true, {});
graph.addNode("node1", "Component", {});
graph.addConnection("node1", "out", "node2", "in", {});

// Graph analysis
const validation = graph.validate();
const cycles = graph.detectCycles();
const layout = graph.calculateLayout();
```

### Network Operations
```javascript
// Network management
const network = new Network();
network.addActor("processor", processorActor);
network.connect("source", "output", "processor", "input");
await network.start();
```

### Message Handling
```javascript
// Message creation and handling
const message = Message.fromJson({"key": "value"});
const result = await actor.process({"input": message});
```

## Environment Variables

### Runtime Environment
- `REFLOW_LOG_LEVEL` - Override logging level
- `REFLOW_THREAD_POOL_SIZE` - Override thread pool size
- `REFLOW_CONFIG_PATH` - Configuration file path

### Development Environment
- `REFLOW_DEV_MODE` - Enable development features
- `REFLOW_HOT_RELOAD` - Enable hot reloading
- `REFLOW_DEBUG_ACTORS` - Enable actor debugging

### Production Environment
- `REFLOW_PRODUCTION` - Enable production optimizations
- `REFLOW_METRICS_ENDPOINT` - Metrics collection endpoint
- `REFLOW_HEALTH_CHECK_PORT` - Health check port

## Performance Considerations

### Memory Management
- Use memory pooling for frequently allocated objects
- Configure appropriate garbage collection settings
- Monitor memory usage with built-in profiling tools

### Concurrency
- Balance thread pool size with available CPU cores
- Use async operations for I/O bound tasks
- Implement backpressure for high-throughput scenarios

### Optimization
- Enable compiler optimizations for production builds
- Use profile-guided optimization when available
- Monitor performance metrics and bottlenecks

## Next Steps

- [Troubleshooting Guide](troubleshooting-guide.md) - Common issues and solutions
- [Performance Optimization](../tutorials/performance-optimization.md) - Advanced optimization techniques
- [Extended APIs](../advanced/extending-reflow/custom-runtimes.md) - Creating custom extensions
