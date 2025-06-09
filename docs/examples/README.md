# Examples and Tutorials

This section provides practical examples and tutorials for building workflows with Reflow.

## Quick Reference

### Tutorials
- **[Audio Processing Flow](./tutorials/audio-processing-flow.md)** - Real-time audio processing pipeline
- **[Data ETL Pipeline](./tutorials/data-etl-pipeline.md)** - Extract, transform, load workflow
- **[Web API Integration](./tutorials/web-api-integration.md)** - REST API consumption and processing
- **[Real-time Analytics](./tutorials/real-time-analytics.md)** - Stream processing and aggregation

### Use Cases
- **[IoT Data Processing](./use-cases/iot-data-processing.md)** - Sensor data collection and analysis
- **[Log Processing](./use-cases/log-processing.md)** - Log aggregation and monitoring
- **[Image Processing](./use-cases/image-processing.md)** - Computer vision workflows
- **[Financial Trading](./use-cases/financial-trading.md)** - Trading algorithms and risk management

### Code Samples
- **[Simple Examples](./code-samples/simple-examples.md)** - Basic workflow patterns
- **[Advanced Patterns](./code-samples/advanced-patterns.md)** - Complex workflow compositions
- **[Error Handling](./code-samples/error-handling.md)** - Robust error management
- **[Performance Optimization](./code-samples/performance-optimization.md)** - High-throughput workflows

## Getting Started Examples

### Hello World Workflow

The simplest possible workflow:

```rust
use reflow_network::Network;
use reflow_components::{utility::LoggerActor, data_operations::MapActor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut network = Network::new();
    
    // Create a simple transformer
    let transformer = MapActor::new(|payload| {
        let mut result = HashMap::new();
        result.insert("message".to_string(), 
                     Message::String("Hello, World!".to_string()));
        Ok(result)
    });
    
    // Create a logger
    let logger = LoggerActor::new()
        .level(LogLevel::Info)
        .format(LogFormat::Pretty);
    
    // Add to network
    network.add_actor("transformer", Box::new(transformer)).await?;
    network.add_actor("logger", Box::new(logger)).await?;
    
    // Connect them
    network.connect("transformer", "output", "logger", "input").await?;
    
    // Start the network
    network.start().await?;
    
    Ok(())
}
```

### Basic Data Processing

```rust
use reflow_components::*;

async fn create_basic_pipeline() -> Result<Network, Box<dyn std::error::Error>> {
    let mut network = Network::new();
    
    // 1. Data source (HTTP endpoint)
    let source = integration::HttpRequestActor::new()
        .timeout(Duration::from_secs(30));
    
    // 2. Data validation
    let validator = data_operations::ValidatorActor::new()
        .add_rule("required", |v| !matches!(v, Message::Null))
        .add_rule("positive", |v| {
            if let Message::Integer(n) = v { *n > 0 } else { true }
        });
    
    // 3. Data transformation
    let transformer = data_operations::MapActor::new(|payload| {
        let mut result = HashMap::new();
        
        // Transform each field
        for (key, value) in payload {
            let transformed = match value {
                Message::String(s) => Message::String(s.to_uppercase()),
                Message::Integer(n) => Message::Integer(n * 2),
                other => other.clone(),
            };
            result.insert(format!("transformed_{}", key), transformed);
        }
        
        Ok(result)
    });
    
    // 4. Output logging
    let logger = utility::LoggerActor::new();
    
    // Build network
    network.add_actor("source", Box::new(source)).await?;
    network.add_actor("validator", Box::new(validator)).await?;
    network.add_actor("transformer", Box::new(transformer)).await?;
    network.add_actor("logger", Box::new(logger)).await?;
    
    // Connect pipeline
    network.connect("source", "output", "validator", "input").await?;
    network.connect("validator", "valid", "transformer", "input").await?;
    network.connect("transformer", "output", "logger", "input").await?;
    
    Ok(network)
}
```

## JavaScript Integration

### Deno Script Actor

