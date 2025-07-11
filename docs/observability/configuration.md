# Configuration

Reflow's observability framework provides flexible configuration options to suit different deployment scenarios and performance requirements.

## Basic Configuration

### TracingConfig Structure

```rust
use reflow_network::tracing::TracingConfig;
use std::time::Duration;

let config = TracingConfig {
    server_url: "ws://localhost:8080".to_string(),
    batch_size: 50,
    batch_timeout: Duration::from_millis(1000),
    enable_compression: true,
    enabled: true,
    retry_config: RetryConfig {
        max_retries: 3,
        initial_delay: Duration::from_millis(500),
        max_delay: Duration::from_secs(5),
        backoff_multiplier: 2.0,
    },
};
```

### Configuration Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `server_url` | String | `"ws://localhost:8080"` | WebSocket URL of the tracing server |
| `batch_size` | usize | `50` | Number of events to batch before sending |
| `batch_timeout` | Duration | `1000ms` | Maximum time to wait before sending incomplete batch |
| `enable_compression` | bool | `true` | Enable gzip compression for network transmission |
| `enabled` | bool | `true` | Global enable/disable switch for tracing |
| `retry_config` | RetryConfig | See below | Configuration for retry logic |

### Retry Configuration

```rust
pub struct RetryConfig {
    pub max_retries: u32,           // Maximum retry attempts
    pub initial_delay: Duration,    // Initial delay before first retry
    pub max_delay: Duration,        // Maximum delay between retries
    pub backoff_multiplier: f64,    // Exponential backoff multiplier
}
```

## Environment-Based Configuration

### Using Environment Variables

```bash
# Basic tracing configuration
export REFLOW_TRACING_ENABLED=true
export REFLOW_TRACING_SERVER_URL="ws://tracing-server:8080"
export REFLOW_TRACING_BATCH_SIZE=100
export REFLOW_TRACING_BATCH_TIMEOUT_MS=2000

# Compression and retry settings
export REFLOW_TRACING_COMPRESSION=true
export REFLOW_TRACING_MAX_RETRIES=5
export REFLOW_TRACING_INITIAL_DELAY_MS=1000
export REFLOW_TRACING_MAX_DELAY_MS=30000
```

### Configuration from Environment

```rust
use reflow_network::tracing::TracingConfig;

let config = TracingConfig::from_env().unwrap_or_default();
```

## File-Based Configuration

### TOML Configuration

```toml
# tracing.toml
[tracing]
enabled = true
server_url = "ws://localhost:8080"
batch_size = 50
batch_timeout_ms = 1000
enable_compression = true

[tracing.retry]
max_retries = 3
initial_delay_ms = 500
max_delay_ms = 5000
backoff_multiplier = 2.0

[tracing.filters]
# Optional: Configure event filtering
actor_patterns = ["sensor_*", "processor_*"]
exclude_actors = ["debug_*", "test_*"]
event_types = ["ActorCreated", "DataFlow", "ActorFailed"]
```

### Loading from File

```rust
use reflow_network::tracing::TracingConfig;

let config = TracingConfig::from_file("tracing.toml")?;
```

## Performance Tuning

### High-Throughput Scenarios

For systems with high message throughput:

```rust
let config = TracingConfig {
    batch_size: 200,                // Larger batches
    batch_timeout: Duration::from_millis(5000), // Longer timeout
    enable_compression: true,       // Reduce network overhead
    retry_config: RetryConfig {
        max_retries: 5,            // More resilient
        initial_delay: Duration::from_millis(100),
        max_delay: Duration::from_secs(30),
        backoff_multiplier: 1.5,   // Gentler backoff
    },
    ..Default::default()
};
```

### Low-Latency Requirements

For real-time monitoring needs:

```rust
let config = TracingConfig {
    batch_size: 1,                  // Send immediately
    batch_timeout: Duration::from_millis(10), // Very short timeout
    enable_compression: false,      // Reduce CPU overhead
    retry_config: RetryConfig {
        max_retries: 1,            // Fast failure
        initial_delay: Duration::from_millis(50),
        max_delay: Duration::from_millis(500),
        backoff_multiplier: 2.0,
    },
    ..Default::default()
};
```

### Memory-Constrained Environments

For embedded or resource-limited deployments:

```rust
let config = TracingConfig {
    batch_size: 10,                 // Small batches
    batch_timeout: Duration::from_millis(500),
    enable_compression: true,       // Save memory in transit
    retry_config: RetryConfig {
        max_retries: 2,            // Limit retry overhead
        initial_delay: Duration::from_millis(1000),
        max_delay: Duration::from_secs(10),
        backoff_multiplier: 2.0,
    },
    ..Default::default()
};
```

