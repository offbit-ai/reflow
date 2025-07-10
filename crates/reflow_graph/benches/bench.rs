use criterion::{black_box, criterion_group, criterion_main, Criterion};
use reflow_graph::Graph;
use std::collections::HashMap;

pub fn benchmark_graph_operations(c: &mut Criterion) {
    // Node Operations Benchmark
    c.bench_function("add_1000_nodes", |b| {
        b.iter(|| {
            let mut graph = Graph::new("test", false, None);
            for i in 0..1000 {
                graph.add_node(&format!("node_{}", i), "test", None);
            }
        });
    });

    // Connection Operations Benchmark
    c.bench_function("add_1000_connections", |b| {
        b.iter(|| {
            let mut graph = Graph::new("test", false, None);
            // Setup nodes first
            for i in 0..100 {
                graph.add_node(&format!("node_{}", i), "test", None);
            }
            // Add connections
            for i in 0..99 {
                graph.add_connection(
                    &format!("node_{}", i),
                    "out",
                    &format!("node_{}", i + 1),
                    "in",
                    None,
                );
            }
        });
    });

    // Group Operations Benchmark
    c.bench_function("group_operations", |b| {
        b.iter(|| {
            let mut graph = Graph::new("test", false, None);
            // Setup nodes
            for i in 0..100 {
                graph.add_node(&format!("node_{}", i), "test", None);
            }
            // Create groups and add nodes
            for i in 0..10 {
                let nodes = (0..10)
                    .map(|j| format!("node_{}", i * 10 + j))
                    .collect::<Vec<_>>();
                graph.add_group(&format!("group_{}", i), nodes, None);
            }
        });
    });

    // Mixed Operations Benchmark
    c.bench_function("mixed_operations", |b| {
        b.iter(|| {
            let mut graph = Graph::new("test", false, None);
            // Add nodes
            for i in 0..100 {
                graph.add_node(&format!("node_{}", i), "test", None);
            }
            // Add connections
            for i in 0..99 {
                graph.add_connection(
                    &format!("node_{}", i),
                    "out",
                    &format!("node_{}", i + 1),
                    "in",
                    None,
                );
            }
            // Remove some nodes
            for i in 0..10 {
                graph.remove_node(&format!("node_{}", i * 5));
            }
            // Add groups
            let nodes = (0..5)
                .map(|i| format!("node_{}", i * 10))
                .collect::<Vec<_>>();
            graph.add_group("test_group", nodes, None);
        });
    });

    c.bench_function("dfs_1000_nodes", |b| {
        b.iter(|| {
            let mut graph = Graph::new("test", false, None);
            // Setup nodes first
            for i in 0..100 {
                graph.add_node(&format!("node_{}", i), "test", None);
            }
            // Add connections
            for i in 0..99 {
                graph.add_connection(
                    &format!("node_{}", i),
                    "out",
                    &format!("node_{}", i + 1),
                    "in",
                    None,
                );
            }

           let _ = graph.traverse_depth_first("node_10", |_| {});
        });
    });

    c.bench_function("bfs_1000_nodes", |b| {
        b.iter(|| {
            let mut graph = Graph::new("test", false, None);
            // Setup nodes first
            for i in 0..100 {
                graph.add_node(&format!("node_{}", i), "test", None);
            }
            // Add connections
            for i in 0..99 {
                graph.add_connection(
                    &format!("node_{}", i),
                    "out",
                    &format!("node_{}", i + 1),
                    "in",
                    None,
                );
            }

           let _ = graph.traverse_breadth_first("node_20", |_| {});
        });
    });
}

criterion_group!(benches, benchmark_graph_operations);
criterion_main!(benches);