```javascript
// scripts/data_processor.js
function process(inputs, context) {
    const data = inputs.data;
    
    if (!Array.isArray(data)) {
        return { error: "Expected array input" };
    }
    
    // Process data
    const processed = data
        .filter(item => item.value > 0)
        .map(item => ({
            ...item,
            processed: true,
            timestamp: new Date().toISOString(),
            hash: calculateHash(item)
        }))
        .sort((a, b) => b.value - a.value);
    
    return {
        processed_data: processed,
        count: processed.length,
        max_value: processed[0]?.value || 0
    };
}

function calculateHash(item) {
    // Simple hash function
    return btoa(JSON.stringify(item)).slice(0, 8);
}

exports.process = process;
```

```rust
// Rust integration
use reflow_script::{ScriptActor, ScriptConfig, ScriptRuntime, ScriptEnvironment};

let script_config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::JavaScript,
    source: std::fs::read("scripts/data_processor.js")?,
    entry_point: "process".to_string(),
    packages: None,
};

let script_actor = ScriptActor::new(script_config);
```

## Real-World Patterns

### Error Handling with Retry

```rust
use reflow_components::{flow_control::ConditionalActor, utility::RetryActor};

async fn create_robust_pipeline() -> Result<Network, Box<dyn std::error::Error>> {
    let mut network = Network::new();
    
    // Main processor (might fail)
    let processor = data_operations::MapActor::new(|payload| {
        // Simulate occasional failures
        if payload.contains_key("trigger_error") {
            return Err(anyhow::anyhow!("Simulated processing error"));
        }
        
        // Normal processing
        Ok(payload.clone())
    });
    
    // Error detector
    let error_detector = ConditionalActor::new(|payload| {
        payload.contains_key("error")
    });
    
    // Retry actor
    let retry_actor = RetryActor::new()
        .max_attempts(3)
        .backoff_strategy(BackoffStrategy::Exponential)
        .base_delay(Duration::from_millis(100));
    
    // Success logger
    let success_logger = utility::LoggerActor::new()
        .level(LogLevel::Info);
    
    // Error logger
    let error_logger = utility::LoggerActor::new()
        .level(LogLevel::Error);
    
    // Build network
    network.add_actor("processor", Box::new(processor)).await?;
    network.add_actor("error_detector", Box::new(error_detector)).await?;
    network.add_actor("retry_actor", Box::new(retry_actor)).await?;
    network.add_actor("success_logger", Box::new(success_logger)).await?;
    network.add_actor("error_logger", Box::new(error_logger)).await?;
    
    // Connect main flow
    network.connect("processor", "output", "error_detector", "input").await?;
    network.connect("error_detector", "false", "success_logger", "input").await?;
    network.connect("error_detector", "true", "retry_actor", "input").await?;
    
    // Retry loop
    network.connect("retry_actor", "retry", "processor", "input").await?;
    network.connect("retry_actor", "failed", "error_logger", "input").await?;
    
    Ok(network)
}
```

### High-Throughput Processing

```rust
use reflow_components::{flow_control::LoadBalancerActor, synchronization::BatchActor};

async fn create_high_throughput_pipeline() -> Result<Network, Box<dyn std::error::Error>> {
    let mut network = Network::new();
    
    // Input batching
    let batcher = BatchActor::new()
        .batch_size(100)
        .timeout(Duration::from_millis(50));
    
    // Load balancer
    let load_balancer = LoadBalancerActor::new()
        .strategy(LoadBalanceStrategy::RoundRobin)
        .worker_count(4);
    
    // Worker actors (parallel processing)
    for i in 0..4 {
        let worker = data_operations::MapActor::new(|payload| {
            // CPU-intensive processing
            process_batch(payload)
        });
        
        network.add_actor(&format!("worker_{}", i), Box::new(worker)).await?;
        network.connect("load_balancer", &format!("output_{}", i),
                       &format!("worker_{}", i), "input").await?;
    }
    
    // Result aggregator
    let aggregator = data_operations::AggregateActor::new()
        .window_size(4) // Collect from all workers
        .timeout(Duration::from_secs(1))
        .aggregation_fn(|results| {
            // Combine results from all workers
            combine_worker_results(results)
        });
    
    // Connect workers to aggregator
    for i in 0..4 {
        network.connect(&format!("worker_{}", i), "output",
                       "aggregator", "input").await?;
    }
    
    network.add_actor("batcher", Box::new(batcher)).await?;
    network.add_actor("load_balancer", Box::new(load_balancer)).await?;
    network.add_actor("aggregator", Box::new(aggregator)).await?;
    
    network.connect("batcher", "output", "load_balancer", "input").await?;
    
    Ok(network)
}

fn process_batch(payload: &HashMap<String, Message>) -> Result<HashMap<String, Message>, anyhow::Error> {
    // Simulate CPU-intensive work
    thread::sleep(Duration::from_millis(10));
    Ok(payload.clone())
}

fn combine_worker_results(results: &[HashMap<String, Message>]) -> HashMap<String, Message> {
    let mut combined = HashMap::new();
    
    let total_processed = results.len() as i64;
    combined.insert("total_processed".to_string(), Message::Integer(total_processed));
    combined.insert("timestamp".to_string(), 
                   Message::String(chrono::Utc::now().to_rfc3339()));
    
    combined
}
```

