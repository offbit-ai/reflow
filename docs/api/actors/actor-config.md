# ActorConfig System

The ActorConfig system provides a unified configuration framework for all actors in Reflow, enabling dynamic configuration, runtime parameter adjustment, and consistent actor behavior across different deployment environments.

## Overview

ActorConfig replaces the previous ad-hoc configuration approach with a structured, type-safe system that supports:

- **Type-Safe Configuration**: Strongly typed configuration parameters with validation
- **Dynamic Updates**: Runtime configuration changes without actor restart
- **Environment Variables**: Automatic environment variable injection
- **JSON/YAML Support**: Flexible configuration file formats
- **Validation & Defaults**: Built-in validation with sensible defaults
- **Metadata Integration**: Rich metadata for configuration documentation

## Basic Usage

### Simple Actor Configuration

```rust
use reflow_network::actor::{Actor, ActorConfig, ActorContext};
use std::collections::HashMap;

#[derive(Debug)]
struct ProcessorActor {
    config: ActorConfig,
}

impl ProcessorActor {
    fn new() -> Self {
        Self {
            config: ActorConfig::default(),
        }
    }
}

impl Actor for ProcessorActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Extract configuration values
        let batch_size = config.get_number("batch_size").unwrap_or(10.0) as usize;
        let timeout_ms = config.get_number("timeout_ms").unwrap_or(5000.0) as u64;
        let enable_retry = config.get_boolean("enable_retry").unwrap_or(true);
        let processor_name = config.get_string("name").unwrap_or("default_processor".to_string().into());
        
        Box::pin(async move {
            println!("Processor {} starting with batch_size={}, timeout={}ms, retry={}", 
                processor_name, batch_size, timeout_ms, enable_retry);
            
            // Actor implementation using configuration...
        })
    }
    
    // ... other Actor trait methods
}
```

### Configuration from JSON

```json
{
  "name": "data_processor",
  "batch_size": 50,
  "timeout_ms": 10000,
  "enable_retry": true,
  "processing_mode": "parallel",
  "max_retries": 3,
  "retry_delay_ms": 1000
}
```

```rust
// Load configuration from JSON
let config_json = r#"
{
  "name": "data_processor",
  "batch_size": 50,
  "timeout_ms": 10000,
  "enable_retry": true,
  "processing_mode": "parallel"
}
"#;

let config = ActorConfig::from_json(config_json)?;
let actor = ProcessorActor::new();

// Use configuration when creating actor process
let process = actor.create_process(config);
tokio::spawn(process);
```

## Configuration Sources

### Environment Variables

ActorConfig automatically reads from environment variables with configurable prefixes:

```rust
// Environment variables:
// PROCESSOR_BATCH_SIZE=100
// PROCESSOR_TIMEOUT_MS=15000
// PROCESSOR_ENABLE_RETRY=false

let config = ActorConfig::from_env("PROCESSOR")?;

// Access values
let batch_size = config.get_number("batch_size").unwrap(); // 100.0
let timeout = config.get_number("timeout_ms").unwrap();   // 15000.0
let retry = config.get_boolean("enable_retry").unwrap();  // false
```

### Configuration Files

```rust
// From YAML file
let config = ActorConfig::from_yaml_file("configs/processor.yaml").await?;

// From JSON file
let config = ActorConfig::from_json_file("configs/processor.json").await?;

// From TOML file
let config = ActorConfig::from_toml_file("configs/processor.toml").await?;
```

### Combined Sources with Precedence

```rust
// Build configuration with precedence: CLI args > env vars > config file > defaults
let config = ActorConfig::builder()
    .from_file("configs/defaults.yaml").await?
    .from_env("PROCESSOR")?
    .from_args(std::env::args())?
    .build()?;
```

## Configuration Schema and Validation

### Defining Configuration Schema

