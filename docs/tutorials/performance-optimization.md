# Performance Optimization Guide

Advanced techniques for optimizing Reflow workflows and applications.

## Overview

This guide covers comprehensive performance optimization strategies for Reflow applications, from basic configuration tweaks to advanced architectural patterns.

## Performance Analysis

### 1. Profiling Your Application

```rust
use reflow_network::profiling::{ProfileConfig, Profiler, PerformanceMetrics};
use std::time::Instant;

// Enable comprehensive profiling
let profile_config = ProfileConfig {
    enable_memory_tracking: true,
    enable_cpu_profiling: true,
    enable_network_monitoring: true,
    sample_rate: 1000, // Sample every 1000 operations
    output_format: OutputFormat::Json,
};

let profiler = Profiler::new(profile_config);
profiler.start();

// Run your workflow
let start = Instant::now();
network.execute().await?;
let duration = start.elapsed();

// Collect profiling data
let metrics = profiler.stop_and_collect();
println!("Execution time: {:?}", duration);
println!("Memory peak: {:.2} MB", metrics.peak_memory_mb);
println!("CPU utilization: {:.1}%", metrics.avg_cpu_percent);

// Save detailed report
metrics.save_report("performance_report.json")?;
```

### 2. Benchmarking Workflows

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_workflow_variants(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    
    let mut group = c.benchmark_group("workflow_comparison");
    
    // Test different configurations
    for batch_size in [10, 50, 100, 500].iter() {
        group.bench_with_input(
            BenchmarkId::new("batched_workflow", batch_size),
            batch_size,
            |b, &batch_size| {
                b.iter(|| {
                    rt.block_on(async {
                        let network = create_batched_workflow(batch_size).await;
                        black_box(network.execute().await)
                    })
                })
            },
        );
    }
    
    group.finish();
}

criterion_group!(benches, benchmark_workflow_variants);
criterion_main!(benches);
```

## Memory Optimization

### 1. Memory Pool Configuration

```rust
use reflow_network::{MemoryPool, PoolConfig};

// Configure memory pools for different object types
let pool_config = PoolConfig {
    message_pool_size: 10000,
    node_pool_size: 1000, 
    connection_pool_size: 5000,
    enable_auto_scaling: true,
    max_pool_size: 50000,
    cleanup_threshold: 0.8,
};

let memory_pool = MemoryPool::new(pool_config);

// Use pooled objects
let network = Network::with_memory_pool(memory_pool);
```

### 2. Message Optimization

```rust
use reflow_network::{Message, MessageBuilder, CompactMessage};

// Use compact message format for large data
fn create_efficient_message(data: &[u8]) -> Message {
    if data.len() > 1024 {
        // Use compressed format for large payloads
        MessageBuilder::new()
            .compress_payload(true)
            .use_binary_format(true)
            .build_from_bytes(data)
    } else {
        // Use standard format for small payloads
        Message::Binary(data.to_vec())
    }
}

// Implement message recycling
struct MessageCache {
    cache: Vec<Message>,
    max_size: usize,
}

impl MessageCache {
    fn get_or_create(&mut self) -> Message {
        self.cache.pop().unwrap_or_else(|| Message::Null)
    }
    
    fn return_message(&mut self, mut msg: Message) {
        if self.cache.len() < self.max_size {
            // Reset message and return to cache
            msg.clear();
            self.cache.push(msg);
        }
    }
}
```

### 3. Zero-Copy Optimizations

```rust
use std::sync::Arc;
use bytes::Bytes;

// Use reference counting for large shared data
#[derive(Clone)]
struct SharedData {
    inner: Arc<Bytes>,
}

impl SharedData {
    fn new(data: Vec<u8>) -> Self {
        Self {
            inner: Arc::new(Bytes::from(data))
        }
    }
    
    fn as_slice(&self) -> &[u8] {
        &self.inner
    }
}

// Actor implementation with zero-copy semantics
impl Actor for OptimizedProcessor {
    fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
        let mut outputs = HashMap::new();
        
        if let Some(Message::Binary(data)) = inputs.get("input") {
            // Process without copying the data
            let shared_data = SharedData::new(data.clone());
            
            // Pass reference to multiple outputs
            outputs.insert("output1".to_string(), 
                          Message::Custom(Box::new(shared_data.clone())));
            outputs.insert("output2".to_string(), 
                          Message::Custom(Box::new(shared_data)));
        }
        
        Ok(outputs)
    }
}
```

## CPU Optimization

### 1. Parallel Processing

```rust
use rayon::prelude::*;
use tokio::task;

