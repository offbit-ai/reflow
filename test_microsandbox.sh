#!/bin/bash

echo "Testing MicroSandbox integration with Reflow"
echo "============================================"

# Run the MicroSandbox integration tests
echo ""
echo "Running integration tests (requires MicroSandbox to be installed)..."
cargo test --package reflow_network --test microsandbox_integration_test -- --ignored --nocapture

# If tests pass, we know the setup works
if [ $? -eq 0 ]; then
    echo ""
    echo "✅ MicroSandbox integration tests passed!"
    echo ""
    echo "To run with Docker:"
    echo "  1. docker-compose up -d"
    echo "  2. cargo test microsandbox_integration -- --ignored"
else
    echo ""
    echo "❌ Tests failed. Make sure:"
    echo "  1. MicroSandbox is installed"
    echo "  2. Redis is running (optional for state persistence)"
    echo "  3. Test scripts exist in test_scripts/"
fi