```rust
use reflow_network::actor::{ActorConfigSchema, ConfigField, ConfigType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct ProcessorConfigSchema {
    #[serde(default = "default_batch_size")]
    batch_size: u32,
    
    #[serde(default = "default_timeout")]
    timeout_ms: u64,
    
    #[serde(default)]
    enable_retry: bool,
    
    #[serde(default = "default_name")]
    name: String,
    
    processing_mode: ProcessingMode,
}

#[derive(Debug, Serialize, Deserialize)]
enum ProcessingMode {
    Sequential,
    Parallel,
    Batch,
}

fn default_batch_size() -> u32 { 10 }
fn default_timeout() -> u64 { 5000 }
fn default_name() -> String { "processor".to_string() }

impl ActorConfigSchema for ProcessorConfigSchema {
    fn schema() -> Vec<ConfigField> {
        vec![
            ConfigField {
                name: "batch_size".to_string(),
                config_type: ConfigType::Number,
                required: false,
                default_value: Some(serde_json::Value::Number(10.into())),
                description: Some("Number of items to process in each batch".to_string()),
                validation: Some("Must be between 1 and 1000".to_string()),
            },
            ConfigField {
                name: "timeout_ms".to_string(),
                config_type: ConfigType::Number,
                required: false,
                default_value: Some(serde_json::Value::Number(5000.into())),
                description: Some("Processing timeout in milliseconds".to_string()),
                validation: Some("Must be positive".to_string()),
            },
            ConfigField {
                name: "enable_retry".to_string(),
                config_type: ConfigType::Boolean,
                required: false,
                default_value: Some(serde_json::Value::Bool(false)),
                description: Some("Enable automatic retry on failure".to_string()),
                validation: None,
            },
            ConfigField {
                name: "name".to_string(),
                config_type: ConfigType::String,
                required: false,
                default_value: Some(serde_json::Value::String("processor".to_string())),
                description: Some("Actor instance name".to_string()),
                validation: Some("Must be non-empty alphanumeric".to_string()),
            },
            ConfigField {
                name: "processing_mode".to_string(),
                config_type: ConfigType::String,
                required: true,
                default_value: None,
                description: Some("Processing execution mode".to_string()),
                validation: Some("Must be one of: sequential, parallel, batch".to_string()),
            },
        ]
    }
    
    fn validate(&self) -> Result<(), String> {
        if self.batch_size == 0 || self.batch_size > 1000 {
            return Err("batch_size must be between 1 and 1000".to_string());
        }
        
        if self.timeout_ms == 0 {
            return Err("timeout_ms must be positive".to_string());
        }
        
        if self.name.is_empty() {
            return Err("name cannot be empty".to_string());
        }
        
        Ok(())
    }
}
```

### Using Typed Configuration

```rust
impl Actor for ProcessorActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Parse and validate configuration against schema
        let typed_config: ProcessorConfigSchema = config.parse_typed()?;
        
        // Configuration is now type-safe and validated
        let batch_size = typed_config.batch_size;
        let timeout = Duration::from_millis(typed_config.timeout_ms);
        let enable_retry = typed_config.enable_retry;
        let name = typed_config.name;
        let mode = typed_config.processing_mode;
        
        Box::pin(async move {
            match mode {
                ProcessingMode::Sequential => {
                    // Sequential processing logic
                },
                ProcessingMode::Parallel => {
                    // Parallel processing logic
                },
                ProcessingMode::Batch => {
                    // Batch processing logic
                },
            }
        })
    }
}
```

## Dynamic Configuration Updates

### Runtime Configuration Changes

