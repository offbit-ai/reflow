# Discovery & Registration

Learn how to use network discovery services and automatic actor registration in distributed Reflow networks.

## Overview

Discovery and registration services enable:

- **Automatic network discovery**: Find available networks without manual configuration
- **Service registration**: Advertise your network's capabilities to others
- **Dynamic actor discovery**: Automatically find and register remote actors
- **Health monitoring**: Track network and actor availability
- **Load balancing**: Distribute connections across available instances

## Discovery Service Types

### 1. Built-in Discovery

Use Reflow's built-in discovery where one network acts as a registry:

```rust
use reflow_network::distributed_network::{DistributedNetwork, DistributedConfig};

// Discovery server (registry)
let registry_config = DistributedConfig {
    network_id: "discovery_registry".to_string(),
    instance_id: "registry_001".to_string(),
    bind_address: "0.0.0.0".to_string(),
    bind_port: 8090,
    discovery_endpoints: vec![], // Empty - this IS the discovery server
    // ... other config
};

let mut registry_network = DistributedNetwork::new(registry_config).await?;
registry_network.start().await?;
println!("🔍 Discovery registry started on port 8090");
```

### 2. Client Networks Using Registry

```rust
// Client networks connect to registry for discovery
let client_config = DistributedConfig {
    network_id: "worker_network".to_string(),
    instance_id: "worker_001".to_string(),
    bind_address: "127.0.0.1".to_string(),
    bind_port: 8091,
    discovery_endpoints: vec!["http://registry:8090".to_string()],
    // ... other config
};

let mut client_network = DistributedNetwork::new(client_config).await?;
client_network.start().await?;
```

### 3. External Discovery Services

Integrate with external service discovery systems:

```rust
// Using Consul
let consul_config = DistributedConfig {
    network_id: "consul_client".to_string(),
    discovery_endpoints: vec![
        "http://consul.service.consul:8500/v1/agent/services".to_string()
    ],
    // ... other config
};

// Using etcd
let etcd_config = DistributedConfig {
    network_id: "etcd_client".to_string(),
    discovery_endpoints: vec![
        "http://etcd.cluster.local:2379/v2/keys/reflow/services".to_string()
    ],
    // ... other config
};

// Using Kubernetes DNS
let k8s_config = DistributedConfig {
    network_id: "k8s_service".to_string(),
    discovery_endpoints: vec![
        "http://reflow-discovery.default.svc.cluster.local:8080".to_string()
    ],
    // ... other config
};
```

## Network Registration

### Basic Registration

Networks automatically register themselves when started:

```rust
let network_config = DistributedConfig {
    network_id: "ml_processing_cluster".to_string(),
    instance_id: "gpu_worker_001".to_string(),
    bind_address: "0.0.0.0".to_string(),
    bind_port: 8080,
    discovery_endpoints: vec!["http://discovery:8090".to_string()],
    // ... other config
};

let mut network = DistributedNetwork::new(network_config).await?;

// Registration happens automatically on start
network.start().await?;
// Network is now registered with discovery service
```

### Registration with Metadata

Include additional metadata during registration:

```rust
// Register with capabilities and metadata
let registration_metadata = serde_json::json!({
    "capabilities": ["ml_training", "gpu_compute", "data_processing"],
    "resources": {
        "cpu_cores": 32,
        "gpu_count": 4,
        "memory_gb": 128
    },
    "version": "1.2.0",
    "tags": ["ml", "gpu", "production"],
    "health_check_url": "http://worker:8080/health"
});

// This metadata is included in registration (implementation detail)
// The discovery service can use this for intelligent routing
```

### Manual Registration Control

Control registration timing and behavior:

```rust
// Start network without auto-registration
let mut network = DistributedNetwork::new(config).await?;
network.start().await?;

// Perform initialization
setup_local_actors(&mut network).await?;
run_health_checks(&network).await?;

// Register manually when ready
network.register_with_discovery().await?;
println!("✅ Network registered and ready for connections");
```

## Network Discovery

### Discover Available Networks

Find networks that are currently available:

