#!/bin/bash

echo "Building all WASM plugin examples..."

# Ensure we have the wasm32 target
rustup target add wasm32-unknown-unknown

# Build multiply_actor
echo "Building multiply_actor..."
(cd examples/multiply_actor && cargo build --release --target wasm32-unknown-unknown)
if [ $? -eq 0 ]; then
    echo "✓ multiply_actor built successfully"
    ls -lh examples/multiply_actor/target/wasm32-unknown-unknown/release/multiply_actor.wasm
else
    echo "✗ multiply_actor build failed"
fi

echo ""

# Build counter_actor
echo "Building counter_actor..."
(cd examples/counter_actor && cargo build --release --target wasm32-unknown-unknown)
if [ $? -eq 0 ]; then
    echo "✓ counter_actor built successfully"
    ls -lh examples/counter_actor/target/wasm32-unknown-unknown/release/counter_actor.wasm
else
    echo "✗ counter_actor build failed"
fi

echo ""

# Build async_actor
echo "Building async_actor..."
(cd examples/async_actor && cargo build --release --target wasm32-unknown-unknown)
if [ $? -eq 0 ]; then
    echo "✓ async_actor built successfully"
    ls -lh examples/async_actor/target/wasm32-unknown-unknown/release/async_actor.wasm
else
    echo "✗ async_actor build failed"
fi

echo -e "\nAll builds complete!"