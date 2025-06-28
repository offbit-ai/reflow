# Distributed Networks

Reflow's distributed network system enables bi-directional communication between separate Reflow instances, allowing you to build scalable, multi-node workflows while maintaining the familiar actor-based programming model.

## Overview

The distributed network architecture extends Reflow's local actor model to support remote communication across network boundaries. This enables:

- **Cross-Network Actor Communication**: Actors in one Reflow instance can send messages to actors in remote instances
- **Network-Transparent Operation**: Remote actors appear as local actors in your workflows
- **Bi-directional Message Flow**: Full duplex communication between distributed nodes
- **Automatic Discovery**: Networks can discover and register with each other automatically
- **Conflict Resolution**: Smart handling of actor name conflicts across networks

## Architecture Components

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Distributed Reflow Network                      │
├─────────────────────────────────────────────────────────────────────┤
│  Instance A (Server)           │  Instance B (Client)               │
│ ┌─────────────────────────────┐ │ ┌─────────────────────────────────┐ │
│ │ Local Network               │ │ │ Local Network                   │ │
│ │ ├─ Actor A1 ─┐              │ │ │ ├─ Actor B1 ─┐                  │ │
│ │ ├─ Actor A2 ─┤              │ │ │ ├─ Actor B2 ─┤                  │ │
│ │ └─ Actor A3 ─┘              │ │ │ └─ Actor B3 ─┘                  │ │
│ └─────────────────────────────┘ │ └─────────────────────────────────┘ │
│            │                    │                    │                │
│ ┌─────────────────────────────┐ │ ┌─────────────────────────────────┐ │
│ │ Network Bridge              │◄─┤ │ Network Bridge                  │ │
│ │ ├─ Discovery Service        │ │ │ ├─ Discovery Service             │ │
│ │ ├─ Message Router           │ │ │ ├─ Message Router                │ │
│ │ ├─ Connection Manager       │ │ │ ├─ Connection Manager            │ │
│ │ └─ Remote Actor Proxy       │ │ │ └─ Remote Actor Proxy            │ │
│ └─────────────────────────────┘ │ └─────────────────────────────────┘ │
│            │                    │                    │                │
│ ┌─────────────────────────────┐ │ ┌─────────────────────────────────┐ │
│ │ Transport Layer             │◄─┤ │ Transport Layer                 │ │
│ │ ├─ WebSocket/TCP Server     │ │ │ ├─ WebSocket/TCP Client          │ │
│ │ ├─ Protocol Handler         │ │ │ ├─ Protocol Handler              │ │
│ │ └─ Serialization            │ │ │ └─ Serialization                 │ │
│ └─────────────────────────────┘ │ └─────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

### Core Components

1. **DistributedNetwork**: Main orchestrator that combines local networks with distributed communication
2. **NetworkBridge**: Handles all cross-network communication and actor registration
3. **DiscoveryService**: Automatic network discovery and registration
4. **MessageRouter**: Routes messages between local and remote actors
5. **RemoteActorProxy**: Local representatives of remote actors
6. **TransportLayer**: WebSocket/TCP communication infrastructure

## Basic Setup

### Creating a Distributed Network

```rust
use reflow_network::distributed_network::{DistributedNetwork, DistributedConfig};
use reflow_network::network::NetworkConfig;

// Configure the distributed network
let config = DistributedConfig {
    network_id: "main_workflow_engine".to_string(),
    instance_id: "server_001".to_string(),
    bind_address: "0.0.0.0".to_string(),
    bind_port: 8080,
    discovery_endpoints: vec![
        "http://discovery.example.com:3000".to_string()
    ],
    auth_token: Some("secure_token".to_string()),
    max_connections: 100,
    heartbeat_interval_ms: 30000,
    local_network_config: NetworkConfig::default(),
};

// Create and start the distributed network
let mut distributed_network = DistributedNetwork::new(config).await?;
distributed_network.start().await?;
```

### Registering Local Actors