// Parallel data processing
impl Actor for ParallelProcessor {
    fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
        if let Some(Message::Array(items)) = inputs.get("input") {
            // Process items in parallel
            let results: Vec<Message> = items
                .par_iter()
                .map(|item| self.process_item(item))
                .collect();
            
            let mut outputs = HashMap::new();
            outputs.insert("output".to_string(), Message::Array(results));
            Ok(outputs)
        } else {
            Err(ActorError::InvalidInput)
        }
    }
}

// Async parallel processing
async fn process_batch_async(items: Vec<Message>) -> Result<Vec<Message>, ActorError> {
    let tasks: Vec<_> = items.into_iter()
        .map(|item| task::spawn(async move { process_item_async(item).await }))
        .collect();
    
    let mut results = Vec::new();
    for task in tasks {
        results.push(task.await??);
    }
    
    Ok(results)
}
```

### 2. CPU Affinity and Thread Management

```rust
use reflow_network::{ThreadConfig, CpuAffinity};

// Configure thread affinity for specific actors
let thread_config = ThreadConfig {
    worker_threads: num_cpus::get(),
    enable_work_stealing: true,
    cpu_affinity: CpuAffinity::Balanced,
    thread_priority: ThreadPriority::High,
};

// Pin specific actors to dedicated threads
let high_priority_executor = ThreadPoolBuilder::new()
    .num_threads(2)
    .thread_name(|i| format!("high-priority-{}", i))
    .build()?;

network.set_actor_executor("critical_processor", high_priority_executor);
```

### 3. SIMD Optimizations

```rust
use std::simd::{f32x8, SimdFloat};

// SIMD-optimized data processing
fn process_array_simd(data: &mut [f32]) {
    let chunks = data.chunks_exact_mut(8);
    let remainder = chunks.remainder();
    
    for chunk in chunks {
        let vec = f32x8::from_slice(chunk);
        let processed = vec * f32x8::splat(2.0) + f32x8::splat(1.0);
        processed.copy_to_slice(chunk);
    }
    
    // Handle remainder
    for item in remainder {
        *item = *item * 2.0 + 1.0;
    }
}

impl Actor for SIMDProcessor {
    fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
        if let Some(Message::Array(items)) = inputs.get("input") {
            let mut float_data: Vec<f32> = items.iter()
                .filter_map(|item| {
                    if let Message::Float(f) = item {
                        Some(*f as f32)
                    } else {
                        None
                    }
                })
                .collect();
            
            process_array_simd(&mut float_data);
            
            let results: Vec<Message> = float_data.into_iter()
                .map(|f| Message::Float(f as f64))
                .collect();
            
            let mut outputs = HashMap::new();
            outputs.insert("output".to_string(), Message::Array(results));
            Ok(outputs)
        } else {
            Err(ActorError::InvalidInput)
        }
    }
}
```

## Network Optimization

### 1. Connection Pooling

```rust
use reflow_network::{ConnectionPool, PooledConnection};

// HTTP client with connection pooling
struct OptimizedHttpClient {
    pool: ConnectionPool,
    config: HttpConfig,
}

impl OptimizedHttpClient {
    fn new() -> Self {
        let pool = ConnectionPool::builder()
            .max_connections(100)
            .idle_timeout(Duration::from_secs(30))
            .connection_timeout(Duration::from_secs(5))
            .keepalive(true)
            .build();
            
        Self {
            pool,
            config: HttpConfig::default(),
        }
    }
    
    async fn request(&self, url: &str) -> Result<Response, HttpError> {
        let connection = self.pool.get_connection(url).await?;
        let response = connection.request(url).await?;
        
        // Connection is automatically returned to pool
        Ok(response)
    }
}
```

### 2. Batch Network Operations

```rust
use reflow_components::integration::BatchHttpActor;

// Batch multiple HTTP requests
let batch_http = BatchHttpActor::new()
    .batch_size(10)
    .batch_timeout(Duration::from_millis(100))
    .max_concurrent_batches(5)
    .retry_config(RetryConfig {
        max_attempts: 3,
        backoff: BackoffStrategy::Exponential,
        ..Default::default()
    });

// Configure request batching
impl Actor for BatchHttpActor {
    fn process(&mut self, inputs: HashMap<String, Message>) -> Result<HashMap<String, Message>, ActorError> {
        if let Some(Message::Array(urls)) = inputs.get("urls") {
            // Batch requests automatically
            let batched_requests = self.create_batches(urls);
            let futures: Vec<_> = batched_requests.into_iter()
                .map(|batch| self.execute_batch(batch))
                .collect();
            
            // Execute batches concurrently
            let results = futures::future::join_all(futures).await;
            
            let mut outputs = HashMap::new();
            outputs.insert("responses".to_string(), Message::Array(results));
            Ok(outputs)
        } else {
            Err(ActorError::InvalidInput)
        }
    }
}
```

### 3. WebSocket Optimization

```rust
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream};

