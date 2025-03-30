# Reflow JS Runtime

A lightweight JavaScript runtime built on top of the Boa JavaScript engine. This runtime is designed to be modular, extensible, memory-safe, thread-safe, and secure while supporting modern JavaScript features.

## Features

- **Core Runtime**: JavaScript execution environment with proper error handling, context management, and support for modern JavaScript syntax.
- **Extension System**: Plugin/extension architecture that allows loading and unloading extensions at runtime.
- **Console Module**: Support for standard console methods (log, info, warn, error, debug).
- **Async/Await Support**: Full Promise implementation compatible with ES standards, native async/await syntax support, and microtask queue implementation.
- **Networking**: Fetch API and WebSocket client implementation.
- **Timers**: setTimeout, clearTimeout, setInterval, and clearInterval functions.
- **Module System**: Support for ES Modules (import/export) and CommonJS modules (require/exports).
- **File System Access**: Asynchronous file operations with permission-based access control.
- **Security Model**: Fine-grained permission system for controlled access to sensitive APIs.

## Usage

### Basic Usage

```rust
use reflow_js::{JsRuntime, RuntimeConfig};

fn main() {
    // Create a runtime configuration
    let config = RuntimeConfig::default();
    
    // Create a new runtime
    let runtime = JsRuntime::new(config);
    
    // Evaluate JavaScript code
    let result = runtime.eval("1 + 2");
    println!("Result: {:?}", result);
}
```

### Using Extensions

```rust
use reflow_js::{JsRuntime, RuntimeConfig, ConsoleModule, TimerModule};
use tokio;

#[tokio::main]
async fn main() {
    // Create a runtime configuration
    let config = RuntimeConfig {
        enable_console: true,
        enable_timers: true,
        ..Default::default()
    };
    
    // Create a new runtime
    let mut runtime = JsRuntime::new(config);
    
    // Load the console module
    let console = ConsoleModule::new();
    runtime.load_extension(Box::new(console)).await.unwrap();
    
    // Load the timer module
    let timers = TimerModule::new();
    runtime.load_extension(Box::new(timers)).await.unwrap();
    
    // Evaluate JavaScript code
    let code = r#"
        console.log("Hello, world!");
        
        setTimeout(() => {
            console.log("This message is logged after 1 second");
        }, 1000);
    "#;
    
    runtime.eval(code).unwrap();
    
    // Wait for the timer to fire
    tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
}
```

### Using the Module System

```rust
use reflow_js::{JsRuntime, RuntimeConfig, ModuleSystem};
use std::path::PathBuf;
use tokio;

#[tokio::main]
async fn main() {
    // Create a runtime configuration
    let config = RuntimeConfig {
        enable_modules: true,
        ..Default::default()
    };
    
    // Create a new runtime
    let mut runtime = JsRuntime::new(config);
    
    // Load the module system
    let module_system = ModuleSystem::new(PathBuf::from("."));
    runtime.load_extension(Box::new(module_system)).await.unwrap();
    
    // Evaluate JavaScript code
    let code = r#"
        const math = require('./math');
        console.log(math.add(1, 2));
    "#;
    
    runtime.eval(code).unwrap();
}
```

### Using the File System

```rust
use reflow_js::{JsRuntime, RuntimeConfig, FileSystemModule, FileSystemPermissions};
use std::path::PathBuf;
use tokio;

#[tokio::main]
async fn main() {
    // Create a runtime configuration
    let config = RuntimeConfig {
        enable_filesystem: true,
        ..Default::default()
    };
    
    // Create a new runtime
    let mut runtime = JsRuntime::new(config);
    
    // Create file system permissions
    let mut fs_permissions = FileSystemPermissions::default();
    fs_permissions.allow_read = true;
    fs_permissions.allow_write = true;
    fs_permissions.allowed_paths = vec![PathBuf::from(".")];
    
    // Load the file system module
    let fs = FileSystemModule::new(fs_permissions, false);
    runtime.load_extension(Box::new(fs)).await.unwrap();
    
    // Evaluate JavaScript code
    let code = r#"
        const fs = require('fs');
        fs.writeFileSync('hello.txt', 'Hello, world!');
        const content = fs.readFileSync('hello.txt', 'utf8');
        console.log(content);
    "#;
    
    runtime.eval(code).unwrap();
}
```

### Using the Fetch API

```rust
use reflow_js::{JsRuntime, RuntimeConfig, FetchModule, NetworkConfig};
use tokio;

#[tokio::main]
async fn main() {
    // Create a runtime configuration
    let config = RuntimeConfig {
        enable_network: true,
        ..Default::default()
    };
    
    // Create a new runtime
    let mut runtime = JsRuntime::new(config);
    
    // Load the fetch module
    let network_config = NetworkConfig::default();
    let fetch = FetchModule::new(network_config);
    runtime.load_extension(Box::new(fetch)).await.unwrap();
    
    // Evaluate JavaScript code
    let code = r#"
        async function fetchData() {
            const response = await fetch('https://api.example.com/data');
            const data = await response.json();
            console.log(data);
        }
        
        fetchData();
    "#;
    
    runtime.eval_async(code).await.unwrap();
}
```

## Security

The runtime includes a permission-based security model that allows you to control access to sensitive APIs. You can configure permissions for file system access, network access, and more.

```rust
use reflow_js::{JsRuntime, RuntimeConfig, Permission, PermissionScope, FileSystemPermissionScope};
use std::path::PathBuf;

fn main() {
    // Create a runtime configuration
    let config = RuntimeConfig::default();
    
    // Create a new runtime
    let mut runtime = JsRuntime::new(config);
    
    // Add a file system permission
    let fs_permission = Permission::new(
        "fs".to_string(),
        PermissionScope::FileSystem(FileSystemPermissionScope::Directories(vec![PathBuf::from(".")])),
        true,
    );
    
    runtime.permissions_mut().add_permission(fs_permission).unwrap();
    
    // Evaluate JavaScript code
    let code = r#"
        const fs = require('fs');
        fs.readFileSync('hello.txt', 'utf8');
    "#;
    
    runtime.eval(code).unwrap();
}
```

## Building for WebAssembly

The runtime can be compiled to WebAssembly for use in browser environments. When compiled to WebAssembly, the runtime will use browser APIs for file system access, networking, and more.

```toml
# Cargo.toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
reflow_js = "0.1.0"
wasm-bindgen = "0.2"
```

```rust
// src/lib.rs
use wasm_bindgen::prelude::*;
use reflow_js::{JsRuntime, RuntimeConfig};

#[wasm_bindgen]
pub fn eval_js(code: &str) -> String {
    let config = RuntimeConfig::default();
    let runtime = JsRuntime::new(config);
    
    match runtime.eval(code) {
        Ok(result) => format!("{:?}", result),
        Err(error) => format!("Error: {:?}", error),
    }
}
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.
