# Creating Actors

This guide covers how to create custom actors in Reflow using the correct implementation patterns. Learn everything from basic actors to advanced patterns with state management and error handling.

## Creating Actors: Two Approaches

Reflow provides two ways to create actors:

1. **Actor Macro** (Recommended): Use the `#[actor]` macro for simple, declarative actor creation
2. **Manual Implementation**: Implement the `Actor` trait directly for maximum control

## Using the Actor Macro

The `#[actor]` macro is the recommended way to create actors. It generates all the necessary boilerplate code including the Actor trait implementation, port management, and process creation.

### Basic Actor

```rust
use std::collections::HashMap;
use reflow_network::{
    actor::ActorContext,
    message::Message,
};
use actor_macro::actor;

#[actor(
    HelloActor,
    inports::<100>(input),
    outports::<50>(output)
)]
async fn hello_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    if let Some(Message::String(text)) = payload.get("input") {
        let response = format!("Hello, {}!", text);
        
        Ok([
            ("output".to_owned(), Message::string(response))
        ].into())
    } else {
        Err(anyhow::anyhow!("Expected string input"))
    }
}
```

### Actor with Multiple Inputs

```rust
#[actor(
    GreeterActor,
    inports::<100>(name, age),
    outports::<50>(greeting),
    await_all_inports  // Wait for both inputs before processing
)]
async fn greeter_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    let name = match payload.get("name").expect("expected name") {
        Message::String(s) => s,
        _ => return Err(anyhow::anyhow!("Name must be a string")),
    };
    
    let age = match payload.get("age").expect("expected age") {
        Message::Integer(n) => *n,
        _ => return Err(anyhow::anyhow!("Age must be an integer")),
    };
    
    let greeting = format!("Hello {}, you are {} years old!", name, age);
    
    Ok([
        ("greeting".to_owned(), Message::string(greeting))
    ].into())
}
```

### Stateful Actor

```rust
use reflow_network::actor::MemoryState;

#[actor(
    CounterActor,
    state(MemoryState),
    inports::<100>(increment, reset),
    outports::<50>(count, total)
)]
async fn counter_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    
    let mut state_guard = state.lock();
    let memory_state = state_guard
        .as_mut_any()
        .downcast_mut::<MemoryState>()
        .expect("Expected MemoryState");
    
    // Initialize state if needed
    if !memory_state.contains_key("count") {
        memory_state.insert("count", serde_json::json!(0));
        memory_state.insert("total", serde_json::json!(0));
    }
    
    let current_count = memory_state.get("count")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    let current_total = memory_state.get("total")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    
    let (new_count, new_total) = if payload.contains_key("reset") {
        // Reset counter
        (0, current_total)
    } else if let Some(Message::Integer(amount)) = payload.get("increment") {
        // Increment by specific amount
        let new_count = current_count + amount;
        (new_count, current_total + amount)
    } else {
        // Default increment by 1
        let new_count = current_count + 1;
        (new_count, current_total + 1)
    };
    
    // Update state
    memory_state.insert("count", serde_json::json!(new_count));
    memory_state.insert("total", serde_json::json!(new_total));
    
    println!("Counter: {} (total: {})", new_count, new_total);
    
    Ok([
        ("count".to_owned(), Message::Integer(new_count)),
        ("total".to_owned(), Message::Integer(new_total)),
    ].into())
}
```

## Actor Macro Parameters

### Port Definitions

```rust
// Basic ports (unbounded channels)
inports(A, B, C)
outports(X, Y)

// Ports with capacity (bounded channels)
inports::<100>(A, B)      // Input ports with capacity 100
outports::<50>(X, Y)      // Output ports with capacity 50
```

### State Management

```rust
// Use built-in MemoryState
state(MemoryState)

// Custom state types can also be used
// (must implement ActorState trait)
```

### Input Synchronization

```rust
// Process inputs as they arrive (default)
#[actor(MyActor, inports(A, B), outports(C))]

// Wait for ALL inputs before processing
#[actor(MyActor, inports(A, B), outports(C), await_all_inports)]
```

## Practical Examples

### Data Processing Pipeline

