#!/bin/bash

# Build script for Go WebAssembly examples

set -e

echo "Building Go WASM plugin examples..."

# Check if tinygo is installed
TINYGO_CMD=""
if command -v tinygo &> /dev/null; then
    TINYGO_CMD="tinygo"
elif [ -x "/Users/amaterasu/tinygo/bin/tinygo" ]; then
    TINYGO_CMD="/Users/amaterasu/tinygo/bin/tinygo"
else
    echo "Error: TinyGo is not installed. Please install TinyGo first."
    echo "Visit: https://tinygo.org/getting-started/install/"
    exit 1
fi

# Function to build a Go plugin
build_plugin() {
    local example_name=$1
    local example_dir="examples/$example_name"
    
    echo "Building $example_name..."
    
    if [ ! -d "$example_dir" ]; then
        echo "Error: Example directory $example_dir does not exist"
        return 1
    fi
    
    cd "$example_dir"
    
    # Ensure go.mod is up to date
    if [ -f "go.mod" ]; then
        go mod tidy
    else
        echo "Error: go.mod not found in $example_dir"
        return 1
    fi
    
    # Build the WASM binary
    echo "Running: $TINYGO_CMD build -o ${example_name}.wasm -target wasi -no-debug ."
    $TINYGO_CMD build -o "${example_name}.wasm" -target wasi -no-debug .
    
    exit_code=$?
    
    if [ $exit_code -eq 0 ]; then
        echo "✓ $example_name built successfully"
        ls -lh "${example_name}.wasm"
    elif [ $exit_code -eq 137 ] || [ $exit_code -eq 9 ]; then
        echo "✗ Build killed by system (exit code: $exit_code)"
        echo ""
        echo "This often happens on macOS due to security restrictions."
        echo "Try one of these solutions:"
        echo "1. Allow TinyGo in Security & Privacy settings:"
        echo "   - Go to System Preferences > Security & Privacy > General"
        echo "   - Click 'Allow' for TinyGo if it appears"
        echo ""
        echo "2. Run with explicit security bypass (use with caution):"
        echo "   sudo spctl --master-disable"
        echo "   ./build_examples.sh"
        echo "   sudo spctl --master-enable"
        echo ""
        echo "3. Build manually with Go instead of TinyGo:"
        echo "   GOOS=wasip1 GOARCH=wasm go build -o ${example_name}.wasm"
        return 1
    else
        echo "✗ Failed to build $example_name (exit code: $exit_code)"
        return 1
    fi
    
    cd ../..
}

# Build all examples
build_plugin "counter"
build_plugin "async_processor"

echo "All builds complete!"