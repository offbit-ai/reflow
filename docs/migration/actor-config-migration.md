# ActorConfig Migration Guide

This guide helps you migrate existing actors from the old HashMap-based configuration approach to the new ActorConfig system, providing a smooth transition path with minimal breaking changes.

## Migration Overview

The ActorConfig system replaces the previous `set_config(HashMap<String, serde_json::Value>)` method with a more robust `create_process(ActorConfig)` approach that provides:

- **Type Safety**: Strongly typed configuration with validation
- **Better Error Handling**: Clear configuration error messages
- **Dynamic Updates**: Runtime configuration changes
- **Multiple Sources**: Support for JSON, YAML, environment variables
- **Schema Validation**: Built-in validation and defaults

## Quick Migration Steps

### 1. Update Actor Trait Implementation

**Before (Old Pattern):**
```rust
use reflow_network::actor::{Actor, ActorContext};
use std::collections::HashMap;

pub struct DataProcessor {
    batch_size: usize,
    timeout: Duration,
    enable_retry: bool,
}

impl Actor for DataProcessor {
    fn set_config(&mut self, config: HashMap<String, serde_json::Value>) {
        self.batch_size = config.get("batch_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0) as usize;
        
        self.timeout = Duration::from_millis(
            config.get("timeout_ms")
                .and_then(|v| v.as_f64()) 
                .unwrap_or(5000.0) as u64
        );
        
        self.enable_retry = config.get("enable_retry")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
    }
    
    async fn run(&self, context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
        // Actor logic using self.batch_size, self.timeout, etc.
        // ...
    }
}
```

**After (New Pattern):**
```rust
use reflow_network::actor::{Actor, ActorConfig, ActorContext};
use std::collections::HashMap;

pub struct DataProcessor;

impl DataProcessor {
    pub fn new() -> Self {
        Self
    }
}

impl Actor for DataProcessor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Extract configuration values
        let batch_size = config.get_number("batch_size").unwrap_or(10.0) as usize;
        let timeout = Duration::from_millis(config.get_number("timeout_ms").unwrap_or(5000.0) as u64);
        let enable_retry = config.get_boolean("enable_retry").unwrap_or(true);
        
        Box::pin(async move {
            // Actor logic using configuration values
            // ...
        })
    }
    
    // Remove the old set_config method
    // fn set_config(&mut self, config: HashMap<String, serde_json::Value>) { ... }
    
    // Remove the old run method  
    // async fn run(&self, context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> { ... }
}
```

### 2. Update Actor Registration

**Before:**
```rust
let mut network = Network::new();
let mut processor = DataProcessor::new();

// Configure actor with HashMap
let config = HashMap::from([
    ("batch_size".to_string(), serde_json::Value::Number(50.into())),
    ("timeout_ms".to_string(), serde_json::Value::Number(10000.into())),
    ("enable_retry".to_string(), serde_json::Value::Bool(false)),
]);

processor.set_config(config);
network.register_actor("processor", processor)?;
```

**After:**
```rust
let mut network = Network::new();
let processor = DataProcessor::new();

// Configuration is provided when adding to network
let config = ActorConfig::from_json(r#"
{
    "batch_size": 50,
    "timeout_ms": 10000,
    "enable_retry": false
}
"#)?;

network.register_actor("processor", processor)?;
network.add_node_with_config("processor", "processor", Some(config))?;
```

## Migration Patterns

### Pattern 1: Simple State-Based Actor

**Before:**
```rust
struct TimerActor {
    interval_ms: u64,
    max_ticks: Option<u64>,
    current_ticks: u64,
}

impl Actor for TimerActor {
    fn set_config(&mut self, config: HashMap<String, serde_json::Value>) {
        self.interval_ms = config.get("interval_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(1000.0) as u64;
        
        self.max_ticks = config.get("max_ticks")
            .and_then(|v| v.as_f64())
            .map(|v| v as u64);
    }
    
    async fn run(&self, context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
        let mut output = HashMap::new();
        
        if self.current_ticks < self.max_ticks.unwrap_or(u64::MAX) {
            // Emit tick
            output.insert("tick".to_string(), Message::Integer(self.current_ticks as i64));
        }
        
        Ok(output)
    }
}
```

