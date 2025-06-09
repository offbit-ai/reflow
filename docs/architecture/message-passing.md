# Message Passing

This document details Reflow's message passing system, which is the primary communication mechanism between actors.

## Message Types

Reflow uses a strongly-typed message system with built-in serialization support:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode, PartialEq)]
pub enum Message {
    Flow,
    Event(EncodableValue),
    Boolean(bool),
    Integer(i64),
    Float(f64),
    String(String),
    Object(EncodableValue),
    Array(Vec<EncodableValue>),
    Stream(Vec<u8>),
    Encoded(Vec<u8>),
    Optional(Option<EncodableValue>),
    Any(EncodableValue),
    Error(String),
}
```

### EncodableValue

Reflow uses `EncodableValue` as a wrapper for complex data types:

```rust
#[derive(Clone, Debug, Serialize, Deserialize, Encode, Decode, PartialEq, Eq)]
pub struct EncodableValue {
    pub(crate) data: Vec<u8>,
}

impl EncodableValue {
    pub fn new<T: Encode>(value: &T) -> Self {
        Self {
            data: bitcode::encode(value),
        }
    }

    pub fn decode<'a, T: Decode<'a>>(&'a self) -> Option<T> {
        bitcode::decode(&self.data).ok()
    }
}
```

### Message Conversion

```rust
use serde_json::Value;

// From JSON values
let msg = Message::from(serde_json::json!(42));

// To JSON values  
let json: Value = message.into();

// Type checking
if let Message::Integer(n) = message {
    println!("Number: {}", n);
}

// Working with EncodableValue
let data = serde_json::json!({"key": "value"});
let encodable = EncodableValue::from(data);
let object_msg = Message::Object(encodable);

// Create arrays with EncodableValue
let array_items = vec![
    EncodableValue::from(serde_json::json!("hello")),
    EncodableValue::from(serde_json::json!(42)),
];
let array_msg = Message::Array(array_items);
```

## Communication Channels

### Ports

Ports are the communication endpoints for actors:

```rust
pub type Port = (
    flume::Sender<HashMap<String, Message>>,
    flume::Receiver<HashMap<String, Message>>,
);

// Actor payload format
pub type ActorPayload = HashMap<String, Message>;
```

### Channel Properties

- **Asynchronous**: Non-blocking send/receive operations
- **Bounded**: Configurable buffer sizes for backpressure
- **Multi-producer, Single-consumer**: Multiple senders, one receiver per port
- **Type-safe**: Compile-time message type checking

## Message Flow Patterns

### Point-to-Point

Direct communication between two actors:

```rust
// Actor A sends to Actor B
let message = HashMap::from([
    ("data".to_string(), Message::String("hello".to_string()))
]);
sender.send_async(message).await?;
```

### Broadcast

One actor sends to multiple receivers:

```rust
// Using actor macro for broadcast
#[actor(
    BroadcastActor,
    inports::<100>(input),
    outports::<50>(output1, output2, output3)
)]
async fn broadcast_actor(context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
    let payload = context.get_payload();
    
    if let Some(input_msg) = payload.get("input") {
        // Broadcast to all output ports
        Ok([
            ("output1".to_owned(), input_msg.clone()),
            ("output2".to_owned(), input_msg.clone()),
            ("output3".to_owned(), input_msg.clone()),
        ].into())
    } else {
        Err(anyhow::anyhow!("No input to broadcast"))
    }
}

// Manual implementation for dynamic outputs
struct ManualBroadcastActor {
    inports: Port,
    outports: Port,
    outputs: Vec<flume::Sender<HashMap<String, Message>>>,
    load: Arc<Mutex<ActorLoad>>,
}

impl ManualBroadcastActor {
    async fn broadcast(&self, message: HashMap<String, Message>) -> Result<(), anyhow::Error> {
        for output in &self.outputs {
            output.send_async(message.clone()).await?;
        }
        Ok(())
    }
}
```

### Fan-In (Merge)

Multiple actors send to one receiver:

```rust
struct MergeActor {
    inputs: Vec<flume::Receiver<HashMap<String, Message>>>,
    output: flume::Sender<HashMap<String, Message>>,
}

impl MergeActor {
    async fn merge_loop(&self) {
        use futures::stream::{FuturesUnordered, StreamExt};
        
        let mut streams: FuturesUnordered<_> = self.inputs
            .iter()
            .map(|rx| rx.recv_async())
            .collect();
            
        while let Some(result) = streams.next().await {
            if let Ok(message) = result {
                let _ = self.output.send_async(message).await;
            }
        }
    }
}
```

## Serialization and Transport

### Local Serialization

For local communication, messages use efficient in-memory representation:

```rust
// Zero-copy for simple types
let msg = Message::Integer(42); // No allocation

// Reference counting for complex types
let complex = Message::Object(data); // Rc<HashMap<String, Message>>
```

### Network Serialization

For distributed communication:

```rust
use bitcode;
use flate2::Compression;

// Compress and serialize
let compressed = compress_message(&message, Compression::default())?;
let bytes = bitcode::serialize(&compressed)?;

// Send over network
network_send(bytes).await?;

