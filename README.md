# Reflow

<div align="center">

**A powerful, actor-based workflow execution engine built in Rust**

[![Build Status](https://img.shields.io/github/workflow/status/reflow-project/reflow/CI)](https://github.com/reflow-project/reflow/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Version](https://img.shields.io/badge/rust-1.70+-blue.svg)](https://www.rust-lang.org)
[![WebAssembly](https://img.shields.io/badge/WebAssembly-compatible-green.svg)](https://webassembly.org/)

[📖 Documentation](./docs/README.md) | [🚀 Quick Start](./docs/getting-started/README.md) | [💡 Examples](./examples/) | [🔧 API Reference](./docs/reference/api-reference.md)

</div>

## What is Reflow?

Reflow is a modular, high-performance workflow execution engine that uses the **actor model** for concurrent, message-passing computation. It enables you to build complex data processing pipelines, real-time systems, and distributed workflows with **multi-language scripting support** and **cross-platform deployment**.

### Key Features

🎭 **Actor-Based Architecture** - Isolated, concurrent actors communicate via message passing  
🌍 **Multi-Language Support** - JavaScript (Deno), Python, and WebAssembly runtimes  
📊 **Visual Workflows** - Graph-based workflow representation with history and undo  
⚡ **High Performance** - Rust-powered execution with zero-copy optimizations  
🌐 **Cross-Platform** - Native execution + WebAssembly for browsers  
🔄 **Real-Time Processing** - Built-in networking, WebSockets, and live data streams  
📦 **Extensible** - Rich component library + custom component creation  
🛠️ **Developer Friendly** - Hot reloading, debugging tools, and comprehensive APIs

## Quick Start

### Installation

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Clone and build Reflow
git clone https://github.com/reflow-project/reflow.git
cd reflow
cargo build --release
```

### Your First Workflow

```rust
use reflow_network::{Graph, Network};
use reflow_components::prelude::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a new graph
    let mut graph = Graph::new("MyWorkflow", true, HashMap::new());
    
    // Add actors to the graph
    graph.add_node("source", "DataSource", json!({
        "data": [1, 2, 3, 4, 5]
    }));
    
    graph.add_node("processor", "MapActor", json!({
        "function": "x => x * 2"
    }));
    
    graph.add_node("sink", "Logger", json!({}));
    
    // Connect the actors
    graph.add_connection("source", "output", "processor", "input", json!({}));
    graph.add_connection("processor", "output", "sink", "input", json!({}));
    
    // Execute the workflow
    let network = Network::from_graph(graph).await?;
    network.execute().await?;
    
    Ok(())
}
```

**Output:**
```
[INFO] sink: 2
[INFO] sink: 4
[INFO] sink: 6
[INFO] sink: 8
[INFO] sink: 10
```

## Architecture Overview

Reflow's architecture is built around three core concepts:

```
┌─────────────────┐    Messages    ┌─────────────────┐    Messages    ┌─────────────────┐
│     Actor A     │────────────────▶│     Actor B     │────────────────▶│     Actor C     │
│  (JavaScript)   │                 │    (Python)     │                 │     (Rust)      │
│                 │                 │                 │                 │                 │
│ ┌─────────────┐ │                 │ ┌─────────────┐ │                 │ ┌─────────────┐ │
│ │Input Ports  │ │                 │ │Input Ports  │ │                 │ │Input Ports  │ │
│ │Output Ports │ │                 │ │Output Ports │ │                 │ │Output Ports │ │
│ └─────────────┘ │                 │ └─────────────┘ │                 │ └─────────────┘ │
└─────────────────┘                 └─────────────────┘                 └─────────────────┘
```

- **Actors**: Isolated units of computation that process messages
- **Messages**: Strongly-typed data passed between actors  
- **Graphs**: Visual representation of actor connections and data flow

## Project Structure

This workspace contains multiple crates that work together:

### Core Engine
- **`reflow_network`** - Core actor runtime and message routing
- **`reflow_components`** - Standard library of reusable actors
- **`actor_macro`** - Procedural macros for actor creation

### Language Runtimes  
- **`reflow_js`** - JavaScript/Deno runtime integration
- **`reflow_py`** - Python runtime integration  
- **`reflow_wasm`** - WebAssembly runtime and browser support

### Execution & Deployment
- **`reflow_script`** - Multi-language script execution
- **`reflow_server`** - HTTP server and API endpoints

### Examples & Tools
- **`examples/`** - Working examples and tutorials
- **`docs/`** - Comprehensive documentation

## Use Cases

### Data Processing Pipelines
```rust
// ETL pipeline with error handling
source → validate → transform → load → audit
```

### Real-Time Analytics  
```rust
// Live data processing
websocket → parse → aggregate → alert → dashboard
```

### IoT Data Processing
```rust
// Sensor data workflow  
mqtt → decode → filter → analyze → store → notify
```

### Media Processing
```rust
// Audio/video pipeline
upload → decode → process → encode → publish
```

## Documentation

📖 **[Complete Documentation](./docs/README.md)**

### Getting Started
- [Installation Guide](./docs/getting-started/installation.md)
- [Basic Concepts](./docs/getting-started/basic-concepts.md)  
- [First Workflow Tutorial](./docs/getting-started/first-workflow.md)

### Architecture & Design
- [System Architecture](./docs/architecture/overview.md)
- [Actor Model](./docs/architecture/actor-model.md)
- [Message Passing](./docs/architecture/message-passing.md)

### API Documentation
- [Creating Actors](./docs/api/actors/creating-actors.md)
- [Graph Management](./docs/api/graph/creating-graphs.md)
- [Component Library](./docs/components/standard-library.md)

### Advanced Topics
- [Building a Visual Editor](./docs/tutorials/building-visual-editor.md)
- [Performance Optimization](./docs/tutorials/performance-optimization.md)
- [Deployment Guide](./docs/deployment/native-deployment.md)

### Reference
- [API Reference](./docs/reference/api-reference.md)
- [Troubleshooting Guide](./docs/reference/troubleshooting-guide.md)

## Examples

Explore working examples in the [`examples/`](./examples/) directory:


### WebAssembly Actor
Demonstrates how to create and deploy actors as WebAssembly modules.

## Development

### Building from Source

```bash
# Clone the repository
git clone https://github.com/reflow-project/reflow.git
cd reflow

# Build all crates
cargo build

# Run tests
cargo test

# Build with optimizations
cargo build --release
```

### WebAssembly Support

```bash
# Install wasm-pack
curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh

# Build WebAssembly package
cd crates/reflow_network
wasm-pack build --target web
```

### Development Tools

```bash
# Install development dependencies
cargo install cargo-watch
cargo install flamegraph

# Run with hot reloading
cargo watch -x run

# Performance profiling
cargo flamegraph --bin reflow-example
```

## Performance

Reflow is designed for high-performance execution:

- **Zero-copy message passing** where possible
- **Parallel actor execution** with work-stealing schedulers  
- **Memory pooling** for frequently allocated objects
- **SIMD optimizations** for numeric processing
- **Async I/O** throughout the system

Benchmark results on modern hardware:
- **1M+ messages/second** processing throughput
- **Sub-millisecond** actor-to-actor latency
- **Linear scaling** with CPU core count

## Contributing

We welcome contributions! Please see our [Contributing Guide](./CONTRIBUTING.md) for details.

### Areas We Need Help With

- 🐛 **Bug Reports** - Found an issue? Let us know!
- 📝 **Documentation** - Help improve our guides and examples
- 🔌 **Components** - Build reusable actors for the community
- 🌐 **Language Bindings** - Add support for more runtimes
- ⚡ **Performance** - Optimization opportunities
- 🧪 **Testing** - Expand our test coverage

### Development Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes and add tests
4. Ensure all tests pass (`cargo test`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## Community

- **GitHub Discussions** - Ask questions and share ideas
<!-- - **Discord** - Real-time chat with the community   -->
<!-- - **Twitter** - Follow [@ReflowEngine](https://twitter.com/ReflowEngine) for updates -->

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

Built with ❤️ using:
- [Rust](https://rust-lang.org) - Systems programming language
- [Tokio](https://tokio.rs) - Asynchronous runtime  
- [Deno](https://deno.land) - JavaScript/TypeScript runtime
- [WebAssembly](https://webassembly.org) - Portable binary format

---

<div align="center">

**⭐ Star us on GitHub if you find Reflow useful! ⭐**

</div>