```rust
use tokio::sync::watch;

struct DynamicProcessorActor {
    config_receiver: watch::Receiver<ActorConfig>,
}

impl DynamicProcessorActor {
    fn new(config_receiver: watch::Receiver<ActorConfig>) -> Self {
        Self { config_receiver }
    }
}

impl Actor for DynamicProcessorActor {
    fn create_process(&self, initial_config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let mut config_receiver = self.config_receiver.clone();
        
        Box::pin(async move {
            let mut current_config = initial_config;
            
            loop {
                // Check for configuration updates
                if config_receiver.has_changed().unwrap_or(false) {
                    current_config = config_receiver.borrow().clone();
                    println!("Configuration updated: {:?}", current_config);
                    
                    // Apply new configuration
                    let batch_size = current_config.get_number("batch_size").unwrap_or(10.0) as usize;
                    println!("New batch size: {}", batch_size);
                }
                
                // Process with current configuration
                // ... actor logic ...
                
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
    }
}

// Update configuration at runtime
async fn update_actor_config() -> Result<(), Box<dyn std::error::Error>> {
    let (config_sender, config_receiver) = watch::channel(ActorConfig::default());
    
    let actor = DynamicProcessorActor::new(config_receiver);
    tokio::spawn(actor.create_process(ActorConfig::default()));
    
    // Update configuration after 5 seconds
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    let new_config = ActorConfig::from_json(r#"
    {
        "batch_size": 100,
        "timeout_ms": 20000,
        "enable_retry": true
    }
    "#)?;
    
    config_sender.send(new_config)?;
    println!("Configuration updated!");
    
    Ok(())
}
```

## Configuration in Networks and Graphs

### Network-Level Configuration

```rust
use reflow_network::network::{Network, NetworkConfig};

// Configure network with global defaults
let network_config = NetworkConfig {
    default_actor_config: Some(ActorConfig::from_json(r#"
    {
        "default_timeout_ms": 10000,
        "enable_monitoring": true,
        "log_level": "info"
    }
    "#)?),
    // ... other network config
};

let mut network = Network::new(network_config);

// Add actor with specific configuration
let actor_config = ActorConfig::from_json(r#"
{
    "batch_size": 50,
    "timeout_ms": 15000,
    "name": "data_processor_1"
}
"#)?;

network.add_node_with_config("processor1", "DataProcessorActor", Some(actor_config))?;
```

### Graph-Level Configuration

```json
{
  "caseSensitive": false,
  "properties": {
    "name": "data_processing_pipeline"
  },
  "processes": {
    "collector": {
      "component": "DataCollectorActor",
      "metadata": {
        "config": {
          "source_url": "https://api.example.com/data",
          "poll_interval_ms": 5000,
          "batch_size": 100
        }
      }
    },
    "processor": {
      "component": "DataProcessorActor",
      "metadata": {
        "config": {
          "processing_mode": "parallel",
          "worker_count": 4,
          "timeout_ms": 30000
        }
      }
    },
    "validator": {
      "component": "DataValidatorActor",
      "metadata": {
        "config": {
          "strict_validation": true,
          "schema_file": "./schemas/data.json"
        }
      }
    }
  },
  "connections": [
    {
      "from": { "nodeId": "collector", "portId": "Output" },
      "to": { "nodeId": "processor", "portId": "Input" }
    },
    {
      "from": { "nodeId": "processor", "portId": "Output" },
      "to": { "nodeId": "validator", "portId": "Input" }
    }
  ]
}
```

### Loading Graph with Configurations

```rust
use reflow_network::graph::Graph;

// Load graph - configurations are automatically extracted from metadata
let graph = Graph::load_from_file("data_pipeline.graph.json").await?;

// Each actor will receive its specific configuration
// Network automatically extracts config from process metadata
let mut network = Network::new(NetworkConfig::default());
network.load_graph(graph).await?;
```

## Environment-Specific Configurations

### Development vs Production

```rust
// Development configuration
let dev_config = ActorConfig::from_json(r#"
{
    "log_level": "debug",
    "enable_profiling": true,
    "timeout_ms": 60000,
    "batch_size": 5
}
"#)?;

// Production configuration
let prod_config = ActorConfig::from_json(r#"
{
    "log_level": "warn",
    "enable_profiling": false,
    "timeout_ms": 10000,
    "batch_size": 100
}
"#)?;

// Select configuration based on environment
let config = match std::env::var("ENVIRONMENT").as_deref() {
    Ok("production") => prod_config,
    Ok("staging") => prod_config, // Use prod config for staging
    _ => dev_config, // Default to dev config
};
```