```rust
// Discover all available networks
let discovered_networks = client_network.discover_networks().await?;

for network_info in discovered_networks {
    println!("🌐 Found network: {} ({})", 
        network_info.network_id, 
        network_info.endpoint
    );
    println!("   Capabilities: {:?}", network_info.capabilities);
    println!("   Last seen: {}", network_info.last_seen);
}
```

### Filtered Discovery

Find networks with specific capabilities:

```rust
// Discover networks with ML capabilities
let ml_networks = client_network.discover_networks_with_capability("ml_training").await?;

for network in ml_networks {
    println!("🧠 ML Network: {} at {}", network.network_id, network.endpoint);
    
    // Connect to ML networks
    client_network.connect_to_network(&network.endpoint).await?;
}
```

### Discovery by Tags

Find networks using tag-based filtering:

```rust
// Discover production GPU networks
let gpu_networks = client_network.discover_networks_by_tags(vec!["gpu", "production"]).await?;

for network in gpu_networks {
    if network.is_healthy() {
        client_network.connect_to_network(&network.endpoint).await?;
        println!("✅ Connected to GPU network: {}", network.network_id);
    }
}
```

## Automatic Actor Discovery

### Discover Actors on Connected Networks

Once connected to a network, discover its available actors:

```rust
// Connect to a network first
client_network.connect_to_network("ml_cluster:8080").await?;

// Discover actors on that network
let actors = client_network.discover_actors_on_network("ml_cluster").await?;

for actor in actors {
    println!("🎭 Actor: {} ({})", actor.name, actor.component_type);
    println!("   Capabilities: {:?}", actor.capabilities);
    println!("   Ports: in={:?}, out={:?}", actor.inports, actor.outports);
}
```

### Automatic Registration

Register all discovered actors automatically:

```rust
// Discover and register all compatible actors
let discovered_actors = client_network.discover_actors_on_network("data_cluster").await?;

for actor in discovered_actors {
    // Only register actors we can use
    if actor.capabilities.contains(&"data_processing".to_string()) {
        match client_network.register_remote_actor(&actor.name, "data_cluster").await {
            Ok(_) => println!("✅ Registered actor: {}", actor.name),
            Err(e) => eprintln!("❌ Failed to register {}: {}", actor.name, e),
        }
    }
}
```

### Selective Auto-Registration

Register actors based on complex criteria:

```rust
async fn smart_actor_registration(
    network: &mut DistributedNetwork,
    remote_network_id: &str
) -> Result<Vec<String>, anyhow::Error> {
    let actors = network.discover_actors_on_network(remote_network_id).await?;
    let mut registered_actors = Vec::new();
    
    for actor in actors {
        // Complex registration logic
        let should_register = match actor.component_type.as_str() {
            "DataProcessorActor" => {
                // Only register if we don't have local data processors
                !network.has_local_actor_of_type("DataProcessorActor").await
            },
            "MLTrainerActor" => {
                // Only register GPU trainers
                actor.capabilities.contains(&"gpu_compute".to_string())
            },
            "DatabaseActor" => {
                // Register if it's a different database type than our local ones
                let local_dbs = network.get_local_database_types().await;
                !local_dbs.contains(&actor.get_database_type())
            },
            _ => false, // Don't auto-register unknown types
        };
        
        if should_register {
            let alias = network.register_remote_actor(&actor.name, remote_network_id).await?;
            registered_actors.push(alias);
            println!("🤖 Smart-registered: {} as {}", actor.name, alias);
        }
    }
    
    Ok(registered_actors)
}
```

## Health Monitoring

### Network Health Checks

Monitor the health of discovered networks:

```rust
// Periodic health monitoring
async fn monitor_network_health(network: &DistributedNetwork) -> Result<(), anyhow::Error> {
    let mut interval = tokio::time::interval(Duration::from_secs(30));
    
    loop {
        interval.tick().await;
        
        let connected_networks = network.get_connected_networks().await;
        for network_id in connected_networks {
            match network.ping_network(&network_id).await {
                Ok(latency) => {
                    println!("✅ Network {} healthy ({}ms)", network_id, latency.as_millis());
                },
                Err(e) => {
                    eprintln!("❌ Network {} unhealthy: {}", network_id, e);
                    
                    // Attempt reconnection
                    if let Ok(network_info) = network.get_network_info(&network_id).await {
                        match network.reconnect_to_network(&network_info.endpoint).await {
                            Ok(_) => println!("🔄 Reconnected to {}", network_id),
                            Err(e) => eprintln!("🔌 Reconnection failed: {}", e),
                        }
                    }
                }
            }
        }
    }
}
```

### Actor Health Monitoring

Monitor remote actor availability:

```rust
async fn monitor_remote_actors(network: &DistributedNetwork) -> Result<(), anyhow::Error> {
    let remote_actors = network.list_registered_remote_actors().await;
    
    for (actor_alias, actor_ref) in remote_actors {
        match network.ping_remote_actor(&actor_ref.network_id, &actor_ref.actor_id).await {
            Ok(_) => {
                println!("✅ Remote actor {} is responsive", actor_alias);
            },
            Err(e) => {
                eprintln!("❌ Remote actor {} is unresponsive: {}", actor_alias, e);
                
                // Try to re-register the actor
                match network.refresh_remote_actor(&actor_alias).await {
                    Ok(_) => println!("🔄 Refreshed remote actor: {}", actor_alias),
                    Err(e) => {
                        eprintln!("🚫 Failed to refresh {}: {}", actor_alias, e);
                        // Consider removing the actor or marking it as unavailable
                    }
                }
            }
        }
    }
    
    Ok(())
}
```

## Load Balancing and Failover

### Discover Multiple Instances

Find multiple instances of the same service:

```rust
// Find all instances of a specific service type
let data_processors = client_network.discover_networks_with_capability("data_processing").await?;

println!("Found {} data processing networks:", data_processors.len());
for (i, network) in data_processors.iter().enumerate() {
    println!("  {}. {} at {} (load: {}%)", 
        i + 1, 
        network.network_id, 
        network.endpoint,
        network.cpu_usage.unwrap_or(0.0)
    );
}
```

### Load-Balanced Registration

Register actors from multiple networks for load balancing:

```rust
// Register the same actor type from multiple networks
let processing_networks = vec!["cluster_1", "cluster_2", "cluster_3"];

for (i, network_id) in processing_networks.iter().enumerate() {
    if client_network.is_network_available(network_id).await {
        let alias = format!("data_processor_{}", i + 1);
        client_network.register_remote_actor_with_alias(
            &alias,
            "data_processor", 
            network_id
        ).await?;
        println!("⚖️  Registered load-balanced actor: {}", alias);
    }
}
```

### Failover Registration

Implement failover with primary and backup actors:

```rust
struct FailoverActorRegistry {
    network: Arc<DistributedNetwork>,
    primary_actors: HashMap<String, String>,    // service -> primary actor alias
    backup_actors: HashMap<String, Vec<String>>, // service -> backup actor aliases
}

impl FailoverActorRegistry {
    async fn register_with_failover(&mut self, 
        service_name: &str, 
        actor_type: &str
    ) -> Result<(), anyhow::Error> {
        let networks = self.network.discover_networks_with_capability(actor_type).await?;
        
        if networks.is_empty() {
            return Err(anyhow::anyhow!("No networks found with capability: {}", actor_type));
        }
        
        // Primary: Use the network with lowest load
        let primary_network = networks.iter()
            .min_by(|a, b| a.cpu_usage.partial_cmp(&b.cpu_usage).unwrap())
            .unwrap();
        
        let primary_alias = format!("{}_primary", service_name);
        self.network.register_remote_actor_with_alias(
            &primary_alias,
            actor_type,
            &primary_network.network_id
        ).await?;
        
        self.primary_actors.insert(service_name.to_string(), primary_alias);
        
        // Backups: Register from other networks
        let mut backup_aliases = Vec::new();
        for (i, network) in networks.iter().skip(1).enumerate() {
            let backup_alias = format!("{}_backup_{}", service_name, i + 1);
            self.network.register_remote_actor_with_alias(
                &backup_alias,
                actor_type,
                &network.network_id
            ).await?;
            backup_aliases.push(backup_alias);
        }
        
        self.backup_actors.insert(service_name.to_string(), backup_aliases);
        
        println!("🛡️  Registered failover service: {} with {} backups", 
            service_name, backup_aliases.len());
        
        Ok(())
    }
    
    async fn handle_primary_failure(&self, service_name: &str) -> Result<String, anyhow::Error> {
        if let Some(backups) = self.backup_actors.get(service_name) {
            if let Some(first_backup) = backups.first() {
                // Promote first backup to primary
                println!("🔄 Promoting backup to primary for service: {}", service_name);
                return Ok(first_backup.clone());
            }
        }
        Err(anyhow::anyhow!("No backup available for service: {}", service_name))
    }
}
```

