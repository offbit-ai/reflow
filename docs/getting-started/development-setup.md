# Development Setup

This guide helps you set up a development environment for building workflows with Reflow.

## Development Environment

### Recommended Tools

- **IDE**: Visual Studio Code or RustRover
- **Version Control**: Git
- **Package Manager**: Cargo for Rust dependencies
- **Terminal**: Modern terminal with good Unicode support

### VS Code Extensions

For the best development experience with VS Code:

```json
{
  "recommendations": [
    "rust-lang.rust-analyzer",
    "vadimcn.vscode-lldb",
    "serayuzgur.crates",
    "tamasfe.even-better-toml",
    "ms-vscode.vscode-json"
  ]
}
```

## Project Structure

### Creating a New Reflow Project

```bash
# Create a new Rust project
cargo new my-reflow-app
cd my-reflow-app

# Add Reflow dependencies
cargo add reflow_network reflow_script reflow_components
```

### Recommended Project Structure

```
my-reflow-app/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── actors/
│   │   ├── mod.rs
│   │   └── custom_actor.rs
│   ├── workflows/
│   │   ├── mod.rs
│   │   └── data_pipeline.rs
│   └── scripts/
│       ├── process.js
│       └── transform.py
├── config/
│   └── reflow.toml
├── tests/
│   └── integration_tests.rs
└── examples/
    └── basic_workflow.rs
```

### Cargo.toml Configuration

```toml
[package]
name = "my-reflow-app"
version = "0.1.0"
edition = "2021"

[dependencies]
reflow_network = "0.1.0"
reflow_script = { version = "0.1.0", features = ["deno", "python"] }
reflow_components = "0.1.0"
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
anyhow = "1.0"

[dev-dependencies]
tokio-test = "0.4"

[[example]]
name = "basic_workflow"
path = "examples/basic_workflow.rs"
```

## Development Workflow

### 1. Setting Up the Main Application

Create `src/main.rs`:

```rust
use reflow_network::network::Network;
use reflow_script::{ScriptActor, ScriptConfig, ScriptRuntime, ScriptEnvironment};
use tokio;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    // Create a network
    let mut network = Network::new();
    
    // Add actors to the network
    // ... your workflow setup
    
    // Start the network
    network.start().await?;
    
    Ok(())
}
```

### 2. Creating Custom Actors

Create `src/actors/mod.rs`:

```rust
pub mod custom_actor;

pub use custom_actor::CustomActor;
```

Create `src/actors/custom_actor.rs`:

```rust
use reflow_network::actor::{Actor, ActorBehavior, ActorContext, Port};
use reflow_network::message::Message;
use std::collections::HashMap;

pub struct CustomActor {
    inports: Port,
    outports: Port,
}

impl CustomActor {
    pub fn new() -> Self {
        Self {
            inports: flume::unbounded(),
            outports: flume::unbounded(),
        }
    }
}

impl Actor for CustomActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(|context: ActorContext| {
            Box::pin(async move {
                let payload = context.get_payload();
                
                // Your processing logic here
                let result = HashMap::from([
                    ("output".to_string(), Message::String("processed".to_string()))
                ]);
                
                Ok(result)
            })
        })
    }
    
    fn get_inports(&self) -> Port {
        self.inports.clone()
    }
    
    fn get_outports(&self) -> Port {
        self.outports.clone()
    }
    
    fn create_process(&self) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'static + Send>> {
        // Default implementation from trait
        todo!("Implement process creation")
    }
}
```

### 3. Organizing Workflows

Create `src/workflows/mod.rs`:

```rust
pub mod data_pipeline;

pub use data_pipeline::create_data_pipeline;
```

Create `src/workflows/data_pipeline.rs`:

```rust
use reflow_network::network::Network;
use crate::actors::CustomActor;

pub async fn create_data_pipeline() -> Result<Network, Box<dyn std::error::Error>> {
    let mut network = Network::new();
    
    // Create actors
    let source_actor = CustomActor::new();
    let transform_actor = CustomActor::new();
    let sink_actor = CustomActor::new();
    
    // Add actors to network
    network.add_actor("source", Box::new(source_actor)).await?;
    network.add_actor("transform", Box::new(transform_actor)).await?;
    network.add_actor("sink", Box::new(sink_actor)).await?;
    
    // Connect actors
    network.connect("source", "output", "transform", "input").await?;
    network.connect("transform", "output", "sink", "input").await?;
    
    Ok(network)
}
```

## Testing Setup

### Unit Tests