// Optimized WebSocket handling
struct OptimizedWebSocket {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
    send_buffer: VecDeque<Message>,
    batch_size: usize,
}

impl OptimizedWebSocket {
    async fn send_batched(&mut self) -> Result<(), WebSocketError> {
        if self.send_buffer.len() >= self.batch_size {
            let batch: Vec<_> = self.send_buffer.drain(..).collect();
            let combined_message = self.combine_messages(batch);
            self.stream.send(combined_message).await?;
        }
        Ok(())
    }
    
    fn combine_messages(&self, messages: Vec<Message>) -> tungstenite::Message {
        // Combine multiple messages into a single frame
        let combined_data = messages.into_iter()
            .map(|msg| msg.to_bytes())
            .collect::<Vec<_>>()
            .concat();
        
        tungstenite::Message::Binary(combined_data)
    }
}
```

## I/O Optimization

### 1. Async I/O Best Practices

```rust
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};

// Efficient file processing
async fn process_large_file(path: &str) -> Result<(), std::io::Error> {
    let file = File::open(path).await?;
    let mut reader = BufReader::with_capacity(64 * 1024, file); // 64KB buffer
    
    let output_file = File::create("output.txt").await?;
    let mut writer = BufWriter::with_capacity(64 * 1024, output_file);
    
    let mut buffer = vec![0; 8192]; // 8KB read buffer
    
    loop {
        let bytes_read = reader.read(&mut buffer).await?;
        if bytes_read == 0 {
            break;
        }
        
        // Process data in chunks
        let processed = process_chunk(&buffer[..bytes_read]).await;
        writer.write_all(&processed).await?;
    }
    
    writer.flush().await?;
    Ok(())
}

// Parallel file processing
async fn process_files_parallel(file_paths: Vec<String>) -> Result<(), std::io::Error> {
    let semaphore = Arc::new(Semaphore::new(10)); // Limit concurrent file operations
    
    let tasks: Vec<_> = file_paths.into_iter()
        .map(|path| {
            let sem = semaphore.clone();
            tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                process_large_file(&path).await
            })
        })
        .collect();
    
    futures::future::try_join_all(tasks).await?;
    Ok(())
}
```

### 2. Database Optimization

```rust
use sqlx::{Pool, Postgres, Row};

// Optimized database operations
struct OptimizedDbActor {
    pool: Pool<Postgres>,
    prepared_statements: HashMap<String, String>,
}

impl OptimizedDbActor {
    async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(20)
            .min_connections(5)
            .acquire_timeout(Duration::from_secs(3))
            .idle_timeout(Duration::from_secs(600))
            .max_lifetime(Duration::from_secs(1800))
            .connect(database_url)
            .await?;
        
        Ok(Self {
            pool,
            prepared_statements: HashMap::new(),
        })
    }
    
    async fn batch_insert(&self, records: Vec<Record>) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        
        for chunk in records.chunks(1000) { // Process in batches of 1000
            let query = self.build_batch_insert_query(chunk);
            sqlx::query(&query).execute(&mut *tx).await?;
        }
        
        tx.commit().await?;
        Ok(())
    }
    
    async fn execute_prepared(&self, statement_name: &str, params: &[&dyn sqlx::Encode<'_, Postgres>]) -> Result<Vec<Row>, sqlx::Error> {
        if let Some(sql) = self.prepared_statements.get(statement_name) {
            let mut query = sqlx::query(sql);
            for param in params {
                query = query.bind(param);
            }
            query.fetch_all(&self.pool).await
        } else {
            Err(sqlx::Error::RowNotFound)
        }
    }
}
```

## Workflow-Specific Optimizations

### 1. Pipeline Optimization

```rust
// Optimized pipeline with backpressure
use tokio::sync::mpsc;

struct OptimizedPipeline {
    stages: Vec<Box<dyn Actor>>,
    buffer_sizes: Vec<usize>,
    channels: Vec<mpsc::Sender<Message>>,
}

impl OptimizedPipeline {
    fn new() -> Self {
        Self {
            stages: Vec::new(),
            buffer_sizes: Vec::new(),
            channels: Vec::new(),
        }
    }
    