```rust
// Sum Actor - adds numbers from multiple sources
#[actor(
    SumActor,
    inports::<100>(numbers),
    outports::<50>(sum, count)
)]
async fn sum_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    if let Some(Message::Array(numbers)) = payload.get("numbers") {
        let mut sum = 0i64;
        let mut count = 0usize;
        
        for num in numbers {
            if let Message::Integer(n) = num {
                sum += n;
                count += 1;
            }
        }
        
        println!("Sum Actor: {} numbers, sum = {}", count, sum);
        
        Ok([
            ("sum".to_owned(), Message::Integer(sum)),
            ("count".to_owned(), Message::Integer(count as i64)),
        ].into())
    } else {
        Err(anyhow::anyhow!("Expected array of numbers"))
    }
}

// Filter Actor - filters values based on condition
#[actor(
    FilterActor,
    inports::<100>(values, threshold),
    outports::<50>(passed, failed),
    await_all_inports
)]
async fn filter_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    let threshold = match payload.get("threshold").expect("expected threshold") {
        Message::Integer(t) => *t,
        _ => return Err(anyhow::anyhow!("Threshold must be integer")),
    };
    
    if let Some(Message::Array(values)) = payload.get("values") {
        let mut passed = Vec::new();
        let mut failed = Vec::new();
        
        for value in values {
            if let Message::Integer(n) = value {
                if *n >= threshold {
                    passed.push(value.clone());
                } else {
                    failed.push(value.clone());
                }
            }
        }
        
        println!("Filter Actor: {} passed, {} failed (threshold: {})", 
                passed.len(), failed.len(), threshold);
        
        Ok([
            ("passed".to_owned(), Message::Array(passed)),
            ("failed".to_owned(), Message::Array(failed)),
        ].into())
    } else {
        Err(anyhow::anyhow!("Expected array of values"))
    }
}
```

### HTTP Client Actor

```rust
use reqwest;

#[actor(
    HttpClientActor,
    inports::<50>(request),
    outports::<25>(response, error)
)]
async fn http_client_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    // Parse request
    let request = match payload.get("request") {
        Some(Message::Object(obj)) => obj,
        _ => return Err(anyhow::anyhow!("Expected request object")),
    };
    
    let url = match request.get("url") {
        Some(Message::String(s)) => s,
        _ => return Err(anyhow::anyhow!("Missing URL in request")),
    };
    
    let method = request.get("method")
        .and_then(|m| if let Message::String(s) = m { Some(s.as_str()) } else { None })
        .unwrap_or("GET");
    
    // Make HTTP request
    let client = reqwest::Client::new();
    
    let result = match method {
        "GET" => {
            match client.get(url).send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    let text = response.text().await.unwrap_or_default();
                    
                    let response_obj = [
                        ("status".to_owned(), Message::Integer(status as i64)),
                        ("body".to_owned(), Message::String(text)),
                        ("url".to_owned(), Message::String(url.clone())),
                    ].into();
                    
                    [("response".to_owned(), Message::Object(response_obj))].into()
                },
                Err(e) => {
                    let error_obj = [
                        ("message".to_owned(), Message::String(e.to_string())),
                        ("url".to_owned(), Message::String(url.clone())),
                    ].into();
                    
                    [("error".to_owned(), Message::Object(error_obj))].into()
                }
            }
        },
        "POST" => {
            let body = request.get("body")
                .and_then(|b| if let Message::String(s) = b { Some(s) } else { None })
                .unwrap_or("");
            
            match client.post(url).body(body.to_string()).send().await {
                Ok(response) => {
                    let status = response.status().as_u16();
                    let text = response.text().await.unwrap_or_default();
                    
                    let response_obj = [
                        ("status".to_owned(), Message::Integer(status as i64)),
                        ("body".to_owned(), Message::String(text)),
                        ("url".to_owned(), Message::String(url.clone())),
                    ].into();
                    
                    [("response".to_owned(), Message::Object(response_obj))].into()
                },
                Err(e) => {
                    let error_obj = [
                        ("message".to_owned(), Message::String(e.to_string())),
                        ("url".to_owned(), Message::String(url.clone())),
                    ].into();
                    
                    [("error".to_owned(), Message::Object(error_obj))].into()
                }
            }
        },
        _ => {
            let error_obj = [
                ("message".to_owned(), Message::String(format!("Unsupported method: {}", method))),
            ].into();
            
            [("error".to_owned(), Message::Object(error_obj))].into()
        }
    };
    
    Ok(result)
}
```

### Batch Processing Actor