## Testing Workflows

### Unit Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};
    
    #[tokio::test]
    async fn test_data_pipeline() {
        let network = create_basic_pipeline().await.unwrap();
        
        // Send test data
        let test_data = HashMap::from([
            ("value".to_string(), Message::Integer(42)),
            ("name".to_string(), Message::String("test".to_string())),
        ]);
        
        // Get input port and send data
        let input_port = network.get_actor_input("source").unwrap();
        input_port.send_async(test_data).await.unwrap();
        
        // Wait for processing
        timeout(Duration::from_secs(5), async {
            // Check that data was processed
            // This would require network introspection capabilities
        }).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_error_handling() {
        let network = create_robust_pipeline().await.unwrap();
        
        // Send data that triggers error
        let error_data = HashMap::from([
            ("trigger_error".to_string(), Message::Boolean(true)),
        ]);
        
        // Verify error handling works correctly
        // Implementation depends on network monitoring capabilities
    }
}
```

### Integration Testing

```rust
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::test]
async fn test_full_workflow_integration() {
    // Shared state for test validation
    let results = Arc::new(Mutex::new(Vec::new()));
    let results_clone = results.clone();
    
    // Create custom sink actor for testing
    let test_sink = TestSinkActor::new(move |payload| {
        let results = results_clone.clone();
        Box::pin(async move {
            let mut results_guard = results.lock().await;
            results_guard.push(payload.clone());
            Ok(HashMap::new())
        })
    });
    
    let mut network = Network::new();
    
    // Build test network
    let source = create_test_source();
    let processor = create_test_processor();
    
    network.add_actor("source", Box::new(source)).await.unwrap();
    network.add_actor("processor", Box::new(processor)).await.unwrap();
    network.add_actor("sink", Box::new(test_sink)).await.unwrap();
    
    network.connect("source", "output", "processor", "input").await.unwrap();
    network.connect("processor", "output", "sink", "input").await.unwrap();
    
    // Start network
    let handle = tokio::spawn(async move {
        network.start().await
    });
    
    // Send test data
    // ... implementation details
    
    // Wait and verify results
    tokio::time::sleep(Duration::from_secs(2)).await;
    
    let final_results = results.lock().await;
    assert!(!final_results.is_empty());
    assert_eq!(final_results.len(), 3); // Expected number of processed messages
    
    handle.abort();
}
```

## Performance Examples

### Benchmarking

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_message_processing(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    c.bench_function("process_1000_messages", |b| {
        b.iter(|| {
            rt.block_on(async {
                let network = create_high_throughput_pipeline().await.unwrap();
                
                // Send 1000 messages
                for i in 0..1000 {
                    let message = HashMap::from([
                        ("id".to_string(), Message::Integer(i)),
                        ("data".to_string(), Message::String(format!("data_{}", i))),
                    ]);
                    
                    // Send message
                    black_box(send_message(&network, message).await);
                }
                
                // Wait for completion
                wait_for_completion(&network).await;
            })
        })
    });
}

criterion_group!(benches, benchmark_message_processing);
criterion_main!(benches);
```

### Memory Profiling

