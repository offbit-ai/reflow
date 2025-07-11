# Observability Quick Start

Get Reflow's observability framework running in under 5 minutes. This guide will walk you through setting up tracing for a simple actor network and viewing the results.

## Prerequisites

- Rust 1.70 or later
- Basic familiarity with Reflow actors

## Step 1: Start the Tracing Server

First, start the reflow_tracing server:

```bash
# From the project root
cd examples/tracing_integration
./scripts/start_server.sh
```

This starts the tracing server on `ws://127.0.0.1:8080` with SQLite storage.

## Step 2: Create a Simple Traced Network

Create a new Rust project or add to an existing one:

```toml
# Cargo.toml
[dependencies]
reflow_network = { path = "../../crates/reflow_network" }
reflow_actor = { path = "../../crates/reflow_actor" }
reflow_tracing_protocol = { path = "../../crates/reflow_tracing_protocol" }
tokio = { version = "1.0", features = ["full"] }
anyhow = "1.0"
tracing = "0.1"
tracing-subscriber = "0.3"
```

Create a simple actor network with tracing enabled:

```rust
// src/main.rs
use anyhow::Result;
use reflow_network::{Network, NetworkConfig};
use reflow_network::tracing::{TracingConfig, init_global_tracing};
use reflow_actor::{Actor, ActorBehavior, ActorContext, Port, ActorLoad, ActorConfig};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use parking_lot::Mutex;

// Simple data processor actor
#[derive(Clone)]
struct DataProcessor {
    name: String,
}

impl DataProcessor {
    fn new(name: &str) -> Self {
        Self { name: name.to_string() }
    }
}

impl Actor for DataProcessor {
    fn get_behavior(&self) -> ActorBehavior {
        let name = self.name.clone();
        Box::new(move |context: ActorContext| {
            let actor_name = name.clone();
            Box::pin(async move {
                println!("🎬 {} processing messages", actor_name);
                
                let mut results = HashMap::new();
                for (port, message) in context.get_payload() {
                    println!("📨 {} received on {}: {:?}", actor_name, port, message);
                    
                    // Simulate processing
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    
                    let output = reflow_actor::message::Message::string(
                        format!("Processed by {}: {:?}", actor_name, message)
                    );
                    results.insert("output".to_string(), output);
                }
                
                Ok(results)
            })
        })
    }

    fn get_inports(&self) -> Port { flume::unbounded() }
    fn get_outports(&self) -> Port { flume::unbounded() }
    fn load_count(&self) -> Arc<Mutex<ActorLoad>> { 
        Arc::new(Mutex::new(ActorLoad::new(0))) 
    }
    
    fn create_process(&self, _config: ActorConfig) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
        Box::pin(async {})
    }
    
    fn shutdown(&self) {}
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();
    
    println!("🚀 Starting traced actor network");
    
    // Step 1: Configure tracing
    let tracing_config = TracingConfig {
        server_url: "ws://127.0.0.1:8080".to_string(),
        batch_size: 10,
        batch_timeout: Duration::from_millis(500),
        enable_compression: false,
        enabled: true,
        retry_config: reflow_network::tracing::RetryConfig {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_multiplier: 2.0,
        },
    };
    
    // Step 2: Initialize global tracing
    init_global_tracing(tracing_config.clone())?;
    println!("✅ Global tracing initialized");
    
    // Step 3: Create network with tracing enabled
    let network_config = NetworkConfig {
        tracing: tracing_config,
        ..Default::default()
    };
    
    let mut network = Network::new(network_config);
    println!("✅ Network created with tracing");
    
    // Step 4: Register actors
    let processor1 = DataProcessor::new("processor1");
    let processor2 = DataProcessor::new("processor2");
    
    network.register_actor("processor", processor1)?;
    network.register_actor("formatter", processor2)?;
    
    // Step 5: Add nodes to network
    network.add_node("proc1", "processor", None)?;
    network.add_node("proc2", "formatter", None)?;
    
    // Step 6: Start the network (automatic tracing begins here)
    network.start()?;
    println!("✅ Network started - tracing active");
    
    // Step 7: Send some messages (these will be automatically traced)
    println!("📨 Sending test messages...");
    
    for i in 1..=3 {
        let message = reflow_actor::message::Message::string(
            format!("Test message {}", i)
        );
        network.send_to_actor("proc1", "input", message)?;
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    
    // Step 8: Execute actors directly for more detailed tracing
    let result = network.execute_actor(
        "proc2",
        HashMap::from([
            ("input".to_string(), reflow_actor::message::Message::string("Direct execution test".to_string()))
        ])
    ).await?;
    
    println!("✅ Direct execution result: {:?}", result);
    
    // Step 9: Let the system run to generate traces
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    // Step 10: Manual tracing API demonstration
    if let Some(tracing) = reflow_network::tracing::global_tracing() {
        println!("🔍 Demonstrating manual tracing API...");
        
        tracing.trace_actor_created("manual_actor").await?;
        tracing.trace_message_sent(
            "manual_actor", 
            "output", 
            "ManualMessage", 
            256
        ).await?;
        
        println!("✅ Manual events recorded");
    }
    
    // Graceful shutdown
    println!("🛑 Shutting down...");
    network.shutdown();
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    println!("🎉 Quick start complete! Check the tracing server for events.");
    println!("💡 Next: Run the monitoring client to see live events:");
    println!("   cargo run --bin monitoring_client");
    
    Ok(())
}
```

