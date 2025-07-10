# Reflow Tracing Integration Example

This example demonstrates the complete end-to-end integration of the reflow_tracing system with reflow_network clients. It showcases how to run reflow_tracing as a standalone service and connect multiple client applications to it for comprehensive observability.

## Architecture Overview

```
┌─────────────────┐    WebSocket    ┌──────────────────┐
│                 │ ───────────────▶ │                  │
│ reflow_network  │                 │ reflow_tracing   │
│    clients      │ ◀─────────────── │     server       │
│                 │    trace events │                  │
└─────────────────┘                 └──────────────────┘
        │                                    │
        │                                    │
        ▼                                    ▼
┌─────────────────┐                 ┌──────────────────┐
│ Actor Networks  │                 │ Storage Backend  │
│ - Simple Demo   │                 │ - SQLite         │
│ - Complex Flow  │                 │ - PostgreSQL     │
│ - Performance   │                 │ - Memory         │
└─────────────────┘                 └──────────────────┘
```

## Features Demonstrated

### Server Features
- **Standalone Service**: reflow_tracing runs as an independent service
- **Multiple Storage Backends**: SQLite, PostgreSQL, Memory-based storage
- **WebSocket Protocol**: Real-time bidirectional communication
- **Compression**: zstd, lz4, brotli, gzip compression support
- **Configuration**: File-based and environment variable configuration
- **Metrics**: Server performance and usage metrics

### Client Features
- **Automatic Tracing**: Integration with reflow_network for seamless tracing
- **Event Types**: All trace event types supported
- **Batching**: Efficient event batching and transmission
- **Retry Logic**: Robust connection and retry mechanisms
- **Cross-Platform**: Native and WASM support

### Protocol Features
- **Rich Events**: Comprehensive trace events with metadata
- **Real-time Subscriptions**: Live event streaming with filters
- **Historical Queries**: Query past traces with flexible filters
- **Performance Metrics**: CPU, memory, throughput measurements
- **Causality Tracking**: Event dependency and causality chains

## Quick Start

### 1. Start the Tracing Server

```bash
# From the project root
./examples/tracing_integration/scripts/start_server.sh
```

This will:
- Build the reflow_tracing server
- Start it with the example configuration
- Create a SQLite database for trace storage
- Listen on `ws://127.0.0.1:8080`

### 2. Run a Simple Client

In another terminal:

```bash
cd examples/tracing_integration
cargo run --bin simple_client
```

### 3. Monitor Live Events

In a third terminal:

```bash
cd examples/tracing_integration
cargo run --bin monitoring_client
```

## Available Examples

### 1. Simple Client (`simple_client`)

A basic example demonstrating:
- Basic actor network with data generator and processor
- Automatic trace event generation
- Clean shutdown and trace finalization

```bash
# Run with default settings
cargo run --bin simple_client

# Run with custom parameters
cargo run --bin simple_client -- --server-url ws://127.0.0.1:8080 --flow-name my-demo --count 20 --verbose
```

### 2. Monitoring Client (`monitoring_client`)

Real-time trace monitoring tool demonstrating:
- WebSocket connection to tracing server
- Live event streaming with filtering
- Historical trace queries
- Server metrics collection

```bash
# Monitor all events
cargo run --bin monitoring_client

# Monitor specific actors
cargo run --bin monitoring_client -- --actor-ids generator,processor

# Monitor specific event types
cargo run --bin monitoring_client -- --event-types created,failed,completed

# Query historical traces
cargo run --bin monitoring_client -- --query --limit 50
```

### 3. Complex Workflow (`complex_workflow`)

Advanced example showcasing:
- Multi-stage data processing pipeline
- Error handling and recovery
- State management and snapshots
- Performance monitoring

```bash
cargo run --bin complex_workflow
```

### 4. Performance Test (`performance_test`)

Load testing example demonstrating:
- High-throughput event generation
- Performance metrics collection
- Resource usage monitoring
- Scalability testing

```bash
cargo run --bin performance_test -- --actors 100 --messages 10000
```

### 5. Query Client (`query_client`)

Historical analysis tool demonstrating:
- Trace querying with filters
- Performance analysis
- Flow dependency visualization
- Export capabilities

```bash
cargo run --bin query_client -- --flow-id my-flow --time-range 1h
```

## Configuration

### Server Configuration

The server uses `config/server_config.toml` for configuration:

```toml
[server]
host = "127.0.0.1"
port = 8080
max_connections = 1000

[storage]
backend = "sqlite"  # "memory", "sqlite", "postgres"

[storage.sqlite]
database_path = "examples/tracing_integration/data/traces.db"
wal_mode = true

[compression]
enabled = true
algorithm = "zstd"
level = 3
```

