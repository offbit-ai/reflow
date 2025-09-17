# MicroSandbox Core Integration Analysis

## Current Architecture Issues

Our current implementation uses the `microsandbox` crate, which is just an HTTP client that talks to an external MicroSandbox server. This means:
- ❌ Network overhead for every script execution
- ❌ No zero-copy message passing possible
- ❌ Requires external MicroSandbox server running
- ❌ Messages are serialized/deserialized multiple times

## Proposed Architecture: Direct MicroVM Embedding

### 1. Use `microsandbox-core` Instead

```toml
[dependencies]
microsandbox-core = "0.2.6"
```

This gives us direct access to:
- MicroVM management
- OCI container orchestration
- Direct execution control

### 2. Zero-Copy Message Passing Strategy

#### Option A: Shared Memory with MicroVMs
```rust
// Allocate shared memory region
let shared_mem = SharedMemory::new(size)?;

// Map into MicroVM's address space
microvm.map_memory(shared_mem.as_ptr(), guest_addr)?;

// Scripts can directly read/write to shared memory
// No serialization needed for large data
```

#### Option B: Memory-Mapped Channels
```rust
// Create memory-mapped ring buffer
let (tx, rx) = mmap_channel::channel();

// Pass file descriptor to MicroVM
microvm.add_vsock_device(tx.as_fd())?;

// Zero-copy transfer of Messages
tx.send_zerocopy(message)?;
```

### 3. OCI Orchestration Considerations

#### Pros of Embedded OCI:
- ✅ Full control over container lifecycle
- ✅ Custom image support for different languages
- ✅ Resource isolation per actor
- ✅ Potential for pre-warmed pools

#### Cons:
- ⚠️ Increased complexity in Reflow
- ⚠️ Need to manage container images
- ⚠️ Resource overhead per actor

#### Mitigation Strategy:
```rust
// Pool of pre-warmed MicroVMs
struct MicroVMPool {
    python_vms: Vec<MicroVM>,
    js_vms: Vec<MicroVM>,
    // Reuse VMs instead of creating new ones
}

// Lightweight process isolation
struct LightweightSandbox {
    // Use process isolation for trusted scripts
    // Use MicroVMs only for untrusted code
}
```

## Implementation Plan

### Phase 1: Replace HTTP Client with Core
```rust
use microsandbox_core::management::{orchestra, menv};

pub struct EmbeddedScriptRuntime {
    // Direct MicroVM management
    vms: HashMap<ActorId, MicroVM>,
    
    // Shared memory regions for zero-copy
    shared_regions: HashMap<ActorId, SharedMemory>,
}

impl EmbeddedScriptRuntime {
    pub async fn execute_script(&mut self, script: &str) -> Result<Message> {
        // Direct execution in MicroVM
        let vm = self.get_or_create_vm()?;
        
        // Zero-copy input
        let input_region = self.shared_regions.get(&actor_id)?;
        input_region.write_message(input)?;
        
        // Execute
        vm.execute(script)?;
        
        // Zero-copy output
        let output = input_region.read_message()?;
        Ok(output)
    }
}
```

### Phase 2: Shared Memory Message Passing
```rust
// Define shared memory layout
#[repr(C)]
struct SharedMessageBuffer {
    // Header
    magic: u32,
    version: u32,
    flags: u32,
    
    // Message metadata
    message_type: u32,
    payload_size: u32,
    
    // Payload (zero-copy)
    payload: [u8; MAX_MESSAGE_SIZE],
}

// Map into both host and guest
impl Message {
    pub fn write_to_shared(&self, buffer: &mut SharedMessageBuffer) {
        // Direct memory write, no serialization
        match self {
            Message::Stream(data) => {
                // Zero-copy for binary data
                unsafe {
                    ptr::copy_nonoverlapping(
                        data.as_ptr(),
                        buffer.payload.as_mut_ptr(),
                        data.len()
                    );
                }
            }
            _ => {
                // Other message types
            }
        }
    }
}
```

### Phase 3: Integration with Reflow Actors

```rust
impl Actor for EmbeddedScriptActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(move |context: ActorContext| {
            // Get shared memory region for this actor
            let shared_mem = self.get_shared_memory(context.actor_id)?;
            
            // Write input directly to shared memory
            shared_mem.write_inputs(&context.payload)?;
            
            // Signal MicroVM to process
            self.vm.signal_execute()?;
            
            // Read output from shared memory (zero-copy)
            let outputs = shared_mem.read_outputs()?;
            
            Ok(outputs)
        })
    }
}
```

## Trade-offs Analysis

### Keep Current Implementation (HTTP Client)
**When to use:**
- Scripts are untrusted and need strong isolation
- Scaling across multiple machines
- Simple deployment model

**Performance:**
- ~1-10ms overhead per message
- Serialization cost

### Switch to Embedded Core
**When to use:**
- Performance critical applications
- Need zero-copy for large data
- Want full control over execution

**Performance:**
- ~100μs overhead per message
- No serialization for binary data

### Hybrid Approach
```rust
enum ScriptExecutor {
    // For trusted, performance-critical scripts
    Embedded(EmbeddedScriptRuntime),
    
    // For untrusted or remote scripts
    Remote(WebSocketRpcClient),
    
    // For simple scripts
    WasmRuntime(WasmRuntime),
}
```

## Decision Matrix

| Factor | HTTP Client | Embedded Core | Hybrid |
|--------|------------|---------------|---------|
| Zero-copy | ❌ | ✅ | ✅ |
| Complexity | Low | High | Medium |
| Security | ✅ (external) | ⚠️ (in-process) | ✅ |
| Performance | Medium | High | High |
| Resource Usage | Low | High | Medium |
| Deployment | Complex | Simple | Medium |

## Recommendation

1. **Short term**: Keep current WebSocket RPC implementation for MVP
2. **Medium term**: Add embedded option for performance-critical actors
3. **Long term**: Implement hybrid approach with intelligent routing

## Next Steps

If we proceed with embedded integration:

1. Add `microsandbox-core` dependency
2. Implement `EmbeddedScriptRuntime` with MicroVM pool
3. Design shared memory message format
4. Implement zero-copy message passing
5. Benchmark vs current implementation
6. Add configuration to choose execution mode

The key question: **Is the performance gain worth the added complexity?**

For actors processing large binary data (images, videos), zero-copy could provide 10-100x speedup.
For simple JSON messages, the current approach might be sufficient.