# MicroSandbox Deployment Guide for Reflow

## Architecture Overview

```
Reflow Actors → WebSocket RPC → MicroSandbox Server → Docker Containers → Scripts
                     ↓                    ↓
                  Bitcode            Redis State
                Serialization         Persistence
```

## Quick Start

### 1. Start MicroSandbox Infrastructure

```bash
# Start Redis and MicroSandbox servers
docker-compose up -d

# Verify services are running
docker-compose ps

# Check logs
docker-compose logs -f microsandbox
```

### 2. Test MicroSandbox RPC

```bash
# Test WebSocket RPC connection
wscat -c ws://localhost:5556

# Send test RPC message
{"jsonrpc":"2.0","method":"list","params":{},"id":1}
```

### 3. Configure Reflow to Use MicroSandbox

```rust
// In your Reflow configuration
let rpc_client = Arc::new(WebSocketRpcClient::new(
    "ws://localhost:5556".to_string()  // Or use load balancer at 8080
));

let actor = WebSocketScriptActor::new(
    metadata,
    rpc_client,
    "redis://localhost:6379".to_string(),
).await;
```

## Scaling Strategy

### Horizontal Scaling

```yaml
# docker-compose.yml
microsandbox:
  deploy:
    replicas: 5  # Scale to 5 instances
```

```bash
# Scale dynamically
docker-compose up -d --scale microsandbox=10
```

### Load Balancing

The nginx configuration provides:
- Round-robin load balancing for HTTP API
- Least-connections for WebSocket RPC
- Sticky sessions for actor affinity

## Message Format & Compression

### Reflow → MicroSandbox

Messages use Reflow's bitcode serialization with compression:

```rust
// Reflow Message format
Message::Stream(data) → bitcode::encode() → zstd::compress() → WebSocket
```

### MicroSandbox → Scripts

Scripts receive decompressed JSON representation:

```python
# In Python script
def process(context):
    # Input already decompressed and deserialized
    data = context.get_input("data")  # Returns native Python types
    
    # Output will be serialized and compressed
    await context.send_output("result", processed_data)
```

## Compression Compatibility

| Component | Serialization | Compression | Threshold |
|-----------|--------------|-------------|-----------|
| Reflow Native | Bitcode | ZSTD/LZ4 | 1KB |
| WebSocket RPC | JSON | ZSTD | 1KB |
| Redis State | JSON | ZSTD | 1KB |
| Script Context | JSON | ZSTD | 1KB |

## Performance Tuning

### 1. Pre-warm Sandboxes

```yaml
# microsandbox.yaml
sandbox:
  pool:
    python:
      min_instances: 10  # Pre-warm 10 Python sandboxes
      max_instances: 100
    nodejs:
      min_instances: 10
      max_instances: 100
```

### 2. Redis Optimization

```bash
# Configure Redis for better performance
redis-cli CONFIG SET maxmemory 2gb
redis-cli CONFIG SET maxmemory-policy allkeys-lru
```

### 3. Resource Limits

```yaml
# Per-sandbox limits
MICROSANDBOX_SANDBOX_MEMORY_MB: 256
MICROSANDBOX_SANDBOX_CPU_LIMIT: 0.5
```

## Monitoring

### Metrics Endpoint

```bash
# Prometheus metrics
curl http://localhost:9090/metrics

# Key metrics to watch:
# - microsandbox_active_sandboxes
# - microsandbox_execution_duration_seconds
# - microsandbox_memory_usage_bytes
# - microsandbox_compression_ratio
```

### Health Checks

```bash
# Check MicroSandbox health
curl http://localhost:5555/health

# Check Redis
redis-cli ping

# Check load balancer
curl http://localhost:8080/health
```

## Production Deployment

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: microsandbox
spec:
  replicas: 10
  selector:
    matchLabels:
      app: microsandbox
  template:
    metadata:
      labels:
        app: microsandbox
    spec:
      containers:
      - name: microsandbox
        image: reflow/microsandbox:latest
        ports:
        - containerPort: 5555
        - containerPort: 5556
        env:
        - name: MICROSANDBOX_REDIS_URL
          value: redis://redis-service:6379
        resources:
          requests:
            memory: "2Gi"
            cpu: "1"
          limits:
            memory: "4Gi"
            cpu: "2"
---
apiVersion: v1
kind: Service
metadata:
  name: microsandbox-service
spec:
  selector:
    app: microsandbox
  ports:
  - name: http
    port: 5555
  - name: websocket
    port: 5556
  type: LoadBalancer
```

### Docker Swarm

```bash
# Deploy as a stack
docker stack deploy -c docker-compose.yml reflow

# Scale service
docker service scale reflow_microsandbox=20
```

## Security Considerations

1. **Network Isolation**: MicroSandbox containers run in isolated network
2. **Resource Limits**: Each sandbox has CPU/memory limits
3. **No Filesystem Access**: Scripts cannot access host filesystem
4. **Redis ACLs**: Configure Redis with proper authentication

```bash
# Set Redis password
redis-cli CONFIG SET requirepass "your-secure-password"

# Update connection strings
MICROSANDBOX_REDIS_URL=redis://:your-secure-password@redis:6379
```

## Troubleshooting

### Common Issues

1. **Connection Refused**
   ```bash
   # Check if services are running
   docker-compose ps
   netstat -an | grep 5556
   ```

2. **Compression Errors**
   ```bash
   # Verify compression settings match
   echo $MICROSANDBOX_COMPRESSION_ALGORITHM  # Should be "zstd"
   ```

3. **Memory Issues**
   ```bash
   # Check Docker memory limits
   docker stats
   
   # Increase limits if needed
   docker update --memory 8g microsandbox
   ```

## Testing Scripts

### Example Python Script

```python
# test_actor.py
@actor
async def process(context):
    # Test compression with large data
    large_data = "x" * 10000  # Will be compressed
    await context.send_output("test", large_data)
    
    # Test Redis state
    counter = await context.get_state("counter", 0)
    await context.set_state("counter", counter + 1)
    
    return {"status": "success", "counter": counter}
```

### Load Test

```bash
# Install artillery for load testing
npm install -g artillery

# Create test scenario
cat > load-test.yml << EOF
config:
  target: "ws://localhost:5556"
  phases:
    - duration: 60
      arrivalRate: 10
scenarios:
  - engine: ws
    flow:
      - send: '{"jsonrpc":"2.0","method":"process","params":{"actor_id":"test"},"id":1}'
EOF

# Run load test
artillery run load-test.yml
```

## Best Practices

1. **Use Compression Wisely**: Only compress messages > 1KB
2. **Pool Connections**: Reuse WebSocket connections
3. **Monitor Redis Memory**: Set appropriate eviction policies
4. **Pre-warm Sandboxes**: Reduce cold start latency
5. **Use Load Balancer**: Distribute load across instances
6. **Enable Metrics**: Monitor performance and resource usage

## Conclusion

This setup provides:
- ✅ Scalable script execution
- ✅ Redis-backed state persistence
- ✅ Compatible compression with Reflow's bitcode
- ✅ Real-time bidirectional communication via RPC
- ✅ Load balancing for high availability
- ✅ Resource isolation and limits