## Configuration Management

### Discovery Service Configuration

Configure discovery service behavior:

```rust
#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    pub refresh_interval_ms: u64,
    pub health_check_interval_ms: u64,
    pub max_discovery_retries: u32,
    pub discovery_timeout_ms: u64,
    pub enable_auto_registration: bool,
    pub registration_metadata: serde_json::Value,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        DiscoveryConfig {
            refresh_interval_ms: 30000,      // 30 seconds
            health_check_interval_ms: 15000, // 15 seconds
            max_discovery_retries: 3,
            discovery_timeout_ms: 5000,      // 5 seconds
            enable_auto_registration: true,
            registration_metadata: serde_json::json!({
                "version": "1.0.0",
                "capabilities": []
            }),
        }
    }
}
```

### Environment-Specific Discovery

Configure discovery for different environments:

```rust
fn create_discovery_config(environment: &str) -> DiscoveryConfig {
    match environment {
        "development" => DiscoveryConfig {
            refresh_interval_ms: 10000,  // Faster refresh for dev
            health_check_interval_ms: 5000,
            discovery_timeout_ms: 2000,  // Shorter timeout
            enable_auto_registration: true,
            registration_metadata: serde_json::json!({
                "environment": "development",
                "auto_discovery": true
            }),
            ..Default::default()
        },
        "production" => DiscoveryConfig {
            refresh_interval_ms: 60000,  // Slower refresh for prod
            health_check_interval_ms: 30000,
            discovery_timeout_ms: 10000, // Longer timeout
            enable_auto_registration: false, // Manual control
            registration_metadata: serde_json::json!({
                "environment": "production",
                "manual_registration": true
            }),
            ..Default::default()
        },
        _ => DiscoveryConfig::default(),
    }
}
```

## Error Handling

### Discovery Errors

Handle common discovery and registration errors:

```rust
async fn robust_discovery(network: &DistributedNetwork) -> Result<Vec<NetworkInfo>, anyhow::Error> {
    let mut retries = 3;
    let mut last_error = None;
    
    while retries > 0 {
        match network.discover_networks().await {
            Ok(networks) => {
                if networks.is_empty() {
                    println!("⚠️  No networks discovered, retrying...");
                } else {
                    return Ok(networks);
                }
            },
            Err(e) => {
                eprintln!("❌ Discovery attempt failed: {}", e);
                last_error = Some(e);
                
                // Wait before retry
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
        
        retries -= 1;
    }
    
    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Discovery failed after retries")))
}
```

### Registration Conflicts

Handle registration conflicts gracefully:

```rust
async fn safe_actor_registration(
    network: &mut DistributedNetwork,
    actor_name: &str,
    remote_network: &str
) -> Result<String, anyhow::Error> {
    match network.register_remote_actor(actor_name, remote_network).await {
        Ok(alias) => Ok(alias),
        Err(e) if e.to_string().contains("name conflict") => {
            // Try with numbered suffix
            for i in 1..=10 {
                let attempt_name = format!("{}_{}", actor_name, i);
                match network.register_remote_actor_with_alias(
                    &attempt_name, 
                    actor_name, 
                    remote_network
                ).await {
                    Ok(alias) => {
                        println!("✅ Registered with conflict resolution: {}", alias);
                        return Ok(alias);
                    },
                    Err(_) => continue,
                }
            }
            Err(anyhow::anyhow!("Could not resolve naming conflict for: {}", actor_name))
        },
        Err(e) => Err(e),
    }
}
```

