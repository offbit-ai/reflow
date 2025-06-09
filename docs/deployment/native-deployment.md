# Native Deployment

This guide covers deploying Reflow workflows as native applications on various platforms.

## Overview

Native deployment provides:
- **Maximum performance** - Direct OS integration
- **Resource efficiency** - No containerization overhead
- **Platform integration** - Native system services
- **Debugging capabilities** - Full toolchain access

## Deployment Options

### Standalone Binary

Compile workflows into self-contained executables:

```bash
# Build optimized release binary
cargo build --release

# Binary includes all dependencies
./target/release/my-workflow

# Cross-compilation for different targets
cargo build --release --target x86_64-pc-windows-gnu
cargo build --release --target aarch64-apple-darwin
```

### System Service

Deploy as a system service for automatic startup:

#### Linux (systemd)

Create `/etc/systemd/system/reflow-workflow.service`:

```ini
[Unit]
Description=Reflow Workflow Service
After=network.target
Wants=network.target

[Service]
Type=exec
User=reflow
Group=reflow
ExecStart=/opt/reflow/bin/my-workflow
ExecReload=/bin/kill -HUP $MAINPID
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

# Environment variables
Environment=RUST_LOG=info
Environment=REFLOW_CONFIG=/etc/reflow/config.toml

# Security settings
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/reflow /var/log/reflow

[Install]
WantedBy=multi-user.target
```

Enable and start the service:

```bash
sudo systemctl enable reflow-workflow
sudo systemctl start reflow-workflow
sudo systemctl status reflow-workflow
```

#### macOS (launchd)

Create `/Library/LaunchDaemons/com.yourcompany.reflow.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.yourcompany.reflow</string>
    
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/my-workflow</string>
    </array>
    
    <key>RunAtLoad</key>
    <true/>
    
    <key>KeepAlive</key>
    <true/>
    
    <key>StandardOutPath</key>
    <string>/usr/local/var/log/reflow.log</string>
    
    <key>StandardErrorPath</key>
    <string>/usr/local/var/log/reflow.error.log</string>
    
    <key>EnvironmentVariables</key>
    <dict>
        <key>RUST_LOG</key>
        <string>info</string>
    </dict>
</dict>
</plist>
```

Load the service:

```bash
sudo launchctl load /Library/LaunchDaemons/com.yourcompany.reflow.plist
```

#### Windows Service

Using the `windows-service` crate:

```rust
// Cargo.toml
[dependencies]
windows-service = "0.6"

// src/main.rs
use windows_service::{
    define_windows_service,
    service::{
        ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType,
        ServiceState, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher, Result,
};

define_windows_service!(ffi_service_main, my_service_main);

fn my_service_main(arguments: Vec<std::ffi::OsString>) {
    if let Err(_e) = run_service(arguments) {
        // Handle error
    }
}

fn run_service(_arguments: Vec<std::ffi::OsString>) -> Result<()> {
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                // Stop the workflow
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register("reflow", event_handler)?;

    // Start workflow
    start_workflow();

    Ok(())
}
```

## Configuration Management

### Configuration Files

Create hierarchical configuration:

```toml
# /etc/reflow/config.toml (system-wide)
[runtime]
thread_pool_size = 8
max_memory_mb = 1024

[logging]
level = "info"
output = "/var/log/reflow/app.log"

[network]
bind_address = "0.0.0.0:8080"
```

```toml
# ~/.config/reflow/config.toml (user-specific)
[runtime]
thread_pool_size = 4  # Override system setting

[development]
hot_reload = true
debug_mode = true
```

### Environment Variables

Support environment variable overrides:

```rust
use config::{Config, Environment, File};

fn load_config() -> Result<AppConfig, config::ConfigError> {
    let mut settings = Config::builder()
        // Start with default values
        .add_source(File::with_name("config/default"))
        // Add environment-specific config
        .add_source(File::with_name(&format!("config/{}", env)).required(false))
        // Add local config
        .add_source(File::with_name("config/local").required(false))
        // Add environment variables with REFLOW_ prefix
        .add_source(Environment::with_prefix("REFLOW"))
        .build()?;

    settings.try_deserialize()
}

// Environment variables:
// REFLOW_RUNTIME__THREAD_POOL_SIZE=16
// REFLOW_LOGGING__LEVEL=debug
// REFLOW_NETWORK__BIND_ADDRESS=127.0.0.1:9090
```

## Resource Management

### Memory Configuration

