# MicroSandbox Runtime Server

This crate provides a WebSocket RPC server that wraps the MicroSandbox SDK for secure script execution in Reflow.

## Architecture

```
Host Application
      ↓
WebSocket RPC (port 5556)
      ↓
MicroSandbox Runtime Server (this crate)
      ↓
MicroSandbox SDK
      ↓
MicroSandbox Server (Docker)
      ↓
Sandboxed Python/JavaScript
```

## Prerequisites

1. **MicroSandbox Server**: The MicroSandbox server must be running in Docker:
   ```bash
   docker-compose up microsandbox redis
   ```

2. **Environment**: The MicroSandbox server provides hardware-level VM isolation for secure code execution.

## Running the Server

### Standalone Mode
```bash
cargo run --package reflow_microsandbox_runtime -- --port 5556
```

### With Debug Logging
```bash
cargo run --package reflow_microsandbox_runtime -- --port 5556 --debug
```

### With Preloaded Actors
```bash
cargo run --package reflow_microsandbox_runtime -- --port 5556 --actors /path/to/actor.py
```

## WebSocket RPC Protocol

The server implements JSON-RPC 2.0 over WebSocket with the following methods:

### `list` - List Loaded Actors
```json
{
  "jsonrpc": "2.0",
  "method": "list",
  "params": {},
  "id": 1
}
```

### `load` - Load an Actor Script
```json
{
  "jsonrpc": "2.0",
  "method": "load",
  "params": {
    "file_path": "/path/to/actor.py"
  },
  "id": 2
}
```

### `process` - Execute an Actor
```json
{
  "jsonrpc": "2.0",
  "method": "process",
  "params": {
    "actor_id": "my_actor",
    "payload": {
      "input": 42
    },
    "config": {},
    "state": {},
    "timestamp": 1234567890
  },
  "id": 3
}
```

## Actor Scripts

Actors are Python or JavaScript scripts that implement the actor pattern. The runtime provides an `ActorContext` object that allows bidirectional communication via WebSocket RPC.

### Python Example
```python
import json

async def process(context):
    # Get input from a port
    value = context.get_input("input", 0)
    
    # Process the value
    result = value * 2
    
    # Send output to a port (via WebSocket RPC back to host)
    await context.send_output("output", result)
    
    return {"status": "completed"}
```

### JavaScript Example
```javascript
async function process(context) {
    // Get input from a port
    const value = context.getInput("input", 0);
    
    // Process the value
    const result = value * 2;
    
    // Send output to a port (via WebSocket RPC back to host)
    await context.sendOutput("output", result);
    
    return { status: "completed" };
}
```

## Testing

Run the integration tests with the server:

```bash
# Start the MicroSandbox server in Docker first
docker-compose up -d microsandbox redis

# Run the tests (they will start the runtime server automatically)
cargo test --package reflow_network microsandbox_integration_tests -- --ignored
```

## Security

- Scripts run in hardware-isolated VMs via MicroSandbox
- Network access is disabled by default in sandboxes
- File system access is restricted to allowed paths
- Resource limits are enforced (CPU, memory, execution time)