**After:**
```rust
struct TimerActor;

impl Actor for TimerActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let interval_ms = config.get_number("interval_ms").unwrap_or(1000.0) as u64;
        let max_ticks = config.get_number("max_ticks").map(|v| v as u64);
        
        Box::pin(async move {
            let mut current_ticks = 0u64;
            let interval = Duration::from_millis(interval_ms);
            
            loop {
                if let Some(max) = max_ticks {
                    if current_ticks >= max {
                        break;
                    }
                }
                
                // Emit tick
                current_ticks += 1;
                
                tokio::time::sleep(interval).await;
            }
        })
    }
}
```

### Pattern 2: Complex Configuration with Validation

**Before:**
```rust
struct DatabaseActor {
    connection_string: String,
    pool_size: u32,
    query_timeout: Duration,
}

impl Actor for DatabaseActor {
    fn set_config(&mut self, config: HashMap<String, serde_json::Value>) {
        self.connection_string = config.get("connection_string")
            .and_then(|v| v.as_str())
            .unwrap_or("postgresql://localhost/db")
            .to_string();
        
        let pool_size = config.get("pool_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0) as u32;
        
        // Manual validation
        self.pool_size = if pool_size > 0 && pool_size <= 100 {
            pool_size
        } else {
            eprintln!("Invalid pool_size {}, using default", pool_size);
            10
        };
        
        self.query_timeout = Duration::from_millis(
            config.get("query_timeout_ms")
                .and_then(|v| v.as_f64())
                .unwrap_or(30000.0) as u64
        );
    }
}
```

**After (with typed configuration):**
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct DatabaseConfig {
    #[serde(default = "default_connection_string")]
    connection_string: String,
    
    #[serde(default = "default_pool_size")]
    pool_size: u32,
    
    #[serde(default = "default_query_timeout")]
    query_timeout_ms: u64,
}

fn default_connection_string() -> String {
    "postgresql://localhost/db".to_string()
}

fn default_pool_size() -> u32 { 10 }
fn default_query_timeout() -> u64 { 30000 }

impl ActorConfigSchema for DatabaseConfig {
    fn validate(&self) -> Result<(), String> {
        if self.connection_string.is_empty() {
            return Err("connection_string cannot be empty".to_string());
        }
        
        if self.pool_size == 0 || self.pool_size > 100 {
            return Err("pool_size must be between 1 and 100".to_string());
        }
        
        if self.query_timeout_ms == 0 {
            return Err("query_timeout_ms must be positive".to_string());
        }
        
        Ok(())
    }
}

struct DatabaseActor;

impl Actor for DatabaseActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Parse and validate configuration
        let db_config: DatabaseConfig = config.parse_typed().expect("Invalid configuration");
        
        let connection_string = db_config.connection_string;
        let pool_size = db_config.pool_size;
        let query_timeout = Duration::from_millis(db_config.query_timeout_ms);
        
        Box::pin(async move {
            // Database actor implementation
            // Configuration is guaranteed to be valid
        })
    }
}
```

### Pattern 3: Actors with Complex State Management

**Before:**
```rust
struct StatefulProcessor {
    state: Arc<Mutex<ProcessorState>>,
    config: ProcessorConfig,
}

#[derive(Clone)]
struct ProcessorConfig {
    batch_size: usize,
    processing_mode: ProcessingMode,
}