### Configuration Profiles

```rust
// Base configuration
let base_config = ActorConfig::from_yaml_file("configs/base.yaml").await?;

// Environment-specific overrides
let env = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());
let env_config_path = format!("configs/{}.yaml", env);

let final_config = if std::path::Path::new(&env_config_path).exists() {
    base_config.merge_with(ActorConfig::from_yaml_file(&env_config_path).await?)?
} else {
    base_config
};
```

## Configuration Migration

### Migrating from Direct HashMap

**Before (Old Pattern):**
```rust
// Old approach - direct HashMap usage
impl Actor for OldActor {
    fn set_config(&mut self, config: HashMap<String, serde_json::Value>) {
        self.batch_size = config.get("batch_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0) as usize;
        
        self.timeout = Duration::from_millis(
            config.get("timeout_ms")
                .and_then(|v| v.as_f64())
                .unwrap_or(5000.0) as u64
        );
    }
}
```

**After (New Pattern):**
```rust
// New approach - ActorConfig
impl Actor for NewActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let batch_size = config.get_number("batch_size").unwrap_or(10.0) as usize;
        let timeout = Duration::from_millis(config.get_number("timeout_ms").unwrap_or(5000.0) as u64);
        
        Box::pin(async move {
            // Actor implementation with configuration
        })
    }
}
```

### Migration Helper

```rust
// Helper function to migrate from old HashMap format
impl ActorConfig {
    pub fn from_legacy_hashmap(legacy: HashMap<String, serde_json::Value>) -> Self {
        let mut config = ActorConfig::default();
        
        for (key, value) in legacy {
            config.set(&key, value);
        }
        
        config
    }
}

// Usage in migration
let legacy_config = HashMap::from([
    ("batch_size".to_string(), serde_json::Value::Number(50.into())),
    ("timeout_ms".to_string(), serde_json::Value::Number(10000.into())),
]);

let actor_config = ActorConfig::from_legacy_hashmap(legacy_config);
```

## Advanced Features

### Conditional Configuration

```rust
#[derive(Debug, Serialize, Deserialize)]
struct ConditionalConfig {
    #[serde(default)]
    enable_cache: bool,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_size_mb: Option<u32>,
    
    #[serde(skip_serializing_if = "Option::is_none")]
    cache_ttl_seconds: Option<u64>,
}

impl ActorConfigSchema for ConditionalConfig {
    fn validate(&self) -> Result<(), String> {
        if self.enable_cache {
            if self.cache_size_mb.is_none() {
                return Err("cache_size_mb is required when cache is enabled".to_string());
            }
            if self.cache_ttl_seconds.is_none() {
                return Err("cache_ttl_seconds is required when cache is enabled".to_string());
            }
        }
        Ok(())
    }
}
```

### Configuration Inheritance

```rust
// Base actor configuration
let base_config = ActorConfig::from_json(r#"
{
    "timeout_ms": 10000,
    "enable_logging": true,
    "log_level": "info"
}
"#)?;

// Specialized configuration inheriting from base
let specialized_config = base_config.extend_with(ActorConfig::from_json(r#"
{
    "batch_size": 50,
    "processing_mode": "parallel",
    "timeout_ms": 20000
}
"#)?)?;

// Result combines both configs with specialized values taking precedence
// timeout_ms: 20000 (overridden)
// enable_logging: true (inherited)
// log_level: "info" (inherited)  
// batch_size: 50 (added)
// processing_mode: "parallel" (added)
```

### Secret Management

