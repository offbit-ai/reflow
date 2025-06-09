# Your First Workflow

This tutorial will guide you through creating and running your first Reflow workflow using the actual implementation patterns. We'll build a simple data processing pipeline that demonstrates the core concepts.

## Overview

We'll create a workflow that:
1. Processes input numbers (Sum Actor)
2. Squares the result (Square Actor) 
3. Validates the output (Assert Actor)

```
┌─────────┐    ┌─────────┐    ┌─────────┐
│   Sum   │───▶│ Square  │───▶│ Assert  │
│ Actor   │    │ Actor   │    │ Actor   │
└─────────┘    └─────────┘    └─────────┘
```

## Prerequisites

Before starting, make sure you have:
- Completed the [Installation](./installation.md) guide
- Set up your [Development Environment](./development-setup.md)
- Understanding of [Basic Concepts](./basic-concepts.md)

## Step 1: Create a New Project

```bash
# Create a new Rust project
cargo new hello-reflow
cd hello-reflow

# Add Reflow dependencies
cargo add reflow_network
cargo add actor_macro
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json anyhow
cargo add parking_lot
```

Your `Cargo.toml` should look like this:

```toml
[package]
name = "hello-reflow"
version = "0.1.0"
edition = "2021"

[dependencies]
reflow_network = { path = "../path/to/reflow/crates/reflow_network" }
actor_macro = { path = "../path/to/reflow/crates/actor_macro" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"
parking_lot = "0.12"
```

## Step 2: Create Your First Actors

Create `src/main.rs` with the correct actor patterns:

```rust
use std::collections::HashMap;
use reflow_network::{
    actor::{ActorContext, MemoryState},
    network::{Network, NetworkConfig},
    connector::{ConnectionPoint, Connector, InitialPacket},
    message::Message,
};
use actor_macro::actor;

// Sum Actor - adds two input numbers
#[actor(
    SumActor,
    inports::<100>(A, B),
    outports::<100>(Out),
    await_all_inports
)]
async fn sum_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();

    let a_msg = payload.get("A").expect("expected to get data from port A");
    let b_msg = payload.get("B").expect("expected to get data from port B");

    let a = match a_msg {
        Message::Integer(value) => *value,
        _ => 0,
    };

    let b = match b_msg {
        Message::Integer(value) => *value,
        _ => 0,
    };

    let result = a + b;
    println!("Sum Actor: {} + {} = {}", a, b, result);

    Ok([("Out".to_owned(), Message::Integer(result))].into())
}

// Square Actor - squares the input number
#[actor(
    SquareActor,
    inports::<100>(In),
    outports::<50>(Out)
)]
async fn square_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let message = payload.get("In").expect("expected input");
    
    let input = match message {
        Message::Integer(value) => *value,
        _ => 0,
    };

    let result = input * input;
    println!("Square Actor: {} squared = {}", input, result);

    Ok([("Out".to_owned(), Message::Integer(result))].into())
}

// Print Actor - displays the final result
#[actor(
    PrintActor,
    inports::<100>(Value),
    outports::<50>(Done)
)]
async fn print_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let message = payload.get("Value").expect("expected value");
    
    match message {
        Message::Integer(value) => {
            println!("🎉 Final Result: {}", value);
        },
        _ => {
            println!("📄 Final Result: {:?}", message);
        }
    }

    Ok([("Done".to_owned(), Message::Boolean(true))].into())
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("🚀 Starting Hello Reflow workflow...");
    
    // Create network with default configuration
    let mut network = Network::new(NetworkConfig::default());

    // Register actor types
    network.register_actor("sum_process", SumActor::new())?;
    network.register_actor("square_process", SquareActor::new())?;
    network.register_actor("print_process", PrintActor::new())?;

    // Add actor instances (nodes)
    network.add_node("sum", "sum_process")?;
    network.add_node("square", "square_process")?;
    network.add_node("print", "print_process")?;

    // Connect the workflow: sum -> square -> print
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "sum".to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "square".to_owned(),
            port: "In".to_owned(),
            ..Default::default()
        },
    });

    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "square".to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "print".to_owned(),
            port: "Value".to_owned(),
            ..Default::default()
        },
    });

    // Add initial data to start the workflow
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "sum".to_owned(),
            port: "A".to_owned(),
            initial_data: Some(Message::Integer(5)),
        },
    });

    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "sum".to_owned(),
            port: "B".to_owned(),
            initial_data: Some(Message::Integer(3)),
        },
    });

    // Start the network
    network.start().await?;

    // Give the workflow time to complete
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    println!("✅ Workflow completed!");
    
    Ok(())
}
```