## Event Filtering

### Actor-Based Filtering

```rust
use reflow_network::tracing::{TracingConfig, EventFilter};

let filter = EventFilter::new()
    .include_actors(&["critical_*", "payment_*"])
    .exclude_actors(&["debug_*", "test_*"])
    .include_event_types(&[
        TraceEventType::ActorCreated,
        TraceEventType::ActorFailed,
        TraceEventType::DataFlow { to_actor: "*".to_string(), to_port: "*".to_string() }
    ]);

let config = TracingConfig::default()
    .with_filter(filter);
```

### Sampling Configuration

```rust
use reflow_network::tracing::SamplingStrategy;

// Sample 10% of all events
let config = TracingConfig::default()
    .with_sampling(SamplingStrategy::Percentage(10.0));

// Sample every 5th event
let config = TracingConfig::default()
    .with_sampling(SamplingStrategy::EveryNth(5));

// Adaptive sampling based on load
let config = TracingConfig::default()
    .with_sampling(SamplingStrategy::Adaptive {
        base_rate: 10.0,
        max_rate: 100.0,
        load_threshold: 0.8,
    });
```

## Security Configuration

### TLS/SSL Configuration

```rust
use reflow_network::tracing::{TracingConfig, TlsConfig};

let tls_config = TlsConfig {
    ca_cert_path: Some("ca-cert.pem".to_string()),
    client_cert_path: Some("client-cert.pem".to_string()),
    client_key_path: Some("client-key.pem".to_string()),
    verify_hostname: true,
};

let config = TracingConfig {
    server_url: "wss://secure-tracing-server:8443".to_string(),
    tls_config: Some(tls_config),
    ..Default::default()
};
```

### Authentication

```rust
use reflow_network::tracing::AuthConfig;

let auth_config = AuthConfig::ApiKey {
    key: "your-api-key".to_string(),
    header: "X-API-Key".to_string(),
};

let config = TracingConfig {
    auth_config: Some(auth_config),
    ..Default::default()
};
```

## Dynamic Configuration

### Runtime Configuration Updates

```rust
use reflow_network::tracing;

// Get current global configuration
let current_config = tracing::get_global_config();

// Update configuration at runtime
let updated_config = current_config
    .with_batch_size(100)
    .with_compression(false);

tracing::update_global_config(updated_config)?;
```

### Configuration Monitoring

```rust
use reflow_network::tracing::ConfigWatcher;

// Watch for configuration file changes
let watcher = ConfigWatcher::new("tracing.toml")?;
watcher.on_change(|new_config| {
    println!("Configuration updated: {:?}", new_config);
    tracing::update_global_config(new_config)
})?;
```

## Validation and Testing

### Configuration Validation

```rust
use reflow_network::tracing::TracingConfig;

let config = TracingConfig::default();

// Validate configuration
if let Err(e) = config.validate() {
    eprintln!("Invalid configuration: {}", e);
    return Err(e);
}

// Test connection
if config.test_connection().await.is_err() {
    eprintln!("Cannot connect to tracing server");
}
```

### Configuration Examples

```rust
// Development configuration
let dev_config = TracingConfig {
    server_url: "ws://localhost:8080".to_string(),
    batch_size: 10,
    batch_timeout: Duration::from_millis(100),
    enable_compression: false,
    enabled: true,
    ..Default::default()
};

// Production configuration
let prod_config = TracingConfig {
    server_url: "wss://tracing.prod.company.com:443".to_string(),
    batch_size: 100,
    batch_timeout: Duration::from_millis(2000),
    enable_compression: true,
    enabled: true,
    tls_config: Some(TlsConfig::default()),
    auth_config: Some(AuthConfig::from_env()?),
    ..Default::default()
};

// Testing configuration (disabled)
let test_config = TracingConfig {
    enabled: false,
    ..Default::default()
};
```

## Best Practices

1. **Start Conservative**: Begin with small batch sizes and short timeouts, then tune based on observed performance.

2. **Monitor Overhead**: Track the performance impact of tracing and adjust configuration accordingly.

3. **Use Environment Variables**: Make configuration environment-specific without code changes.

4. **Enable Compression**: For network-constrained environments, compression typically provides significant benefits.

5. **Configure Retries**: Set appropriate retry parameters based on your network reliability.

6. **Filter Strategically**: Use event filtering to reduce overhead while maintaining necessary observability.

7. **Secure Connections**: Always use TLS in production environments.

8. **Test Configuration**: Validate configuration in development and staging environments.

9. **Document Settings**: Maintain clear documentation of configuration choices and their rationale.

10. **Version Configuration**: Track configuration changes alongside code changes.