impl Actor for StatefulProcessor {
    fn set_config(&mut self, config: HashMap<String, serde_json::Value>) {
        self.config.batch_size = config.get("batch_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0) as usize;
        
        let mode_str = config.get("processing_mode")
            .and_then(|v| v.as_str())
            .unwrap_or("sequential");
        
        self.config.processing_mode = match mode_str {
            "parallel" => ProcessingMode::Parallel,
            "batch" => ProcessingMode::Batch,
            _ => ProcessingMode::Sequential,
        };
    }
    
    async fn run(&self, context: ActorContext) -> Result<HashMap<String, Message>, anyhow::Error> {
        // Use self.config and self.state
        // ...
    }
}
```

**After:**
```rust
struct StatefulProcessor;

impl Actor for StatefulProcessor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        let batch_size = config.get_number("batch_size").unwrap_or(10.0) as usize;
        let processing_mode = match config.get_string("processing_mode").as_deref() {
            Some("parallel") => ProcessingMode::Parallel,
            Some("batch") => ProcessingMode::Batch,
            _ => ProcessingMode::Sequential,
        };
        
        Box::pin(async move {
            // Create state inside the process
            let state = Arc::new(Mutex::new(ProcessorState::new()));
            
            // Actor implementation with local state
            // ...
        })
    }
}
```

## Graph Migration

### Updating Graph Definitions

**Before:**
```json
{
  "processes": {
    "processor": {
      "component": "DataProcessor",
      "metadata": {
        "batch_size": 50,
        "timeout_ms": 10000,
        "enable_retry": false
      }
    }
  }
}
```

**After:**
```json
{
  "processes": {
    "processor": {
      "component": "DataProcessor", 
      "metadata": {
        "config": {
          "batch_size": 50,
          "timeout_ms": 10000,
          "enable_retry": false
        }
      }
    }
  }
}
```

The configuration is now nested under a `"config"` key in the metadata, which the system automatically extracts and converts to an ActorConfig.

## Migration Utilities

### Automatic Configuration Migration

```rust
use reflow_network::actor::ActorConfig;

// Helper to migrate old graph metadata format
pub fn migrate_graph_metadata(old_metadata: &serde_json::Value) -> serde_json::Value {
    if let Some(obj) = old_metadata.as_object() {
        // Check if it already has a "config" key
        if obj.contains_key("config") {
            return old_metadata.clone(); // Already migrated
        }
        
        // Wrap existing metadata in "config" key
        let mut new_metadata = serde_json::Map::new();
        new_metadata.insert("config".to_string(), old_metadata.clone());
        
        serde_json::Value::Object(new_metadata)
    } else {
        old_metadata.clone()
    }
}

// Helper to migrate legacy HashMap config to ActorConfig
impl ActorConfig {
    pub fn from_legacy_hashmap(legacy: HashMap<String, serde_json::Value>) -> Self {
        let mut config = ActorConfig::default();
        
        for (key, value) in legacy {
            config.set(&key, value);
        }
        
        config
    }
}
```

### Migration Script

```rust
// migration_script.rs - Tool to migrate existing graph files
use std::path::Path;
use tokio::fs;

pub async fn migrate_graph_file(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path).await?;
    let mut graph: serde_json::Value = serde_json::from_str(&content)?;
    
    // Migrate processes metadata
    if let Some(processes) = graph.get_mut("processes").and_then(|p| p.as_object_mut()) {
        for (_, process) in processes.iter_mut() {
            if let Some(metadata) = process.get_mut("metadata") {
                *metadata = migrate_graph_metadata(metadata);
            }
        }
    }
    
    // Write back the migrated graph
    let migrated_content = serde_json::to_string_pretty(&graph)?;
    fs::write(path, migrated_content).await?;
    
    println!("Migrated: {}", path.display());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let graph_files = glob::glob("**/*.graph.json")?;
    
    for entry in graph_files {
        if let Ok(path) = entry {
            migrate_graph_file(&path).await?;
        }
    }
    
    println!("Migration completed!");
    Ok(())
}
```

## Backward Compatibility

### Temporary Compatibility Layer

If you need to maintain compatibility with old and new systems during migration:

```rust
use reflow_network::actor::ActorConfig;

