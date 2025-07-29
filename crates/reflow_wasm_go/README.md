# Reflow WebAssembly Go SDK

This crate provides the Go SDK for building Reflow actors as WebAssembly plugins using TinyGo and Extism.

## Current Status

The Go SDK is fully functional with support for:
- ✅ **Host functions** - `SendOutput`, `GetState`, and `SetState` work correctly using memory allocation
- ✅ **State management** - Both through host functions and ActorContext/ActorResult
- ✅ **Async outputs** - Send outputs during processing via `SendOutput`
- ✅ **All message types** - Full compatibility with Rust's serde serialization

## Structure

- `sdk/` - Go SDK source code
- `examples/` - Example Go actors
- `src/` - Rust test code for validating Go plugins
- `build_examples.sh` - Script to build example WASM files

## Prerequisites

1. **TinyGo** (recommended): Install TinyGo for WebAssembly compilation
   ```bash
   # macOS
   brew tap tinygo-org/tools
   brew install tinygo
   
   # Or download from: https://tinygo.org/getting-started/install/
   ```

2. **Go**: Go 1.21 or later (as alternative to TinyGo)

## Quick Start

### 1. Set up Go module

```bash
cd examples/myactor
go mod init myactor
go mod edit -require github.com/darmie/reflow/reflow_wasm_go/sdk@v0.0.0
go mod edit -replace github.com/darmie/reflow/reflow_wasm_go/sdk=../../sdk
go get github.com/extism/go-pdk
```

### 2. Create a Basic Actor

```go
package main

import (
    "fmt"
    reflow "github.com/darmie/reflow/reflow_wasm_go/sdk"
)

func processMyActor(context reflow.ActorContext) (reflow.ActorResult, error) {
    outputs := make(map[string]reflow.Message)
    
    // Read from state
    count := int64(0)
    if context.State != nil {
        if stateMap, ok := context.State.(map[string]interface{}); ok {
            if val, exists := stateMap["count"]; exists {
                if f, ok := val.(float64); ok {
                    count = int64(f)
                }
            }
        }
    }
    
    // Process inputs
    if msg, exists := context.Payload["input"]; exists {
        if msg.Type == "Integer" {
            value := int64(msg.Data.(float64))
            count += value
            outputs["output"] = reflow.NewIntegerMessage(count)
        }
    }
    
    // Update state
    newState := map[string]interface{}{
        "count": count,
    }
    
    return reflow.ActorResult{
        Outputs: outputs,
        State:   newState,
    }, nil
}

func init() {
    // Register plugin metadata and process function
    reflow.RegisterPlugin(
        reflow.PluginMetadata{
            Component:   "MyActor",
            Description: "Example Go actor",
            Inports: []reflow.PortDefinition{
                reflow.NewOptionalPort("input", "Input port", "Integer"),
            },
            Outports: []reflow.PortDefinition{
                reflow.NewOptionalPort("output", "Output port", "Integer"),
            },
        },
        processMyActor,
    )
}

// Required exports for Extism (use //go:wasmexport for Go 1.21+)
//go:wasmexport get_metadata
func get_metadata() int32 {
    return reflow.GetMetadata()
}

//go:wasmexport process
func process() int32 {
    return reflow.Process()
}

func main() {}
```

### 3. Build the Plugin

#### Using TinyGo (Recommended - Smaller binaries)
```bash
tinygo build -o my_actor.wasm -target wasi -no-debug main.go
```

#### Using Standard Go (Alternative - Larger binaries)
```bash
GOOS=wasip1 GOARCH=wasm go build -o my_actor.wasm main.go
```

## Message Types

The SDK provides constructors for all Reflow message types:
- `NewFlowMessage()` - Flow control message
- `NewEventMessage(data)` - Event with JSON data
- `NewBooleanMessage(value)` - Boolean value
- `NewIntegerMessage(value)` - Integer value
- `NewFloatMessage(value)` - Float value
- `NewStringMessage(value)` - String value
- `NewObjectMessage(data)` - JSON object
- `NewArrayMessage(data)` - Array of values
- `NewStreamMessage(data)` - Binary stream
- `NewOptionalMessage(data)` - Optional value
- `NewAnyMessage(data)` - Any JSON value
- `NewErrorMessage(error)` - Error message

