# Troubleshooting Guide

Common issues and solutions when working with Reflow.

## Installation Issues

### Rust Compilation Errors

**Problem**: Build fails with compiler errors
```
error[E0432]: unresolved import `reflow_network::Graph`
```

**Solution**: 
1. Ensure you have the latest Rust version (1.70+)
2. Update dependencies: `cargo update`
3. Clean build cache: `cargo clean && cargo build`

**Problem**: Missing system dependencies
```
error: linking with `cc` failed: exit status: 1
```

**Solution**:
- **Linux**: Install build essentials: `sudo apt-get install build-essential`
- **macOS**: Install Xcode command line tools: `xcode-select --install`
- **Windows**: Install Visual Studio Build Tools

### WebAssembly Build Issues

**Problem**: wasm-pack fails to build
```
Error: failed to execute `wasm-pack build`: No such file or directory
```

**Solution**:
1. Install wasm-pack: `curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh`
2. Add wasm target: `rustup target add wasm32-unknown-unknown`
3. Verify installation: `wasm-pack --version`

## Runtime Issues

### Actor Initialization Failures

**Problem**: Actors fail to start
```
Error: Actor 'data_processor' failed to initialize: E001
```

**Solutions**:
1. Check actor configuration:
   ```rust
   // Verify all required ports are defined
   fn get_input_ports(&self) -> Vec<PortDefinition> {
       vec![
           PortDefinition::new("input", PortType::Any),
       ]
   }
   ```

2. Validate actor state:
   ```rust
   // Ensure actor is in valid initial state
   impl Actor for MyActor {
       fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
           if !self.initialized {
               return Err(ActorError::NotInitialized);
           }
           // ... processing logic
       }
   }
   ```

3. Check dependencies:
   ```rust
   // Verify all dependencies are available
   impl MyActor {
       pub fn new() -> Result<Self, ActorError> {
           let dependency = SomeDependency::connect()
               .map_err(|_| ActorError::DependencyUnavailable)?;
           
           Ok(Self { dependency, initialized: true })
       }
   }
   ```

### Message Routing Errors

**Problem**: Messages not reaching destination actors
```
Warning: Message dropped - no route to 'processor.input'
```

**Solutions**:
1. Verify connections:
   ```rust
   // Check connection exists
   network.connect("source", "output", "processor", "input").await?;
   
   // Verify actor and port names
   let actors = network.list_actors();
   println!("Available actors: {:?}", actors);
   ```

2. Check port compatibility:
   ```rust
   // Ensure port types match
   source_actor.get_output_ports(); // Returns Vec<PortDefinition>
   processor_actor.get_input_ports(); // Should have compatible types
   ```

3. Monitor message flow:
   ```rust
   network.enable_message_tracing(true);
   // Check logs for message routing information
   ```

### Memory Issues

**Problem**: Out of memory errors
```
Error: Memory allocation failed: E005
```

**Solutions**:
1. Configure memory limits:
   ```toml
   [memory]
   max_heap_size = "2GB"
   gc_frequency = 50
   enable_memory_pooling = true
   ```

2. Implement proper cleanup:
   ```rust
   impl Drop for MyActor {
       fn drop(&mut self) {
           // Clean up resources
           self.cleanup_connections();
           self.release_buffers();
       }
   }
   ```

3. Use memory profiling:
   ```rust
   use memory_stats::memory_stats;
   
   if let Some(usage) = memory_stats() {
       println!("Memory usage: {} bytes", usage.physical_mem);
   }
   ```

## Network Issues

### Connection Timeouts

**Problem**: Network operations timeout
```
Error: Connection timeout after 5000ms: E003
```

**Solutions**:
1. Increase timeout values:
   ```toml
   [network]
   timeout_ms = 30000  # Increase to 30 seconds
   ```

2. Implement retry logic:
   ```rust
   use reflow_components::utility::RetryActor;
   
   let retry_actor = RetryActor::new()
       .max_attempts(3)
       .backoff_strategy(BackoffStrategy::Exponential)
       .base_delay(Duration::from_millis(100));
   ```

3. Check network connectivity:
   ```bash
   # Test connectivity
   curl -I http://your-endpoint
   ping your-server
   ```

### WebSocket Connection Issues

**Problem**: WebSocket connections fail
```
Error: WebSocket connection failed: Connection refused
```

**Solutions**:
1. Verify server configuration:
   ```toml
   [websocket]
   enable = true
   port = 8080
   bind_address = "0.0.0.0"
   ```

