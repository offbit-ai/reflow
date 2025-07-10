#!/bin/bash

# Start Reflow Tracing Server
# This script demonstrates how to run the reflow_tracing server as a standalone service

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}🚀 Starting Reflow Tracing Server...${NC}"

# Check if we're in the right directory
if [[ ! -f "Cargo.toml" ]]; then
    echo -e "${RED}❌ Error: Please run this script from the project root directory${NC}"
    exit 1
fi

# Create data directory if it doesn't exist
mkdir -p examples/tracing_integration/data

# Set configuration file path
CONFIG_FILE="examples/tracing_integration/config/server_config.toml"

# Check if config file exists
if [[ ! -f "$CONFIG_FILE" ]]; then
    echo -e "${RED}❌ Error: Configuration file not found at $CONFIG_FILE${NC}"
    exit 1
fi

# Export environment variable to use our config
export REFLOW_TRACING_CONFIG_FILE="$CONFIG_FILE"

echo -e "${GREEN}📋 Configuration: $CONFIG_FILE${NC}"
echo -e "${GREEN}📊 Storage: SQLite (examples/tracing_integration/data/traces.db)${NC}"
echo -e "${GREEN}🌐 Server: http://127.0.0.1:8080${NC}"
echo -e "${GREEN}📈 Metrics: http://127.0.0.1:9090/metrics${NC}"
echo ""

# Function to handle cleanup
cleanup() {
    echo -e "\n${YELLOW}🛑 Shutting down server gracefully...${NC}"
    exit 0
}

# Set up signal handlers
trap cleanup SIGINT SIGTERM

# Start the server
echo -e "${BLUE}🔧 Building reflow_tracing...${NC}"
cargo build --release --bin reflow_tracing

echo -e "${GREEN}✅ Starting server...${NC}"
echo -e "${YELLOW}Press Ctrl+C to stop the server${NC}"
echo ""

# Run the server with our configuration
RUST_LOG=info ./target/release/reflow_tracing
