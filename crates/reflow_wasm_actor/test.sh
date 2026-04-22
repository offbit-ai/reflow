#!/bin/bash

echo "Running Reflow WASM SDK tests..."

# Ensure we have the wasm32 target
rustup target add wasm32-unknown-unknown

# Run SDK unit tests
echo "Running SDK unit tests..."
cargo test --test sdk_tests

# Run plugin integration tests (these will build the plugins as needed)
echo -e "\nRunning plugin integration tests..."
cargo test --test plugin_tests -- --nocapture

# Optional: Run ignored tests to generate extism CLI commands
echo -e "\nTo test with extism CLI, run:"
echo "cargo test --test plugin_tests -- --ignored --nocapture"

echo -e "\nDone!"