Create `src/actors/custom_actor.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio_test;
    
    #[tokio::test]
    async fn test_custom_actor() {
        let actor = CustomActor::new();
        let behavior = actor.get_behavior();
        
        // Create test context
        let payload = HashMap::from([
            ("input".to_string(), Message::String("test".to_string()))
        ]);
        
        // Test behavior
        // Note: You'll need to create proper ActorContext for testing
        let result = behavior(/* test context */).await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests

Create `tests/integration_tests.rs`:

```rust
use my_reflow_app::workflows::create_data_pipeline;

#[tokio::test]
async fn test_data_pipeline() {
    let network = create_data_pipeline().await.unwrap();
    
    // Test the complete workflow
    // Send test data and verify output
}
```

## Configuration Management

### Environment Configuration

Create `config/reflow.toml`:

```toml
[development]
log_level = "debug"
thread_pool_size = 4

[production]
log_level = "info"
thread_pool_size = 8

[scripting]
deno_permissions = ["--allow-net", "--allow-read"]
python_interpreter = "python3"

[networking]
bind_address = "127.0.0.1:8080"
enable_metrics = true
```

### Loading Configuration

```rust
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    development: Option<EnvConfig>,
    production: Option<EnvConfig>,
    scripting: Option<ScriptingConfig>,
    networking: Option<NetworkingConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct EnvConfig {
    log_level: String,
    thread_pool_size: usize,
}

fn load_config() -> Result<Config, Box<dyn std::error::Error>> {
    let config_str = fs::read_to_string("config/reflow.toml")?;
    let config: Config = toml::from_str(&config_str)?;
    Ok(config)
}
```

## Development Scripts

### Makefile

Create a `Makefile` for common tasks:

```makefile
.PHONY: build test run clean docs

build:
	cargo build

test:
	cargo test

run:
	cargo run

clean:
	cargo clean

docs:
	cargo doc --open

check:
	cargo check
	cargo clippy -- -D warnings
	cargo fmt -- --check

dev:
	cargo watch -x run

install-tools:
	cargo install cargo-watch
	cargo install cargo-expand
```

### Development Commands

```bash
# Development workflow
make build          # Build the project
make test           # Run tests
make check          # Run linting and formatting checks
make dev            # Run with auto-reload on changes

# Documentation
make docs           # Generate and open documentation
cargo doc --document-private-items  # Include private items
```

## Debugging

### Logging Setup

```rust
use tracing::{info, warn, error, debug};
use tracing_subscriber;

// In main.rs
fn init_logging() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();
}

// In your actors
debug!("Processing message: {:?}", message);
info!("Actor started successfully");
warn!("High memory usage detected");
error!("Failed to process message: {}", error);
```

### Debug Configuration

Add to `Cargo.toml`:

```toml
[profile.dev]
debug = true
debug-assertions = true
overflow-checks = true

[dependencies]
tracing = "0.1"
tracing-subscriber = "0.3"
```

### Using Debugger

For VS Code, create `.vscode/launch.json`:

```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug Reflow App",
            "cargo": {
                "args": ["build", "--bin=my-reflow-app"],
                "filter": {
                    "name": "my-reflow-app",
                    "kind": "bin"
                }
            },
            "args": [],
            "cwd": "${workspaceFolder}"
        }
    ]
}
```

## Performance Profiling

### Basic Profiling

```rust
use std::time::Instant;

// Time critical sections
let start = Instant::now();
// ... your code
let duration = start.elapsed();
println!("Time elapsed: {:?}", duration);
```

### Advanced Profiling Tools

```bash
# Install profiling tools
cargo install cargo-profiler
cargo install flamegraph

# Generate flame graphs
cargo flamegraph --bin my-reflow-app

# Memory profiling with valgrind (Linux)
cargo build --release
valgrind --tool=massif ./target/release/my-reflow-app
```

## Continuous Integration

### GitHub Actions

Create `.github/workflows/ci.yml`:

```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
    - uses: actions/checkout@v2
    - uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
    - name: Build
      run: cargo build --verbose
    - name: Run tests
      run: cargo test --verbose
    - name: Check formatting
      run: cargo fmt -- --check
    - name: Run clippy
      run: cargo clippy -- -D warnings
```

## Next Steps

Now that your development environment is set up:

1. **Create your first workflow**: [First Workflow](./first-workflow.md)
2. **Learn about actors**: [Creating Actors](../api/actors/creating-actors.md)
3. **Explore scripting**: [Deno Runtime](../scripting/javascript/deno-runtime.md)
4. **See examples**: [Examples](../examples/README.md)

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Reflow Examples](../examples/README.md)
- [API Reference](../reference/api-reference.md)
