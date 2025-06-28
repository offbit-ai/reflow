# Setting Up Distributed Networks

This guide covers how to set up and configure distributed Reflow networks for cross-network actor communication.

## Overview

Distributed networks allow multiple Reflow instances to communicate with each other, enabling:

- **Cross-network workflows**: Actors in different networks can send messages to each other
- **Resource sharing**: Share computational resources across multiple machines
- **Scalability**: Scale workflows beyond a single machine's capabilities
- **Fault tolerance**: Continue operation even if some network nodes fail

## Basic Setup

### 1. Server Network Configuration

First, set up a server network that will accept connections:

```rust
use reflow_network::distributed_network::{DistributedNetwork, DistributedConfig};
use reflow_network::network::NetworkConfig;

let server_config = DistributedConfig {
    network_id: "main_server".to_string(),
    instance_id: "server_001".to_string(),
    bind_address: "0.0.0.0".to_string(),
    bind_port: 8080,
    discovery_endpoints: vec![],
    auth_token: Some("secure_token".to_string()),
    max_connections: 100,
    heartbeat_interval_ms: 30000,
    local_network_config: NetworkConfig::default(),
};

let mut server_network = DistributedNetwork::new(server_config).await?;
```

### 2. Client Network Configuration

Set up a client network that connects to the server:

```rust
let client_config = DistributedConfig {
    network_id: "client_worker".to_string(),
    instance_id: "client_001".to_string(),
    bind_address: "127.0.0.1".to_string(),
    bind_port: 8081,
    discovery_endpoints: vec!["http://discovery.example.com:3000".to_string()],
    auth_token: Some("secure_token".to_string()),
    max_connections: 10,
    heartbeat_interval_ms: 30000,
    local_network_config: NetworkConfig::default(),
};

let mut client_network = DistributedNetwork::new(client_config).await?;
```

### 3. Start Networks

Start both networks and establish connection:

```rust
// Start server first
server_network.start().await?;
println!("✅ Server network started on port 8080");

// Start client
client_network.start().await?;
println!("✅ Client network started on port 8081");

// Connect client to server
client_network.connect_to_network("127.0.0.1:8080").await?;
println!("🔗 Client connected to server");
```

## Configuration Options

### DistributedConfig Fields

| Field | Type | Description | Example |
|-------|------|-------------|---------|
| `network_id` | `String` | Unique identifier for this network | `"data_processing_cluster"` |
| `instance_id` | `String` | Unique identifier for this instance | `"worker_001"` |
| `bind_address` | `String` | IP address to bind server to | `"0.0.0.0"` or `"127.0.0.1"` |
| `bind_port` | `u16` | Port number for server | `8080` |
| `discovery_endpoints` | `Vec<String>` | URLs of discovery services | `["http://discovery:3000"]` |
| `auth_token` | `Option<String>` | Authentication token | `Some("secret_token")` |
| `max_connections` | `usize` | Maximum concurrent connections | `100` |
| `heartbeat_interval_ms` | `u64` | Heartbeat interval in milliseconds | `30000` |
| `local_network_config` | `NetworkConfig` | Local network configuration | `NetworkConfig::default()` |

### Security Configuration

```rust
let secure_config = DistributedConfig {
    // ... other fields
    auth_token: Some("your_secure_token_here".to_string()),
    max_connections: 50, // Limit connections for security
    heartbeat_interval_ms: 15000, // More frequent heartbeats
};
```

### High-Performance Configuration

```rust
let performance_config = DistributedConfig {
    // ... other fields
    max_connections: 1000,
    heartbeat_interval_ms: 60000, // Less frequent heartbeats
    local_network_config: NetworkConfig {
        max_buffer_size: 1024 * 1024, // 1MB buffer
        enable_compression: true,
        // ... other performance settings
    },
};
```

## Network Topologies

### Star Topology (Hub and Spoke)

```rust
// Central hub
let hub_config = DistributedConfig {
    network_id: "central_hub".to_string(),
    bind_port: 8080,
    max_connections: 100,
    // ... other fields
};

// Multiple spokes connect to hub
let spoke_configs = vec![
    ("data_processor", 8081),
    ("ml_trainer", 8082),
    ("analytics", 8083),
];

for (name, port) in spoke_configs {
    let spoke_config = DistributedConfig {
        network_id: name.to_string(),
        bind_port: port,
        discovery_endpoints: vec!["http://hub:8080".to_string()],
        // ... other fields
    };
}
```

### Mesh Topology (Peer-to-Peer)

```rust
// Each node connects to multiple others
let mesh_discovery = vec![
    "http://node1:8080".to_string(),
    "http://node2:8081".to_string(),
    "http://node3:8082".to_string(),
];

let node_config = DistributedConfig {
    network_id: "mesh_node_1".to_string(),
    discovery_endpoints: mesh_discovery,
    // ... other fields
};
```

## Discovery Service Integration

### Using External Discovery Service

