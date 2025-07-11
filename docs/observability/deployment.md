# Production Deployment

This guide covers deploying Reflow's observability framework in production environments, including scalability considerations, security best practices, and operational procedures.

## Architecture Overview

### Production Architecture

```mermaid
graph TB
    App1[Reflow App 1] --> LB[Load Balancer]
    App2[Reflow App 2] --> LB
    App3[Reflow App N] --> LB
    
    LB --> TS1[Tracing Server 1]
    LB --> TS2[Tracing Server 2]
    
    TS1 --> DB[(PostgreSQL Primary)]
    TS2 --> DB
    
    DB --> Replica[(PostgreSQL Replica)]
    
    TS1 --> Cache[(Redis Cache)]
    TS2 --> Cache
    
    Grafana[Grafana] --> DB
    Grafana --> Cache
    
    Monitor[Monitoring] --> TS1
    Monitor --> TS2
    Monitor --> DB
```

### Component Responsibilities

- **Reflow Applications**: Generate trace events
- **Load Balancer**: Distribute connections across tracing servers
- **Tracing Servers**: Receive, process, and store trace data
- **PostgreSQL**: Primary data storage with replication
- **Redis**: Caching and real-time data
- **Grafana**: Visualization and dashboards
- **Monitoring**: Health checks and alerting

## Infrastructure Requirements

### Minimum Production Setup

**Tracing Server**:
- CPU: 2 cores
- Memory: 4GB RAM
- Storage: 50GB SSD
- Network: 1Gbps

**Database (PostgreSQL)**:
- CPU: 4 cores
- Memory: 8GB RAM
- Storage: 200GB SSD (for data) + 100GB (for WAL)
- Network: 1Gbps

**Cache (Redis)**:
- CPU: 2 cores
- Memory: 4GB RAM
- Storage: 20GB SSD
- Network: 1Gbps

### High-Scale Production Setup

**Tracing Server Cluster**:
- 3+ instances
- CPU: 8 cores each
- Memory: 16GB RAM each
- Storage: 100GB SSD each
- Network: 10Gbps

**Database Cluster**:
- Primary + 2 replicas
- CPU: 16 cores each
- Memory: 64GB RAM each
- Storage: 1TB NVMe SSD each
- Network: 10Gbps

**Cache Cluster**:
- 3 instance Redis cluster
- CPU: 4 cores each
- Memory: 16GB RAM each
- Storage: 50GB SSD each
- Network: 10Gbps

## Container Deployment

### Docker Compose

```yaml
# docker-compose.prod.yml
version: '3.8'

services:
  tracing-server:
    image: reflow/tracing-server:latest
    deploy:
      replicas: 3
      resources:
        limits:
          cpus: '4'
          memory: 8G
        reservations:
          cpus: '2'
          memory: 4G
    environment:
      - RUST_LOG=info
      - TRACING_DATABASE_URL=postgresql://user:pass@postgres:5432/tracing
      - TRACING_REDIS_URL=redis://redis:6379
      - TRACING_BIND_ADDRESS=0.0.0.0:8080
      - TRACING_MAX_CONNECTIONS=1000
    ports:
      - "8080:8080"
    networks:
      - tracing-network
    depends_on:
      - postgres
      - redis
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  postgres:
    image: postgres:15
    environment:
      - POSTGRES_DB=tracing
      - POSTGRES_USER=tracing_user
      - POSTGRES_PASSWORD_FILE=/run/secrets/postgres_password
      - POSTGRES_INITDB_ARGS=--auth-host=scram-sha-256
    volumes:
      - postgres_data:/var/lib/postgresql/data
      - ./init.sql:/docker-entrypoint-initdb.d/init.sql
    ports:
      - "5432:5432"
    networks:
      - tracing-network
    secrets:
      - postgres_password
    command: postgres -c shared_preload_libraries=pg_stat_statements
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U tracing_user -d tracing"]
      interval: 30s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7-alpine
    volumes:
      - redis_data:/data
    ports:
      - "6379:6379"
    networks:
      - tracing-network
    command: redis-server --appendonly yes --maxmemory 2gb --maxmemory-policy allkeys-lru
    healthcheck:
      test: ["CMD", "redis-cli", "ping"]
      interval: 30s
      timeout: 5s
      retries: 3

  nginx:
    image: nginx:alpine
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf
    ports:
      - "80:80"
      - "443:443"
    networks:
      - tracing-network
    depends_on:
      - tracing-server

volumes:
  postgres_data:
  redis_data:

networks:
  tracing-network:
    driver: overlay

secrets:
  postgres_password:
    external: true
```

### Kubernetes Deployment

