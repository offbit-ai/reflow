# Deno Runtime

Reflow's Deno runtime enables JavaScript and TypeScript actors with secure, sandboxed execution.

## Overview

The Deno runtime provides:
- **Secure sandbox** with configurable permissions
- **TypeScript support** out of the box
- **NPM package** ecosystem access
- **Modern JavaScript** features (ES2022+)
- **Async/await** support for non-blocking operations

## Basic Usage

### Creating a JavaScript Actor

```rust
use reflow_script::{ScriptActor, LanguageEngine};
use reflow_network::{Network, NetworkConfig};
use reflow_network::connector::{Connector, ConnectionPoint, InitialPacket};
use reflow_network::message::Message;

// Create script actor with JavaScript/Deno engine
let script_content = r#"
function process(inputs, context) {
    const data = inputs.data;
    
    if (typeof data === 'string') {
        return {
            result: data.toUpperCase(),
            length: data.length,
            timestamp: new Date().toISOString()
        };
    }
    
    return { error: 'Expected string input' };
}
"#;

let actor = ScriptActor::new(LanguageEngine::JavaScript, script_content.to_string());

// Register and use in network
let mut network = Network::new(NetworkConfig::default());
network.register_actor("js_processor", actor)?;
network.add_node("script1", "js_processor")?;

// Connect to other actors
network.add_connection(Connector {
    from: ConnectionPoint {
        actor: "source_actor".to_owned(),
        port: "output".to_owned(),
        ..Default::default()
    },
    to: ConnectionPoint {
        actor: "script1".to_owned(),
        port: "data".to_owned(),
        ..Default::default()
    },
});
```

### JavaScript Actor Script

```javascript
// script.js - Simple transformation actor
function process(inputs, context) {
    const data = inputs.data;
    
    if (typeof data === 'string') {
        return {
            result: data.toUpperCase(),
            length: data.length,
            timestamp: new Date().toISOString()
        };
    }
    
    return { error: 'Expected string input' };
}

// Export for Reflow
exports.process = process;
```

## Actor Function Signature

### Input Parameters

```javascript
function process(inputs, context) {
    // inputs: Object containing input port data
    // context: Actor execution context
}
```

### Context Object

```javascript
const context = {
    // Actor configuration
    config: {
        // Custom configuration values
    },
    
    // Utility functions
    log: (level, message) => {},
    
    // State management
    getState: () => {},
    setState: (state) => {},
    
    // Metrics
    incrementCounter: (name) => {},
    recordTimer: (name, duration) => {},
};
```

### Return Values

```javascript
// Success - return output object
return {
    output1: "value1",
    output2: 42,
    status: "success"
};

// Error - return error object
return {
    error: "Something went wrong",
    code: 500
};

// Async operations
async function process(inputs, context) {
    const result = await fetchData(inputs.url);
    return { data: result };
}
```

## Data Types

### Supported Types

```javascript
// Primitive types
return {
    string: "hello",
    number: 42,
    boolean: true,
    null: null,
};

// Complex types
return {
    array: [1, 2, 3],
    object: { key: "value" },
    nested: {
        array: [{ id: 1 }, { id: 2 }],
        metadata: { timestamp: Date.now() }
    }
};

// Binary data
return {
    buffer: new Uint8Array([1, 2, 3, 4])
};
```

## State Management

### Persistent State

```javascript
function process(inputs, context) {
    // Get current state
    const state = context.getState() || { counter: 0 };
    
    // Update state
    state.counter += 1;
    state.lastInput = inputs.data;
    
    // Save state
    context.setState(state);
    
    return {
        count: state.counter,
        data: state.lastInput
    };
}
```

## Async Operations

### HTTP Requests

```javascript
async function process(inputs, context) {
    try {
        const response = await fetch(inputs.url, {
            method: 'GET',
            headers: {
                'Content-Type': 'application/json'
            }
        });
        
        if (!response.ok) {
            return { error: `HTTP ${response.status}` };
        }
        
        const data = await response.json();
        return { result: data };
        
    } catch (error) {
        return { error: error.message };
    }
}
```

### File Operations