    fn add_stage(&mut self, actor: Box<dyn Actor>, buffer_size: usize) {
        self.stages.push(actor);
        self.buffer_sizes.push(buffer_size);
        
        let (tx, rx) = mpsc::channel(buffer_size);
        self.channels.push(tx);
    }
    
    async fn execute_with_backpressure(&mut self, input: Message) -> Result<Message, ActorError> {
        let mut current_message = input;
        
        for (i, stage) in self.stages.iter_mut().enumerate() {
            // Apply backpressure using channel capacity
            if let Some(tx) = self.channels.get(i) {
                tx.send(current_message.clone()).await
                    .map_err(|_| ActorError::ChannelClosed)?;
            }
            
            let inputs = HashMap::from([("input".to_string(), current_message)]);
            let outputs = stage.process(inputs)?;
            
            current_message = outputs.get("output")
                .ok_or(ActorError::MissingOutput)?
                .clone();
        }
        
        Ok(current_message)
    }
}
```

### 2. Dynamic Load Balancing

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

struct LoadBalancer {
    workers: Vec<Box<dyn Actor>>,
    load_counters: Vec<AtomicUsize>,
    strategy: LoadBalanceStrategy,
}

impl LoadBalancer {
    fn select_worker(&self) -> usize {
        match self.strategy {
            LoadBalanceStrategy::RoundRobin => {
                static COUNTER: AtomicUsize = AtomicUsize::new(0);
                COUNTER.fetch_add(1, Ordering::Relaxed) % self.workers.len()
            }
            LoadBalanceStrategy::LeastLoaded => {
                self.load_counters
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, counter)| counter.load(Ordering::Relaxed))
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            }
            LoadBalanceStrategy::WeightedRoundRobin => {
                // Implement weighted selection based on worker capacity
                self.select_weighted_worker()
            }
        }
    }
    
    fn update_load_metrics(&self, worker_index: usize, processing_time: Duration) {
        // Update load metrics for adaptive load balancing
        let load_score = self.calculate_load_score(processing_time);
        self.load_counters[worker_index].store(load_score, Ordering::Relaxed);
    }
}
```

## Monitoring and Optimization

### 1. Real-time Metrics

```rust
use prometheus::{Counter, Histogram, Gauge, register_counter, register_histogram, register_gauge};

struct PerformanceMonitor {
    message_counter: Counter,
    processing_time: Histogram,
    memory_usage: Gauge,
    active_connections: Gauge,
}

impl PerformanceMonitor {
    fn new() -> Self {
        Self {
            message_counter: register_counter!("reflow_messages_total", "Total messages processed").unwrap(),
            processing_time: register_histogram!("reflow_processing_duration_seconds", "Processing time in seconds").unwrap(),
            memory_usage: register_gauge!("reflow_memory_usage_bytes", "Memory usage in bytes").unwrap(),
            active_connections: register_gauge!("reflow_active_connections", "Number of active connections").unwrap(),
        }
    }
    
    fn record_message_processed(&self, processing_time: Duration) {
        self.message_counter.inc();
        self.processing_time.observe(processing_time.as_secs_f64());
    }
    
    fn update_memory_usage(&self, bytes: u64) {
        self.memory_usage.set(bytes as f64);
    }
    
    async fn collect_system_metrics(&self) {
        if let Some(usage) = memory_stats::memory_stats() {
            self.update_memory_usage(usage.physical_mem as u64);
        }
        
        // Collect other system metrics
        let cpu_usage = get_cpu_usage().await;
        // ... record other metrics
    }
}
```

### 2. Adaptive Optimization

```rust
struct AdaptiveOptimizer {
    performance_history: VecDeque<PerformanceSnapshot>,
    optimization_strategies: Vec<Box<dyn OptimizationStrategy>>,
    current_config: OptimizationConfig,
}

impl AdaptiveOptimizer {
    async fn optimize_based_on_metrics(&mut self, current_metrics: &PerformanceMetrics) {
        let snapshot = PerformanceSnapshot {
            timestamp: std::time::Instant::now(),
            metrics: current_metrics.clone(),
            config: self.current_config.clone(),
        };
        
        self.performance_history.push_back(snapshot);
        if self.performance_history.len() > 100 {
            self.performance_history.pop_front();
        }
        
        // Analyze trends and apply optimizations
        if let Some(optimization) = self.analyze_and_suggest_optimization() {
            self.apply_optimization(optimization).await;
        }
    }
    
    fn analyze_and_suggest_optimization(&self) -> Option<OptimizationAction> {
        // Machine learning-based optimization suggestions
        let trend_analyzer = TrendAnalyzer::new(&self.performance_history);
        
        if trend_analyzer.detect_memory_pressure() {
            Some(OptimizationAction::ReduceMemoryUsage)
        } else if trend_analyzer.detect_cpu_bottleneck() {
            Some(OptimizationAction::IncreaseParallelism)
        } else if trend_analyzer.detect_io_bottleneck() {
            Some(OptimizationAction::OptimizeIo)
        } else {
            None
        }
    }
}
```