```yaml
# tracing-server-deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: tracing-server
  labels:
    app: tracing-server
spec:
  replicas: 3
  selector:
    matchLabels:
      app: tracing-server
  template:
    metadata:
      labels:
        app: tracing-server
    spec:
      containers:
      - name: tracing-server
        image: reflow/tracing-server:latest
        ports:
        - containerPort: 8080
        env:
        - name: RUST_LOG
          value: "info"
        - name: TRACING_DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: tracing-secrets
              key: database-url
        - name: TRACING_REDIS_URL
          value: "redis://redis:6379"
        resources:
          requests:
            memory: "2Gi"
            cpu: "1000m"
          limits:
            memory: "4Gi"
            cpu: "2000m"
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 30
          periodSeconds: 10
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 5

---
apiVersion: v1
kind: Service
metadata:
  name: tracing-server
spec:
  selector:
    app: tracing-server
  ports:
  - protocol: TCP
    port: 8080
    targetPort: 8080
  type: ClusterIP

---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: tracing-server-ingress
  annotations:
    nginx.ingress.kubernetes.io/rewrite-target: /
    nginx.ingress.kubernetes.io/ssl-redirect: "true"
spec:
  tls:
  - hosts:
    - tracing.yourdomain.com
    secretName: tracing-tls
  rules:
  - host: tracing.yourdomain.com
    http:
      paths:
      - path: /
        pathType: Prefix
        backend:
          service:
            name: tracing-server
            port:
              number: 8080
```

### PostgreSQL Configuration

```yaml
# postgres-deployment.yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres
spec:
  serviceName: postgres
  replicas: 1
  selector:
    matchLabels:
      app: postgres
  template:
    metadata:
      labels:
        app: postgres
    spec:
      containers:
      - name: postgres
        image: postgres:15
        env:
        - name: POSTGRES_DB
          value: tracing
        - name: POSTGRES_USER
          value: tracing_user
        - name: POSTGRES_PASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-secrets
              key: password
        ports:
        - containerPort: 5432
        volumeMounts:
        - name: postgres-storage
          mountPath: /var/lib/postgresql/data
        - name: postgres-config
          mountPath: /etc/postgresql/postgresql.conf
          subPath: postgresql.conf
        resources:
          requests:
            memory: "4Gi"
            cpu: "2000m"
          limits:
            memory: "8Gi"
            cpu: "4000m"
      volumes:
      - name: postgres-config
        configMap:
          name: postgres-config
  volumeClaimTemplates:
  - metadata:
      name: postgres-storage
    spec:
      accessModes: ["ReadWriteOnce"]
      resources:
        requests:
          storage: 200Gi
      storageClassName: fast-ssd
```

## Configuration Management

### Environment-Specific Configuration

```toml
# config/production.toml
[server]
bind_address = "0.0.0.0:8080"
max_connections = 1000
worker_threads = 8
keep_alive_timeout = 30

[database]
url = "postgresql://user:pass@postgres-cluster:5432/tracing"
max_connections = 20
min_connections = 5
connection_timeout = 5000
statement_timeout = 30000

[redis]
url = "redis://redis-cluster:6379"
pool_size = 10
connection_timeout = 3000

[tracing]
batch_size = 100
batch_timeout_ms = 2000
max_event_size = 1048576  # 1MB
compression = true

[logging]
level = "info"
format = "json"
target = "stdout"

[metrics]
enabled = true
bind_address = "0.0.0.0:9090"
```

### Secret Management

```bash
# Kubernetes secrets
kubectl create secret generic tracing-secrets \
  --from-literal=database-url="postgresql://user:pass@postgres:5432/tracing" \
  --from-literal=redis-url="redis://redis:6379" \
  --from-literal=jwt-secret="your-jwt-secret"

kubectl create secret generic postgres-secrets \
  --from-literal=password="secure-postgres-password"

# Docker secrets
echo "secure-postgres-password" | docker secret create postgres_password -
```

## Security Configuration

### TLS/SSL Setup

```nginx
# nginx.conf
events {
    worker_connections 1024;
}

http {
    upstream tracing_backend {
        server tracing-server:8080;
        keepalive 32;
    }

    server {
        listen 80;
        return 301 https://$server_name$request_uri;
    }

    server {
        listen 443 ssl http2;
        server_name tracing.yourdomain.com;

        ssl_certificate /etc/ssl/certs/tracing.crt;
        ssl_certificate_key /etc/ssl/private/tracing.key;
        ssl_protocols TLSv1.2 TLSv1.3;
        ssl_ciphers ECDHE-RSA-AES256-GCM-SHA512:DHE-RSA-AES256-GCM-SHA512;

        location / {
            proxy_pass http://tracing_backend;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
    }
}
```