```rust
#[actor(
    BatchActor,
    state(MemoryState),
    inports::<200>(item, flush),
    outports::<50>(batch, count)
)]
async fn batch_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    
    let mut state_guard = state.lock();
    let memory_state = state_guard
        .as_mut_any()
        .downcast_mut::<MemoryState>()
        .expect("Expected MemoryState");
    
    // Initialize batch storage
    if !memory_state.contains_key("batch") {
        memory_state.insert("batch", serde_json::json!([]));
        memory_state.insert("batch_size", serde_json::json!(10)); // Configurable batch size
    }
    
    let batch_size = memory_state.get("batch_size")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;
    
    let mut current_batch: Vec<serde_json::Value> = memory_state.get("batch")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    
    // Handle flush command
    if payload.contains_key("flush") {
        if !current_batch.is_empty() {
            let batch_messages: Vec<Message> = current_batch
                .into_iter()
                .map(|v| Message::from(v))
                .collect();
            
            // Clear batch
            memory_state.insert("batch", serde_json::json!([]));
            
            let count = batch_messages.len();
            println!("Batch Actor: Flushing {} items", count);
            
            return Ok([
                ("batch".to_owned(), Message::Array(batch_messages)),
                ("count".to_owned(), Message::Integer(count as i64)),
            ].into());
        } else {
            return Ok(HashMap::new()); // No items to flush
        }
    }
    
    // Handle new item
    if let Some(item) = payload.get("item") {
        current_batch.push(serde_json::json!(item));
        
        // Check if batch is full
        if current_batch.len() >= batch_size {
            let batch_messages: Vec<Message> = current_batch
                .iter()
                .map(|v| Message::from(v.clone()))
                .collect();
            
            // Clear batch
            memory_state.insert("batch", serde_json::json!([]));
            
            let count = batch_messages.len();
            println!("Batch Actor: Full batch of {} items", count);
            
            Ok([
                ("batch".to_owned(), Message::Array(batch_messages)),
                ("count".to_owned(), Message::Integer(count as i64)),
            ].into())
        } else {
            // Update batch state
            memory_state.insert("batch", serde_json::json!(current_batch));
            
            // Return empty result (batch not ready yet)
            Ok(HashMap::new())
        }
    } else {
        Err(anyhow::anyhow!("Expected item or flush command"))
    }
}
```

## Error Handling Patterns

### Graceful Error Handling

```rust
#[actor(
    ValidatorActor,
    inports::<100>(data),
    outports::<50>(valid, invalid, error)
)]
async fn validator_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    match payload.get("data") {
        Some(Message::Integer(n)) if *n > 0 => {
            println!("Validator: Valid number {}", n);
            Ok([("valid".to_owned(), Message::Integer(*n))].into())
        },
        Some(Message::Integer(n)) => {
            println!("Validator: Invalid number {} (must be positive)", n);
            Ok([("invalid".to_owned(), Message::Integer(*n))].into())
        },
        Some(other) => {
            let error_msg = format!("Expected integer, got {:?}", other);
            println!("Validator: {}", error_msg);
            Ok([("error".to_owned(), Message::Error(error_msg))].into())
        },
        None => {
            Err(anyhow::anyhow!("Missing data field"))
        }
    }
}

// Retry Actor - implements retry logic with exponential backoff
#[actor(
    RetryActor,
    state(MemoryState),
    inports::<50>(task),
    outports::<25>(success, failure)
)]
async fn retry_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    let state = context.get_state();
    
    let task = payload.get("task")
        .ok_or_else(|| anyhow::anyhow!("Missing task"))?;
    
    let max_retries = 3;
    let base_delay_ms = 100;
    
    for attempt in 1..=max_retries {
        match simulate_task_processing(task).await {
            Ok(result) => {
                println!("Retry Actor: Task succeeded on attempt {}", attempt);
                return Ok([("success".to_owned(), result)].into());
            },
            Err(e) => {
                if attempt < max_retries {
                    let delay = base_delay_ms * (2_u64.pow(attempt - 1));
                    println!("Retry Actor: Attempt {} failed, retrying in {}ms: {}", 
                            attempt, delay, e);
                    
                    tokio::time::sleep(tokio::time::Duration::from_millis(delay)).await;
                } else {
                    println!("Retry Actor: All {} attempts failed: {}", max_retries, e);
                    return Ok([
                        ("failure".to_owned(), Message::Error(format!("Failed after {} attempts: {}", max_retries, e)))
                    ].into());
                }
            }
        }
    }
    
    unreachable!()
}

async fn simulate_task_processing(task: &Message) -> Result<Message, anyhow::Error> {
    // Simulate processing that might fail
    use rand::Rng;
    let mut rng = rand::thread_rng();
    
    if rng.gen_bool(0.7) { // 70% success rate
        Ok(Message::String(format!("Processed: {:?}", task)))
    } else {
        Err(anyhow::anyhow!("Simulated task failure"))
    }
}
```

## Using Actors in Networks

### Registration and Instantiation