// Receive and deserialize
let received = network_receive().await?;
let message = bitcode::deserialize(&received)?;
let decompressed = decompress_message(&message)?;
```

## Message Routing

### Router Actor

```rust
pub struct RouterActor {
    routes: HashMap<String, flume::Sender<HashMap<String, Message>>>,
    default_route: Option<flume::Sender<HashMap<String, Message>>>,
}

impl RouterActor {
    pub fn route_message(&self, key: &str, message: HashMap<String, Message>) -> Result<()> {
        if let Some(sender) = self.routes.get(key) {
            sender.try_send(message)?;
        } else if let Some(default) = &self.default_route {
            default.try_send(message)?;
        }
        Ok(())
    }
}
```

### Content-Based Routing

```rust
impl RouterActor {
    fn route_by_content(&self, message: &HashMap<String, Message>) -> Option<&str> {
        // Route based on message content
        if let Some(Message::String(msg_type)) = message.get("type") {
            match msg_type.as_str() {
                "user_event" => Some("user_handler"),
                "system_event" => Some("system_handler"),
                "error" => Some("error_handler"),
                _ => None,
            }
        } else {
            None
        }
    }
}
```

## Error Handling

### Error Message Format

```rust
// Standard error message structure
let error_msg = HashMap::from([
    ("error".to_string(), Message::Error("Processing failed".to_string())),
    ("code".to_string(), Message::Integer(500)),
    ("source".to_string(), Message::String("database_actor".to_string())),
    ("timestamp".to_string(), Message::String(Utc::now().to_rfc3339())),
    ("details".to_string(), Message::Object(error_details)),
]);
```

### Dead Letter Queue

```rust
pub struct DeadLetterQueue {
    storage: Arc<Mutex<Vec<(String, HashMap<String, Message>)>>>,
    max_size: usize,
}

impl DeadLetterQueue {
    pub async fn store_failed_message(
        &self, 
        reason: String, 
        message: HashMap<String, Message>
    ) {
        let mut storage = self.storage.lock();
        if storage.len() >= self.max_size {
            storage.remove(0); // Remove oldest
        }
        storage.push((reason, message));
    }
}
```

## Backpressure Management

### Flow Control

```rust
pub struct FlowControlActor {
    input: flume::Receiver<HashMap<String, Message>>,
    output: flume::Sender<HashMap<String, Message>>,
    buffer_size: usize,
    current_load: Arc<AtomicUsize>,
}