## Step 3: Run the Workflow

```bash
cargo run
```

You should see output like:

```
🚀 Starting Hello Reflow workflow...
Sum Actor: 5 + 3 = 8
Square Actor: 8 squared = 64
🎉 Final Result: 64
✅ Workflow completed!
```

## Step 4: Understanding the Code

### Actor Macro Usage

The `#[actor]` macro simplifies actor creation:

```rust
#[actor(
    SumActor,                    // Generated struct name
    inports::<100>(A, B),        // Input ports with capacity
    outports::<100>(Out),        // Output ports with capacity
    await_all_inports            // Wait for all inputs before processing
)]
async fn sum_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error>
```

### Function Signature

All actor functions must have this exact signature:
- `async fn` - Asynchronous function
- `context: ActorContext` - Single parameter containing payload, state, config
- `Result<HashMap<String, Message>, anyhow::Error>` - Return type

### Network API Pattern

1. **Register** actor types: `network.register_actor("name", ActorStruct::new())`
2. **Add** node instances: `network.add_node("instance_id", "actor_type")`  
3. **Connect** with `Connector` structs
4. **Initialize** with `InitialPacket` structs

## Step 5: Add State Management

Let's create a stateful actor that counts operations:

```rust
// Counter Actor - keeps track of how many values it has processed
#[actor(
    CounterActor,
    state(MemoryState),
    inports::<100>(Value),
    outports::<50>(Count, Total)
)]
async fn counter_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    let input = payload.get("Value").expect("expected value");
    
    let value = match input {
        Message::Integer(n) => *n,
        _ => 0,
    };

    // Update state
    let (count, total) = {
        let mut state_guard = state.lock();
        let memory_state = state_guard
            .as_mut_any()
            .downcast_mut::<MemoryState>()
            .expect("Expected MemoryState");
        
        // Get current count and total
        let current_count = memory_state
            .get("count")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        
        let current_total = memory_state
            .get("total")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        
        // Update values
        let new_count = current_count + 1;
        let new_total = current_total + value;
        
        memory_state.insert("count", serde_json::json!(new_count));
        memory_state.insert("total", serde_json::json!(new_total));
        
        (new_count, new_total)
    };

    println!("Counter Actor: processed {} values, total sum: {}", count, total);

    Ok([
        ("Count".to_owned(), Message::Integer(count)),
        ("Total".to_owned(), Message::Integer(total)),
    ].into())
}
```

## Step 6: Multiple Input Example

Create an actor that waits for multiple inputs:

```rust
// Multiply Actor - multiplies two inputs
#[actor(
    MultiplyActor,
    inports::<100>(X, Y),
    outports::<50>(Result),
    await_all_inports  // This makes it wait for both X and Y
)]
async fn multiply_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();

    let x = match payload.get("X").expect("expected X") {
        Message::Integer(value) => *value,
        _ => 1,
    };

    let y = match payload.get("Y").expect("expected Y") {
        Message::Integer(value) => *value,
        _ => 1,
    };

    let result = x * y;
    println!("Multiply Actor: {} × {} = {}", x, y, result);

    Ok([("Result".to_owned(), Message::Integer(result))].into())
}
```

## Step 7: Complex Workflow Example

Here's a more complex workflow that demonstrates multiple patterns:

