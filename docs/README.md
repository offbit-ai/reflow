# Reflow Documentation

Welcome to the Reflow documentation! Reflow is a powerful, actor-based workflow execution engine built in Rust that supports multi-language scripting and cross-platform deployment.

## What is Reflow?

Reflow is a modular workflow engine that uses the actor model for concurrent, message-passing execution. It supports:

- **Zeal IDE Integration**: Real-time event streaming to Zeal via ZIP protocol (WebSocket + HTTP traces)
- **6,700+ API Actors**: Pre-generated actors for 88 API services (Slack, GitHub, Stripe, etc.)
- **Actor-Based Architecture**: Isolated, concurrent actors with message passing
- **Graph-Based Workflows**: Visual workflow representation with history/undo
- **Real-Time Observability**: EventBridge pipeline forwarding execution events to TraceCollector and ZipSession
- **Media Processing**: Image, audio, video, and optional graph-driven ML pipelines
- **REST API + WebSocket**: HTTP and WebSocket interfaces for headless workflow execution
- **Cross-Platform**: Native Rust execution + WebAssembly for browsers

## Documentation Structure

### [Getting Started](./getting-started/README.md)
Quick start guide, installation, and basic concepts

### [Architecture](./architecture/overview.md)
System architecture, actor model, execution engine, and event pipeline

### [Zeal Integration](./integration/zeal-ide.md)
ZIP session, template registration, real-time event streaming to Zeal IDE

### [REST API](./integration/rest-api.md)
HTTP and WebSocket API for direct workflow execution

### [Core API](./api/actors/creating-actors.md)
Detailed API documentation for actors, messaging, and graphs

### [Components](./components/standard-library.md)
Standard component library: flow control, transforms, logic, media, optional ML, and 6,700+ API actors

### [Observability](./observability/architecture.md)
EventBridge, TraceCollector, ZIP event translation, and trace sessions

### [Deployment](./deployment/native-deployment.md)
Deployment options and operational considerations

### [Reference](./reference/api-reference.md)
Complete API reference and configuration options

## Quick Links

- [Installation Guide](./getting-started/installation.md)
- [First Workflow Tutorial](./getting-started/first-workflow.md)
- [Component Library](./components/standard-library.md)
- [API Service Actors](./components/api-actors.md)
- [Zeal IDE Integration](./integration/zeal-ide.md)
- [REST API Reference](./integration/rest-api.md)

## Community and Support

- **GitHub Issues**: Report bugs and request features
- **Discussions**: Community Q&A and announcements
- **Contributing**: See CONTRIBUTING.md for development guidelines

## License

This project is licensed under the MIT License - see the LICENSE file for details.
