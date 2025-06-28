# Conflict Resolution

Learn how to handle actor name conflicts in distributed Reflow networks.

## Overview

Name conflicts occur when multiple networks have actors with identical names. This guide covers:

- **Understanding conflict types**: Different scenarios that cause conflicts
- **Resolution strategies**: Automatic and manual approaches to resolve conflicts  
- **Prevention techniques**: Best practices to avoid conflicts
- **Hierarchical namespacing**: Advanced organization patterns

## Conflict Types

### 1. Local-Remote Conflicts

Conflict between a local actor and a remote actor with the same name:

```rust
// Local network has "data_processor"
network.register_local_actor("data_processor", DataProcessorActor::new())?;

// Trying to register remote actor with same name
match client_network.register_remote_actor("data_processor", "server_network").await {
    Err(e) if e.to_string().contains("name conflict") => {
        println!("❌ Conflict: local 'data_processor' vs remote 'data_processor'");
    },
    _ => {}
}
```

### 2. Remote-Remote Conflicts

Multiple remote networks have actors with the same name:

```rust
// Both networks have "authentication_service"
client_network.register_remote_actor("authentication_service", "primary_auth").await?;

// This will conflict:
match client_network.register_remote_actor("authentication_service", "backup_auth").await {
    Err(e) => println!("❌ Conflict: primary_auth/authentication_service vs backup_auth/authentication_service"),
    _ => {}
}
```

### 3. Alias Conflicts

Custom aliases that conflict with existing names:

```rust
// Register with alias that conflicts with local actor
match client_network.register_remote_actor_with_alias(
    "local_actor_name",  // This alias conflicts!
    "remote_actor",
    "remote_network"
).await {
    Err(e) => println!("❌ Alias conflicts with existing local actor"),
    _ => {}
}
```

## Resolution Strategies

### 1. Automatic Aliasing

Let the system automatically generate unique aliases:

```rust
use reflow_network::distributed_network::ConflictResolutionStrategy;

// Automatic resolution with numbered suffixes
let alias = client_network.register_remote_actor_with_strategy(
    "data_processor",
    "server_network", 
    ConflictResolutionStrategy::AutoAlias
).await?;

// Results in aliases like:
// - "data_processor" (if no conflict)
// - "data_processor_1" (first conflict)
// - "data_processor_2" (second conflict)

println!("✅ Registered as: {}", alias);
```

### 2. Network Prefixing

Prefix remote actors with their network name:

```rust
let alias = client_network.register_remote_actor_with_strategy(
    "data_processor",
    "server_network",
    ConflictResolutionStrategy::NetworkPrefix
).await?;

// Results in: "server_network_data_processor"
println!("✅ Network-prefixed actor: {}", alias);
```

### 3. Fully Qualified Names

Use complete network::actor notation:

```rust
let alias = client_network.register_remote_actor_with_strategy(
    "data_processor", 
    "server_network",
    ConflictResolutionStrategy::FullyQualified
).await?;

// Results in: "server_network::data_processor"
println!("✅ Fully qualified actor: {}", alias);
```

### 4. Manual Aliases

Provide explicit custom aliases:

```rust
let alias = client_network.register_remote_actor_with_strategy(
    "data_processor",
    "server_network", 
    ConflictResolutionStrategy::ManualAlias("server_data_proc".to_string())
).await?;

// Results in: "server_data_proc"
println!("✅ Custom alias: {}", alias);
```

### 5. Fail on Conflict

Explicitly handle conflicts in application code:

```rust
match client_network.register_remote_actor_with_strategy(
    "data_processor",
    "server_network",
    ConflictResolutionStrategy::Fail
).await {
    Ok(alias) => println!("✅ No conflict, registered as: {}", alias),
    Err(e) => {
        println!("❌ Registration failed due to conflict: {}", e);
        // Handle conflict manually
        handle_naming_conflict(&mut client_network, "data_processor", "server_network").await?;
    }
}
```

## Advanced Conflict Resolution

### Intelligent Conflict Detection

Detect and analyze conflicts before registration:

