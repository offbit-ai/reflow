# Wasm Actor Example: Counter Actor

This is a simple WebAssembly actor that demonstrates state management with a counter.

## Features

- Increment, decrement, or reset a counter
- Maintains state between calls
- Outputs both the current and previous counter values

## Building

To build this example, you'll need the `wasm32-wasi` target installed:

```bash
rustup target add wasm32-wasi
```