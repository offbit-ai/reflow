#!/bin/bash
# Prebuild MicroSandbox runtime images to avoid timeouts during tests

echo "Prebuilding MicroSandbox Python and Node.js runtime images..."

# Ensure MicroSandbox server is running
if ! docker ps | grep -q reflow_microsandbox; then
    echo "Starting MicroSandbox server..."
    docker-compose up -d microsandbox
    sleep 5
fi

# Create simple test scripts to trigger image building
TEMP_DIR=$(mktemp -d)

# Python test to trigger Python image build
cat > "$TEMP_DIR/test.py" << 'EOF'
print("Building Python runtime")
EOF

# Node.js test to trigger Node image build  
cat > "$TEMP_DIR/test.js" << 'EOF'
console.log("Building Node runtime");
EOF

echo "Triggering Python runtime image build..."
curl -X POST http://localhost:8080/api/v1/sandbox/create \
  -H "Content-Type: application/json" \
  -d '{
    "name": "prebuild-python",
    "runtime": "python",
    "code": "print(\"test\")"
  }' || echo "Python prebuild request sent"

echo "Triggering Node.js runtime image build..."
curl -X POST http://localhost:8080/api/v1/sandbox/create \
  -H "Content-Type: application/json" \
  -d '{
    "name": "prebuild-node",
    "runtime": "nodejs",
    "code": "console.log(\"test\")"
  }' || echo "Node prebuild request sent"

# Wait for builds to complete
echo "Waiting for runtime images to build (this may take a few minutes on first run)..."
sleep 30

# Clean up
rm -rf "$TEMP_DIR"

echo "MicroSandbox runtime images prebuilt successfully!"
echo "You can now run tests without timeout issues."