pub struct CompatibilityActor {
    // Store configuration in both formats during transition
    legacy_config: Option<HashMap<String, serde_json::Value>>,
    actor_config: Option<ActorConfig>,
}

impl CompatibilityActor {
    pub fn new() -> Self {
        Self {
            legacy_config: None,
            actor_config: None,
        }
    }
    
    // Helper to get config value from either format
    fn get_config_value<T>(&self, key: &str) -> Option<T> 
    where
        T: serde::de::DeserializeOwned + Clone,
    {
        // Try new format first
        if let Some(config) = &self.actor_config {
            if let Ok(value) = config.get::<T>(key) {
                return Some(value);
            }
        }
        
        // Fall back to legacy format
        if let Some(legacy) = &self.legacy_config {
            if let Some(value) = legacy.get(key) {
                if let Ok(parsed) = serde_json::from_value::<T>(value.clone()) {
                    return Some(parsed);
                }
            }
        }
        
        None
    }
}

impl Actor for CompatibilityActor {
    // Support old method during transition
    fn set_config(&mut self, config: HashMap<String, serde_json::Value>) {
        self.legacy_config = Some(config);
    }
    
    // Implement new method
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        // Use helper method to get values from either format
        let batch_size = self.get_config_value::<f64>("batch_size").unwrap_or(10.0) as usize;
        let timeout_ms = self.get_config_value::<f64>("timeout_ms").unwrap_or(5000.0) as u64;
        
        Box::pin(async move {
            // Actor implementation
        })
    }
}
```

## Testing Migration

### Unit Tests for Migrated Actors

```rust
#[cfg(test)]
mod migration_tests {
    use super::*;
    use reflow_network::actor::testing::TestActorConfig;
    
    #[tokio::test]
    async fn test_migrated_actor_with_legacy_values() {
        // Test that migrated actor works with old-style values
        let config = TestActorConfig::builder()
            .with_number("batch_size", 50.0)
            .with_number("timeout_ms", 10000.0)
            .with_boolean("enable_retry", false)
            .build();
        
        let actor = DataProcessor::new();
        
        // Should not panic with valid configuration
        let process = actor.create_process(config.into());
        
        // Test that process can be spawned
        let handle = tokio::spawn(process);
        
        // Clean shutdown for test
        tokio::time::sleep(Duration::from_millis(100)).await;
        handle.abort();
    }
    
    #[test]
    fn test_configuration_migration_helper() {
        let legacy_config = HashMap::from([
            ("batch_size".to_string(), serde_json::Value::Number(25.into())),
            ("timeout_ms".to_string(), serde_json::Value::Number(15000.into())),
        ]);
        
        let actor_config = ActorConfig::from_legacy_hashmap(legacy_config);
        
        assert_eq!(actor_config.get_number("batch_size"), Some(25.0));
        assert_eq!(actor_config.get_number("timeout_ms"), Some(15000.0));
    }
}
```

## Common Migration Issues

### Issue 1: Missing Configuration Values

**Problem:** Actor expects configuration values that aren't provided.

**Solution:** Use default values and graceful degradation:

```rust
// Before: Could panic
let batch_size = config.get("batch_size").unwrap().as_f64().unwrap() as usize;

// After: Graceful with defaults
let batch_size = config.get_number("batch_size").unwrap_or(10.0) as usize;
```

### Issue 2: Type Conversion Errors

**Problem:** Configuration values have different types than expected.

**Solution:** Use explicit type checking and conversion:

```rust
// Robust type handling
let batch_size = match config.get_number("batch_size") {
    Some(size) if size > 0.0 => size as usize,
    Some(invalid) => {
        eprintln!("Invalid batch_size: {}, using default", invalid);
        10
    },
    None => {
        println!("No batch_size specified, using default");
        10
    }
};
```

### Issue 3: State Management Migration

**Problem:** Actors with complex internal state need restructuring.

**Solution:** Move state into the process:

```rust
// Before: State as struct fields
struct StatefulActor {
    state: ProcessorState,
    config: Config,
}

