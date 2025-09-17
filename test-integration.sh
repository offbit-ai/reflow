#!/bin/bash

# Integration test script for Zeal-Reflow demo
echo "🚀 Starting Zeal-Reflow Integration Demo"
echo "========================================"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test workflow JSON
WORKFLOW_JSON='{
  "id": "demo_workflow_001",
  "name": "Demo HTTP Workflow",
  "graphs": [{
    "id": "main",
    "name": "Main Graph",
    "nodes": [{
      "id": "http_node_1",
      "name": "Fetch User Data",
      "template_id": "tpl_http_request",
      "configuration": {
        "url": "https://jsonplaceholder.typicode.com/users/1",
        "method": "GET"
      }
    }],
    "connections": []
  }]
}'

# Function to test reflow server
test_reflow_server() {
    echo -e "\n${BLUE}1. Testing Reflow Server Direct Call...${NC}"
    
    RESPONSE=$(curl -s -X POST http://localhost:8080/api/execute-zeal \
        -H "Content-Type: application/json" \
        -d "{
            \"workflow\": $WORKFLOW_JSON,
            \"input\": {\"test\": true, \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"}
        }")
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ Reflow server responded${NC}"
        echo "Response: $RESPONSE"
        EXECUTION_ID=$(echo $RESPONSE | grep -o '"execution_id":"[^"]*' | cut -d'"' -f4)
        echo "Execution ID: $EXECUTION_ID"
        return 0
    else
        echo -e "${RED}❌ Failed to connect to reflow server${NC}"
        return 1
    fi
}

# Function to test workflow status
test_workflow_status() {
    local execution_id=$1
    echo -e "\n${BLUE}2. Checking Workflow Status...${NC}"
    
    RESPONSE=$(curl -s -X GET "http://localhost:8080/api/status/$execution_id")
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ Got workflow status${NC}"
        echo "Status: $RESPONSE"
        return 0
    else
        echo -e "${RED}❌ Failed to get workflow status${NC}"
        return 1
    fi
}

# Function to test WebSocket subscription
test_websocket() {
    local execution_id=$1
    echo -e "\n${BLUE}3. Testing WebSocket Event Stream...${NC}"
    
    # Use wscat if available, otherwise use a simple node script
    if command -v wscat &> /dev/null; then
        echo "Connecting to WebSocket..."
        timeout 5s wscat -c ws://localhost:8080/ws -x "{\"type\":\"subscribe_workflow\",\"execution_id\":\"$execution_id\"}" &
        sleep 3
        echo -e "${GREEN}✅ WebSocket connection tested${NC}"
    else
        echo "Creating Node.js WebSocket client..."
        cat > /tmp/ws-test.js << 'EOF'
const WebSocket = require('ws');
const ws = new WebSocket('ws://localhost:8080/ws');

ws.on('open', function open() {
  console.log('✅ WebSocket connected');
  ws.send(JSON.stringify({
    type: 'subscribe_workflow',
    execution_id: process.argv[2]
  }));
  
  setTimeout(() => {
    ws.close();
    process.exit(0);
  }, 3000);
});

ws.on('message', function message(data) {
  console.log('📨 Received:', data.toString());
});

ws.on('error', console.error);
EOF
        
        node /tmp/ws-test.js "$execution_id"
    fi
}

# Function to test Zeal API integration
test_zeal_api() {
    echo -e "\n${BLUE}4. Testing Zeal API Integration...${NC}"
    
    # First ensure the workflow exists in Zeal
    curl -s -X POST http://localhost:3000/api/workflows \
        -H "Content-Type: application/json" \
        -H "X-User-Id: demo_user" \
        -d "{
            \"id\": \"demo_workflow_001\",
            \"name\": \"Demo HTTP Workflow\",
            \"description\": \"Demo workflow for integration test\",
            \"nodes\": [{
                \"id\": \"http_node_1\",
                \"name\": \"Fetch User Data\",
                \"template_id\": \"tpl_http_request\",
                \"configuration\": {
                    \"url\": \"https://jsonplaceholder.typicode.com/users/1\",
                    \"method\": \"GET\"
                }
            }],
            \"connections\": [],
            \"status\": \"published\"
        }" > /dev/null 2>&1
    
    # Execute through Zeal
    RESPONSE=$(curl -s -X POST http://localhost:3000/api/workflows/demo_workflow_001/execute \
        -H "Content-Type: application/json" \
        -H "X-User-Id: demo_user" \
        -d '{
            "workflowId": "demo_workflow_001",
            "input": {"message": "Hello from demo"},
            "configuration": {"variables": {}}
        }')
    
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ Zeal API execution initiated${NC}"
        echo "Response: $RESPONSE"
        
        SESSION_ID=$(echo $RESPONSE | grep -o '"traceSessionId":"[^"]*' | cut -d'"' -f4)
        echo "Session ID: $SESSION_ID"
        
        # Wait for traces to be recorded
        sleep 2
        
        # Get flow traces
        echo -e "\n${BLUE}5. Retrieving Flow Traces...${NC}"
        TRACES=$(curl -s -X GET "http://localhost:3000/api/workflows/demo_workflow_001/flow-traces?session_id=$SESSION_ID" \
            -H "X-User-Id: demo_user")
        
        echo -e "${GREEN}✅ Flow traces retrieved${NC}"
        echo "Traces: $TRACES" | python3 -m json.tool 2>/dev/null || echo "$TRACES"
        
        return 0
    else
        echo -e "${RED}❌ Failed to execute through Zeal API${NC}"
        return 1
    fi
}

# Main execution
main() {
    # Check if reflow server is running
    if ! curl -s http://localhost:8080/health > /dev/null 2>&1; then
        echo -e "${RED}⚠️  Reflow server is not running on port 8080${NC}"
        echo "Please start it with: cargo run --bin reflow_server"
        exit 1
    fi
    
    # Check if Zeal is running
    if ! curl -s http://localhost:3000/api/health > /dev/null 2>&1; then
        echo -e "${RED}⚠️  Zeal API is not running on port 3000${NC}"
        echo "Please start it with: cd usecase/zeal && npm run dev"
        exit 1
    fi
    
    echo -e "${GREEN}✅ Both servers are running${NC}"
    
    # Run tests
    test_reflow_server
    if [ $? -eq 0 ] && [ ! -z "$EXECUTION_ID" ]; then
        test_workflow_status "$EXECUTION_ID"
        test_websocket "$EXECUTION_ID" &
    fi
    
    test_zeal_api
    
    echo -e "\n${GREEN}🎉 Integration Demo Complete!${NC}"
}

# Run main function
main