impl FlowControlActor {
    async fn process_with_backpressure(&self) {
        while let Ok(message) = self.input.recv_async().await {
            // Check current load
            let load = self.current_load.load(Ordering::Relaxed);
            
            if load > self.buffer_size {
                // Apply backpressure - slow down
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            
            self.current_load.fetch_add(1, Ordering::Relaxed);
            
            // Process message
            if let Err(_) = self.output.try_send(message) {
                // Output buffer full, apply backpressure
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            
            self.current_load.fetch_sub(1, Ordering::Relaxed);
        }
    }
}
```

## Message Ordering

### Ordered Delivery

```rust
pub struct OrderedDeliveryActor {
    sequence_number: AtomicU64,
    expected_sequence: AtomicU64,
    buffer: Arc<Mutex<BTreeMap<u64, HashMap<String, Message>>>>,
}

impl OrderedDeliveryActor {
    fn add_sequence_number(&self, mut message: HashMap<String, Message>) -> HashMap<String, Message> {
        let seq = self.sequence_number.fetch_add(1, Ordering::Relaxed);
        message.insert("sequence".to_string(), Message::Integer(seq as i64));
        message
    }
    
    async fn deliver_in_order(&self, message: HashMap<String, Message>) {
        if let Some(Message::Integer(seq)) = message.get("sequence") {
            let seq = *seq as u64;
            let expected = self.expected_sequence.load(Ordering::Relaxed);
            
            if seq == expected {
                // Deliver immediately
                self.deliver_message(message).await;
                self.expected_sequence.fetch_add(1, Ordering::Relaxed);
                
                // Check buffer for next messages
                self.deliver_buffered_messages().await;
            } else {
                // Buffer out-of-order message
                self.buffer.lock().insert(seq, message);
            }
        }
    }
}
```

## Performance Optimization

### Message Batching

```rust
use crate::message::{Message, EncodableValue};

pub struct BatchingActor {
    batch_size: usize,
    batch_timeout: Duration,
    current_batch: Vec<HashMap<String, Message>>,
    input: flume::Receiver<HashMap<String, Message>>,
    output: flume::Sender<HashMap<String, Message>>,
}

impl BatchingActor {
    async fn process_with_batching(&mut self) {
        let mut interval = tokio::time::interval(self.batch_timeout);
        
        loop {
            tokio::select! {
                // Receive new message
                Ok(message) = self.input.recv_async() => {
                    self.current_batch.push(message);
                    
                    if self.current_batch.len() >= self.batch_size {
                        self.flush_batch().await;
                    }
                }
                
                // Timeout - flush partial batch
                _ = interval.tick() => {
                    if !self.current_batch.is_empty() {
                        self.flush_batch().await;
                    }
                }
            }
        }
    }
    
    async fn flush_batch(&mut self) {
        if !self.current_batch.is_empty() {
            // Convert to EncodableValue for proper serialization
            let batch_items: Vec<EncodableValue> = self.current_batch
                .drain(..)
                .map(|msg| EncodableValue::from(serde_json::to_value(msg).unwrap()))
                .collect();
            
            let batch = Message::Array(batch_items);
            
            let batch_message = HashMap::from([
                ("batch".to_string(), batch)
            ]);
            
            let _ = self.output.send_async(batch_message).await;
        }
    }
}
```

### Zero-Copy Optimization

```rust
use bytes::Bytes;

// Use Bytes for zero-copy binary data
let data = Bytes::from(vec![1, 2, 3, 4]);
let message = Message::Binary(data.to_vec());

// Reference counting for large objects
use std::sync::Arc;

struct LargeData {
    content: Vec<u8>,
}

let large_data = Arc::new(LargeData { content: vec![0; 1000000] });
// Pass Arc around instead of cloning large data
```

## Message Validation

### Schema Validation

```rust
use serde_json::Value;

pub struct MessageValidator {
    schemas: HashMap<String, Value>, // JSON Schema
}

impl MessageValidator {
    pub fn validate_message(
        &self, 
        message_type: &str, 
        message: &HashMap<String, Message>
    ) -> Result<(), ValidationError> {
        if let Some(schema) = self.schemas.get(message_type) {
            let json_value: Value = message.clone().into();
            validate_json_schema(&json_value, schema)?;
        }
        Ok(())
    }
}
```

### Type Safety

```rust
// Type-safe message builders
pub struct UserEventBuilder {
    user_id: Option<String>,
    event_type: Option<String>,
    timestamp: Option<String>,
}

impl UserEventBuilder {
    pub fn user_id(mut self, id: String) -> Self {
        self.user_id = Some(id);
        self
    }
    
    pub fn event_type(mut self, event_type: String) -> Self {
        self.event_type = Some(event_type);
        self
    }
    
    pub fn build(self) -> Result<HashMap<String, Message>, BuildError> {
        let user_id = self.user_id.ok_or(BuildError::MissingUserId)?;
        let event_type = self.event_type.ok_or(BuildError::MissingEventType)?;
        
        Ok(HashMap::from([
            ("user_id".to_string(), Message::String(user_id)),
            ("event_type".to_string(), Message::String(event_type)),
            ("timestamp".to_string(), Message::String(Utc::now().to_rfc3339())),
        ]))
    }
}
```

## Testing Message Passing

### Mock Channels

```rust
pub struct MockChannel {
    sent_messages: Arc<Mutex<Vec<HashMap<String, Message>>>>,
    responses: Arc<Mutex<VecDeque<HashMap<String, Message>>>>,
}

impl MockChannel {
    pub fn new() -> Self {
        Self {
            sent_messages: Arc::new(Mutex::new(Vec::new())),
            responses: Arc::new(Mutex::new(VecDeque::new())),
        }
    }
    
    pub fn expect_message(&self, message: HashMap<String, Message>) {
        self.responses.lock().push_back(message);
    }
    
    pub fn verify_sent(&self, expected: &HashMap<String, Message>) -> bool {
        self.sent_messages.lock().contains(expected)
    }
}
```

### Integration Testing

```rust
#[tokio::test]
async fn test_message_pipeline() {
    let (tx1, rx1) = flume::unbounded();
    let (tx2, rx2) = flume::unbounded();
    
    // Create test actors
    let source = TestSourceActor::new(tx1);
    let processor = TestProcessorActor::new(rx1, tx2);
    let sink = TestSinkActor::new(rx2);
    
    // Start actors
    tokio::spawn(source.run());
    tokio::spawn(processor.run());
    tokio::spawn(sink.run());
    
    // Test message flow
    let test_message = HashMap::from([
        ("data".to_string(), Message::String("test".to_string()))
    ]);
    
    source.send(test_message.clone()).await;
    
    // Verify message received
    let received = sink.receive_next().await;
    assert_eq!(received.get("data"), test_message.get("data"));
}
```

## Best Practices

### Message Design

1. **Keep messages immutable** - Never modify after creation
2. **Use appropriate granularity** - Not too fine, not too coarse
3. **Include enough context** - Messages should be self-contained
4. **Design for evolution** - Use versioned message formats

### Performance

1. **Batch when possible** - Reduce overhead
2. **Use appropriate data types** - Binary for large data
3. **Implement backpressure** - Prevent resource exhaustion
4. **Monitor message rates** - Track performance metrics

### Error Handling

1. **Use structured errors** - Include error codes and context
2. **Implement dead letter queues** - Don't lose failed messages
3. **Design for retry** - Make operations idempotent
4. **Log message failures** - Enable debugging

## Next Steps

- [Graph System](./graph-system.md) - Workflow composition
- [Multi-Language Support](./multi-language-support.md) - Script integration
- [Performance Considerations](./performance-considerations.md) - Optimization
- [Creating Actors](../api/actors/creating-actors.md) - Practical implementation