2. Check firewall settings:
   ```bash
   # Linux
   sudo ufw allow 8080
   
   # macOS
   sudo pfctl -f /etc/pf.conf
   ```

3. Test WebSocket endpoint:
   ```javascript
   // Test in browser console
   const ws = new WebSocket('ws://localhost:8080');
   ws.onopen = () => console.log('Connected');
   ws.onerror = (error) => console.error('Error:', error);
   ```

## Script Runtime Issues

### Deno Permission Errors

**Problem**: Deno scripts fail due to permissions
```
Error: Requires read access to "./data", run again with --allow-read
```

**Solutions**:
1. Configure permissions:
   ```toml
   [scripts.deno]
   allow_read = true
   allow_net = false
   allow_write = false
   ```

2. Specify allowed paths:
   ```rust
   let config = ScriptConfig {
       runtime: ScriptRuntime::JavaScript,
       permissions: ScriptPermissions {
           allow_read: Some(vec!["./data".to_string(), "./config".to_string()]),
           allow_net: Some(vec!["api.example.com".to_string()]),
           allow_write: None,
       },
       ..Default::default()
   };
   ```

### Python Import Errors

**Problem**: Python modules not found
```
ModuleNotFoundError: No module named 'requests'
```

**Solutions**:
1. Install dependencies:
   ```bash
   pip install -r requirements.txt
   ```

2. Configure virtual environment:
   ```toml
   [scripts.python]
   virtual_env = "./venv"
   requirements = "requirements.txt"
   ```

3. Verify Python path:
   ```python
   import sys
   print(sys.path)
   ```

### WebAssembly Module Loading

**Problem**: WASM modules fail to load
```
Error: Invalid WASM module format
```

**Solutions**:
1. Verify WASM file:
   ```bash
   wasm-objdump -h module.wasm
   ```

2. Check module exports:
   ```bash
   wasm-objdump -x module.wasm | grep Export
   ```

3. Validate memory configuration:
   ```toml
   [scripts.wasm]
   max_memory = "64MB"
   stack_size = "1MB"
   ```

## Graph Issues

### Cycle Detection Errors

**Problem**: Graph contains cycles
```
Error: Cycle detected in graph: node1 -> node2 -> node3 -> node1
```

**Solutions**:
1. Analyze graph structure:
   ```rust
   let analysis = graph.analyze_structure();
   if analysis.has_cycles {
       println!("Cycles found: {:?}", analysis.cycles);
   }
   ```

2. Remove problematic connections:
   ```rust
   // Remove cycle-causing connection
   graph.remove_connection("node3", "output", "node1", "input")?;
   ```

3. Implement cycle breaking:
   ```rust
   let cycles = graph.detect_cycles();
   for cycle in cycles {
       // Break cycle by removing weakest connection
       let weakest_connection = find_weakest_connection(&cycle);
       graph.remove_connection_by_id(&weakest_connection.id)?;
   }
   ```

### Port Type Mismatches

**Problem**: Incompatible port types
```
Error: Port type mismatch: Cannot connect String output to Integer input
```

**Solutions**:
1. Add type conversion:
   ```rust
   use reflow_components::data_operations::ConverterActor;
   
   let converter = ConverterActor::new()
       .add_conversion(PortType::String, PortType::Integer, |value| {
           if let Message::String(s) = value {
               s.parse::<i64>().map(Message::Integer).ok()
           } else {
               None
           }
       });
   ```

2. Use flexible port types:
   ```rust
   PortDefinition::new("input", PortType::Any)
   ```

3. Implement custom validation:
   ```rust
   fn validate_connection(&self, output_type: &PortType, input_type: &PortType) -> bool {
       match (output_type, input_type) {
           (PortType::String, PortType::Integer) => true, // Allow with conversion
           (PortType::Any, _) => true,
           (_, PortType::Any) => true,
           (a, b) => a == b,
       }
   }
   ```

## Performance Issues

### High CPU Usage

**Problem**: Actors consuming excessive CPU
```
Warning: Actor 'data_processor' CPU usage: 95%
```

**Solutions**:
1. Profile actor performance:
   ```rust
   use std::time::Instant;
   
   impl Actor for MyActor {
       fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
           let start = Instant::now();
           
           // ... processing logic
           
           let duration = start.elapsed();
           if duration.as_millis() > 100 {
               log::warn!("Slow processing: {:?}", duration);
           }
           
           Ok(result)
       }
   }
   ```

2. Implement batching:
   ```rust
   use reflow_components::synchronization::BatchActor;
   
   let batcher = BatchActor::new()
       .batch_size(100)
       .timeout(Duration::from_millis(50));
   ```