```rust
use memory_stats::memory_stats;

async fn profile_memory_usage() {
    let initial_memory = memory_stats().unwrap().physical_mem;
    println!("Initial memory: {} bytes", initial_memory);
    
    // Create large workflow
    let network = create_memory_intensive_workflow().await.unwrap();
    
    let after_creation = memory_stats().unwrap().physical_mem;
    println!("After creation: {} bytes", after_creation);
    println!("Creation overhead: {} bytes", after_creation - initial_memory);
    
    // Process data
    for batch in 0..10 {
        process_large_batch(&network, batch).await;
        
        let current_memory = memory_stats().unwrap().physical_mem;
        println!("After batch {}: {} bytes", batch, current_memory);
    }
    
    // Cleanup
    drop(network);
    tokio::time::sleep(Duration::from_secs(1)).await; // Allow GC
    
    let final_memory = memory_stats().unwrap().physical_mem;
    println!("Final memory: {} bytes", final_memory);
}
```

## Configuration Examples

### Environment-Based Configuration

```toml
# config/development.toml
[runtime]
thread_pool_size = 2
log_level = "debug"
hot_reload = true

[performance]
batch_size = 10
timeout_ms = 1000

[scripts]
enable_deno = true
enable_python = false
```

```toml
# config/production.toml
[runtime]
thread_pool_size = 16
log_level = "info"
hot_reload = false

[performance]
batch_size = 1000
timeout_ms = 5000

[scripts]
enable_deno = true
enable_python = true
```

```rust
// Configuration loading
use config::{Config, Environment, File};

#[derive(Debug, Deserialize)]
struct AppConfig {
    runtime: RuntimeConfig,
    performance: PerformanceConfig,
    scripts: ScriptConfig,
}

fn load_configuration() -> Result<AppConfig, config::ConfigError> {
    let env = std::env::var("REFLOW_ENV").unwrap_or_else(|_| "development".into());
    
    let settings = Config::builder()
        .add_source(File::with_name("config/default"))
        .add_source(File::with_name(&format!("config/{}", env)).required(false))
        .add_source(File::with_name("config/local").required(false))
        .add_source(Environment::with_prefix("REFLOW").separator("__"))
        .build()?;
    
    settings.try_deserialize()
}
```

## Deployment Examples

### Docker Composition

```yaml
# docker-compose.yml
version: '3.8'

services:
  reflow-app:
    build: .
    ports:
      - "8080:8080"
    environment:
      - REFLOW_ENV=production
      - RUST_LOG=info
    volumes:
      - ./config:/app/config:ro
      - ./data:/app/data
    depends_on:
      - postgres
      - redis
    restart: unless-stopped
    
  postgres:
    image: postgres:13
    environment:
      POSTGRES_DB: reflow
      POSTGRES_USER: reflow
      POSTGRES_PASSWORD: ${DB_PASSWORD}
    volumes:
      - postgres_data:/var/lib/postgresql/data
    
  redis:
    image: redis:6-alpine
    command: redis-server --appendonly yes
    volumes:
      - redis_data:/data

volumes:
  postgres_data:
  redis_data:
```

### Kubernetes Deployment

```yaml
# k8s/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: reflow-app
spec:
  replicas: 3
  selector:
    matchLabels:
      app: reflow-app
  template:
    metadata:
      labels:
        app: reflow-app
    spec:
      containers:
      - name: reflow-app
        image: reflow:latest
        ports:
        - containerPort: 8080
        env:
        - name: REFLOW_ENV
          value: "production"
        - name: RUST_LOG
          value: "info"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5
```

## Next Steps

Explore specific tutorials and use cases:

- **[Audio Processing Tutorial](./tutorials/audio-processing-flow.md)** - Build a real-time audio pipeline
- **[Data ETL Tutorial](./tutorials/data-etl-pipeline.md)** - Create a data processing workflow
- **[API Integration Tutorial](./tutorials/web-api-integration.md)** - Connect to external services
- **[IoT Use Case](./use-cases/iot-data-processing.md)** - Process sensor data streams

For more advanced topics:
- **[Performance Optimization](../architecture/performance-considerations.md)**
- **[Advanced Patterns](./code-samples/advanced-patterns.md)**
- **[Custom Components](../components/custom-components.md)**
