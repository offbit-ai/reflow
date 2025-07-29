#!/bin/bash

# Alternative build script using standard Go (instead of TinyGo)
# This produces larger binaries but may work when TinyGo has issues

set -e

echo "Building Go WASM plugin examples using standard Go..."

# Check if go is installed
if ! command -v go &> /dev/null; then
    echo "Error: Go is not installed. Please install Go first."
    echo "Visit: https://golang.org/dl/"
    exit 1
fi

# Check Go version supports WASI
go_version=$(go version | awk '{print $3}' | sed 's/go//')
major=$(echo $go_version | cut -d. -f1)
minor=$(echo $go_version | cut -d. -f2)

if [ "$major" -lt 1 ] || ([ "$major" -eq 1 ] && [ "$minor" -lt 21 ]); then
    echo "Error: Go 1.21 or later is required for WASI support"
    echo "Current version: $go_version"
    exit 1
fi

# Function to build a Go plugin
build_plugin() {
    local example_name=$1
    local example_dir="examples/$example_name"
    
    echo "Building $example_name with standard Go..."
    
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
    
    # Build the WASM binary using standard Go
    echo "Running: GOOS=wasip1 GOARCH=wasm go build -o ${example_name}.wasm ."
    GOOS=wasip1 GOARCH=wasm go build -o "${example_name}.wasm" .
    
    if [ $? -eq 0 ]; then
        echo "✓ $example_name built successfully with standard Go"
        ls -lh "${example_name}.wasm"
        echo "Note: Standard Go produces larger WASM files than TinyGo"
    else
        echo "✗ Failed to build $example_name"
        return 1
    fi
    
    cd ../..
}

# Build all examples
build_plugin "counter"
build_plugin "async_processor"

echo ""
echo "All builds complete!"
echo "Note: These binaries are larger than TinyGo builds but should work on systems where TinyGo has issues."