```rust
use reflow_network::{Network, NetworkConfig};
use reflow_network::connector::{Connector, ConnectionPoint, InitialPacket};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut network = Network::new(NetworkConfig::default());
    
    // Register actor types
    network.register_actor("hello_process", HelloActor::new())?;
    network.register_actor("counter_process", CounterActor::new())?;
    network.register_actor("validator_process", ValidatorActor::new())?;
    
    // Create actor instances
    network.add_node("hello1", "hello_process")?;
    network.add_node("counter1", "counter_process")?;
    network.add_node("validator1", "validator_process")?;
    
    // Connect actors
    network.add_connection(Connector {
        from: ConnectionPoint {
            actor: "hello1".to_owned(),
            port: "output".to_owned(),
            ..Default::default()
        },
        to: ConnectionPoint {
            actor: "validator1".to_owned(),
            port: "data".to_owned(),
            ..Default::default()
        },
    });
    
    // Send initial data
    network.add_initial(InitialPacket {
        to: ConnectionPoint {
            actor: "hello1".to_owned(),
            port: "input".to_owned(),
            initial_data: Some(Message::String("World".to_owned())),
        },
    });
    
    // Start the network
    network.start().await?;
    
    // Wait for processing
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    Ok(())
}
```

## Testing Actors

### Unit Testing Actor Functions

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use reflow_network::actor::{ActorContext, MemoryState, ActorLoad};
    use std::sync::Arc;
    use parking_lot::Mutex;
    
    fn create_test_context(payload: HashMap<String, Message>) -> ActorContext {
        let (tx, _rx) = flume::unbounded();
        let state: Arc<Mutex<dyn reflow_network::actor::ActorState>> = 
            Arc::new(Mutex::new(MemoryState::default()));
        
        ActorContext::new(
            payload,
            (tx, _rx),
            state,
            HashMap::new(),
            Arc::new(Mutex::new(ActorLoad::new(0))),
        )
    }
    
    #[tokio::test]
    async fn test_hello_actor() {
        let payload = HashMap::from([
            ("input".to_string(), Message::String("Test".to_string()))
        ]);
        
        let context = create_test_context(payload);
        let result = hello_actor(context).await.unwrap();
        
        assert_eq!(
            result.get("output"),
            Some(&Message::String("Hello, Test!".to_string()))
        );
    }
    
    #[tokio::test]
    async fn test_counter_actor_increment() {
        let payload = HashMap::from([
            ("increment".to_string(), Message::Integer(5))
        ]);
        
        let context = create_test_context(payload);
        let result = counter_actor(context).await.unwrap();
        
        assert_eq!(result.get("count"), Some(&Message::Integer(5)));
        assert_eq!(result.get("total"), Some(&Message::Integer(5)));
    }
    
    #[tokio::test]
    async fn test_greeter_actor() {
        let payload = HashMap::from([
            ("name".to_string(), Message::String("Alice".to_string())),
            ("age".to_string(), Message::Integer(30)),
        ]);
        
        let context = create_test_context(payload);
        let result = greeter_actor(context).await.unwrap();
        
        assert_eq!(
            result.get("greeting"),
            Some(&Message::String("Hello Alice, you are 30 years old!".to_string()))
        );
    }
    
    #[tokio::test]
    async fn test_validator_actor_valid() {
        let payload = HashMap::from([
            ("data".to_string(), Message::Integer(42))
        ]);
        
        let context = create_test_context(payload);
        let result = validator_actor(context).await.unwrap();
        
        assert_eq!(result.get("valid"), Some(&Message::Integer(42)));
        assert!(!result.contains_key("invalid"));
        assert!(!result.contains_key("error"));
    }
    
    #[tokio::test]
    async fn test_validator_actor_invalid() {
        let payload = HashMap::from([
            ("data".to_string(), Message::Integer(-5))
        ]);
        
        let context = create_test_context(payload);
        let result = validator_actor(context).await.unwrap();
        
        assert_eq!(result.get("invalid"), Some(&Message::Integer(-5)));
        assert!(!result.contains_key("valid"));
    }
}
```

## Best Practices

### Actor Design Guidelines

1. **Single Responsibility**: Each actor should have one clear purpose
2. **Idempotent Processing**: Handle duplicate messages gracefully
3. **Error Propagation**: Use both Result returns and error output ports
4. **State Minimal**: Keep state minimal and well-defined
5. **Port Naming**: Use descriptive port names

### Performance Tips

```rust
// Use appropriate channel capacities
inports::<1000>(high_volume_input)   // High throughput
inports::<10>(low_volume_input)      // Low throughput

// Batch processing for efficiency
#[actor(
    EfficientProcessor,
    inports::<500>(batch),
    outports::<100>(results)
)]
async fn efficient_processor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    if let Some(Message::Array(items)) = payload.get("batch") {
        // Process items in parallel
        use futures::stream::{self, StreamExt};
        
        let results: Vec<Message> = stream::iter(items.iter())
            .map(|item| async move {
                process_single_item(item).await
            })
            .buffer_unordered(10) // Process 10 items concurrently
            .collect()
            .await;
        
        Ok([("results".to_owned(), Message::Array(results))].into())
    } else {
        Err(anyhow::anyhow!("Expected batch of items"))
    }
}