```javascript
async function process(inputs, context) {
    try {
        // Read file (requires --allow-read permission)
        const content = await Deno.readTextFile(inputs.filename);
        
        // Process content
        const lines = content.split('\n').length;
        
        return {
            content: content,
            lineCount: lines
        };
        
    } catch (error) {
        return { error: `File error: ${error.message}` };
    }
}
```

## NPM Package Support

### Using External Packages

```javascript
// Import from NPM
import { format } from "https://deno.land/x/date_fns/index.js";
import _ from "https://cdn.skypack.dev/lodash";

function process(inputs, context) {
    const now = new Date();
    const formatted = format(now, 'yyyy-MM-dd HH:mm:ss');
    
    const processed = _.map(inputs.data, item => ({
        ...item,
        timestamp: formatted
    }));
    
    return { result: processed };
}
```

### Package Configuration

```rust
let config = ScriptConfig {
    environment: ScriptEnvironment::SYSTEM,
    runtime: ScriptRuntime::JavaScript,
    source: script_source,
    entry_point: "process".to_string(),
    packages: Some(vec![
        "https://deno.land/x/date_fns@v2.29.3/index.js".to_string(),
        "https://cdn.skypack.dev/lodash@4.17.21".to_string(),
    ]),
};
```

## Error Handling

### Error Patterns

```javascript
function process(inputs, context) {
    try {
        // Validate inputs
        if (!inputs.data) {
            return { error: "Missing required 'data' input" };
        }
        
        if (typeof inputs.data !== 'string') {
            return { 
                error: "Invalid input type",
                expected: "string",
                received: typeof inputs.data
            };
        }
        
        // Process data
        const result = inputs.data.toLowerCase();
        
        if (result.length === 0) {
            return { 
                error: "Empty result",
                warning: "Input data was empty after processing"
            };
        }
        
        return { result: result };
        
    } catch (error) {
        // Log error for debugging
        context.log('error', `Processing failed: ${error.message}`);
        
        return {
            error: error.message,
            stack: error.stack,
            timestamp: new Date().toISOString()
        };
    }
}
```

## Security and Permissions

### Permission Configuration

```rust
use reflow_script::PermissionConfig;

let config = ScriptConfig {
    // ... other fields
    permissions: Some(PermissionConfig {
        allow_net: vec!["https://api.example.com".to_string()],
        allow_read: vec!["/tmp/data".to_string()],
        allow_write: vec!["/tmp/output".to_string()],
        allow_run: false,
        allow_env: false,
    }),
};
```

### Safe Practices

```javascript
function process(inputs, context) {
    // Validate and sanitize inputs
    const sanitized = sanitizeInput(inputs.userInput);
    
    // Use try-catch for external operations
    try {
        return processData(sanitized);
    } catch (error) {
        // Don't expose internal details
        return { error: "Processing failed" };
    }
}

function sanitizeInput(input) {
    if (typeof input !== 'string') return '';
    
    // Remove potentially dangerous characters
    return input
        .replace(/[<>]/g, '')
        .trim()
        .substring(0, 1000); // Limit length
}
```

## Performance Optimization

### Efficient Processing

```javascript
// Use streaming for large data
async function process(inputs, context) {
    const results = [];
    
    // Process in chunks to avoid memory issues
    const chunkSize = 100;
    const data = inputs.data || [];
    
    for (let i = 0; i < data.length; i += chunkSize) {
        const chunk = data.slice(i, i + chunkSize);
        const processed = await processChunk(chunk);
        results.push(...processed);
        
        // Allow other actors to run
        if (i % 1000 === 0) {
            await new Promise(resolve => setTimeout(resolve, 0));
        }
    }
    
    return { results: results };
}

async function processChunk(chunk) {
    return chunk.map(item => ({
        ...item,
        processed: true,
        timestamp: Date.now()
    }));
}
```

### Caching

```javascript
// Simple in-memory cache
const cache = new Map();

function process(inputs, context) {
    const key = inputs.cacheKey;
    
    // Check cache first
    if (cache.has(key)) {
        context.log('info', `Cache hit for key: ${key}`);
        return { result: cache.get(key), fromCache: true };
    }
    
    // Expensive computation
    const result = expensiveOperation(inputs.data);
    
    // Store in cache with TTL
    cache.set(key, result);
    setTimeout(() => cache.delete(key), 60000); // 1 minute TTL
    
    return { result: result, fromCache: false };
}
```