```rust
async fn analyze_potential_conflicts(
    network: &DistributedNetwork,
    actor_name: &str,
    remote_network_id: &str
) -> Result<ConflictAnalysis, anyhow::Error> {
    let mut analysis = ConflictAnalysis::new();
    
    // Check local conflicts
    if network.has_local_actor(actor_name).await {
        analysis.local_conflicts.push(LocalConflict {
            actor_name: actor_name.to_string(),
            actor_type: network.get_local_actor_type(actor_name).await?,
        });
    }
    
    // Check remote conflicts
    let remote_actors = network.list_registered_remote_actors().await;
    for (alias, actor_ref) in remote_actors {
        if alias == actor_name {
            analysis.remote_conflicts.push(RemoteConflict {
                alias,
                actor_ref,
            });
        }
    }
    
    // Suggest resolutions
    analysis.suggested_resolutions = suggest_resolutions(&analysis, actor_name, remote_network_id);
    
    Ok(analysis)
}

#[derive(Debug)]
struct ConflictAnalysis {
    local_conflicts: Vec<LocalConflict>,
    remote_conflicts: Vec<RemoteConflict>,
    suggested_resolutions: Vec<SuggestedResolution>,
}

#[derive(Debug)]
struct SuggestedResolution {
    strategy: ConflictResolutionStrategy,
    resulting_alias: String,
    confidence: f32,
    description: String,
}
```

### Multi-Network Batch Registration

Handle conflicts when registering actors from multiple networks:

```rust
async fn batch_register_with_conflict_resolution(
    network: &mut DistributedNetwork,
    registrations: Vec<(String, String)>  // (actor_name, network_id)
) -> Result<BatchRegistrationResult, anyhow::Error> {
    let mut results = BatchRegistrationResult::new();
    let mut name_usage = HashMap::new();
    
    // Analyze all potential conflicts first
    for (actor_name, network_id) in &registrations {
        name_usage.entry(actor_name.clone())
            .or_insert_with(Vec::new)
            .push(network_id.clone());
    }
    
    // Register with conflict resolution
    for (actor_name, network_id) in registrations {
        let strategy = if name_usage[&actor_name].len() > 1 {
            // Multiple networks have same actor name
            ConflictResolutionStrategy::NetworkPrefix
        } else if network.has_local_actor(&actor_name).await {
            // Conflicts with local actor
            ConflictResolutionStrategy::FullyQualified
        } else {
            // No conflicts expected
            ConflictResolutionStrategy::AutoAlias
        };
        
        match network.register_remote_actor_with_strategy(&actor_name, &network_id, strategy).await {
            Ok(alias) => {
                results.successful.push(SuccessfulRegistration {
                    actor_name: actor_name.clone(),
                    network_id: network_id.clone(),
                    alias,
                    strategy_used: strategy,
                });
            },
            Err(e) => {
                results.failed.push(FailedRegistration {
                    actor_name,
                    network_id,
                    error: e.to_string(),
                });
            }
        }
    }
    
    Ok(results)
}
```

### Context-Aware Resolution

Choose resolution strategies based on actor types and usage patterns:

```rust
async fn smart_conflict_resolution(
    network: &mut DistributedNetwork,
    actor_name: &str,
    network_id: &str,
    actor_metadata: &ActorMetadata
) -> Result<String, anyhow::Error> {
    // Analyze actor characteristics
    let strategy = match actor_metadata.actor_type.as_str() {
        "DatabaseActor" => {
            // For databases, use descriptive prefixes
            let db_type = actor_metadata.get_database_type().unwrap_or("db");
            ConflictResolutionStrategy::ManualAlias(
                format!("{}_{}", db_type, actor_name)
            )
        },
        "MLTrainerActor" => {
            // For ML trainers, include model type
            let model_type = actor_metadata.get_model_type().unwrap_or("model");
            ConflictResolutionStrategy::ManualAlias(
                format!("{}_trainer_{}", model_type, network_id)
            )
        },
        "AuthenticationActor" => {
            // For auth services, indicate primary/backup
            let is_primary = actor_metadata.is_primary_service().unwrap_or(false);
            let role = if is_primary { "primary" } else { "backup" };
            ConflictResolutionStrategy::ManualAlias(
                format!("auth_{}_{}", role, network_id)
            )
        },
        _ => {
            // Default strategy for other types
            if network.has_local_actor(actor_name).await {
                ConflictResolutionStrategy::NetworkPrefix
            } else {
                ConflictResolutionStrategy::AutoAlias
            }
        }
    };
    
    network.register_remote_actor_with_strategy(actor_name, network_id, strategy).await
}
```

