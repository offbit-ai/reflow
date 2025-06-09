# Standard Library

Reflow's standard library provides a collection of pre-built components for common workflow operations.

## Overview

The standard library includes components for:
- **Flow Control**: Conditional logic, loops, and branching
- **Data Operations**: Transformations, aggregations, and validation
- **Integration**: External API connectivity and data sources
- **Synchronization**: Coordination and timing primitives
- **Utility**: Helper functions and common operations

## Flow Control Components

### ConditionalActor

Routes messages based on conditions:

```rust
use reflow_components::flow_control::ConditionalActor;
use reflow_network::message::Message;

// Create conditional actor
let conditional = ConditionalActor::new(|payload| {
    if let Some(Message::Integer(n)) = payload.get("value") {
        *n > 0
    } else {
        false
    }
});

// Usage in workflow
let mut network = Network::new();
network.add_actor("filter", Box::new(conditional)).await?;
```

### SwitchActor

Multi-way routing based on message content:

```rust
use reflow_components::flow_control::SwitchActor;

let switch = SwitchActor::new()
    .route("user_event", |msg| {
        matches!(msg.get("type"), Some(Message::String(s)) if s == "user")
    })
    .route("system_event", |msg| {
        matches!(msg.get("type"), Some(Message::String(s)) if s == "system")
    })
    .default_route("unknown");
```

### LoopActor

Iterative processing with configurable conditions:

```rust
use reflow_components::flow_control::LoopActor;

let loop_actor = LoopActor::new()
    .max_iterations(100)
    .condition(|payload, iteration| {
        // Continue looping while condition is true
        if let Some(Message::Array(items)) = payload.get("items") {
            !items.is_empty() && iteration < 50
        } else {
            false
        }
    });
```

## Data Operations

### TransformActor

Applies transformations to input data:

```rust
use reflow_components::data_operations::TransformActor;
use reflow_network::{Network, NetworkConfig};

// Create network and add transform actor
let mut network = Network::new(NetworkConfig::default());
network.register_actor("transform", TransformActor::new())?;
network.add_node("transformer", "transform")?;

// Usage: Send data with optional transform function
let transform_request = HashMap::from([
    ("In".to_string(), Message::String("hello world".to_string())),
    ("Function".to_string(), Message::String("uppercase".to_string())),
]);

// Supported transformations:
// - "identity": No change
// - "uppercase": Convert string to uppercase
// - "lowercase": Convert string to lowercase  
// - "number_to_string": Convert numbers to strings
// - "parse_int": Parse string to integer
// - "parse_float": Parse string to float
// - "to_json": Convert to JSON string
// - "from_json": Parse JSON string
```

### MapActor

Apply transformations to each item in a collection:

```rust
use reflow_components::data_operations::MapActor;

// Create map actor
let mut network = Network::new(NetworkConfig::default());
network.register_actor("map", MapActor::new())?;
network.add_node("mapper", "map")?;

// Usage: Transform each item in an array
let map_request = HashMap::from([
    ("Collection".to_string(), Message::Array(vec![
        Message::String("hello".to_string()).into(),
        Message::String("world".to_string()).into(),
    ])),
    ("Function".to_string(), Message::String("uppercase".to_string())),
]);
// Result: ["HELLO", "WORLD"]
```

### ReduceActor

Combine collection items into a single value:

```rust
use reflow_components::data_operations::ReduceActor;

// Create reduce actor
let mut network = Network::new(NetworkConfig::default());
network.register_actor("reduce", ReduceActor::new())?;
network.add_node("reducer", "reduce")?;

// Usage: Sum numbers in an array
let reduce_request = HashMap::from([
    ("Collection".to_string(), Message::Array(vec![
        Message::Integer(1).into(),
        Message::Integer(2).into(),
        Message::Integer(3).into(),
    ])),
    ("Function".to_string(), Message::String("sum".to_string())),
]);
// Result: 6

// Supported operations:
// - "sum": Add all numbers or concatenate strings
// - "product": Multiply all numbers
// - "join": Join strings with separator
// - "max": Find maximum value
// - "min": Find minimum value
```