## Step 3: Run Your Application

```bash
cargo run
```

You should see output like:

```
🚀 Starting traced actor network
✅ Global tracing initialized
✅ Network created with tracing
✅ Network started - tracing active
📨 Sending test messages...
🎬 processor1 processing messages
📨 processor1 received on input: String("Test message 1")
...
🎉 Quick start complete! Check the tracing server for events.
```

## Step 4: View Live Events

In another terminal, run the monitoring client:

```bash
cd examples/tracing_integration
cargo run --bin monitoring_client
```

You'll see real-time trace events:

```
🔍 Monitoring live trace events...
📊 Connected to tracing server at ws://127.0.0.1:8080

[2025-01-07T06:00:00Z] ActorCreated: processor1
[2025-01-07T06:00:00Z] ActorCreated: processor2  
[2025-01-07T06:00:00Z] MessageSent: processor1 -> output (String, 256 bytes)
[2025-01-07T06:00:00Z] DataFlow: processor1:output -> processor2:input (String, 256 bytes)
[2025-01-07T06:00:01Z] ActorCompleted: processor1
...
```

## What Just Happened?

Your simple actor network generated several types of trace events:

1. **Actor Creation**: When actors were instantiated
2. **Message Sending**: When messages were sent between actors
3. **Data Flow**: Automatic tracing of data flowing between connected actors
4. **Actor Completion**: When actors finished processing

All of this happened **automatically** - the tracing framework integrated seamlessly with your existing Reflow network.

## Exploring the Data

### SQLite Database
The trace data is stored in `examples/tracing_integration/data/traces.db`. You can explore it directly:

```bash
sqlite3 examples/tracing_integration/data/traces.db
.tables
SELECT * FROM trace_events LIMIT 5;
```

### Query API
Use the monitoring client with query options:

```bash
# Get last 10 events
cargo run --bin monitoring_client -- --query --limit 10

# Filter by actor
cargo run --bin monitoring_client -- --actor-ids processor1

# Filter by event type
cargo run --bin monitoring_client -- --event-types ActorCreated,MessageSent
```

## Next Steps

### 🎯 **Learn More About Event Types**
- Read about all [event types and their uses](event-types.md)
- Understand [data flow tracing capabilities](data-flow-tracing.md)

### ⚙️ **Customize Your Setup**
- Configure [different storage backends](storage-backends.md)
- Explore [configuration options](configuration.md)

### 🚀 **Production Deployment**
- Set up [production monitoring](deployment.md)
- Learn about [scaling and performance](../tutorials/advanced-tracing-setup.md)

### 🔧 **Integration**
- Build [custom monitoring dashboards](../api/tracing/client-integration.md)
- Integrate with [existing monitoring systems](../tutorials/advanced-tracing-setup.md)

## Troubleshooting

### Connection Issues
If the client can't connect to the tracing server:

```bash
# Check if server is running
curl -I http://127.0.0.1:8080
# or
telnet 127.0.0.1 8080
```

### No Events Appearing
- Ensure `enabled: true` in your `TracingConfig`
- Check that `init_global_tracing()` was called before network operations
- Verify the server URL is correct

### Performance Impact
For production systems, consider:
- Increasing `batch_size` to reduce network overhead
- Enabling compression with `enable_compression: true`
- Using PostgreSQL backend for better concurrent performance

Get help in our [troubleshooting guide](../reference/troubleshooting-guide.md) or check the [architecture documentation](architecture.md) for deeper understanding.