## Hierarchical Namespacing

### Subgraph Organization

Organize remote actors in hierarchical namespaces:

```rust
// Instead of flat aliases, use hierarchical organization
let mount_config = SubgraphMountConfig {
    mount_point: "server".to_string(),
    network_id: "server_network".to_string(),
    include_patterns: vec!["*".to_string()],
    exclude_patterns: vec!["internal_*".to_string()],
};

// Mount entire network as subgraph
network.mount_network_as_subgraph(mount_config).await?;

// Actors are now accessible as:
// - "server/data_processor"
// - "server/validator" 
// - "server/transformer"

// Use in workflows
network.add_node("remote_proc", "server/data_processor", None)?;
```

### Nested Organization

Create deeply nested hierarchies for complex setups:

```rust
// Mount multiple networks with organized structure
let mount_configs = vec![
    SubgraphMountConfig {
        mount_point: "ml/training".to_string(),
        network_id: "ml_training_cluster".to_string(),
        // ...
    },
    SubgraphMountConfig {
        mount_point: "ml/inference".to_string(), 
        network_id: "ml_inference_cluster".to_string(),
        // ...
    },
    SubgraphMountConfig {
        mount_point: "data/processing".to_string(),
        network_id: "data_processing_cluster".to_string(),
        // ...
    },
];

for config in mount_configs {
    network.mount_network_as_subgraph(config).await?;
}

// Result: Clean hierarchical structure
// ml/training/model_trainer
// ml/training/feature_engineer
// ml/inference/predictor
// ml/inference/batch_scorer
// data/processing/cleaner
// data/processing/transformer
```

## Conflict Prevention

### 1. Naming Conventions

Establish clear naming conventions to prevent conflicts:

```rust
// Good: Descriptive, domain-specific names
"user_authentication_service"
"payment_data_processor"
"ml_model_trainer_gpu"
"postgres_connection_pool"

// Avoid: Generic names likely to conflict
"processor"
"handler"
"service"
"actor"
"worker"
```

### 2. Network-Aware Registration

Include network identity in actor names during registration:

```rust
// Register with network context
async fn register_with_network_context(
    network: &mut DistributedNetwork,
    actor_name: &str,
    remote_network_id: &str
) -> Result<String, anyhow::Error> {
    // Auto-generate context-aware names
    let network_context = remote_network_id.split('_').next().unwrap_or(remote_network_id);
    let contextual_name = format!("{}_{}", network_context, actor_name);
    
    network.register_remote_actor_with_alias(
        &contextual_name,
        actor_name,
        remote_network_id
    ).await
}
```

### 3. Capability-Based Naming

Name actors based on their capabilities rather than generic terms:

```rust
// Analyze actor capabilities and suggest names
async fn suggest_capability_based_name(
    actor_metadata: &ActorMetadata
) -> String {
    let capabilities = &actor_metadata.capabilities;
    
    let primary_capability = capabilities.first().unwrap_or(&"generic".to_string());
    let secondary_capability = capabilities.get(1);
    
    match (primary_capability.as_str(), secondary_capability) {
        ("ml_training", Some(sec)) if sec.contains("gpu") => "gpu_ml_trainer".to_string(),
        ("data_processing", Some(sec)) if sec.contains("stream") => "stream_data_processor".to_string(),
        ("database", Some(sec)) => format!("{}_database", sec),
        (primary, _) => format!("{}_service", primary),
    }
}
```

## Error Handling

### Conflict Resolution Errors

Handle errors during conflict resolution:

```rust
async fn handle_conflict_resolution_error(
    error: &anyhow::Error,
    actor_name: &str,
    network_id: &str
) -> ConflictResolutionAction {
    if error.to_string().contains("maximum retries exceeded") {
        ConflictResolutionAction::UseFullyQualified
    } else if error.to_string().contains("invalid alias") {
        ConflictResolutionAction::GenerateAlternative
    } else if error.to_string().contains("network disconnected") {
        ConflictResolutionAction::RetryLater
    } else {
        ConflictResolutionAction::FailRegistration
    }
}

enum ConflictResolutionAction {
    UseFullyQualified,
    GenerateAlternative,
    RetryLater,
    FailRegistration,
}
```