### GroupActor

Group collection items by key or criteria:

```rust
use reflow_components::data_operations::GroupActor;

// Create group actor
let mut network = Network::new(NetworkConfig::default());
network.register_actor("group", GroupActor::new())?;
network.add_node("grouper", "group")?;

// Usage: Group objects by property
let group_request = HashMap::from([
    ("Collection".to_string(), Message::Array(vec![
        Message::Object(serde_json::json!({"type": "user", "name": "Alice"}).into()).into(),
        Message::Object(serde_json::json!({"type": "admin", "name": "Bob"}).into()).into(),
        Message::Object(serde_json::json!({"type": "user", "name": "Charlie"}).into()).into(),
    ])),
    ("Key".to_string(), Message::String("type".to_string())),
]);
// Result: {"user": [Alice, Charlie], "admin": [Bob]}
```

### SortActor

Sort collection items by criteria:

```rust
use reflow_components::data_operations::SortActor;

// Create sort actor
let mut network = Network::new(NetworkConfig::default());
network.register_actor("sort", SortActor::new())?;
network.add_node("sorter", "sort")?;

// Usage: Sort numbers in descending order
let sort_request = HashMap::from([
    ("Collection".to_string(), Message::Array(vec![
        Message::Integer(3).into(),
        Message::Integer(1).into(),
        Message::Integer(2).into(),
    ])),
    ("Order".to_string(), Message::String("desc".to_string())),
]);
// Result: [3, 2, 1]

// Sort objects by property:
let sort_objects = HashMap::from([
    ("Collection".to_string(), Message::Array(vec![
        Message::Object(serde_json::json!({"age": 30, "name": "Alice"}).into()).into(),
        Message::Object(serde_json::json!({"age": 25, "name": "Bob"}).into()).into(),
    ])),
    ("Key".to_string(), Message::String("age".to_string())),
    ("Order".to_string(), Message::String("asc".to_string())),
]);
// Result: [Bob, Alice] (sorted by age)
```

## Integration Components

### HttpRequestActor

Make HTTP requests:

```rust
use reflow_components::integration::HttpRequestActor;

let http_client = HttpRequestActor::new()
    .timeout(Duration::from_secs(30))
    .retry_count(3)
    .default_headers(vec![
        ("User-Agent".to_string(), "Reflow/1.0".to_string()),
        ("Accept".to_string(), "application/json".to_string()),
    ]);

// Usage: send message with url, method, headers, body
let request = HashMap::from([
    ("url".to_string(), Message::String("https://api.example.com/data".to_string())),
    ("method".to_string(), Message::String("GET".to_string())),
]);
```

### DatabaseActor

Database connectivity:

```rust
use reflow_components::integration::DatabaseActor;

let db_actor = DatabaseActor::new("postgresql://user:pass@localhost/db")
    .pool_size(10)
    .connection_timeout(Duration::from_secs(5));

// Usage: send SQL queries
let query = HashMap::from([
    ("sql".to_string(), Message::String("SELECT * FROM users WHERE active = $1".to_string())),
    ("params".to_string(), Message::Array(vec![Message::Boolean(true)])),
]);
```

### FileSystemActor

File operations:

```rust
use reflow_components::integration::FileSystemActor;

let fs_actor = FileSystemActor::new()
    .base_path("/data")
    .allowed_extensions(vec!["txt", "json", "csv"]);

// Read file
let read_request = HashMap::from([
    ("operation".to_string(), Message::String("read".to_string())),
    ("path".to_string(), Message::String("input.txt".to_string())),
]);

// Write file
let write_request = HashMap::from([
    ("operation".to_string(), Message::String("write".to_string())),
    ("path".to_string(), Message::String("output.txt".to_string())),
    ("content".to_string(), Message::String("Hello, World!".to_string())),
]);
```

## Synchronization Components

### BarrierActor

Wait for multiple inputs before proceeding:

```rust
use reflow_components::synchronization::BarrierActor;

let barrier = BarrierActor::new()
    .input_count(3) // Wait for 3 inputs
    .timeout(Duration::from_secs(60)) // Max wait time
    .combine_strategy(CombineStrategy::Merge); // How to combine inputs

// Outputs combined message when all inputs received
```

### ThrottleActor

Rate limiting:

```rust
use reflow_components::synchronization::ThrottleActor;

let throttle = ThrottleActor::new()
    .rate_limit(100) // Max 100 messages per second
    .burst_size(10)  // Allow burst of 10 messages
    .strategy(ThrottleStrategy::DropOldest);
```

### DelayActor

Add delays to message processing:

```rust
use reflow_components::synchronization::DelayActor;

let delay = DelayActor::new()
    .fixed_delay(Duration::from_millis(500))
    .jitter_range(Duration::from_millis(100)); // Add random jitter
```

### SchedulerActor

Time-based message generation:

```rust
use reflow_components::synchronization::SchedulerActor;

let scheduler = SchedulerActor::new()
    .cron_schedule("0 0 * * *") // Daily at midnight
    .message_template(HashMap::from([
        ("event".to_string(), Message::String("daily_job".to_string())),
        ("timestamp".to_string(), Message::String("{{now}}".to_string())),
    ]));
```

## Utility Components

### LoggerActor

Structured logging:

```rust
use reflow_components::utility::LoggerActor;

let logger = LoggerActor::new()
    .level(LogLevel::Info)
    .format(LogFormat::Json)
    .output(LogOutput::File("/var/log/reflow.log"));

// Usage: send messages to log
let log_message = HashMap::from([
    ("level".to_string(), Message::String("info".to_string())),
    ("message".to_string(), Message::String("Processing started".to_string())),
    ("context".to_string(), Message::Object(context_data)),
]);
```

### MetricsActor

Collect and emit metrics:

```rust
use reflow_components::utility::MetricsActor;

let metrics = MetricsActor::new()
    .endpoint("http://prometheus:9090/metrics")
    .namespace("reflow")
    .labels(vec![
        ("environment".to_string(), "production".to_string()),
        ("service".to_string(), "workflow-engine".to_string()),
    ]);

// Usage: send metrics data
let metric = HashMap::from([
    ("type".to_string(), Message::String("counter".to_string())),
    ("name".to_string(), Message::String("messages_processed".to_string())),
    ("value".to_string(), Message::Integer(1)),
    ("tags".to_string(), Message::Object(tags)),
]);
```

### CacheActor

In-memory caching:

```rust
use reflow_components::utility::CacheActor;

let cache = CacheActor::new()
    .max_size(1000)
    .ttl(Duration::from_hours(1))
    .eviction_strategy(EvictionStrategy::LRU);

// Get from cache
let get_request = HashMap::from([
    ("operation".to_string(), Message::String("get".to_string())),
    ("key".to_string(), Message::String("user:123".to_string())),
]);

// Set cache value
let set_request = HashMap::from([
    ("operation".to_string(), Message::String("set".to_string())),
    ("key".to_string(), Message::String("user:123".to_string())),
    ("value".to_string(), Message::Object(user_data)),
    ("ttl".to_string(), Message::Integer(3600)), // Optional custom TTL
]);
```

## Configuration Examples

### Building a Data Pipeline

