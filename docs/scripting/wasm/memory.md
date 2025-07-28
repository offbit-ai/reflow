# Memory Management

This document covers memory management in Reflow's WebAssembly runtime, including allocation strategies, limits, and best practices.

## Memory Model

WebAssembly uses a linear memory model where memory is a contiguous, byte-addressable array:

```
┌─────────────────────────────────────┐
│          WASM Linear Memory         │
├─────────────────────────────────────┤
│ Stack (Function calls, locals)      │
├─────────────────────────────────────┤
│ Heap (Dynamic allocations)          │
├─────────────────────────────────────┤
│ Data Section (Constants, strings)   │
└─────────────────────────────────────┘
```

## Memory Limits

### Default Limits

```rust
// Default memory limits in ExtismEngine
const DEFAULT_MEMORY_LIMIT: usize = 16 * 1024 * 1024;  // 16MB
const MAX_MEMORY_LIMIT: usize = 100 * 1024 * 1024;     // 100MB
```

### Configuring Limits

```rust
// Set custom memory limits
let manifest = Manifest::default()
    .with_wasm(wasm)
    .with_memory_limit(32 * 1024 * 1024);  // 32MB limit
```

### Per-Actor Configuration

```json
{
  "component": "MemoryIntensiveActor",
  "metadata": {
    "memory_limit_mb": 64
  }
}
```

## Memory Allocation

### Plugin-Side Allocation

In your WASM plugin, memory is managed by the language runtime:

```rust
// Rust automatically manages memory
let mut data = Vec::with_capacity(1000);
data.push(42);  // Automatic allocation

// Manual allocation (rarely needed)
use std::alloc::{alloc, dealloc, Layout};

unsafe {
    let layout = Layout::array::<u8>(1024).unwrap();
    let ptr = alloc(layout);
    // Use memory...
    dealloc(ptr, layout);
}
```

### Host-Side Allocation

The host manages memory for plugin communication:

```rust
// Extism handles serialization/deserialization
let input = Json(actor_context);
let result = plugin.call::<Json<_>, Json<_>>("process", input)?;
```

## Memory Efficiency

### 1. Minimize Allocations

```rust
// Bad: Creates new string each time
fn process_bad(input: &str) -> String {
    format!("Processed: {}", input)  // Allocation
}

// Good: Reuse buffer
fn process_good(input: &str, buffer: &mut String) {
    buffer.clear();
    buffer.push_str("Processed: ");
    buffer.push_str(input);
}
```

### 2. Use Stack When Possible

```rust
// Stack allocation (preferred)
let array: [u8; 1024] = [0; 1024];

// Heap allocation
let vec: Vec<u8> = vec![0; 1024];
```

### 3. Stream Large Data

```rust
// Instead of loading entire file
let contents = std::fs::read("large_file.txt")?;

// Stream in chunks
use std::io::{BufReader, BufRead};
let reader = BufReader::new(File::open("large_file.txt")?);
for line in reader.lines() {
    process_line(line?);
}
```

## State Management

### Efficient State Storage

```rust
use reflow_wasm::*;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct CompactState {
    // Use appropriate data types
    count: u32,        // Not i64 if u32 suffices
    flags: u8,         // Bit flags instead of multiple bools
    #[serde(skip_serializing_if = "Option::is_none")]
    optional_data: Option<String>,  // Skip null values
}

fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    // Deserialize only what's needed
    let count = context.state
        .get("count")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(0);
    
    // Process...
    
    // Update only changed values
    let mut state_update = serde_json::Map::new();
    if count_changed {
        state_update.insert("count".to_string(), count.into());
    }
    
    Ok(ActorResult {
        outputs,
        state: Some(serde_json::Value::Object(state_update)),
    })
}
```

## Memory Monitoring

### Plugin Memory Usage

```rust
// Monitor memory usage in plugin
#[cfg(target_arch = "wasm32")]
fn get_memory_usage() -> usize {
    core::arch::wasm32::memory_size(0) * 65536  // Pages to bytes
}

fn process_actor(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    #[cfg(target_arch = "wasm32")]
    {
        let mem_before = get_memory_usage();
        // Process...
        let mem_after = get_memory_usage();
        eprintln!("Memory delta: {} bytes", mem_after - mem_before);
    }
    
    // Rest of processing...
}
```

### Host Memory Monitoring

```rust
// Monitor from host side
use sysinfo::{System, SystemExt, ProcessExt};

fn monitor_plugin_memory() {
    let mut system = System::new_all();
    system.refresh_all();
    
    if let Some(process) = system.processes_by_name("reflow").next() {
        println!("Memory usage: {} MB", process.memory() / 1024 / 1024);
    }
}
```

## Memory Leaks Prevention

### Common Leak Sources

