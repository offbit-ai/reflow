#!/bin/bash

# Quick demo test for Zeal-Reflow integration

echo "🚀 Testing Zeal-Reflow Integration"
echo "=================================="

# Test direct reflow server
echo -e "\n1️⃣  Testing direct reflow server call..."
curl -X POST http://localhost:8080/api/execute-zeal \
  -H "Content-Type: application/json" \
  -d '{
    "workflow": {
      "id": "demo_001",
      "name": "Demo Workflow",
      "graphs": [{
        "id": "main",
        "name": "Main",
        "nodes": [{
          "id": "http_1",
          "name": "HTTP Request",
          "template_id": "tpl_http_request",
          "configuration": {
            "url": "https://jsonplaceholder.typicode.com/posts/1",
            "method": "GET"
          }
        }],
        "connections": []
      }]
    },
    "input": {"demo": true}
  }' | python3 -m json.tool

echo -e "\n✅ Reflow server test complete"

# Get execution status
echo -e "\n2️⃣  Checking workflow status..."
curl -s http://localhost:8080/api/status | python3 -m json.tool

echo -e "\n✨ Demo complete!"