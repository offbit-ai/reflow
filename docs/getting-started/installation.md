# Installation

This guide covers installing and setting up Reflow on your system.

## Prerequisites

Before installing Reflow, ensure you have:

### Required
- **Rust** 1.70 or later
- **Git** for cloning the repository

### Optional (for scripting support)
- **Deno** 1.30+ for JavaScript/TypeScript actors
- **Python** 3.8+ for Python actors
- **Docker** for isolated Python execution

## Installation Methods

### Method 1: Install from Crates.io (Recommended)

```bash
cargo install reflow
```

### Method 2: Build from Source

1. **Clone the repository:**
   ```bash
   git clone https://github.com/your-org/reflow.git
   cd reflow
   ```

2. **Build the project:**
   ```bash
   cargo build --release
   ```

3. **Install globally (optional):**
   ```bash
   cargo install --path .
   ```

### Method 3: Use as a Library

Add Reflow to your `Cargo.toml`:

```toml
[dependencies]
reflow_network = "0.1.0"
reflow_script = { version = "0.1.0", features = ["deno"] }
reflow_components = "0.1.0"
```

## Feature Flags

Reflow uses feature flags to control which runtimes are included:

```toml
[dependencies]
reflow_script = { 
    version = "0.1.0", 
    features = [
        "deno",      # JavaScript/TypeScript support via Deno
        "python",    # Python script support
        "extism"     # WebAssembly plugin support
    ] 
}
```

### Available Features

| Feature | Description | Requirements |
|---------|-------------|--------------|
| `deno` | JavaScript/TypeScript runtime | Deno installed |
| `python` | Python script execution | Python 3.8+ |
| `extism` | WebAssembly plugin support | None |
| `flowtrace` | Debug tracing support | None |

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
# If installed globally
reflow --version

# If built from source
./target/release/reflow --version

# Test basic functionality
reflow test-actors
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