1. **Circular References**
   ```rust
   // Avoid circular Rc references
   use std::rc::{Rc, Weak};
   
   struct Node {
       next: Option<Rc<Node>>,     // Can cause leaks
       prev: Option<Weak<Node>>,   // Use Weak for back references
   }
   ```

2. **Forgotten Handlers**
   ```rust
   // Remember to clean up
   struct Actor {
       handlers: Vec<Box<dyn Fn()>>,
   }
   
   impl Drop for Actor {
       fn drop(&mut self) {
           self.handlers.clear();  // Explicit cleanup
       }
   }
   ```

3. **Growing Collections**
   ```rust
   // Limit collection sizes
   const MAX_CACHE_SIZE: usize = 1000;
   
   struct Cache {
       items: HashMap<String, Value>,
   }
   
   impl Cache {
       fn insert(&mut self, key: String, value: Value) {
           if self.items.len() >= MAX_CACHE_SIZE {
               // Remove oldest or implement LRU
               self.evict_oldest();
           }
           self.items.insert(key, value);
       }
   }
   ```

## Binary Data Handling

### Efficient Binary Operations

```rust
use reflow_wasm::*;

fn process_binary(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    let mut outputs = HashMap::new();
    
    if let Some(Message::Stream(data)) = context.payload.get("binary_input") {
        // Process without copying when possible
        let processed = process_in_place(data.as_ref());
        
        // Only allocate for output
        outputs.insert("output".to_string(), 
            Message::Stream(Arc::new(processed)));
    }
    
    Ok(ActorResult { outputs, state: None })
}

fn process_in_place(data: &[u8]) -> Vec<u8> {
    // Example: Simple transformation
    data.iter().map(|&b| b.wrapping_add(1)).collect()
}
```

## Best Practices

### 1. Profile Before Optimizing

```rust
// Use conditional compilation for profiling
#[cfg(feature = "profiling")]
fn measure_allocation<F, R>(name: &str, f: F) -> R 
where F: FnOnce() -> R 
{
    let before = get_memory_usage();
    let result = f();
    let after = get_memory_usage();
    eprintln!("{}: {} bytes", name, after - before);
    result
}
```

### 2. Use Appropriate Data Structures

```rust
// Choose the right collection
use std::collections::{HashMap, BTreeMap, Vec, VecDeque};

// Fast lookup, more memory
let mut hash_map: HashMap<String, Value> = HashMap::new();

// Sorted, less memory
let mut btree_map: BTreeMap<String, Value> = BTreeMap::new();

// Sequential access
let mut vec: Vec<Value> = Vec::new();

// Queue operations
let mut deque: VecDeque<Value> = VecDeque::new();
```

### 3. Batch Operations

```rust
// Instead of many small allocations
for item in items {
    results.push(process(item));
}

// Pre-allocate
let mut results = Vec::with_capacity(items.len());
for item in items {
    results.push(process(item));
}
```

## Memory Safety

### Bounds Checking

WebAssembly provides memory safety through:

1. **Automatic bounds checking** on all memory accesses
2. **No raw pointers** across plugin boundary
3. **Sandboxed execution** prevents access outside allocated memory

### Safe Practices

```rust
// Always validate input sizes
fn process_array(context: ActorContext) -> Result<ActorResult, Box<dyn std::error::Error>> {
    const MAX_ARRAY_SIZE: usize = 10_000;
    
    if let Some(Message::Array(arr)) = context.payload.get("input") {
        if arr.len() > MAX_ARRAY_SIZE {
            return Err("Array too large".into());
        }
        
        // Safe to process
        let result = process_bounded_array(arr);
        // ...
    }
    
    Ok(ActorResult { outputs, state: None })
}
```

## Debugging Memory Issues

### Tools and Techniques

1. **WASM Memory Profiler**
   ```bash
   # Use wasm-instrument for profiling
   wasm-instrument --heap-profiling input.wasm -o profiled.wasm
   ```

2. **Debug Logging**
   ```rust
   #[cfg(debug_assertions)]
   macro_rules! debug_mem {
       ($msg:expr) => {
           eprintln!("[MEM] {} - {} bytes", $msg, get_memory_usage());
       };
   }
   ```

3. **Memory Dumps**
   ```rust
   fn dump_memory_stats() {
       #[cfg(target_arch = "wasm32")]
       {
           let pages = core::arch::wasm32::memory_size(0);
           let bytes = pages * 65536;
           eprintln!("Memory: {} pages ({} MB)", pages, bytes / 1024 / 1024);
       }
   }
   ```

## See Also

- [WASM Runtime](./wasm-runtime.md) - Overview of the runtime
- [Module Loading](./modules.md) - How modules are loaded
- [Performance Guide](../../guides/performance.md) - Performance optimization