### Authentication Configuration

```rust
// Server configuration with authentication
use reflow_tracing::auth::{AuthConfig, JwtAuth};

let auth_config = AuthConfig {
    jwt_secret: env::var("JWT_SECRET")?,
    token_expiry: Duration::from_hours(24),
    issuer: "reflow-tracing".to_string(),
    audience: "reflow-clients".to_string(),
};

let server_config = ServerConfig {
    auth: Some(auth_config),
    require_auth: true,
    ..Default::default()
};
```

### Network Security

```yaml
# Network policies
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: tracing-network-policy
spec:
  podSelector:
    matchLabels:
      app: tracing-server
  policyTypes:
  - Ingress
  - Egress
  ingress:
  - from:
    - podSelector:
        matchLabels:
          app: reflow-client
    ports:
    - protocol: TCP
      port: 8080
  egress:
  - to:
    - podSelector:
        matchLabels:
          app: postgres
    ports:
    - protocol: TCP
      port: 5432
  - to:
    - podSelector:
        matchLabels:
          app: redis
    ports:
    - protocol: TCP
      port: 6379
```

## Monitoring and Observability

### Prometheus Metrics

```yaml
# prometheus-config.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: prometheus-config
data:
  prometheus.yml: |
    global:
      scrape_interval: 15s
    
    scrape_configs:
    - job_name: 'tracing-server'
      static_configs:
      - targets: ['tracing-server:9090']
      metrics_path: /metrics
      scrape_interval: 10s
    
    - job_name: 'postgres'
      static_configs:
      - targets: ['postgres-exporter:9187']
    
    - job_name: 'redis'
      static_configs:
      - targets: ['redis-exporter:9121']
```

### Health Checks

```rust
// Health check endpoints
use warp::Filter;

let health = warp::path("health")
    .and(warp::get())
    .map(|| {
        // Check database connectivity
        // Check Redis connectivity
        // Check disk space
        warp::reply::json(&json!({
            "status": "healthy",
            "timestamp": Utc::now(),
            "checks": {
                "database": "ok",
                "redis": "ok",
                "disk_space": "ok"
            }
        }))
    });

let ready = warp::path("ready")
    .and(warp::get())
    .map(|| {
        // Check if server is ready to accept traffic
        warp::reply::json(&json!({
            "status": "ready",
            "timestamp": Utc::now()
        }))
    });
```

### Alerting Rules

```yaml
# alerting-rules.yaml
groups:
- name: tracing-server
  rules:
  - alert: TracingServerDown
    expr: up{job="tracing-server"} == 0
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "Tracing server is down"
      description: "Tracing server {{ $labels.instance }} has been down for more than 5 minutes"

  - alert: HighLatency
    expr: tracing_request_duration_seconds{quantile="0.95"} > 0.5
    for: 10m
    labels:
      severity: warning
    annotations:
      summary: "High latency detected"
      description: "95th percentile latency is {{ $value }}s"

  - alert: HighErrorRate
    expr: rate(tracing_requests_total{status="error"}[5m]) > 0.1
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "High error rate"
      description: "Error rate is {{ $value }} requests/second"

  - alert: DatabaseConnectionsHigh
    expr: pg_stat_activity_count > 80
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High number of database connections"
      description: "{{ $value }} active connections to PostgreSQL"
```

## Performance Tuning

### PostgreSQL Optimization

```sql
-- postgresql.conf optimizations
shared_buffers = 2GB                    # 25% of RAM
effective_cache_size = 6GB              # 75% of RAM
maintenance_work_mem = 512MB
work_mem = 16MB
max_connections = 200
wal_buffers = 16MB
checkpoint_completion_target = 0.9
random_page_cost = 1.1                  # For SSDs
effective_io_concurrency = 200          # For SSDs

-- Enable query logging for optimization
log_min_duration_statement = 1000       # Log queries > 1s
log_checkpoints = on
log_connections = on
log_disconnections = on
log_lock_waits = on
```

### Redis Optimization

```conf
# redis.conf
maxmemory 4gb
maxmemory-policy allkeys-lru
save 900 1
save 300 10
save 60 10000
tcp-keepalive 300
tcp-backlog 511
```

### Application Tuning

```rust
// Server configuration for high performance
let config = ServerConfig {
    worker_threads: num_cpus::get(),
    max_connections: 1000,
    connection_pool_size: 20,
    batch_size: 200,
    batch_timeout: Duration::from_millis(5000),
    compression: true,
    buffer_size: 65536,
    ..Default::default()
};
```

## Backup and Recovery

### Database Backup