async fn process_single_item(item: &Message) -> Message {
    // Simulate processing
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    item.clone()
}
```

### Memory Management

```rust
// Avoid cloning large data when possible
#[actor(
    MemoryEfficientActor,
    inports::<100>(data),
    outports::<50>(processed)
)]
async fn memory_efficient_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    // Process data in-place when possible
    if let Some(Message::Array(items)) = payload.get("data") {
        let count = items.len();
        
        // Instead of cloning all items, just extract what we need
        let summary = Message::Object([
            ("count".to_owned(), Message::Integer(count as i64)),
            ("first_item".to_owned(), items.first().cloned().unwrap_or(Message::None)),
            ("last_item".to_owned(), items.last().cloned().unwrap_or(Message::None)),
        ].into());
        
        Ok([("processed".to_owned(), summary)].into())
    } else {
        Err(anyhow::anyhow!("Expected array data"))
    }
}
```

## Manual Actor Implementation

For maximum control or when the macro limitations are insufficient, you can implement the Actor trait manually. This approach gives you complete control over the actor's behavior and lifecycle.

### Basic Manual Actor

```rust
use reflow_network::actor::{Actor, ActorBehavior, ActorContext, Port, MemoryState, ActorLoad};
use reflow_network::message::Message;
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use std::pin::Pin;
use std::future::Future;

pub struct ManualActor {
    inports: Port,
    outports: Port,
    name: String,
    load: Arc<Mutex<ActorLoad>>,
}

impl ManualActor {
    pub fn new(name: String) -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            name,
            load: Arc::new(Mutex::new(ActorLoad::new(0))),
        }
    }
}

impl Actor for ManualActor {
    fn get_behavior(&self) -> ActorBehavior {
        let name = self.name.clone();
        
        Box::new(move |context: ActorContext| {
            let name = name.clone();
            
            Box::pin(async move {
                let payload = context.get_payload();
                
                if let Some(Message::String(text)) = payload.get("input") {
                    let response = format!("{}: Processing '{}'", name, text);
                    println!("{}", response);
                    
                    Ok([
                        ("output".to_owned(), Message::String(response))
                    ].into())
                } else {
                    Err(anyhow::anyhow!("Expected string input"))
                }
            })
        })
    }
    
    fn get_inports(&self) -> Port {
        self.inports.clone()
    }
    
    fn get_outports(&self) -> Port {
        self.outports.clone()
    }
    
    fn load_count(&self) -> Arc<Mutex<ActorLoad>> {
        self.load.clone()
    }
    
    fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let outports = self.get_outports();
        let state: Arc<Mutex<dyn reflow_network::actor::ActorState>> = 
            Arc::new(Mutex::new(MemoryState::default()));
        let load_count = self.load_count();
        
        Box::pin(async move {
            use futures::stream::StreamExt;
            
            loop {
                if let Some(payload) = inports.1.stream().next().await {
                    // Increment load count
                    {
                        let mut load = load_count.lock();
                        load.inc();
                    }
                    
                    let context = ActorContext::new(
                        payload,
                        outports.clone(),
                        state.clone(),
                        HashMap::new(),
                        load_count.clone(),
                    );
                    
                    match behavior(context).await {
                        Ok(result) => {
                            if !result.is_empty() {
                                let _ = outports.0.send(result);
                            }
                        },
                        Err(e) => {
                            eprintln!("Error in actor behavior: {:?}", e);
                        }
                    }
                    
                    // Decrement load count
                    {
                        let mut load = load_count.lock();
                        load.dec();
                    }
                }
            }
        })
    }
}
```

### Stateful Manual Actor

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomState {
    pub counter: i64,
    pub last_message: String,
    pub timestamps: Vec<i64>,
}

impl reflow_network::actor::ActorState for CustomState {
    fn as_mut_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub struct StatefulManualActor {
    inports: Port,
    outports: Port,
    initial_state: CustomState,
    load: Arc<Mutex<ActorLoad>>,
}

impl StatefulManualActor {
    pub fn new(initial_state: CustomState) -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            initial_state,
            load: Arc::new(Mutex::new(ActorLoad::new(0))),
        }
    }
}

impl Actor for StatefulManualActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(|context: ActorContext| {
            Box::pin(async move {
                let payload = context.get_payload();
                let state = context.get_state();
                
                let mut state_guard = state.lock();
                let custom_state = state_guard
                    .as_mut_any()
                    .downcast_mut::<CustomState>()
                    .expect("Expected CustomState");
                
                // Update counter
                custom_state.counter += 1;
                
                // Record timestamp
                let now = chrono::Utc::now().timestamp_millis();
                custom_state.timestamps.push(now);
                
                // Keep only last 10 timestamps
                if custom_state.timestamps.len() > 10 {
                    custom_state.timestamps.remove(0);
                }
                
                // Process message
                if let Some(Message::String(text)) = payload.get("message") {
                    custom_state.last_message = text.clone();
                    
                    let response = format!(
                        "Processed message #{}: '{}' (last 5 timestamps: {:?})",
                        custom_state.counter,
                        text,
                        custom_state.timestamps.iter().rev().take(5).collect::<Vec<_>>()
                    );
                    
                    Ok([
                        ("response".to_owned(), Message::String(response)),
                        ("counter".to_owned(), Message::Integer(custom_state.counter)),
                    ].into())
                } else {
                    Err(anyhow::anyhow!("Expected message field"))
                }
            })
        })
    }
    
    fn get_inports(&self) -> Port {
        self.inports.clone()
    }
    
    fn get_outports(&self) -> Port {
        self.outports.clone()
    }
    
    fn load_count(&self) -> Arc<Mutex<ActorLoad>> {
        self.load.clone()
    }
    
    fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let outports = self.get_outports();
        let state: Arc<Mutex<dyn reflow_network::actor::ActorState>> = 
            Arc::new(Mutex::new(self.initial_state.clone()));
        let load_count = self.load_count();
        
        Box::pin(async move {
            use futures::stream::StreamExt;
            
            loop {
                if let Some(payload) = inports.1.stream().next().await {
                    // Increment load count
                    {
                        let mut load = load_count.lock();
                        load.inc();
                    }
                    
                    let context = ActorContext::new(
                        payload,
                        outports.clone(),
                        state.clone(),
                        HashMap::new(),
                        load_count.clone(),
                    );
                    
                    match behavior(context).await {
                        Ok(result) => {
                            if !result.is_empty() {
                                let _ = outports.0.send(result);
                            }
                        },
                        Err(e) => {
                            eprintln!("Error in stateful actor behavior: {:?}", e);
                        }
                    }
                    
                    // Decrement load count
                    {
                        let mut load = load_count.lock();
                        load.dec();
                    }
                }
            }
        })
    }
}
```