```rust
use reflow_network::actor::SecretResolver;

// Configuration with secret references
let config_with_secrets = ActorConfig::from_json(r#"
{
    "database_url": "${secret:DATABASE_URL}",
    "api_key": "${secret:API_KEY}",
    "batch_size": 100
}
"#)?;

// Resolve secrets from environment or secret store
let secret_resolver = SecretResolver::new()
    .with_env_prefix("SECRET_")
    .with_vault_client(vault_client);

let resolved_config = secret_resolver.resolve(config_with_secrets).await?;

// Secrets are now resolved:
// database_url: "postgresql://user:password@localhost/db"  
// api_key: "sk-1234567890abcdef"
// batch_size: 100
```

## Testing with ActorConfig

### Test Configuration Helpers

```rust
use reflow_network::actor::testing::TestActorConfig;

#[tokio::test]
async fn test_actor_with_config() {
    let test_config = TestActorConfig::builder()
        .with_number("batch_size", 10.0)
        .with_boolean("enable_retry", false)
        .with_string("name", "test_actor")
        .build();
    
    let actor = MyActor::new();
    let process = actor.create_process(test_config.into());
    
    // Test actor behavior with specific configuration
    // ...
}

#[tokio::test]
async fn test_actor_configuration_validation() {
    let invalid_config = ActorConfig::from_json(r#"
    {
        "batch_size": -1,
        "timeout_ms": 0
    }
    "#).unwrap();
    
    let schema = MyActorConfigSchema::default();
    assert!(schema.validate_config(&invalid_config).is_err());
}
```

### Configuration Mocking

```rust
// Mock configuration for testing
struct MockConfigProvider {
    configs: HashMap<String, ActorConfig>,
}

impl MockConfigProvider {
    fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }
    
    fn add_config(&mut self, actor_id: &str, config: ActorConfig) {
        self.configs.insert(actor_id.to_string(), config);
    }
}

impl ConfigProvider for MockConfigProvider {
    async fn get_config(&self, actor_id: &str) -> Result<ActorConfig, ConfigError> {
        self.configs.get(actor_id)
            .cloned()
            .ok_or_else(|| ConfigError::NotFound(actor_id.to_string()))
    }
}
```

## Best Practices

### Configuration Organization

1. **Use Typed Schemas**: Define strongly typed configuration schemas for validation
2. **Provide Sensible Defaults**: Always provide reasonable default values
3. **Document Configuration**: Include descriptions and validation rules
4. **Environment Separation**: Use different configurations for different environments
5. **Secret Security**: Never store secrets in plain text configuration files

### Performance Considerations

```rust
// Cache parsed configuration for performance
use std::sync::Arc;
use once_cell::sync::OnceCell;

struct CachedConfigActor {
    cached_config: OnceCell<Arc<ProcessorConfigSchema>>,
}

impl Actor for CachedConfigActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Parse configuration once and cache it
        let parsed_config = self.cached_config.get_or_init(|| {
            Arc::new(config.parse_typed().expect("Invalid configuration"))
        }).clone();
        
        Box::pin(async move {
            // Use cached configuration
            let batch_size = parsed_config.batch_size;
            // ...
        })
    }
}
```

### Error Handling

```rust
use reflow_network::actor::ConfigError;

impl Actor for RobustActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            // Graceful configuration error handling
            let batch_size = match config.get_number("batch_size") {
                Some(size) if size > 0.0 => size as usize,
                Some(_) => {
                    eprintln!("Invalid batch_size, using default");
                    10
                },
                None => {
                    println!("No batch_size specified, using default");
                    10
                }
            };
            
            // Continue with actor logic
        })
    }
}
```

## Next Steps

- [Actor Creation Guide](creating-actors.md) - Learn how to create actors that use ActorConfig
- [Multi-Graph Composition](../../architecture/multi-graph-composition.md) - Using ActorConfig in multi-graph setups
- [ActorConfig Migration Guide](../../migration/actor-config-migration.md) - Migrating existing actors