```rust
use your_actors::DataProcessorActor;

// Register actors that will be available to remote networks
distributed_network.register_local_actor(
    "data_processor",
    DataProcessorActor::new(),
    Some(HashMap::from([
        ("capability".to_string(), serde_json::Value::String("data_processing".to_string())),
        ("version".to_string(), serde_json::Value::String("1.0.0".to_string())),
    ]))
)?;
```

### Connecting to Remote Networks

```rust
// Connect to another network
distributed_network.connect_to_network("192.168.1.100:8080").await?;

// Register a remote actor for local use
distributed_network.register_remote_actor(
    "remote_validator",      // Remote actor ID
    "validation_network"     // Remote network ID
).await?;
```

## Actor Communication Patterns

### Direct Remote Messaging

```rust
use reflow_network::message::Message;

// Send message to remote actor
distributed_network.send_to_remote_actor(
    "validation_network",    // Target network
    "remote_validator",      // Target actor
    "input",                 // Target port
    Message::String("validate this data".to_string().into())
).await?;
```

### Workflow Integration

Remote actors integrate seamlessly into local workflows:

```rust
// Get local network handle
let local_network = distributed_network.get_local_network();
let mut network = local_network.write();

// Add local actor
network.add_node("local_collector", "data_collector", None)?;

// Add remote actor (appears as local)
network.add_node("remote_processor", "remote_validator@validation_network", None)?;

// Connect them in a workflow
network.add_connection(Connector {
    from: ConnectionPoint {
        actor: "local_collector".to_string(),
        port: "output".to_string(),
        ..Default::default()
    },
    to: ConnectionPoint {
        actor: "remote_processor".to_string(),
        port: "input".to_string(),
        ..Default::default()
    },
})?;
```

## Network Discovery

### Automatic Discovery

The discovery service can automatically find and register remote networks:

```rust
// Enable automatic discovery
let config = DistributedConfig {
    // ... other config
    discovery_endpoints: vec![
        "http://service-discovery.local:3000".to_string(),
        "http://backup-discovery.local:3000".to_string(),
    ],
    // ...
};
```

### Manual Network Registration

```rust
// Manually connect to specific networks
let networks_to_connect = vec![
    "analytics.company.com:8080",
    "ml-pipeline.company.com:8080",
    "data-warehouse.company.com:8080",
];

for endpoint in networks_to_connect {
    match distributed_network.connect_to_network(endpoint).await {
        Ok(_) => println!("Connected to {}", endpoint),
        Err(e) => eprintln!("Failed to connect to {}: {}", endpoint, e),
    }
}
```

## Conflict Resolution

When multiple networks have actors with the same name, Reflow provides several resolution strategies:

### Automatic Aliasing

```rust
// Register remote actor with automatic conflict resolution
let alias = distributed_network.register_remote_actor_with_strategy(
    "data_processor",                    // Remote actor name (conflicts with local)
    "analytics_network",                 // Remote network
    ConflictResolutionStrategy::AutoAlias // Strategy
).await?;

println!("Remote actor available as: {}", alias);
// Output: "Remote actor available as: analytics_network_data_processor"
```

### Manual Aliasing

```rust
// Provide custom aliases for clarity
distributed_network.register_remote_actor_with_strategy(
    "validator",
    "security_network",
    ConflictResolutionStrategy::ManualAlias("security_validator".to_string())
).await?;
```

## Security Considerations

### Authentication

```rust
let config = DistributedConfig {
    // Use authentication tokens
    auth_token: Some("your_secure_token_here".to_string()),
    // ... other config
};
```

### Network Isolation

```rust
// Restrict which networks can connect
let config = DistributedConfig {
    // Only allow specific discovery endpoints
    discovery_endpoints: vec![
        "https://trusted-discovery.company.com:3000".to_string()
    ],
    max_connections: 10, // Limit concurrent connections
    // ... other config
};
```

## Monitoring and Health Checks