### Environment Variables

Override configuration with environment variables:

```bash
export REFLOW_TRACING_HOST=0.0.0.0
export REFLOW_TRACING_PORT=8080
export REFLOW_TRACING_STORAGE_BACKEND=postgres
export REFLOW_TRACING_POSTGRES_URL="postgresql://user:pass@localhost/traces"
```

## Storage Backends

### SQLite (Default)
- **Pros**: No setup required, embedded database
- **Cons**: Single-writer limitation
- **Use Case**: Development, small deployments

### PostgreSQL
- **Pros**: Concurrent access, ACID compliance, scalability
- **Cons**: Requires PostgreSQL server
- **Use Case**: Production deployments

### Memory
- **Pros**: Fastest performance, no disk I/O
- **Cons**: Data loss on restart, memory limited
- **Use Case**: Testing, temporary analysis

## Advanced Usage

### Custom Event Filters

Monitor only specific types of events:

```bash
# Monitor only actor lifecycle events
cargo run --bin monitoring_client -- --event-types created,started,completed,failed

# Monitor specific flows
cargo run --bin monitoring_client -- --flow-ids flow1,flow2

# Combine filters
cargo run --bin monitoring_client -- --actor-ids worker --event-types message_sent,message_received
```

### Historical Analysis

Query traces from specific time periods:

```bash
# Query last hour
cargo run --bin query_client -- --time-range 1h

# Query specific flow
cargo run --bin query_client -- --flow-id production-pipeline

# Query with status filter
cargo run --bin query_client -- --status failed --limit 100
```

### Performance Monitoring

Monitor system performance:

```bash
# Check server metrics
cargo run --bin monitoring_client -- --metrics

# Run performance test
cargo run --bin performance_test -- --duration 60s --actors 50
```

## Docker Deployment

### Using Docker Compose

```bash
cd examples/tracing_integration
docker-compose up -d
```

This starts:
- reflow_tracing server with PostgreSQL backend
- PostgreSQL database
- Optional monitoring dashboard

### Manual Docker

```bash
# Build server image
docker build -t reflow-tracing -f docker/Dockerfile.server .

# Run with SQLite
docker run -p 8080:8080 -v $(pwd)/data:/data reflow-tracing

# Run with PostgreSQL
docker run -p 8080:8080 -e REFLOW_TRACING_POSTGRES_URL=postgresql://... reflow-tracing
```

## Monitoring and Metrics

### Server Metrics

Access server metrics at: `http://127.0.0.1:9090/metrics`

Key metrics:
- Connection count
- Message throughput
- Storage performance
- Error rates
- Memory usage

### Client Metrics

Each client reports:
- Events generated
- Network latency
- Compression ratios
- Retry attempts
- Error counts

## Troubleshooting

### Common Issues

1. **Connection Refused**
   - Ensure server is running
   - Check firewall settings
   - Verify port availability

2. **High Memory Usage**
   - Adjust batch size in client configuration
   - Enable compression
   - Consider PostgreSQL backend

3. **Slow Performance**
   - Enable compression
   - Increase batch sizes
   - Use memory backend for testing

### Debug Mode

Enable verbose logging:

```bash
RUST_LOG=debug cargo run --bin simple_client -- --verbose
```

### Health Checks

```bash
# Check server status
curl http://127.0.0.1:9090/metrics

# Ping server via WebSocket
cargo run --bin monitoring_client -- --metrics
```

## Development

### Building

```bash
# Build all examples
cargo build --release

# Build specific example
cargo build --bin simple_client
```

### Testing

```bash
# Run unit tests
cargo test

# Run integration tests
cargo test --test integration

# Run performance tests
cargo test --test performance --release
```



## API Reference

### Client Configuration

```rust
use reflow_network::tracing::TracingConfig;

let config = TracingConfig {
    server_url: "ws://127.0.0.1:8080".to_string(),
    batch_size: 100,
    batch_timeout: Duration::from_millis(500),
    enable_compression: true,
    enabled: true,
};
```

### Event Types

- `ActorCreated`: Actor instantiation
- `ActorStarted`: Actor begins execution
- `ActorCompleted`: Actor finishes successfully
- `ActorFailed`: Actor encounters error
- `MessageSent`: Message transmission
- `MessageReceived`: Message reception
- `StateChanged`: Actor state modification
- `PortConnected`: Port connection established
- `PortDisconnected`: Port connection severed
- `NetworkEvent`: Network-level events

## Contributing

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Ensure all tests pass
5. Submit a pull request

## License

This example is part of the Reflow project and follows the same license terms.