```rust
use tikv_jemallocator::Jemalloc;

#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

fn configure_memory() {
    // Set memory limits
    std::env::set_var("MALLOC_CONF", "lg_dirty_mult:8,lg_muzzy_mult:8");
    
    // Configure actor memory limits
    let config = ActorSystemConfig {
        max_actors: 10000,
        max_memory_per_actor: 100 * 1024 * 1024, // 100MB
        gc_threshold: 0.8,
    };
}
```

### File Descriptors

```bash
# Increase file descriptor limits
echo "reflow soft nofile 65536" >> /etc/security/limits.conf
echo "reflow hard nofile 65536" >> /etc/security/limits.conf

# For systemd services
echo "LimitNOFILE=65536" >> /etc/systemd/system/reflow-workflow.service
```

### CPU Affinity

Pin actors to specific CPU cores:

```rust
use core_affinity;

fn configure_cpu_affinity() {
    let core_ids = core_affinity::get_core_ids().unwrap();
    
    // Pin high-priority actors to specific cores
    for (i, actor) in high_priority_actors.iter().enumerate() {
        let core_id = core_ids[i % core_ids.len()];
        
        tokio::spawn(async move {
            core_affinity::set_for_current(core_id);
            actor.run().await;
        });
    }
}
```

## Monitoring and Observability

### Logging Configuration

```rust
use tracing::{info, warn, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

fn setup_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "reflow=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_appender::rolling::daily("/var/log/reflow", "app.log")
        )
        .init();
}
```

### Metrics Integration

```rust
use prometheus::{Encoder, TextEncoder, register_counter, register_histogram};

lazy_static! {
    static ref MESSAGES_PROCESSED: prometheus::Counter = register_counter!(
        "reflow_messages_processed_total",
        "Total number of messages processed"
    ).unwrap();
    
    static ref MESSAGE_PROCESSING_TIME: prometheus::Histogram = register_histogram!(
        "reflow_message_processing_seconds",
        "Time spent processing messages"
    ).unwrap();
}

// Expose metrics endpoint
async fn metrics_handler() -> impl warp::Reply {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    
    warp::reply::with_header(buffer, "content-type", "text/plain")
}
```

### Health Checks

```rust
use warp::Filter;

#[derive(Serialize)]
struct HealthStatus {
    status: String,
    actors: usize,
    uptime: u64,
    memory_usage: u64,
}

async fn health_check() -> Result<impl warp::Reply, warp::Rejection> {
    let status = HealthStatus {
        status: "healthy".to_string(),
        actors: get_active_actor_count(),
        uptime: get_uptime_seconds(),
        memory_usage: get_memory_usage(),
    };
    
    Ok(warp::reply::json(&status))
}

let health = warp::path("health")
    .and(warp::get())
    .and_then(health_check);
```

## Security Considerations

### User Permissions

Run with minimal privileges:

```bash
# Create dedicated user
sudo useradd -r -s /bin/false reflow
sudo mkdir -p /var/lib/reflow /var/log/reflow
sudo chown reflow:reflow /var/lib/reflow /var/log/reflow
```

### File System Sandboxing

```rust
use std::os::unix::fs::PermissionsExt;

fn setup_sandbox() -> Result<(), Box<dyn std::error::Error>> {
    // Create chroot environment
    let sandbox_dir = "/var/lib/reflow/sandbox";
    std::fs::create_dir_all(sandbox_dir)?;
    
    // Set restrictive permissions
    let mut perms = std::fs::metadata(sandbox_dir)?.permissions();
    perms.set_mode(0o700);
    std::fs::set_permissions(sandbox_dir, perms)?;
    
    // Change root directory (requires root privileges)
    // unsafe { libc::chroot(sandbox_dir.as_ptr()) };
    
    Ok(())
}
```

### Network Security

```rust
use tokio::net::TcpListener;
use rustls::{Certificate, PrivateKey, ServerConfig};

async fn start_secure_server() -> Result<(), Box<dyn std::error::Error>> {
    // Load TLS certificates
    let certs = load_certs("cert.pem")?;
    let key = load_private_key("key.pem")?;
    
    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
    let listener = TcpListener::bind("0.0.0.0:8443").await?;
    
    while let Ok((stream, _)) = listener.accept().await {
        let acceptor = acceptor.clone();
        
        tokio::spawn(async move {
            if let Ok(tls_stream) = acceptor.accept(stream).await {
                handle_connection(tls_stream).await;
            }
        });
    }
    
    Ok(())
}
```

## Performance Optimization

### Profile-Guided Optimization

```bash
# Build with instrumentation
RUSTFLAGS="-Cprofile-generate=/tmp/pgo-data" \
    cargo build --release

# Run with representative workload
./target/release/my-workflow --benchmark

# Rebuild with optimization data
RUSTFLAGS="-Cprofile-use=/tmp/pgo-data" \
    cargo build --release
```