```rust
use reflow_network::Network;
use reflow_components::*;

async fn create_data_pipeline() -> Result<Network, Box<dyn std::error::Error>> {
    let mut network = Network::new();
    
    // 1. HTTP source to fetch data
    let http_source = integration::HttpRequestActor::new()
        .timeout(Duration::from_secs(30));
    
    // 2. Validate incoming data
    let validator = data_operations::ValidatorActor::new()
        .add_rule("required_field", |v| !matches!(v, Message::Null));
    
    // 3. Transform data
    let transformer = data_operations::MapActor::new(|payload| {
        // Custom transformation logic
        transform_data(payload)
    });
    
    // 4. Filter based on criteria
    let filter = data_operations::FilterActor::new(|payload| {
        filter_criteria(payload)
    });
    
    // 5. Aggregate results
    let aggregator = data_operations::AggregateActor::new()
        .window_size(100)
        .timeout(Duration::from_secs(60));
    
    // 6. Store results
    let database = integration::DatabaseActor::new("postgresql://...")
        .pool_size(5);
    
    // 7. Log activity
    let logger = utility::LoggerActor::new()
        .level(LogLevel::Info);
    
    // Add actors to network
    network.add_actor("http_source", Box::new(http_source)).await?;
    network.add_actor("validator", Box::new(validator)).await?;
    network.add_actor("transformer", Box::new(transformer)).await?;
    network.add_actor("filter", Box::new(filter)).await?;
    network.add_actor("aggregator", Box::new(aggregator)).await?;
    network.add_actor("database", Box::new(database)).await?;
    network.add_actor("logger", Box::new(logger)).await?;
    
    // Connect the pipeline
    network.connect("http_source", "output", "validator", "input").await?;
    network.connect("validator", "valid", "transformer", "input").await?;
    network.connect("transformer", "output", "filter", "input").await?;
    network.connect("filter", "output", "aggregator", "input").await?;
    network.connect("aggregator", "output", "database", "input").await?;
    
    // Log all stages
    network.connect("validator", "output", "logger", "input").await?;
    network.connect("transformer", "output", "logger", "input").await?;
    network.connect("aggregator", "output", "logger", "input").await?;
    
    Ok(network)
}

fn transform_data(payload: &HashMap<String, Message>) -> Result<HashMap<String, Message>, anyhow::Error> {
    // Implementation details...
    Ok(HashMap::new())
}

fn filter_criteria(payload: &HashMap<String, Message>) -> bool {
    // Implementation details...
    true
}
```

### Real-time Processing Workflow

```rust
async fn create_realtime_workflow() -> Result<Network, Box<dyn std::error::Error>> {
    let mut network = Network::new();
    
    // Stream processor with throttling
    let throttle = synchronization::ThrottleActor::new()
        .rate_limit(1000) // 1000 msgs/sec
        .burst_size(50);
    
    // Real-time analytics
    let analytics = data_operations::AggregateActor::new()
        .window_size(1000)
        .timeout(Duration::from_secs(5)) // 5-second windows
        .aggregation_fn(|messages| {
            let mut stats = HashMap::new();
            
            // Calculate real-time statistics
            let count = messages.len() as i64;
            let avg_processing_time = calculate_avg_time(&messages);
            
            stats.insert("count".to_string(), Message::Integer(count));
            stats.insert("avg_time".to_string(), Message::Float(avg_processing_time));
            stats.insert("window_end".to_string(), 
                        Message::String(chrono::Utc::now().to_rfc3339()));
            
            stats
        });
    
    // Metrics collection
    let metrics = utility::MetricsActor::new()
        .namespace("realtime")
        .endpoint("http://prometheus:9090/metrics");
    
    // Alert on anomalies
    let anomaly_detector = flow_control::ConditionalActor::new(|payload| {
        if let Some(Message::Float(avg_time)) = payload.get("avg_time") {
            *avg_time > 1000.0 // Alert if avg time > 1 second
        } else {
            false
        }
    });
    
    // Add to network
    network.add_actor("throttle", Box::new(throttle)).await?;
    network.add_actor("analytics", Box::new(analytics)).await?;
    network.add_actor("metrics", Box::new(metrics)).await?;
    network.add_actor("anomaly_detector", Box::new(anomaly_detector)).await?;
    
    // Connect pipeline
    network.connect("throttle", "output", "analytics", "input").await?;
    network.connect("analytics", "output", "metrics", "input").await?;
    network.connect("analytics", "output", "anomaly_detector", "input").await?;
    
    Ok(network)
}

fn calculate_avg_time(messages: &[HashMap<String, Message>]) -> f64 {
    // Calculate average processing time
    0.0
}
```

## Custom Component Creation