## Host Functions

The SDK provides full host function support:

```go
// Send outputs asynchronously during processing
err := reflow.SendOutput(map[string]interface{}{
    "status": "Processing...",
    "progress": 50,
})

// Send typed outputs using Messages
err := reflow.SendOutputMessages(map[string]reflow.Message{
    "result": reflow.NewIntegerMessage(42),
    "status": reflow.NewStringMessage("Complete"),
})

// Get state value
count, err := reflow.GetState("counter")
if err == nil && count != nil {
    // Use the value
}

// Set state value
err := reflow.SetState("counter", 42)
```

## State Management

State can be managed in two ways:

```go
// Reading state
if context.State != nil {
    if stateMap, ok := context.State.(map[string]interface{}); ok {
        // Access state values
        if value, exists := stateMap["myKey"]; exists {
            // Use value
        }
    }
}

// Writing state
newState := map[string]interface{}{
    "myKey": "myValue",
    "counter": 42,
}

return reflow.ActorResult{
    Outputs: outputs,
    State:   newState,
}
```

## Configuration Access

```go
// Get config values
if val, ok := context.Config.GetString("my_setting"); ok {
    // Use string value
}

if val, ok := context.Config.GetInt("chunk_size"); ok {
    // Use integer value
}

if val, ok := context.Config.GetBool("enabled"); ok {
    // Use boolean value
}

// Get environment variables
if val, ok := context.Config.GetEnv("API_KEY"); ok {
    // Use environment variable
}
```

## Examples

### Counter Actor

See `examples/counter/main.go` for a simple stateful counter that demonstrates:
- State management through context
- Multiple output ports
- Operation handling

### Counter with State (Advanced)

See `examples/counter_with_state/main.go` for an advanced example that demonstrates:
- Complex state management
- Event messages
- Multiple operations

## Building Examples

```bash
# Build all examples
./build_examples.sh

# Or build individually
cd examples/counter
tinygo build -o counter.wasm -target wasi -no-debug main.go
```

## Testing

The Rust test suite verifies Go plugin functionality:

```bash
# Run tests (requires examples to be built first)
cargo test
```

## Troubleshooting

### macOS Security Restrictions
If you encounter "Killed: 9" errors with TinyGo on macOS:
1. Install TinyGo via Homebrew: `brew install tinygo`
2. Or allow the binary in System Preferences > Security & Privacy
3. Or use standard Go with `GOOS=wasip1 GOARCH=wasm`

### JSON Serialization
The Go SDK automatically handles JSON serialization to match Rust's serde format. The `Message` struct tags ensure proper serialization:
```go
type Message struct {
    Type string      `json:"type"`
    Data interface{} `json:"data"`
}
```

### Export Directives
For Go 1.21+, use `//go:wasmexport` instead of `//export` for WASM exports:
```go
//go:wasmexport get_metadata
func get_metadata() int32 { ... }

//go:wasmexport process
func process() int32 { ... }
```

## Architecture

The Go SDK provides:

1. **Type System**: Go structs that mirror the Reflow actor types with proper JSON tags
2. **Message Creation**: Helper functions for creating typed messages
3. **Plugin Registration**: Automatic registration with the Extism runtime
4. **State Management**: Through ActorContext (read) and ActorResult (write)

The SDK compiles to WebAssembly using TinyGo or standard Go and runs in the same Extism runtime used by all WASM language targets.

## Technical Details

### Host Function Implementation

The Go SDK uses Extism PDK's memory allocation functions to pass data to host functions:

1. **SendOutput**: Serializes outputs to JSON, allocates memory, and passes the offset
2. **GetState**: Passes key as string offset, receives JSON value at returned offset
3. **SetState**: Passes both key and value as separate memory offsets

This approach works around the differences between Go's `//go:wasmimport` and Rust's `#[host_fn]` mechanisms.

### JSON Output Handling

The Go SDK outputs JSON data using `pdk.Output()`, which may encode the data as base64. The Extism engine automatically detects and decodes base64-encoded JSON responses from Go plugins, ensuring compatibility with the Rust ecosystem.

## Future Improvements

1. **Better Type Safety**: Add generic helpers for state management
2. **Performance Optimization**: Reduce JSON serialization overhead
3. **More Examples**: Add examples for different actor patterns