### Multi-Input Manual Actor

```rust
pub struct MultiInputActor {
    inports: Port,
    outports: Port,
    await_all_inputs: bool,
    input_ports: Vec<String>,
    load: Arc<Mutex<ActorLoad>>,
}

impl MultiInputActor {
    pub fn new(input_ports: Vec<String>, await_all_inputs: bool) -> Self {
        Self {
            inports: flume::bounded(100),
            outports: flume::bounded(50),
            await_all_inputs,
            input_ports,
            load: Arc::new(Mutex::new(ActorLoad::new(0))),
        }
    }
}

impl Actor for MultiInputActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(|context: ActorContext| {
            Box::pin(async move {
                let payload = context.get_payload();
                
                // Collect all available data
                let mut results = HashMap::new();
                let mut total_value = 0i64;
                let mut value_count = 0;
                
                for (port, message) in &payload {
                    if let Message::Integer(value) = message {
                        total_value += value;
                        value_count += 1;
                        
                        results.insert(
                            format!("processed_{}", port), 
                            Message::Integer(value * 2)
                        );
                    }
                }
                
                if value_count > 0 {
                    results.insert("sum".to_owned(), Message::Integer(total_value));
                    results.insert("average".to_owned(), Message::Integer(total_value / value_count));
                    results.insert("count".to_owned(), Message::Integer(value_count));
                }
                
                println!("MultiInput Actor: processed {} values, sum = {}", value_count, total_value);
                
                Ok(results)
            })
        })
    }
    
    fn get_inports(&self) -> Port {
        self.inports.clone()
    }
    
    fn get_outports(&self) -> Port {
        self.outports.clone()
    }
    
    fn load_count(&self) -> Arc<Mutex<ActorLoad>> {
        self.load.clone()
    }
    
    fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let outports = self.get_outports();
        let state: Arc<Mutex<dyn reflow_network::actor::ActorState>> = 
            Arc::new(Mutex::new(MemoryState::default()));
        let load_count = self.load_count();
        let await_all_inputs = self.await_all_inputs;
        let input_ports_count = self.input_ports.len();
        
        Box::pin(async move {
            use futures::stream::StreamExt;
            let mut all_inputs: HashMap<String, Message> = HashMap::new();
            
            loop {
                if let Some(packet) = inports.1.stream().next().await {
                    // Increment load count
                    {
                        let mut load = load_count.lock();
                        load.inc();
                    }
                    
                    if await_all_inputs {
                        // Accumulate inputs until we have all expected ports
                        all_inputs.extend(packet);
                        
                        if all_inputs.len() >= input_ports_count {
                            let context = ActorContext::new(
                                all_inputs.clone(),
                                outports.clone(),
                                state.clone(),
                                HashMap::new(),
                                load_count.clone(),
                            );
                            
                            match behavior(context).await {
                                Ok(result) => {
                                    if !result.is_empty() {
                                        let _ = outports.0.send(result);
                                    }
                                },
                                Err(e) => {
                                    eprintln!("Error in multi-input actor behavior: {:?}", e);
                                }
                            }
                            
                            all_inputs.clear();
                        } else {
                            // Continue without decrementing load count
                            {
                                let mut load = load_count.lock();
                                load.dec();
                            }
                            continue;
                        }
                    } else {
                        // Process immediately
                        let context = ActorContext::new(
                            packet,
                            outports.clone(),
                            state.clone(),
                            HashMap::new(),
                            load_count.clone(),
                        );
                        
                        match behavior(context).await {
                            Ok(result) => {
                                if !result.is_empty() {
                                    let _ = outports.0.send(result);
                                }
                            },
                            Err(e) => {
                                eprintln!("Error in multi-input actor behavior: {:?}", e);
                            }
                        }
                    }
                    
                    // Decrement load count
                    {
                        let mut load = load_count.lock();
                        load.dec();
                    }
                }
            }
        })
    }
}
```