### Creating a Custom Component

```rust
use reflow_components::ComponentBuilder;

// Define component configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CustomConfig {
    threshold: f64,
    operation: String,
}

// Create component using builder
let custom_component = ComponentBuilder::new("custom_processor")
    .description("Custom data processor")
    .input_ports(vec!["data", "control"])
    .output_ports(vec!["result", "error"])
    .config_schema(serde_json::to_value(CustomConfig::default())?)
    .behavior(|payload, config| {
        Box::pin(async move {
            let config: CustomConfig = serde_json::from_value(config)?;
            
            // Custom processing logic
            process_custom_logic(payload, &config).await
        })
    })
    .build()?;
```

### Component Registration

```rust
use reflow_components::ComponentRegistry;

// Register custom components
let mut registry = ComponentRegistry::new();

registry.register("custom_processor", custom_component)?;
registry.register("special_filter", special_filter_component)?;

// Use in workflows
let component = registry.create("custom_processor", custom_config)?;
network.add_actor("processor", component).await?;
```

## Error Handling

### Error Propagation

Components follow consistent error handling patterns:

```rust
// Components return structured errors
let error_result = HashMap::from([
    ("error".to_string(), Message::Error("Validation failed".to_string())),
    ("error_code".to_string(), Message::String("VALIDATION_ERROR".to_string())),
    ("details".to_string(), Message::Object(error_details)),
    ("timestamp".to_string(), Message::String(Utc::now().to_rfc3339())),
]);
```

### Error Recovery

```rust
// Use error handling components
let error_handler = flow_control::ConditionalActor::new(|payload| {
    payload.contains_key("error")
});

let retry_actor = utility::RetryActor::new()
    .max_attempts(3)
    .backoff_strategy(BackoffStrategy::Exponential);

// Connect for error recovery
network.connect("processor", "error", "error_handler", "input").await?;
network.connect("error_handler", "true", "retry_actor", "input").await?;
network.connect("retry_actor", "output", "processor", "input").await?;
```

## Performance Tuning

### Batching

```rust
// Use batching for high-throughput scenarios
let batch_processor = data_operations::MapActor::new(|payload| {
    if let Some(Message::Array(batch)) = payload.get("batch") {
        // Process entire batch at once
        process_batch(batch)
    } else {
        // Handle single message
        process_single(payload)
    }
});

let batcher = utility::BatchActor::new()
    .batch_size(100)
    .timeout(Duration::from_millis(100));
```

### Parallel Processing

```rust
// Distribute work across multiple workers
let load_balancer = flow_control::LoadBalancerActor::new()
    .strategy(LoadBalanceStrategy::RoundRobin)
    .worker_count(4);

// Workers process in parallel
for i in 0..4 {
    let worker = data_operations::MapActor::new(process_function);
    network.add_actor(&format!("worker_{}", i), Box::new(worker)).await?;
    network.connect("load_balancer", &format!("output_{}", i), 
                   &format!("worker_{}", i), "input").await?;
}
```

## Best Practices

### Component Selection

1. **Use appropriate granularity** - Not too fine, not too coarse
2. **Prefer composition** - Combine simple components over complex ones
3. **Consider performance** - Choose components based on throughput requirements
4. **Plan for errors** - Include error handling components in workflows

### Configuration

1. **Validate configuration** - Use schema validation
2. **Use environment variables** - For deployment-specific settings
3. **Document requirements** - Clear component dependencies
4. **Test configurations** - Validate settings before deployment

### Monitoring

1. **Add logging** - Include LoggerActor for observability
2. **Collect metrics** - Use MetricsActor for performance monitoring
3. **Set up alerts** - Use ConditionalActor for anomaly detection
4. **Monitor resource usage** - Track memory and CPU usage

## Next Steps

- [Creating Custom Components](./custom-components.md) - Build your own components
- [Component Testing](./component-testing.md) - Testing strategies
- [Performance Guide](./performance-optimization.md) - Optimization techniques
- [Examples](../examples/README.md) - Real-world component usage