```rust
let config_with_discovery = DistributedConfig {
    network_id: "auto_discovery_client".to_string(),
    discovery_endpoints: vec![
        "http://consul.service.consul:8500".to_string(),
        "http://etcd.cluster.local:2379".to_string(),
    ],
    // ... other fields
};
```

### Built-in Discovery

```rust
// Server acts as discovery endpoint for others
let discovery_server_config = DistributedConfig {
    network_id: "discovery_server".to_string(),
    bind_port: 8080,
    discovery_endpoints: vec![], // Empty - this is the discovery server
    // ... other fields
};

// Clients use server for discovery
let discovery_client_config = DistributedConfig {
    network_id: "discovery_client".to_string(),
    discovery_endpoints: vec!["http://discovery_server:8080".to_string()],
    // ... other fields
};
```

## Error Handling

### Connection Errors

```rust
match client_network.connect_to_network("127.0.0.1:8080").await {
    Ok(_) => println!("✅ Connected successfully"),
    Err(e) => {
        eprintln!("❌ Connection failed: {}", e);
        // Implement retry logic
        tokio::time::sleep(Duration::from_secs(5)).await;
        // Retry connection...
    }
}
```

### Network Startup Errors

```rust
match server_network.start().await {
    Ok(_) => println!("✅ Network started"),
    Err(e) => {
        eprintln!("❌ Failed to start network: {}", e);
        match e.to_string().as_str() {
            s if s.contains("Address already in use") => {
                eprintln!("Port {} is already in use", server_config.bind_port);
                // Try different port
            },
            s if s.contains("Permission denied") => {
                eprintln!("Permission denied - try running as administrator or use port > 1024");
            },
            _ => eprintln!("Unknown error: {}", e),
        }
    }
}
```

## Monitoring and Diagnostics

### Network Status

```rust
// Check network configuration
let config = server_network.get_config();
println!("Network ID: {}", config.network_id);
println!("Listening on: {}:{}", config.bind_address, config.bind_port);

// Monitor connections (if available in future API)
// let connections = server_network.get_active_connections().await?;
// println!("Active connections: {}", connections.len());
```

### Health Checks

```rust
// Implement health check endpoint
async fn health_check(network: &DistributedNetwork) -> bool {
    // Check if network is responsive
    match network.ping_network("target_network").await {
        Ok(_) => true,
        Err(_) => false,
    }
}
```

## Best Practices

### 1. Network Naming

```rust
// Good: Descriptive, hierarchical names
"company.department.service"
"prod.ml.training"
"dev.data.processing"

// Avoid: Generic or conflicting names
"network"
"server"
"client"
```

### 2. Security

```rust
// Use strong authentication tokens
let auth_token = generate_secure_token(); // Use proper token generation

// Limit connections based on expected load
max_connections: calculate_expected_connections(),

// Use appropriate heartbeat intervals
heartbeat_interval_ms: match environment {
    Environment::Local => 10000,     // Fast for development
    Environment::LAN => 30000,       // Normal for LAN
    Environment::WAN => 60000,       // Slower for WAN
},
```

### 3. Resource Management

```rust
// Proper shutdown sequence
async fn shutdown_gracefully(mut network: DistributedNetwork) -> Result<(), anyhow::Error> {
    // Stop accepting new connections
    network.stop_accepting_connections().await?;
    
    // Wait for existing operations to complete
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Shutdown network
    network.shutdown().await?;
    
    Ok(())
}
```

### 4. Development vs Production

```rust
// Development configuration
let dev_config = DistributedConfig {
    bind_address: "127.0.0.1".to_string(), // Local only
    heartbeat_interval_ms: 10000,          // Fast heartbeats
    max_connections: 10,                   // Low limit
    auth_token: None,                      // No auth for dev
    // ...
};

// Production configuration
let prod_config = DistributedConfig {
    bind_address: "0.0.0.0".to_string(),   // Accept external connections
    heartbeat_interval_ms: 30000,          // Balanced heartbeats
    max_connections: 1000,                 // Higher limit
    auth_token: Some(env::var("AUTH_TOKEN")?), // Required auth
    // ...
};
```

## Troubleshooting

### Common Issues

1. **Port Already in Use**
   ```bash
   # Check what's using the port
   lsof -i :8080
   # Use different port or kill conflicting process
   ```

2. **Connection Refused**
   ```rust
   // Check firewall settings
   // Verify correct IP/port combination
   // Ensure server is started before client connects
   ```

3. **Authentication Failures**
   ```rust
   // Verify auth_token matches between networks
   // Check token is not None when required
   ```

4. **High Memory Usage**
   ```rust
   // Reduce max_connections
   // Increase heartbeat_interval_ms
   // Monitor for connection leaks
   ```

## Next Steps

- [Remote Actors](remote-actors.md) - Learn how to register and use remote actors
- [Discovery & Registration](discovery-registration.md) - Advanced discovery patterns
- [Conflict Resolution](conflict-resolution.md) - Handle actor name conflicts

## Related Documentation

- [Architecture: Distributed Networks](../../architecture/distributed-networks.md)
- [Tutorial: Distributed Workflow Example](../../tutorials/distributed-workflow-example.md)