```rust
#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    println!("🚀 Starting Complex Reflow workflow...");
    
    let mut network = Network::new(NetworkConfig::default());

    // Register all actor types
    network.register_actor("sum_process", SumActor::new())?;
    network.register_actor("multiply_process", MultiplyActor::new())?;
    network.register_actor("counter_process", CounterActor::new())?;
    network.register_actor("print_process", PrintActor::new())?;

    // Create network topology
    network.add_node("sum1", "sum_process")?;
    network.add_node("multiply1", "multiply_process")?;
    network.add_node("counter1", "counter_process")?;
    network.add_node("print1", "print_process")?;

    // Connect workflow
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "sum1".to_owned(),
            port: "Out".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "multiply1".to_owned(),
            port: "X".to_owned(),
            ..Default::default()
        },
    });

    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "multiply1".to_owned(),
            port: "Result".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "counter1".to_owned(),
            port: "Value".to_owned(),
            ..Default::default()
        },
    });

    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "counter1".to_owned(),
            port: "Total".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "print1".to_owned(),
            port: "Value".to_owned(),
            ..Default::default()
        },
    });

    // Initial data
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "sum1".to_owned(),
            port: "A".to_owned(),
            initial_data: Some(Message::Integer(10)),
        },
    });

    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "sum1".to_owned(),
            port: "B".to_owned(),
            initial_data: Some(Message::Integer(5)),
        },
    });

    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "multiply1".to_owned(),
            port: "Y".to_owned(),
            initial_data: Some(Message::Integer(3)),
        },
    });

    // Start the network
    network.start().await?;
    
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    println!("✅ Complex workflow completed!");
    
    Ok(())
}
```

Expected output:
```
🚀 Starting Complex Reflow workflow...
Sum Actor: 10 + 5 = 15
Multiply Actor: 15 × 3 = 45
Counter Actor: processed 1 values, total sum: 45
🎉 Final Result: 45
✅ Complex workflow completed!
```

## Key Concepts Demonstrated

### Actor Macro Features
- **Port Definitions**: `inports::<capacity>(Port1, Port2)`
- **State Management**: `state(MemoryState)` for stateful actors
- **Input Synchronization**: `await_all_inports` waits for all inputs

### Network Configuration
- **Registration**: Register actor types before use
- **Instantiation**: Create specific instances with unique IDs
- **Connection**: Use structured `Connector` objects
- **Initialization**: Send initial data with `InitialPacket`

### Message Flow
- Messages flow through typed ports
- Actors process inputs and produce outputs
- State is maintained per actor instance

## Error Handling

Actors can return errors that will be logged:

```rust
#[actor(
    ValidatorActor,
    inports::<100>(Input),
    outports::<50>(Valid, Invalid)
)]
async fn validator_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let input = payload.get("Input").expect("expected input");
    
    match input {
        Message::Integer(n) if *n > 0 => {
            Ok([("Valid".to_owned(), input.clone())].into())
        },
        Message::Integer(n) if *n <= 0 => {
            Ok([("Invalid".to_owned(), input.clone())].into())
        },
        _ => {
            Err(anyhow::anyhow!("Expected integer input, got {:?}", input))
        }
    }
}
```

## Next Steps

Now that you understand the basic patterns:

1. **Learn more actor patterns**: [Creating Actors](../api/actors/creating-actors.md)
2. **Explore message types**: [Message Passing](../architecture/message-passing.md)
3. **Add scripting**: [JavaScript Integration](../scripting/javascript/deno-runtime.md)
4. **Use pre-built components**: [Standard Library](../components/standard-library.md)
5. **See more examples**: [Examples](../examples/README.md)

## Troubleshooting

### Common Issues

**Compilation errors with actor macro**: Make sure `actor_macro` is in your dependencies

**Port connection errors**: Verify port names match exactly between connections

**Runtime panics**: Check that initial data types match what actors expect

**Deadlocks**: Ensure `await_all_inports` actors receive all required inputs

For more help, see the [Troubleshooting Guide](../reference/troubleshooting-guide.md).

## Complete Example Code

The complete working examples are available in the [examples directory](../examples/tutorials/hello-reflow/).