```bash
#!/bin/bash
# backup.sh
BACKUP_DIR="/backups"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
DB_NAME="tracing"

# Create backup
pg_dump -h postgres -U tracing_user -d $DB_NAME | gzip > "$BACKUP_DIR/tracing_$TIMESTAMP.sql.gz"

# Upload to S3
aws s3 cp "$BACKUP_DIR/tracing_$TIMESTAMP.sql.gz" s3://your-backup-bucket/database/

# Cleanup old backups (keep 30 days)
find $BACKUP_DIR -name "tracing_*.sql.gz" -mtime +30 -delete
```

### Automated Backup with CronJob

```yaml
# backup-cronjob.yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: postgres-backup
spec:
  schedule: "0 2 * * *"  # Daily at 2 AM
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: postgres-backup
            image: postgres:15
            env:
            - name: PGPASSWORD
              valueFrom:
                secretKeyRef:
                  name: postgres-secrets
                  key: password
            command:
            - /bin/bash
            - -c
            - |
              pg_dump -h postgres -U tracing_user tracing | gzip > /backup/tracing_$(date +%Y%m%d_%H%M%S).sql.gz
              # Upload to cloud storage
              aws s3 cp /backup/tracing_*.sql.gz s3://backup-bucket/
            volumeMounts:
            - name: backup-storage
              mountPath: /backup
          volumes:
          - name: backup-storage
            persistentVolumeClaim:
              claimName: backup-pvc
          restartPolicy: OnFailure
```

### Disaster Recovery

```bash
#!/bin/bash
# restore.sh
BACKUP_FILE=$1

if [ -z "$BACKUP_FILE" ]; then
    echo "Usage: $0 <backup_file>"
    exit 1
fi

# Download backup from S3
aws s3 cp "s3://your-backup-bucket/database/$BACKUP_FILE" ./

# Restore database
gunzip -c "$BACKUP_FILE" | psql -h postgres -U tracing_user -d tracing

echo "Database restored from $BACKUP_FILE"
```

## Scaling Strategies

### Horizontal Scaling

```yaml
# hpa.yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: tracing-server-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: tracing-server
  minReplicas: 3
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

### Database Scaling

```sql
-- Read replicas configuration
-- Primary server
ALTER SYSTEM SET wal_level = replica;
ALTER SYSTEM SET max_wal_senders = 3;
ALTER SYSTEM SET max_replication_slots = 3;
SELECT pg_reload_conf();

-- Create replication slot
SELECT pg_create_physical_replication_slot('replica_1');

-- Replica server setup
standby_mode = 'on'
primary_conninfo = 'host=postgres-primary port=5432 user=replicator'
```

## Maintenance Procedures

### Rolling Updates

```bash
#!/bin/bash
# rolling-update.sh
kubectl set image deployment/tracing-server tracing-server=reflow/tracing-server:v2.0.0
kubectl rollout status deployment/tracing-server
kubectl rollout history deployment/tracing-server
```

### Database Maintenance

```sql
-- Regular maintenance tasks
VACUUM ANALYZE tracing.events;
VACUUM ANALYZE tracing.traces;
REINDEX INDEX CONCURRENTLY idx_events_timestamp;

-- Partition maintenance
SELECT create_monthly_partitions('tracing.events', '2024-01-01'::date);
SELECT drop_old_partitions('tracing.events', interval '90 days');
```

### Log Rotation

```yaml
# fluent-bit-config.yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: fluent-bit-config
data:
  fluent-bit.conf: |
    [INPUT]
        Name tail
        Path /var/log/containers/*tracing-server*.log
        Parser docker
        Tag tracing.*
    
    [OUTPUT]
        Name es
        Match tracing.*
        Host elasticsearch.logging.svc.cluster.local
        Port 9200
        Index tracing-logs
        Type _doc
```

## Troubleshooting

### Common Issues

**High Memory Usage**:
```bash
# Check memory usage
kubectl top pods
kubectl describe pod tracing-server-xxx

# Adjust memory limits
kubectl patch deployment tracing-server -p '{"spec":{"template":{"spec":{"containers":[{"name":"tracing-server","resources":{"limits":{"memory":"8Gi"}}}]}}}}'
```

**Database Connection Issues**:
```sql
-- Check active connections
SELECT count(*) FROM pg_stat_activity;

-- Kill long-running queries
SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE state = 'active' AND query_start < NOW() - INTERVAL '10 minutes';
```

**Performance Issues**:
```bash
# Check metrics
curl http://tracing-server:9090/metrics | grep -E "latency|throughput|errors"

# Scale up
kubectl scale deployment tracing-server --replicas=10
```

This production deployment guide provides a comprehensive foundation for running Reflow's observability framework at scale with proper security, monitoring, and operational procedures.