### When to Use Manual Implementation

**Use manual implementation when:**

1. **Complex State Requirements**: You need custom state types or complex state initialization
2. **Custom Port Logic**: You need dynamic port creation or complex routing logic
3. **Advanced Error Handling**: You need sophisticated error recovery or circuit breaker patterns
4. **Performance Optimization**: You need fine-grained control over message processing
5. **Integration Requirements**: You need to integrate with external systems in specific ways

**Example: Circuit Breaker Actor**

```rust
#[derive(Debug, Clone)]
enum CircuitState {
    Closed,   // Normal operation
    Open,     // Failing, rejecting requests
    HalfOpen, // Testing if service recovered
}

pub struct CircuitBreakerActor {
    inports: Port,
    outports: Port,
    failure_threshold: u32,
    timeout_ms: u64,
    load: Arc<Mutex<ActorLoad>>,
}

impl CircuitBreakerActor {
    pub fn new(failure_threshold: u32, timeout_ms: u64) -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
            failure_threshold,
            timeout_ms,
            load: Arc::new(Mutex::new(ActorLoad::new(0))),
        }
    }
}

impl Actor for CircuitBreakerActor {
    fn get_behavior(&self) -> ActorBehavior {
        let failure_threshold = self.failure_threshold;
        let timeout_ms = self.timeout_ms;
        
        Box::new(move |context: ActorContext| {
            Box::pin(async move {
                let payload = context.get_payload();
                let state = context.get_state();
                
                let mut state_guard = state.lock();
                let memory_state = state_guard
                    .as_mut_any()
                    .downcast_mut::<MemoryState>()
                    .expect("Expected MemoryState");
                
                // Initialize circuit breaker state
                if !memory_state.contains_key("circuit_state") {
                    memory_state.insert("circuit_state", serde_json::json!("Closed"));
                    memory_state.insert("failure_count", serde_json::json!(0));
                    memory_state.insert("last_failure_time", serde_json::json!(0));
                }
                
                let circuit_state_str = memory_state.get("circuit_state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Closed");
                
                let failure_count = memory_state.get("failure_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                
                let last_failure_time = memory_state.get("last_failure_time")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                
                let circuit_state = match circuit_state_str {
                    "Open" => CircuitState::Open,
                    "HalfOpen" => CircuitState::HalfOpen,
                    _ => CircuitState::Closed,
                };
                
                let now = chrono::Utc::now().timestamp_millis();
                
                match circuit_state {
                    CircuitState::Open => {
                        // Check if timeout has passed
                        if now - last_failure_time > timeout_ms as i64 {
                            memory_state.insert("circuit_state", serde_json::json!("HalfOpen"));
                            println!("Circuit breaker: Transitioning to HalfOpen");
                        } else {
                            return Ok([
                                ("rejected".to_owned(), 
                                 Message::Error("Circuit breaker is OPEN".to_string()))
                            ].into());
                        }
                    },
                    CircuitState::HalfOpen => {
                        // Process one request to test
                    },
                    CircuitState::Closed => {
                        // Normal operation
                    }
                }
                
                // Simulate processing the request
                if let Some(request) = payload.get("request") {
                    // Simulate success/failure (in real implementation, you'd call actual service)
                    let success = payload.get("simulate_success")
                        .and_then(|v| if let Message::Boolean(b) = v { Some(*b) } else { None })
                        .unwrap_or(true);
                    
                    if success {
                        // Success - reset failure count if in HalfOpen
                        if matches!(circuit_state, CircuitState::HalfOpen) {
                            memory_state.insert("circuit_state", serde_json::json!("Closed"));
                            memory_state.insert("failure_count", serde_json::json!(0));
                            println!("Circuit breaker: Transitioning to Closed");
                        }
                        
                        Ok([
                            ("success".to_owned(), Message::String("Request processed".to_string()))
                        ].into())
                    } else {
                        // Failure
                        let new_failure_count = failure_count + 1;
                        memory_state.insert("failure_count", serde_json::json!(new_failure_count));
                        memory_state.insert("last_failure_time", serde_json::json!(now));
                        
                        if new_failure_count >= failure_threshold {
                            memory_state.insert("circuit_state", serde_json::json!("Open"));
                            println!("Circuit breaker: Transitioning to Open");
                        }
                        
                        Ok([
                            ("failure".to_owned(), 
                             Message::Error(format!("Request failed (failures: {})", new_failure_count)))
                        ].into())
                    }
                } else {
                    Err(anyhow::anyhow!("Missing request"))
                }
            })
        })
    }
    
    fn get_inports(&self) -> Port { self.inports.clone() }
    fn get_outports(&self) -> Port { self.outports.clone() }
    fn load_count(&self) -> Arc<Mutex<ActorLoad>> { self.load.clone() }
    
    fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let outports = self.get_outports();
        let state: Arc<Mutex<dyn reflow_network::actor::ActorState>> = 
            Arc::new(Mutex::new(MemoryState::default()));
        let load_count = self.load_count();
        
        Box::pin(async move {
            use futures::stream::StreamExt;
            
            loop {
                if let Some(payload) = inports.1.stream().next().await {
                    {
                        let mut load = load_count.lock();
                        load.inc();
                    }
                    
                    let context = ActorContext::new(
                        payload,
                        outports.clone(),
                        state.clone(),
                        HashMap::new(),
                        load_count.clone(),
                    );
                    
                    match behavior(context).await {
                        Ok(result) => {
                            if !result.is_empty() {
                                let _ = outports.0.send(result);
                            }
                        },
                        Err(e) => {
                            eprintln!("Error in circuit breaker: {:?}", e);
                        }
                    }
                    
                    {
                        let mut load = load_count.lock();
                        load.dec();
                    }
                }
            }
        })
    }
}
```

