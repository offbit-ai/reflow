# Testing MicroSandbox Integration

## Our Implementation vs CLI

### What Makes Our Implementation Unique:

1. **WebSocket RPC Server**: MicroSandbox doesn't provide this - we built it
2. **ActorContext Injection**: We wrap every script with our custom context
3. **Bidirectional Communication**: Scripts can send outputs back via RPC
4. **Reflow Integration**: Scripts become full Reflow actors

### Prerequisites

```bash
# 1. Install Docker (required by MicroSandbox SDK)
# macOS:
brew install docker

# Linux:
sudo apt-get install docker.io

# 2. Start Docker
sudo systemctl start docker  # Linux
open -a Docker  # macOS

# 3. Optional: Install MicroSandbox CLI for debugging
curl -sSL https://get.microsandbox.dev | sh
```

## Running Our Tests

### Unit Tests (No Docker Required)
```bash
# Test basic compilation and structure
cargo test --package reflow_network script_discovery::test_microsandbox
```

### Integration Tests (Requires Docker)
```bash
# Run full integration tests
cargo test --package reflow_network --test microsandbox_integration -- --ignored --nocapture

# Run specific test
cargo test --package reflow_network test_python_actor_with_microsandbox -- --ignored --nocapture
```

## Testing the Full Stack Manually

### 1. Start the MicroSandbox Runtime Server
```rust
// Create a binary in src/bin/microsandbox_server.rs
use reflow_network::script_discovery::microsandbox_runtime::run_microsandbox_runtime;

#[tokio::main]
async fn main() {
    // Start server with test scripts
    run_microsandbox_runtime(
        "127.0.0.1".to_string(),
        8765,
        vec![
            "examples/scripts/calculator.py".to_string(),
            "examples/scripts/processor.js".to_string(),
        ]
    ).await.unwrap();
}
```

```bash
cargo run --bin microsandbox_server
```

### 2. Connect with WebSocketScriptActor
```rust
// In another terminal/process
let rpc_client = Arc::new(WebSocketRpcClient::new("ws://127.0.0.1:8765"));
let actor = WebSocketScriptActor::new(metadata, rpc_client, redis_url).await;

// Send messages and receive outputs
let outputs = actor.process_message(inputs).await?;
```

## Debugging with MicroSandbox CLI (Optional)

The CLI can help debug sandbox issues, but it won't test our WebSocket RPC layer:

```bash
# Test if MicroSandbox works at all
microsandbox run python -c "print('Hello from sandbox')"

# Check sandbox status
microsandbox status

# List available sandboxes
microsandbox list
```

## What Our Tests Verify

### 1. Script Wrapping
- We inject ActorContext before script execution
- Scripts can call `context.send_output()` 

### 2. WebSocket RPC
- Bidirectional communication works
- Scripts can send multiple outputs
- Connection tracking routes messages correctly

### 3. Sandbox Isolation
- Scripts can't access filesystem (unless allowed)
- Resource limits are enforced
- Scripts run in isolated environments

### 4. Actor Integration
- Scripts work as Reflow actors
- State persistence via Redis
- Message passing through ports

## Example Test Script

Create `test_actor.py`:
```python
async def process(context):
    # This uses our injected ActorContext
    input_val = context.get_input("value")
    
    # Send intermediate output (our feature)
    await context.send_output("status", "processing")
    
    # Send final output
    await context.send_output("result", input_val * 2)
    
    return context.get_outputs()
```

Run through our system:
```bash
# This goes through our entire stack
cargo test test_python_actor_with_microsandbox -- --ignored --nocapture
```

## Monitoring

Watch the logs to see our implementation in action:
```bash
RUST_LOG=debug cargo test -- --ignored --nocapture
```

You'll see:
1. MicroSandbox creating sandboxes
2. Our WebSocket RPC server starting
3. Scripts being wrapped with ActorContext
4. RPC messages for output streaming
5. Sandbox cleanup

## Key Differences from CLI Usage

| Feature | CLI | Our Implementation |
|---------|-----|-------------------|
| Script Execution | ✓ | ✓ |
| Sandbox Isolation | ✓ | ✓ |
| WebSocket RPC | ✗ | ✓ |
| ActorContext | ✗ | ✓ |
| Async Outputs | ✗ | ✓ |
| Reflow Integration | ✗ | ✓ |
| Redis State | ✗ | ✓ |
| Message Routing | ✗ | ✓ |

Our implementation builds on top of MicroSandbox to provide a complete actor system for scripts.