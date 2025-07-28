#!/bin/bash

# Build script for the multiply_actor plugin

echo "Building multiply_actor WASM plugin..."

# Ensure we have the wasm32-unknown-unknown target
rustup target add wasm32-unknown-unknown

# Build the plugin
cargo build --release --target wasm32-unknown-unknown

# Copy the WASM file to a more convenient location
mkdir -p build
cp target/wasm32-unknown-unknown/release/multiply_actor.wasm build/

echo "Plugin built successfully: build/multiply_actor.wasm"
echo "Size: $(ls -lh build/multiply_actor.wasm | awk '{print $5}')"

# Optional: Run wasm-opt if available to optimize further
if command -v wasm-opt &> /dev/null; then
    echo "Optimizing with wasm-opt..."
    wasm-opt -Oz build/multiply_actor.wasm -o build/multiply_actor_opt.wasm
    echo "Optimized size: $(ls -lh build/multiply_actor_opt.wasm | awk '{print $5}')"
fi