### Testing Manual Actors

```rust
#[cfg(test)]
mod manual_actor_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_manual_actor() {
        let actor = ManualActor::new("TestActor".to_string());
        let behavior = actor.get_behavior();
        
        let payload = HashMap::from([
            ("input".to_string(), Message::String("test".to_string()))
        ]);
        
        let (tx, _rx) = flume::unbounded();
        let state: Arc<Mutex<dyn reflow_network::actor::ActorState>> = 
            Arc::new(Mutex::new(MemoryState::default()));
        
        let context = ActorContext::new(
            payload,
            (tx, _rx),
            state,
            HashMap::new(),
            Arc::new(Mutex::new(ActorLoad::new(0))),
        );
        
        let result = behavior(context).await.unwrap();
        
        assert!(result.contains_key("output"));
        if let Some(Message::String(output)) = result.get("output") {
            assert!(output.contains("TestActor"));
            assert!(output.contains("test"));
        }
    }
    
    #[tokio::test]
    async fn test_stateful_manual_actor() {
        let initial_state = CustomState {
            counter: 0,
            last_message: String::new(),
            timestamps: Vec::new(),
        };
        
        let actor = StatefulManualActor::new(initial_state);
        let behavior = actor.get_behavior();
        
        let payload = HashMap::from([
            ("message".to_string(), Message::String("hello".to_string()))
        ]);
        
        let (tx, _rx) = flume::unbounded();
        let state: Arc<Mutex<dyn reflow_network::actor::ActorState>> = 
            Arc::new(Mutex::new(CustomState::default()));
        
        let context = ActorContext::new(
            payload,
            (tx, _rx),
            state,
            HashMap::new(),
            Arc::new(Mutex::new(ActorLoad::new(0))),
        );
        
        let result = behavior(context).await.unwrap();
        
        assert_eq!(result.get("counter"), Some(&Message::Integer(1)));
        assert!(result.contains_key("response"));
    }
}
```

## Choosing Between Macro and Manual Implementation

### Use Actor Macro When:
- Simple, stateless processing
- Standard input/output patterns
- Rapid prototyping
- Most common use cases

### Use Manual Implementation When:
- Complex state management
- Custom error handling strategies
- Performance-critical applications
- Integration with external systems
- Advanced patterns (circuit breakers, rate limiting, etc.)

## Next Steps

- **State Management**: [Advanced State Patterns](../state/advanced-patterns.md)
- **Network Integration**: [Building Workflows](../network/workflows.md)  
- **Performance**: [Actor Optimization](../performance/optimization.md)
- **Examples**: [Real-World Examples](../../examples/README.md)