## Best Practices

### 1. Discovery Strategy

```rust
// Good: Use hierarchical discovery with fallbacks
let discovery_endpoints = vec![
    "http://local-discovery:8090",      // Local first
    "http://regional-discovery:8090",   // Regional second
    "http://global-discovery:8090",     // Global fallback
];

// Configure discovery timeouts appropriately
let config = DiscoveryConfig {
    discovery_timeout_ms: 5000,    // 5 seconds max
    max_discovery_retries: 3,      // Try 3 times
    refresh_interval_ms: 30000,    // Refresh every 30s
    ..Default::default()
};
```

### 2. Health Monitoring

```rust
// Implement comprehensive health monitoring
async fn comprehensive_health_check(network: &DistributedNetwork) -> HealthStatus {
    let mut status = HealthStatus::new();
    
    // Check discovery service connectivity
    status.discovery_healthy = network.ping_discovery_service().await.is_ok();
    
    // Check connected networks
    let networks = network.get_connected_networks().await;
    for network_id in networks {
        let network_healthy = network.ping_network(&network_id).await.is_ok();
        status.network_health.insert(network_id, network_healthy);
    }
    
    // Check remote actors
    let actors = network.list_registered_remote_actors().await;
    for (alias, actor_ref) in actors {
        let actor_healthy = network.ping_remote_actor(
            &actor_ref.network_id, 
            &actor_ref.actor_id
        ).await.is_ok();
        status.actor_health.insert(alias, actor_healthy);
    }
    
    status
}
```

### 3. Resource Cleanup

```rust
// Proper cleanup on shutdown
async fn graceful_shutdown(mut network: DistributedNetwork) -> Result<(), anyhow::Error> {
    // Stop discovery refresh
    network.stop_discovery_refresh().await?;
    
    // Unregister from discovery service
    network.unregister_from_discovery().await?;
    
    // Clean up remote actor registrations
    let remote_actors = network.list_registered_remote_actors().await;
    for (alias, _) in remote_actors {
        network.unregister_remote_actor(&alias).await?;
    }
    
    // Disconnect from all networks
    let connected = network.get_connected_networks().await;
    for network_id in connected {
        network.disconnect_from_network(&network_id).await?;
    }
    
    // Finally shutdown the network
    network.shutdown().await?;
    
    Ok(())
}
```

## Integration Examples

### Docker Swarm Integration

```yaml
# docker-compose.yml
version: '3.8'
services:
  reflow-discovery:
    image: reflow:latest
    command: --mode discovery --port 8090
    ports:
      - "8090:8090"
    deploy:
      replicas: 1
      
  reflow-worker:
    image: reflow:latest
    command: --mode worker --discovery http://reflow-discovery:8090
    deploy:
      replicas: 3
    depends_on:
      - reflow-discovery
```

### Kubernetes Integration

```yaml
# reflow-discovery-service.yaml
apiVersion: v1
kind: Service
metadata:
  name: reflow-discovery
spec:
  selector:
    app: reflow-discovery
  ports:
    - port: 8090
      targetPort: 8090
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: reflow-discovery
spec:
  replicas: 1
  selector:
    matchLabels:
      app: reflow-discovery
  template:
    metadata:
      labels:
        app: reflow-discovery
    spec:
      containers:
      - name: reflow
        image: reflow:latest
        args: ["--mode", "discovery", "--port", "8090"]
        ports:
        - containerPort: 8090
```

## Next Steps

- [Conflict Resolution](conflict-resolution.md) - Handle actor name conflicts
- [Setting Up Distributed Networks](setting-up-networks.md) - Basic network setup
- [Remote Actors](remote-actors.md) - Working with remote actors

## Related Documentation

- [Architecture: Distributed Networks](../../architecture/distributed-networks.md)
- [Tutorial: Distributed Workflow Example](../../tutorials/distributed-workflow-example.md)
