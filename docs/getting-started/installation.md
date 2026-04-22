# Installation

This guide covers installing and setting up Reflow on your system.

## Prerequisites

Before installing Reflow, ensure you have:

### Required
- **Rust** 1.85 or later
- **Git** for cloning the repository

### Optional (for scripting support)
- **Deno** 1.30+ for JavaScript/TypeScript actors
- **Python** 3.8+ for Python actors
- **Docker** for isolated Python execution

## Installation Methods

### Method 1: Use Reflow as a Rust Library

For application code, depend on the unified runtime crate:

```toml
[dependencies]
reflow_rt = "0.1"
```

### Method 2: Build from Source

1. **Clone the repository:**
   ```bash
   git clone https://github.com/offbit-ai/reflow.git
   cd reflow
   ```

2. **Build the project:**
   ```bash
   cargo build --release
   ```

3. **Run examples or package crates locally:**
   ```bash
   cargo test -p reflow_rt
   cargo package -p reflow_rt --list
   ```

### Method 3: Use Lower-Level Crates Directly

`reflow_rt` is the recommended user-facing entry point. Lower-level crates remain available when a project needs a narrower dependency surface:

```toml
[dependencies]
reflow_graph = "0.1"
reflow_actor = "0.1"
reflow_network = "0.1"
reflow_components = { version = "0.1", default-features = false }
```

## Feature Flags

`reflow_rt` keeps optional component families out of the default install path:

```toml
[dependencies]
reflow_rt = { version = "0.1", features = ["gpu", "media", "ml"] }
```

### Available Features

| Feature | Description | Requirements |
|---------|-------------|--------------|
| `gpu` | GPU-backed rendering and compute components | Native GPU backend |
| `av-core` | Audio/signal processing components | None |
| `window-events` | Window/input event components | None |
| `camera-native` | Native camera capture | Platform camera backend |
| `media` | Typed frame/tensor packet crates | None |
| `ml` | CV/ML actors, model manifests, taskpacks, and mock inference | None |
| `external-litert` | Real LiteRT adapter support | LiteRT native runtime |
| `api-services` | Generated API-service actors | Larger compile surface |
| `network-flowtrace` | Debug tracing support in `reflow_network` | None |

## Runtime Dependencies

### JavaScript/TypeScript (Deno)

Install Deno:

```bash
# macOS/Linux
curl -fsSL https://deno.land/x/install/install.sh | sh

# Windows (PowerShell)
iwr https://deno.land/x/install/install.ps1 -useb | iex

# Using package managers
brew install deno          # macOS
scoop install deno         # Windows
snap install deno          # Linux
```

### Python Support

Install Python 3.8+:

```bash
# macOS
brew install python

# Ubuntu/Debian
sudo apt update
sudo apt install python3 python3-pip

# Windows
# Download from https://python.org
```

For Docker-based Python execution:

```bash
# Install Docker
# macOS/Windows: Docker Desktop
# Linux: docker.io package
sudo apt install docker.io  # Ubuntu/Debian
```

## Verification

Verify your installation:

```bash
# Check the user-facing runtime crate
cargo check -p reflow_rt

# Verify the crate package contents
cargo package -p reflow_rt --list
```

## Platform-Specific Notes

### macOS

- Use Homebrew for easy dependency management
- Xcode Command Line Tools required for Rust compilation

### Linux

- Ensure `build-essential` is installed
- Some distributions may need `pkg-config` and `libssl-dev`

```bash
# Ubuntu/Debian
sudo apt install build-essential pkg-config libssl-dev

# CentOS/RHEL
sudo yum groupinstall "Development Tools"
sudo yum install openssl-devel
```

### Windows

- Use Windows Subsystem for Linux (WSL) for best experience
- Visual Studio Build Tools required for Rust compilation
- Consider using `scoop` or `chocolatey` for dependency management

## Configuration

### Environment Variables

Set these environment variables for optimal performance:

```bash
# Enable shared Python environment (optional)
export USE_SHARED_ENV=true

# Set Python path (if needed)
export PYTHON_PATH=/usr/bin/python3

# Configure Deno permissions (optional)
export DENO_PERMISSIONS="--allow-all"
```

### Config File

Create a `reflow.toml` configuration file:

```toml
[runtime]
default_engine = "deno"
enable_networking = true
enable_filesystem = true

[deno]
allow_all = false
allow_net = true
allow_read = true

[python]
use_docker = false
shared_environment = true

[performance]
thread_pool_size = 8
max_memory_mb = 1024
```

## Next Steps

Now that Reflow is installed:

1. **Learn the basics**: Read [Basic Concepts](./basic-concepts.md)
2. **Set up development**: Follow [Development Setup](./development-setup.md)
3. **Create your first workflow**: Try [First Workflow](./first-workflow.md)
4. **Explore examples**: Check out the [Examples](../examples/README.md)

## Troubleshooting

### Common Issues

**Rust compilation errors:**
```bash
# Update Rust to latest version
rustup update
```

**Deno not found:**
```bash
# Add Deno to PATH
export PATH="$HOME/.deno/bin:$PATH"
```

**Python import errors:**
```bash
# Install required Python packages
pip install numpy pandas  # or other dependencies
```

**Permission denied errors:**
```bash
# Fix file permissions
chmod +x reflow
```

For more troubleshooting, see the [Troubleshooting Guide](../reference/troubleshooting-guide.md).
