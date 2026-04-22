# Reflow

<div align="center">

**A graph-driven actor runtime for workflows, media pipelines, and visual systems**

[![Build Status](https://img.shields.io/github/actions/workflow/status/offbit-ai/reflow/ci.yml?branch=main)](https://github.com/offbit-ai/reflow/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust Version](https://img.shields.io/badge/rust-1.85+-blue.svg)](https://www.rust-lang.org)
[![WebAssembly](https://img.shields.io/badge/WebAssembly-compatible-green.svg)](https://webassembly.org/)

[Documentation](./docs/README.md) | [Runtime Crate](./crates/reflow_rt/README.md) | [Quick Start](./docs/getting-started/README.md) | [Examples](./examples/) | [API Reference](./docs/reference/api-reference.md)

</div>

## What is Reflow?

Reflow is a modular workflow execution engine that uses the **actor model** for concurrent, message-passing computation. It enables graph-authored DAGs for data processing, real-time media, visual tooling, distributed workflows, and optional ML/CV taskpacks.

### Key Features

🎭 **Actor-Based Architecture** - Isolated, concurrent actors communicate via typed messages  
📊 **Graph-Authored Workflows** - DAG representation with subgraphs, IIPs, history, and undo  
📦 **The Reflow Runtime API** - `reflow_rt` is the public Rust crate for building and running Reflow applications  
🎛️ **Optional Component Families** - GPU, AV, window events, camera, API services, media, and ML are feature gated  
🌍 **Multi-Language Support** - JavaScript, Python, and WebAssembly actor paths  
🌐 **Cross-Platform** - Native Rust execution plus WebAssembly-oriented runtime surfaces  
🔄 **Real-Time Processing** - Networking, WebSockets, streams, and live graph events  
🧠 **Media / ML Ready** - Typed media packets, mockable inference, LiteRT backend boundary, and taskpack subgraphs

## Quick Start

### Use Reflow In Rust

For application code, start with the unified runtime crate:

```toml
[dependencies]
reflow_rt = "0.1"
```

The default `reflow_rt` feature set is intentionally lightweight: runtime plus core utility components. Optional stacks are selected explicitly:

```toml
# Runtime + core component catalog only.
reflow_rt = "0.1"

# Add GPU rendering/compute components.
reflow_rt = { version = "0.1", features = ["gpu"] }

# Add typed media packets and graph-driven ML/CV taskpacks.
reflow_rt = { version = "0.1", features = ["media", "ml"] }
```

```rust
use reflow_rt::prelude::*;

fn main() {
    let mut graph = Graph::new("example", false, None);
    graph.add_node("tap", "tpl_passthrough", None);

    let network = Network::with_graph(NetworkConfig::default(), &graph);
    let _ = network;
}
```

See the [`reflow_rt` README](./crates/reflow_rt/README.md) for the runtime API surface and feature map.

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
- **`reflow_rt`** - The public Rust runtime crate for Reflow applications
- **`reflow_network`** - Core actor runtime and message routing
- **`reflow_components`** - Standard library of reusable actors
- **`reflow_graph`** - Graph data model, exports, nodes, connections, and IIPs
- **`reflow_actor`** - Actor traits, contexts, messages, streams, and state
- **`actor_macro`** - Procedural macros for actor creation

### Media, Graphics & Assets
- **`reflow_assets`** - Content-addressed asset database conventions
- **`reflow_pixel`** - Pixel/image helpers for media components
- **`reflow_sdf`** - SDF primitives and procedural geometry/codegen support
- **`reflow_shader`** - Shader graph IR and WGSL generation
- **`reflow_vector`** - 2D vector paths, booleans, and rasterization
- **`reflow_media_types` / `reflow_media_codec`** - Optional typed media and tensor packet contracts
- **`reflow_litert` / `reflow_ml_ops` / `reflow_cv_ops` / `reflow_taskpacks`** - Optional ML/CV stack and taskpack graph builders

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
git clone https://github.com/offbit-ai/reflow.git
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

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

Built with ❤️ using:
- [Rust](https://rust-lang.org) - Systems programming language
- [Tokio](https://tokio.rs) - Asynchronous runtime  

---

<div align="center">

**⭐ Star us on GitHub if you find Reflow useful! ⭐**

</div>