// After: State managed in process
impl Actor for StatefulActor {
    fn create_process(&self, config: ActorConfig) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> {
        Box::pin(async move {
            let mut state = ProcessorState::new();
            
            loop {
                // Process using local state
                // ...
            }
        })
    }
}
```

## Migration Checklist

### Pre-Migration
- [ ] Identify all actors using `set_config`
- [ ] Document current configuration formats
- [ ] Create backup of existing graph files
- [ ] Plan migration order (dependencies first)

### During Migration
- [ ] Update actor trait implementations
- [ ] Migrate configuration extraction logic
- [ ] Add typed configuration schemas (recommended)
- [ ] Update graph file metadata format
- [ ] Update actor registration code

### Post-Migration
- [ ] Test all actors with new configuration system
- [ ] Verify graph loading and execution
- [ ] Remove old `set_config` implementations
- [ ] Update documentation and examples
- [ ] Performance testing with new system

### Validation
- [ ] All actors receive expected configuration
- [ ] Configuration validation works correctly
- [ ] Default values are applied appropriately
- [ ] Error handling for invalid configurations
- [ ] Dynamic configuration updates (if used)

## Performance Considerations

### Before and After Performance

The new ActorConfig system provides better performance in several areas:

1. **Configuration Parsing**: One-time parsing vs repeated HashMap lookups
2. **Type Safety**: Compile-time validation reduces runtime errors
3. **Memory Usage**: More efficient internal representation
4. **Validation**: Built-in validation vs manual checking

### Benchmarking Migration

```rust
// benchmark_migration.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_old_config(c: &mut Criterion) {
    let config = HashMap::from([
        ("batch_size".to_string(), serde_json::Value::Number(50.into())),
        ("timeout_ms".to_string(), serde_json::Value::Number(10000.into())),
    ]);
    
    c.bench_function("old_config_extraction", |b| b.iter(|| {
        let batch_size = black_box(config.get("batch_size")
            .and_then(|v| v.as_f64())
            .unwrap_or(10.0) as usize);
        let timeout = black_box(config.get("timeout_ms")
            .and_then(|v| v.as_f64())
            .unwrap_or(5000.0) as u64);
    }));
}

fn benchmark_new_config(c: &mut Criterion) {
    let config = ActorConfig::from_json(r#"
    {
        "batch_size": 50,
        "timeout_ms": 10000
    }
    "#).unwrap();
    
    c.bench_function("new_config_extraction", |b| b.iter(|| {
        let batch_size = black_box(config.get_number("batch_size").unwrap_or(10.0) as usize);
        let timeout = black_box(config.get_number("timeout_ms").unwrap_or(5000.0) as u64);
    }));
}

criterion_group!(benches, benchmark_old_config, benchmark_new_config);
criterion_main!(benches);
```

## Next Steps

After completing the migration:

1. **Remove Legacy Code**: Clean up old `set_config` implementations
2. **Add Validation**: Implement typed configuration schemas for better validation
3. **Dynamic Configuration**: Consider adding runtime configuration updates
4. **Documentation**: Update all examples and documentation
5. **Monitoring**: Add configuration monitoring and alerting

## Getting Help

If you encounter issues during migration:

1. **Check Examples**: Look at migrated examples in the documentation
2. **Configuration Validation**: Use typed schemas to catch issues early
3. **Testing**: Write comprehensive tests for migrated actors
4. **Community**: Ask for help in the Reflow community forums
5. **GitHub Issues**: Report bugs or ask for clarification

The migration to ActorConfig provides significant benefits in terms of type safety, validation, and maintainability. While it requires some initial effort, the improved developer experience and runtime reliability make it worthwhile.