## Platform-Specific Optimizations

### 1. Linux-Specific Optimizations

```rust
#[cfg(target_os = "linux")]
mod linux_optimizations {
    use libc::{sched_setaffinity, cpu_set_t, CPU_SET, CPU_ZERO};
    
    pub fn set_cpu_affinity(thread_id: u32, cpu_cores: &[usize]) -> Result<(), std::io::Error> {
        unsafe {
            let mut cpuset: cpu_set_t = std::mem::zeroed();
            CPU_ZERO(&mut cpuset);
            
            for &core in cpu_cores {
                CPU_SET(core, &mut cpuset);
            }
            
            let result = sched_setaffinity(
                thread_id, 
                std::mem::size_of::<cpu_set_t>(), 
                &cpuset
            );
            
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
    }
    
    pub fn configure_memory_policy() {
        // Configure NUMA memory policy for optimal performance
        use libc::{mbind, MPOL_BIND};
        // Implementation details...
    }
}
```

### 2. macOS-Specific Optimizations

```rust
#[cfg(target_os = "macos")]
mod macos_optimizations {
    use std::ffi::CString;
    use libc::{pthread_t, pthread_self, thread_policy_set, THREAD_PRECEDENCE_POLICY};
    
    pub fn set_thread_priority(priority: i32) -> Result<(), std::io::Error> {
        unsafe {
            let thread = pthread_self();
            let policy = THREAD_PRECEDENCE_POLICY;
            
            let result = thread_policy_set(
                thread as *mut _,
                policy,
                &priority as *const _ as *const _,
                1
            );
            
            if result == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        }
    }
}
```

## Best Practices Summary

### 1. General Optimization Principles

- **Measure First**: Always profile before optimizing
- **Optimize Bottlenecks**: Focus on the slowest components
- **Cache Wisely**: Cache expensive computations, not cheap ones
- **Batch Operations**: Group similar operations together
- **Use Appropriate Data Structures**: Choose the right tool for the job

### 2. Memory Management

- **Pool Resources**: Use object pools for frequently allocated items
- **Minimize Allocations**: Reuse buffers and data structures
- **Compress Large Data**: Use compression for large payloads
- **Monitor Memory Usage**: Track allocation patterns

### 3. Concurrency and Parallelism

- **Match Threading to Workload**: CPU-bound vs I/O-bound considerations
- **Avoid Lock Contention**: Use lock-free data structures when possible
- **Balance Load**: Distribute work evenly across threads
- **Handle Backpressure**: Prevent memory exhaustion in pipelines

### 4. Network and I/O

- **Connection Pooling**: Reuse network connections
- **Batch Network Operations**: Reduce round-trip overhead
- **Async I/O**: Use non-blocking I/O operations
- **Buffer Sizing**: Optimize buffer sizes for your workload

## Troubleshooting Performance Issues

### Common Performance Problems

1. **Memory Leaks**: Use memory profilers to identify leaks
2. **CPU Hotspots**: Profile CPU usage to find bottlenecks
3. **Lock Contention**: Monitor lock wait times
4. **I/O Blocking**: Identify blocking I/O operations
5. **Network Latency**: Measure network round-trip times

### Performance Testing

```rust
#[cfg(test)]
mod performance_tests {
    use super::*;
    use criterion::{criterion_group, criterion_main, Criterion};
    
    fn benchmark_workflow_throughput(c: &mut Criterion) {
        c.bench_function("workflow_1000_messages", |b| {
            b.iter(|| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let network = create_test_network().await;
                    let messages = create_test_messages(1000);
                    
                    let start = std::time::Instant::now();
                    for message in messages {
                        network.process_message(message).await.unwrap();
                    }
                    start.elapsed()
                })
            })
        });
    }
    
    criterion_group!(benches, benchmark_workflow_throughput);
    criterion_main!(benches);
}
```

## Next Steps

- [Architecture Overview](../architecture/overview.md) - Understanding Reflow's architecture
- [Advanced Features](../api/graph/advanced.md) - Advanced graph operations
- [Troubleshooting Guide](../reference/troubleshooting-guide.md) - Common issues and solutions
- [Deployment Guide](../deployment/native-deployment.md) - Production deployment strategies