### Connection Status

```rust
// Check network health
let bridge_status = distributed_network.get_bridge_status().await?;
println!("Connected networks: {}", bridge_status.connected_networks.len());

for (network_id, status) in &bridge_status.connected_networks {
    println!("  {}: {:?}", network_id, status);
}
```

### Heartbeat Monitoring

```rust
let config = DistributedConfig {
    heartbeat_interval_ms: 15000, // 15 second heartbeats
    // ... other config
};
```

## Error Handling

### Connection Failures

```rust
use reflow_network::distributed_network::DistributedError;

match distributed_network.connect_to_network("unreachable:8080").await {
    Ok(_) => println!("Connected successfully"),
    Err(DistributedError::ConnectionTimeout) => {
        eprintln!("Connection timed out - network may be down");
    },
    Err(DistributedError::AuthenticationFailed) => {
        eprintln!("Authentication failed - check token");
    },
    Err(e) => eprintln!("Other error: {}", e),
}
```

### Message Delivery Failures

```rust
// Messages automatically retry with backoff
match distributed_network.send_to_remote_actor(
    "target_network", "target_actor", "input", message
).await {
    Ok(_) => println!("Message sent successfully"),
    Err(e) => {
        eprintln!("Failed to send message: {}", e);
        // Message will be retried automatically
    }
}
```

## Performance Considerations

### Connection Pooling

```rust
let config = DistributedConfig {
    max_connections: 50, // Adjust based on load
    // ... other config
};
```

### Message Batching

Messages are automatically batched for efficiency, but you can tune batching behavior:

```rust
// Large messages are automatically compressed
let large_data = Message::Object(/* large JSON object */);
distributed_network.send_to_remote_actor(
    "target_network", "target_actor", "bulk_input", large_data
).await?;
```

## Best Practices

### Network Design

1. **Use Descriptive Network IDs**: Choose meaningful names like `analytics_cluster` instead of `network1`
2. **Plan for Conflicts**: Use descriptive actor names to minimize naming conflicts
3. **Group Related Services**: Co-locate related actors in the same network for efficiency
4. **Design for Failure**: Always handle network partitions and connection failures gracefully

### Actor Organization

```rust
// Good: Descriptive, specific names
distributed_network.register_local_actor("customer_data_validator", validator, None)?;
distributed_network.register_local_actor("payment_processor", processor, None)?;

// Avoid: Generic names likely to conflict
// distributed_network.register_local_actor("validator", validator, None)?;
// distributed_network.register_local_actor("processor", processor, None)?;
```

### Resource Management

```rust
// Always clean up connections
struct DistributedWorkflow {
    network: DistributedNetwork,
}

impl Drop for DistributedWorkflow {
    fn drop(&mut self) {
        // Gracefully shutdown connections
        if let Err(e) = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.network.shutdown())
        }) {
            eprintln!("Error during cleanup: {}", e);
        }
    }
}
```

## Troubleshooting

### Common Issues

1. **Connection Refused**: Check firewall settings and ensure target network is running
2. **Authentication Failed**: Verify auth tokens match between networks
3. **Actor Not Found**: Ensure remote actor is registered and network is connected
4. **Message Timeouts**: Check network latency and increase timeout values if needed

### Debug Logging

Enable detailed logging for troubleshooting:

```rust
use tracing_subscriber;

// Enable debug logging
tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .init();
```

### Health Check Endpoint

Networks automatically expose health endpoints:

```bash
# Check network health
curl http://your-network:8080/health

# Get network status
curl http://your-network:8080/status
```

## Next Steps

- [Remote Actors](../api/distributed/remote-actors.md) - Detailed remote actor API
- [Discovery & Registration](../api/distributed/discovery-registration.md) - Network discovery details
- [Conflict Resolution](../api/distributed/conflict-resolution.md) - Advanced conflict handling
- [Distributed Workflow Tutorial](../tutorials/distributed-workflow-example.md) - Step-by-step example
