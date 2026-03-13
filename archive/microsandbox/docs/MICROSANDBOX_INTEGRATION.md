# MicroSandbox Integration Summary

## Overview
This document summarizes the effort to integrate MicroSandbox for secure script execution in the Reflow framework. MicroSandbox provides hardware-level VM isolation for running untrusted code safely.

## Architecture Implemented

```
Host Application (Reflow)
        ↓
WebSocket RPC Client
        ↓
MicroSandbox Runtime Server (Port 7777/7778)
        ↓
MicroSandbox SDK (Rust)
        ↓
MicroSandbox Server (Docker, Port 8080)
        ↓
Sandboxed VMs (Python/JavaScript)
```

## Components Created

### 1. MicroSandbox Runtime Server (`/crates/reflow_microsandbox_runtime/`)
- **Purpose**: Bridge between Reflow and MicroSandbox SDK
- **Features**:
  - WebSocket RPC server for actor communication
  - Actor script loading and management
  - Python and JavaScript sandbox support
  - Metadata extraction from script decorators
- **Status**: ✅ Implemented and functional

### 2. Docker Setup (`/docker/microsandbox/`)
- **Dockerfile**: Ubuntu-based container with MicroSandbox server
- **Features**:
  - MicroSandbox server installation via official installer
  - Python and Node.js runtime support
  - Health checks using netcat
  - Privileged mode for VM operations
- **Status**: ✅ Container builds and starts successfully

### 3. Integration Tests (`/crates/reflow_network/tests/`)
- **microsandbox_integration_test.rs**: Full integration tests
- **microsandbox_runtime_basic_test.rs**: Basic RPC communication tests
- **Status**: ⚠️ Partially working (RPC layer works, sandbox execution blocked)

### 4. Metadata Extraction (`/crates/reflow_microsandbox_runtime/src/extractor.rs`)
- **Purpose**: Extract actor metadata from script decorators
- **Supported Patterns**:
  - Python: `@actor({metadata})` decorator
  - JavaScript: `const metadata = {...}` object
- **Status**: ✅ Implemented with strict pattern matching (no fallbacks)

## Key Implementation Details

### Actor Pattern (Python)
```python
@actor({
    "component": "actor_name",
    "description": "Actor description",
    "inports": ["input"],
    "outports": ["output"]
})
async def process(context):
    input_value = context.get_input("input")
    result = process_value(input_value)
    await context.send_output("output", result)
    return {"status": "completed"}
```

### WebSocket RPC Protocol
- JSON-RPC 2.0 compliant
- Methods: `list`, `load`, `process`
- Bidirectional communication for real-time output streaming

## Current Status

### ✅ Working Components
1. **MicroSandbox Runtime Server**: Successfully starts and handles WebSocket connections
2. **WebSocket RPC Layer**: Properly processes requests and responses
3. **Docker Container**: Builds and runs the MicroSandbox server
4. **Basic Communication**: Test `test_microsandbox_runtime_server` passes
5. **Metadata Extraction**: Correctly extracts actor metadata from decorated functions

### ❌ Current Blocker

#### Portal Connection Issue
**Error**: `Failed to connect to portal after 10000 retries: error sending request for url (http://127.0.0.1:XXXXX/)`

**Description**: The MicroSandbox server cannot establish connection with the portal component that runs inside the sandbox VM. This prevents actual code execution in sandboxed environments.

**Root Cause Analysis**:
1. The portal is a component that runs inside each sandbox VM to handle communication with the host
2. When MicroSandbox tries to create a sandbox, it:
   - Pulls the runtime image (✅ succeeds)
   - Creates the VM/container (likely succeeds)
   - Attempts to connect to the portal inside the VM (❌ fails)

**Possible Reasons**:
- **Docker-in-Docker limitations**: Running VMs inside Docker containers has limitations, especially on macOS
- **Networking issues**: The portal might be starting but not accessible due to Docker networking constraints
- **Missing kernel modules**: MicroSandbox might require KVM or other virtualization features not available in the Docker environment
- **macOS virtualization limits**: Docker Desktop on macOS runs in a VM itself, creating nested virtualization issues

## Test Results

### Test: `test_microsandbox_runtime_server`
- **Status**: ✅ PASSES
- **What it tests**: WebSocket RPC communication, list operation
- **Why it passes**: Doesn't require actual sandbox execution

### Test: `test_actor_execution_via_websocket_rpc`
- **Status**: ❌ FAILS
- **What it tests**: Full actor execution in sandbox
- **Failure point**: MicroSandbox portal connection when trying to execute Python code
- **Error**: `Failed to start Python sandbox: HTTP error: error sending request for url`

## Attempted Solutions

1. **Docker Configuration**:
   - Added `privileged: true` for VM operations
   - Added Docker socket mounting (`/var/run/docker.sock`)
   - Added capabilities: `SYS_ADMIN`, `NET_ADMIN`
   - Result: Container starts but portal issue persists

2. **Environment Variables**:
   - Tried setting `MSB_SERVER_URL`, `MICROSANDBOX_SERVER_URL`, `MSB_URL`
   - Result: No effect on portal connection

3. **Server Flags**:
   - Attempted `--disable-sandbox` flag (doesn't exist)
   - Running in `--dev` mode
   - Result: No solution for portal issue

## Recommendations

### Short-term Solutions
1. **Mock MicroSandbox for Testing**: Create a mock implementation for unit tests that doesn't require actual sandboxing
2. **Alternative Sandbox**: Consider WebAssembly (WASM) or other sandboxing solutions that work better in containerized environments
3. **Direct Process Execution**: For development, bypass MicroSandbox and run scripts directly (insecure, dev-only)

### Long-term Solutions
1. **Native Installation**: Run MicroSandbox server natively on the host instead of in Docker
2. **Cloud Deployment**: Deploy MicroSandbox server to a cloud VM with proper virtualization support
3. **Contact MicroSandbox Team**: Report the Docker compatibility issue and seek guidance

## Next Steps

1. **Immediate**: Document the portal issue as a known limitation
2. **Priority**: Investigate alternative sandboxing solutions (WASM, Firecracker, gVisor)
3. **Testing**: Create mock-based tests to validate the integration logic without actual sandboxing
4. **Future**: Re-evaluate when MicroSandbox improves Docker compatibility

## Conclusion

The MicroSandbox integration is architecturally complete and the WebSocket RPC layer is fully functional. However, the actual sandbox execution is blocked by the portal connection issue, which appears to be a fundamental limitation of running MicroSandbox's VM-based isolation inside Docker containers, particularly on macOS.

The integration work is valuable and can be easily adapted to other sandboxing solutions that are more container-friendly.