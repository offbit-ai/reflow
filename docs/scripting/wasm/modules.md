# WASM Module Loading

This document describes how WebAssembly modules are loaded and managed in Reflow's WASM runtime.

## Module Loading Process

The module loading process follows these steps:

```
1. Read WASM binary
2. Create Extism manifest
3. Initialize plugin with host functions
4. Validate exported functions
5. Cache plugin instance
```

## Loading Methods

### From File System

```rust
// Load from compiled WASM file
let wasm_bytes = std::fs::read("path/to/actor.wasm")?;

let config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::Extism,
    source: wasm_bytes,
    entry_point: "process".to_string(),
    packages: None,
};

let actor = ScriptActor::new(config);
```

### From Memory

```rust
// Load from embedded bytes
let wasm_bytes = include_bytes!("../wasm/actor.wasm");

let config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::Extism,
    source: wasm_bytes.to_vec(),
    entry_point: "process".to_string(),
    packages: None,
};
```

### From Network

```rust
// Load from URL (requires custom implementation)
let wasm_bytes = fetch_wasm_from_url("https://example.com/actor.wasm").await?;

let config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::Extism,
    source: wasm_bytes,
    entry_point: "process".to_string(),
    packages: None,
};
```

## Module Requirements

WASM modules must export the following functions to be valid Reflow plugins:

### Required Exports

1. **`get_metadata`** - Returns plugin metadata
   ```rust
   #[plugin_fn]
   pub fn get_metadata() -> FnResult<Json<PluginMetadata>> {
       Ok(Json(metadata()))
   }
   ```

2. **`process`** - Main actor behavior function
   ```rust
   #[plugin_fn]
   pub fn process(context: Json<ActorContext>) -> FnResult<Json<ActorResult>> {
       // Actor logic here
   }
   ```

### Optional Exports

Additional functions can be exported for extended functionality:

```rust
#[plugin_fn]
pub fn validate_config(config: Json<Value>) -> FnResult<Json<bool>> {
    // Validate configuration
}

#[plugin_fn]
pub fn get_version() -> FnResult<Json<String>> {
    Ok(Json("1.0.0".to_string()))
}
```

## Manifest Configuration

The Extism manifest controls how modules are loaded:

```rust
let manifest = Manifest::default()
    .with_wasm(wasm)
    .with_allowed_host("*")  // Allow all host functions
    .with_memory_limit(100 * 1024 * 1024)  // 100MB limit
    .with_timeout(Duration::from_secs(30));  // 30s timeout
```

## Module Validation

Before a module can be used, it undergoes validation:

1. **Binary Validation**: Ensure it's valid WASM
2. **Export Validation**: Check required functions exist
3. **Metadata Validation**: Verify plugin metadata is valid
4. **Memory Requirements**: Check memory requirements

```rust
// Example validation in ExtismEngine
async fn validate_module(wasm_bytes: &[u8]) -> Result<()> {
    // Create temporary plugin for validation
    let manifest = Manifest::default()
        .with_wasm(Wasm::Data {
            data: wasm_bytes.to_vec(),
            meta: WasmMetadata::default(),
        });
    
    let plugin = Plugin::new(&manifest, [], false)?;
    
    // Check required exports
    if !plugin.has_function("get_metadata") {
        return Err(anyhow!("Missing required export: get_metadata"));
    }
    
    if !plugin.has_function("process") {
        return Err(anyhow!("Missing required export: process"));
    }
    
    // Validate metadata
    let metadata: PluginMetadata = plugin
        .call::<(), Json<PluginMetadata>>("get_metadata", ())?
        .into_inner();
    
    if metadata.component.is_empty() {
        return Err(anyhow!("Invalid metadata: empty component name"));
    }
    
    Ok(())
}
```

## Module Caching

Modules can be cached to improve performance:

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

struct ModuleCache {
    cache: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl ModuleCache {
    fn get(&self, key: &str) -> Option<Vec<u8>> {
        self.cache.lock().unwrap().get(key).cloned()
    }
    
    fn insert(&self, key: String, module: Vec<u8>) {
        self.cache.lock().unwrap().insert(key, module);
    }
}
```

## Hot Reloading

Support for hot reloading of WASM modules:

```rust
// Watch for file changes and reload
use notify::{Watcher, RecursiveMode, watcher};

fn watch_module(path: &Path, actor: Arc<Mutex<ScriptActor>>) {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = watcher(tx, Duration::from_secs(2)).unwrap();
    
    watcher.watch(path, RecursiveMode::NonRecursive).unwrap();
    
    loop {
        match rx.recv() {
            Ok(event) => {
                // Reload module
                let wasm_bytes = std::fs::read(path).unwrap();
                let config = ScriptConfig {
                    // ... config
                    source: wasm_bytes,
                };
                
                // Replace actor's engine
                let mut actor_guard = actor.lock().unwrap();
                *actor_guard = ScriptActor::new(config);
            }
            Err(e) => eprintln!("Watch error: {:?}", e),
        }
    }
}
```

## Module Lifecycle

```
┌─────────────┐
│   Loading   │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│ Validation  │
└──────┬──────┘
       │
       ▼
┌─────────────┐
│Initialization│
└──────┬──────┘
       │
       ▼
┌─────────────┐
│   Active    │◄─── Message Processing
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  Cleanup    │
└─────────────┘
```

## Error Handling

Common module loading errors:

```rust
#[derive(Debug, thiserror::Error)]
pub enum ModuleError {
    #[error("Invalid WASM binary")]
    InvalidBinary,
    
    #[error("Missing required export: {0}")]
    MissingExport(String),
    
    #[error("Invalid metadata: {0}")]
    InvalidMetadata(String),
    
    #[error("Module too large: {size} bytes (max: {max})")]
    TooLarge { size: usize, max: usize },
    
    #[error("Initialization failed: {0}")]
    InitFailed(String),
}
```

## Best Practices

1. **Validate Early**: Validate modules before deployment
2. **Size Limits**: Set appropriate size limits for modules
3. **Version Checking**: Include version information in metadata
4. **Error Recovery**: Handle module loading failures gracefully
5. **Monitoring**: Log module loading events and errors

## Example: Dynamic Module Loading

```rust
use reflow_script::{ScriptActor, ScriptConfig};
use std::collections::HashMap;

pub struct DynamicModuleLoader {
    modules: HashMap<String, ScriptActor>,
}

impl DynamicModuleLoader {
    pub async fn load_module(&mut self, name: &str, path: &str) -> Result<()> {
        // Read module
        let wasm_bytes = tokio::fs::read(path).await?;
        
        // Validate module
        Self::validate_module(&wasm_bytes).await?;
        
        // Create actor
        let config = ScriptConfig {
            environment: ScriptEnvironment::SYSTEM,
            runtime: ScriptRuntime::Extism,
            source: wasm_bytes,
            entry_point: "process".to_string(),
            packages: None,
        };
        
        let actor = ScriptActor::new(config);
        
        // Store in registry
        self.modules.insert(name.to_string(), actor);
        
        Ok(())
    }
    
    pub fn get_module(&self, name: &str) -> Option<&ScriptActor> {
        self.modules.get(name)
    }
}
```

## See Also

- [WASM Runtime](./wasm-runtime.md) - Overview of the WASM runtime
- [Memory Management](./memory.md) - Memory allocation and limits
- [Security Considerations](../../guides/security.md) - Security best practices