## Testing JavaScript Actors

### Unit Testing

```javascript
// test_actor.js
import { assertEquals } from "https://deno.land/std/testing/asserts.ts";

// Import your actor function
import { process } from "./my_actor.js";

Deno.test("actor processes string input", () => {
    const inputs = { data: "hello world" };
    const context = { 
        log: () => {},
        getState: () => ({}),
        setState: () => {}
    };
    
    const result = process(inputs, context);
    
    assertEquals(result.result, "HELLO WORLD");
    assertEquals(result.length, 11);
});

Deno.test("actor handles missing input", () => {
    const inputs = {};
    const context = { log: () => {} };
    
    const result = process(inputs, context);
    
    assertEquals(result.error, "Expected string input");
});
```

### Integration Testing

```rust
#[tokio::test]
async fn test_javascript_actor_integration() {
    let script = include_str!("test_script.js");
    let config = ScriptConfig {
        environment: ScriptEnvironment::SYSTEM,
        runtime: ScriptRuntime::JavaScript,
        source: script.as_bytes().to_vec(),
        entry_point: "process".to_string(),
        packages: None,
    };
    
    let actor = ScriptActor::new(config);
    
    // Test actor behavior
    let inputs = HashMap::from([
        ("data".to_string(), Message::String("test".to_string()))
    ]);
    
    let result = test_actor_behavior(actor, inputs).await;
    assert!(result.is_ok());
}
```

## Examples

### Data Transformation

```javascript
// Transform JSON data
function process(inputs, context) {
    const data = inputs.json_data;
    
    if (!Array.isArray(data)) {
        return { error: "Expected array input" };
    }
    
    const transformed = data.map(item => ({
        id: item.id,
        name: item.name?.toUpperCase(),
        email: item.email?.toLowerCase(),
        createdAt: new Date(item.created_at).toISOString(),
        tags: item.tags?.map(tag => tag.toLowerCase()) || []
    }));
    
    return {
        data: transformed,
        count: transformed.length,
        processedAt: new Date().toISOString()
    };
}
```

### API Integration

```javascript
async function process(inputs, context) {
    const { endpoint, payload, authToken } = inputs;
    
    try {
        const response = await fetch(endpoint, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                'Authorization': `Bearer ${authToken}`
            },
            body: JSON.stringify(payload)
        });
        
        const result = await response.json();
        
        return {
            status: response.status,
            data: result,
            success: response.ok
        };
        
    } catch (error) {
        return {
            error: error.message,
            success: false
        };
    }
}
```

## Best Practices

### Code Organization

```javascript
// Separate concerns into functions
function process(inputs, context) {
    try {
        const validated = validateInputs(inputs);
        const processed = processData(validated);
        const formatted = formatOutput(processed);
        
        return { result: formatted };
    } catch (error) {
        return handleError(error, context);
    }
}

function validateInputs(inputs) {
    if (!inputs.data) throw new Error("Missing data");
    return inputs;
}

function processData(inputs) {
    // Main processing logic
    return inputs.data.map(transform);
}

function formatOutput(data) {
    return {
        items: data,
        timestamp: new Date().toISOString()
    };
}

function handleError(error, context) {
    context.log('error', error.message);
    return { error: "Processing failed" };
}
```

### Resource Management

```javascript
// Clean up resources
function process(inputs, context) {
    const resources = [];
    
    try {
        // Acquire resources
        const db = openDatabase(inputs.connectionString);
        resources.push(db);
        
        const file = openFile(inputs.filename);
        resources.push(file);
        
        // Use resources
        const result = processWithResources(db, file);
        
        return { result: result };
        
    } finally {
        // Always clean up
        resources.forEach(resource => {
            try {
                resource.close();
            } catch (e) {
                // Log but don't throw
                console.error("Cleanup error:", e);
            }
        });
    }
}
```

## Next Steps

- [Python Runtime](../python/python-engine.md) - Python scripting support
- [WebAssembly Runtime](../wasm/wasm-plugins.md) - WASM plugin system
- [Script Configuration](./script-configuration.md) - Advanced configuration
- [Creating Actors](../../api/actors/creating-actors.md) - Actor development guide