### Link-Time Optimization

```toml
# Cargo.toml
[profile.release]
lto = true
codegen-units = 1
panic = "abort"
```

### Memory Pool Configuration

```rust
use object_pool::Pool;

lazy_static! {
    static ref MESSAGE_POOL: Pool<HashMap<String, Message>> = Pool::new(1000, || {
        HashMap::with_capacity(16)
    });
}

fn get_message_buffer() -> object_pool::Reusable<HashMap<String, Message>> {
    MESSAGE_POOL.try_pull().unwrap_or_else(|| {
        MESSAGE_POOL.attach(HashMap::with_capacity(16))
    })
}
```

## Deployment Scripts

### Automated Deployment

```bash
#!/bin/bash
# deploy.sh

set -e

APP_NAME="reflow-workflow"
SERVICE_USER="reflow"
INSTALL_DIR="/opt/reflow"
CONFIG_DIR="/etc/reflow"
LOG_DIR="/var/log/reflow"

echo "Deploying $APP_NAME..."

# Stop existing service
sudo systemctl stop $APP_NAME || true

# Create directories
sudo mkdir -p $INSTALL_DIR/bin $CONFIG_DIR $LOG_DIR
sudo chown $SERVICE_USER:$SERVICE_USER $LOG_DIR

# Copy binary
sudo cp target/release/$APP_NAME $INSTALL_DIR/bin/
sudo chmod +x $INSTALL_DIR/bin/$APP_NAME

# Copy configuration
sudo cp config/production.toml $CONFIG_DIR/config.toml
sudo chown root:$SERVICE_USER $CONFIG_DIR/config.toml
sudo chmod 640 $CONFIG_DIR/config.toml

# Install service file
sudo cp scripts/$APP_NAME.service /etc/systemd/system/
sudo systemctl daemon-reload

# Start service
sudo systemctl enable $APP_NAME
sudo systemctl start $APP_NAME

echo "Deployment complete. Checking status..."
sudo systemctl status $APP_NAME
```

### Rollback Script

```bash
#!/bin/bash
# rollback.sh

APP_NAME="reflow-workflow"
BACKUP_DIR="/opt/reflow/backups"

echo "Rolling back $APP_NAME..."

# Stop current service
sudo systemctl stop $APP_NAME

# Restore previous version
LATEST_BACKUP=$(ls -t $BACKUP_DIR/*.tar.gz | head -n1)
sudo tar -xzf $LATEST_BACKUP -C /opt/reflow/

# Restart service
sudo systemctl start $APP_NAME
sudo systemctl status $APP_NAME

echo "Rollback complete."
```

## Troubleshooting

### Common Issues

**High Memory Usage:**
```bash
# Check memory allocation
echo "Memory usage by process:"
ps aux --sort=-%mem | grep reflow

# Monitor real-time usage
top -p $(pgrep reflow)

# Check for memory leaks
valgrind --tool=memcheck --leak-check=full ./my-workflow
```

**Performance Issues:**
```bash
# Profile CPU usage
perf record -g ./my-workflow
perf report

# Check system resources
iostat -x 1
vmstat 1
```

**File Descriptor Limits:**
```bash
# Check current limits
ulimit -n

# Check process usage
lsof -p $(pgrep reflow) | wc -l

# Monitor file descriptor usage
watch -n 1 'ls /proc/$(pgrep reflow)/fd | wc -l'
```

### Log Analysis

```bash
# Real-time log monitoring
tail -f /var/log/reflow/app.log

# Search for errors
grep -i error /var/log/reflow/app.log

# Analyze performance patterns
awk '/processing_time/ {sum += $3; count++} END {print "Average:", sum/count}' app.log
```

## Best Practices

### Deployment Checklist

- [ ] Resource limits configured
- [ ] Security permissions set
- [ ] Monitoring enabled
- [ ] Health checks implemented
- [ ] Backup strategy defined
- [ ] Rollback procedure tested
- [ ] Documentation updated

### Production Readiness

1. **Load Testing** - Validate performance under expected load
2. **Failure Testing** - Test recovery from various failure scenarios
3. **Security Audit** - Review permissions and access controls
4. **Monitoring Setup** - Ensure comprehensive observability
5. **Backup Verification** - Test backup and restore procedures

## Next Steps

- [Container Deployment](./container-deployment.md) - Docker and Kubernetes
- [Cloud Deployment](./cloud-deployment.md) - AWS, GCP, Azure
- [Monitoring Setup](./monitoring-setup.md) - Comprehensive observability
- [Security Hardening](./security-hardening.md) - Production security