### Registration Rollback

Implement rollback for failed batch registrations:

```rust
async fn register_with_rollback(
    network: &mut DistributedNetwork,
    registrations: Vec<(String, String)>
) -> Result<Vec<String>, anyhow::Error> {
    let mut successful_aliases = Vec::new();
    
    for (actor_name, network_id) in registrations {
        match network.register_remote_actor(&actor_name, &network_id).await {
            Ok(alias) => {
                successful_aliases.push(alias);
            },
            Err(e) => {
                // Rollback previous registrations
                for alias in &successful_aliases {
                    if let Err(rollback_err) = network.unregister_remote_actor(alias).await {
                        eprintln!("⚠️  Rollback failed for {}: {}", alias, rollback_err);
                    }
                }
                return Err(e);
            }
        }
    }
    
    Ok(successful_aliases)
}
```

## Best Practices

### 1. Proactive Conflict Analysis

```rust
// Analyze potential conflicts before registration
async fn plan_registrations(
    network: &DistributedNetwork,
    planned_registrations: &[(String, String)]
) -> RegistrationPlan {
    let mut plan = RegistrationPlan::new();
    
    for (actor_name, network_id) in planned_registrations {
        let analysis = analyze_potential_conflicts(network, actor_name, network_id).await.unwrap();
        
        if analysis.has_conflicts() {
            plan.add_with_resolution(actor_name, network_id, analysis.best_resolution());
        } else {
            plan.add_direct(actor_name, network_id);
        }
    }
    
    plan
}
```

### 2. Documentation and Tracking

```rust
// Track and document conflict resolutions
struct ConflictResolutionLog {
    entries: Vec<ConflictLogEntry>,
}

struct ConflictLogEntry {
    timestamp: chrono::DateTime<chrono::Utc>,
    original_name: String,
    resolved_alias: String,
    strategy_used: ConflictResolutionStrategy,
    reason: String,
}

impl ConflictResolutionLog {
    fn log_resolution(&mut self, 
        original_name: String, 
        resolved_alias: String, 
        strategy: ConflictResolutionStrategy,
        reason: String
    ) {
        self.entries.push(ConflictLogEntry {
            timestamp: chrono::Utc::now(),
            original_name,
            resolved_alias,
            strategy_used: strategy,
            reason,
        });
    }
    
    fn generate_report(&self) -> String {
        // Generate human-readable conflict resolution report
        format!("Conflict Resolution Report\n{:#?}", self.entries)
    }
}
```

### 3. Testing Conflict Scenarios

```rust
#[cfg(test)]
mod conflict_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_local_remote_conflict_resolution() {
        let mut network = create_test_network().await;
        
        // Register local actor
        network.register_local_actor("processor", TestActor::new()).unwrap();
        
        // Try to register remote actor with same name
        let alias = network.register_remote_actor_with_strategy(
            "processor",
            "remote_network",
            ConflictResolutionStrategy::AutoAlias
        ).await.unwrap();
        
        assert_eq!(alias, "processor_1");
    }
    
    #[tokio::test] 
    async fn test_multiple_remote_conflicts() {
        let mut network = create_test_network().await;
        
        // Register multiple remote actors with same name
        let alias1 = network.register_remote_actor("auth", "network1").await.unwrap();
        let alias2 = network.register_remote_actor_with_strategy(
            "auth", 
            "network2",
            ConflictResolutionStrategy::NetworkPrefix
        ).await.unwrap();
        
        assert_eq!(alias1, "auth");
        assert_eq!(alias2, "network2_auth");
    }
}
```

## Next Steps

- [Setting Up Distributed Networks](setting-up-networks.md) - Basic network setup
- [Remote Actors](remote-actors.md) - Working with remote actors
- [Discovery & Registration](discovery-registration.md) - Network discovery

## Related Documentation

- [Architecture: Distributed Networks](../../architecture/distributed-networks.md)
- [Tutorial: Distributed Workflow Example](../../tutorials/distributed-workflow-example.md)
