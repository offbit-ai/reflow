# Getting Started with Reflow

Welcome to Reflow! This guide will help you get up and running with the actor-based workflow engine.

## What You'll Learn

This getting started guide covers:

1. **[Installation](./installation.md)** - Setting up Reflow on your system
2. **[Basic Concepts](./basic-concepts.md)** - Understanding actors, messages, and workflows
3. **[Development Setup](./development-setup.md)** - Setting up your development environment
4. **[First Workflow](./first-workflow.md)** - Creating your first workflow

## Quick Overview

Reflow is an actor-based workflow engine that allows you to:

- Create **actors** that process data and communicate via messages
- Connect actors into **workflows** that define data flow and processing logic
- Execute workflows with **multi-language support** (JavaScript/Deno, Python, WASM)
- Deploy workflows **natively** or in **WebAssembly** environments

## Prerequisites

Before getting started with Reflow, you should have:

- **Rust** (1.70 or later) - for building and running Reflow
- **Basic understanding** of concurrent programming concepts
- **Familiarity** with at least one of: JavaScript, Python, or Rust

## Architecture at a Glance

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Actor A   │───▶│   Actor B   │───▶│   Actor C   │
│ (JavaScript)│    │  (Python)   │    │    (Rust)   │
└─────────────┘    └─────────────┘    └─────────────┘
       │                  │                  │
       ▼                  ▼                  ▼
┌─────────────────────────────────────────────────────┐
│              Message Bus & Routing                  │
└─────────────────────────────────────────────────────┘
```

## Key Concepts

- **Actor**: An isolated unit of computation that processes messages
- **Message**: Data passed between actors
- **Port**: Input/output connections on actors
- **Workflow**: A graph of connected actors
- **Runtime**: The execution environment (Deno, Python, etc.)

## Next Steps

1. Start with **[Installation](./installation.md)** to set up Reflow
2. Read **[Basic Concepts](./basic-concepts.md)** to understand the fundamentals
3. Follow the **[First Workflow](./first-workflow.md)** tutorial
4. Explore the **[Examples](../examples/README.md)** for more complex use cases

## Getting Help

- Check the **[Troubleshooting Guide](../reference/troubleshooting-guide.md)**
- Browse the **[API Reference](../reference/api-reference.md)**
- Look at **[Examples](../examples/README.md)** for working code
- Open an issue on GitHub for bugs or feature requests

Ready to start? Let's **[install Reflow](./installation.md)**!