3. Use async processing:
   ```rust
   async fn process_async(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
       // Use tokio::task::yield_now() to yield control
       tokio::task::yield_now().await;
       
       // CPU-intensive work
       let result = heavy_computation(inputs).await;
       
       Ok(result)
   }
   ```

### Memory Leaks

**Problem**: Memory usage continuously increases
```
Warning: Memory usage increased to 2.1GB (threshold: 2GB)
```

**Solutions**:
1. Monitor memory allocation:
   ```rust
   use reflow_network::profiling::MemoryProfiler;
   
   let profiler = MemoryProfiler::new();
   profiler.start_monitoring();
   
   // ... run workflows
   
   let report = profiler.generate_report();
   println!("Memory hotspots: {:?}", report.hotspots);
   ```

2. Implement proper cleanup:
   ```rust
   impl Actor for MyActor {
       fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
           // Process inputs
           let result = self.do_processing(inputs)?;
           
           // Clean up temporary data
           self.cleanup_temp_data();
           
           Ok(result)
       }
   }
   ```

3. Use memory limits:
   ```rust
   let config = ActorConfig {
       memory_limit: Some(100 * 1024 * 1024), // 100MB limit
       ..Default::default()
   };
   ```

## Debugging Tips

### Enable Debug Logging

```toml
[logging]
level = "debug"
targets = [
    "reflow_network=debug",
    "reflow_components=info",
    "my_app=trace"
]
```

### Use Network Introspection

```rust
// Enable network monitoring
network.enable_monitoring(true);

// Get network statistics
let stats = network.get_statistics();
println!("Messages processed: {}", stats.total_messages);
println!("Average latency: {:?}", stats.average_latency);

// List active actors
let actors = network.list_active_actors();
for (id, status) in actors {
    println!("Actor {}: {:?}", id, status);
}
```

### Actor State Inspection

```rust
// Enable actor introspection
actor.enable_introspection(true);

// Get actor state
let state = actor.get_internal_state();
println!("Actor state: {:?}", state);

// Monitor port activity
let port_stats = actor.get_port_statistics();
for (port, stats) in port_stats {
    println!("Port {}: {} messages", port, stats.message_count);
}
```

### Graph Visualization

```rust
// Export graph for visualization
let dot_format = graph.export_dot();
std::fs::write("graph.dot", dot_format)?;

// Generate SVG visualization
// Use graphviz: dot -Tsvg graph.dot -o graph.svg
```

## Common Error Patterns

### Error E001: Actor Initialization Failed
- Check actor dependencies
- Verify configuration parameters
- Ensure required resources are available

### Error E002: Message Routing Error
- Verify connection exists
- Check port names and types
- Ensure target actor is running

### Error E003: Network Connection Failed
- Check network connectivity
- Verify server endpoints
- Review firewall settings

### Error G002: Cycle Detected
- Analyze graph structure
- Remove cycle-causing connections
- Consider using async patterns

### Error C004: Port Compatibility Error
- Check port type definitions
- Add type conversion actors
- Use flexible port types

## Getting Help

1. **Check logs**: Enable debug logging for detailed information
2. **Use monitoring tools**: Enable network and actor monitoring
3. **Review configuration**: Verify all configuration parameters
4. **Test in isolation**: Create minimal test cases
5. **Community support**: Open issues on GitHub with detailed error information

## Diagnostic Tools

### Health Check Endpoint

```rust
// Add health check to your application
use warp::Filter;

let health = warp::path("health")
    .map(|| {
        let status = check_system_health();
        warp::reply::json(&status)
    });

warp::serve(health)
    .run(([127, 0, 0, 1], 8080))
    .await;
```

### Metrics Collection

```rust
use prometheus::{Counter, Histogram, register_counter, register_histogram};

let message_counter = register_counter!("reflow_messages_total", "Total messages processed").unwrap();
let processing_time = register_histogram!("reflow_processing_duration_seconds", "Processing time").unwrap();

// In actor processing
message_counter.inc();
let _timer = processing_time.start_timer();
```

### Performance Profiling

```bash
# CPU profiling
cargo install flamegraph
cargo flamegraph --bin reflow-app

# Memory profiling  
cargo install heaptrack
heaptrack target/release/reflow-app
```

For additional help, see:
- [API Reference](api-reference.md) - Complete API documentation
- [Architecture Overview](../architecture/overview.md) - System architecture
- [Performance Optimization](../tutorials/performance-optimization.md) - Optimization techniques
