//! Integration tests for distributed workflows, subgraph composition, and their intersection.

#[cfg(test)]
mod tests {
    use crate::graph::types::{GraphConnection, GraphEdge, GraphExport, GraphNode};
    use crate::multi_graph::*;
    use std::collections::HashMap;

    // === Test helpers ===

    fn make_graph(name: &str, namespace: Option<&str>, process_names: &[&str]) -> GraphExport {
        let mut properties = HashMap::new();
        properties.insert(
            "name".to_string(),
            serde_json::Value::String(name.to_string()),
        );
        if let Some(ns) = namespace {
            properties.insert(
                "namespace".to_string(),
                serde_json::Value::String(ns.to_string()),
            );
        }

        let mut processes = HashMap::new();
        for pname in process_names {
            processes.insert(
                pname.to_string(),
                GraphNode {
                    id: pname.to_string(),
                    component: format!("{}Component", pname),
                    metadata: Some(HashMap::new()),
                    ..Default::default()
                },
            );
        }

        GraphExport {
            properties,
            processes,
            ..Default::default()
        }
    }

    fn make_graph_with_deps(
        name: &str,
        namespace: Option<&str>,
        process_names: &[&str],
        dependencies: &[&str],
    ) -> GraphExport {
        let mut graph = make_graph(name, namespace, process_names);
        if !dependencies.is_empty() {
            graph.properties.insert(
                "dependencies".to_string(),
                serde_json::Value::Array(
                    dependencies
                        .iter()
                        .map(|d| serde_json::Value::String(d.to_string()))
                        .collect(),
                ),
            );
        }
        graph
    }

    // === 5.1: Router unit tests ===

    #[cfg(not(target_arch = "wasm32"))]
    mod router_tests {
        use crate::router::MessageRouter;

        #[tokio::test]
        async fn test_register_remote_actor_with_default_capabilities() {
            let router = MessageRouter::new();
            let result = router
                .register_remote_actor("actor_a", "network_1", None)
                .await;
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_register_remote_actor_with_custom_capabilities() {
            let router = MessageRouter::new();
            let caps = vec!["processing".to_string(), "storage".to_string()];
            let result = router
                .register_remote_actor("actor_b", "network_2", Some(caps))
                .await;
            assert!(result.is_ok());
        }

        #[tokio::test]
        async fn test_route_message_no_connection_returns_error() {
            let router = MessageRouter::new();
            let msg = crate::message::Message::String(std::sync::Arc::new("test".to_string()));
            let result = router
                .route_message("nonexistent_network", "actor_a", "input", msg, Some("sender"))
                .await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("No connection to network"));
        }

        #[tokio::test]
        async fn test_handle_incoming_message_no_local_network() {
            let router = MessageRouter::new();
            let msg = crate::router::RemoteMessage {
                message_id: "test-123".to_string(),
                source_network: "remote".to_string(),
                source_actor: "sender".to_string(),
                target_network: "local".to_string(),
                target_actor: "receiver".to_string(),
                target_port: "input".to_string(),
                payload: crate::message::Message::String(std::sync::Arc::new("hello".to_string())),
                timestamp: chrono::Utc::now(),
            };
            let result = router.handle_incoming_message(msg).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("No local network configured"));
        }

        #[tokio::test]
        async fn test_get_local_actor_list_empty_without_network() {
            let router = MessageRouter::new();
            let actors = router.get_local_actor_list();
            assert!(actors.is_empty());
        }
    }

    // === 5.3: Subgraph composition tests ===

    #[tokio::test]
    async fn test_compose_two_graphs_namespace_isolation() {
        let graph_a = make_graph("graph_a", Some("ns_a"), &["processor", "transformer"]);
        let graph_b = make_graph("graph_b", Some("ns_b"), &["loader", "writer"]);

        let composition = GraphComposition {
            sources: vec![
                GraphSource::GraphExport(graph_a),
                GraphSource::GraphExport(graph_b),
            ],
            connections: vec![CompositionConnection {
                from: CompositionEndpoint {
                    process: "ns_a/transformer".to_string(),
                    port: "output".to_string(),
                    index: None,
                },
                to: CompositionEndpoint {
                    process: "ns_b/loader".to_string(),
                    port: "input".to_string(),
                    index: None,
                },
                metadata: None,
            }],
            shared_resources: vec![],
            properties: HashMap::from([(
                "name".to_string(),
                serde_json::Value::String("composed".to_string()),
            )]),
            case_sensitive: None,
            metadata: None,
        };

        let mut composer = GraphComposer::new();
        let result = composer.compose_graphs(composition).await;
        assert!(result.is_ok(), "Composition should succeed: {:?}", result.err());

        let graph = result.unwrap();
        // Verify namespace isolation: processes are prefixed
        let process_names: Vec<String> = graph.nodes.keys().cloned().collect();
        assert!(
            process_names.iter().any(|n| n.contains("ns_a/")),
            "Should have ns_a namespaced processes, got: {:?}",
            process_names
        );
        assert!(
            process_names.iter().any(|n| n.contains("ns_b/")),
            "Should have ns_b namespaced processes, got: {:?}",
            process_names
        );
    }

    // === 5.5: Namespace conflict resolution tests ===

    #[test]
    fn test_namespace_conflict_fail_policy() {
        let mut manager = GraphNamespaceManager::new(NamespaceConflictPolicy::Fail);

        // Register first graph "my_graph" under namespace "shared_ns"
        let graph_a = make_graph("my_graph", Some("shared_ns"), &["proc_a"]);
        let result_a = manager.register_graph(&graph_a);
        assert!(result_a.is_ok());
        assert_eq!(result_a.unwrap(), "shared_ns");

        // Register second graph with SAME name "my_graph" but wanting DIFFERENT namespace.
        // With Fail policy, this should error because "my_graph" is already mapped to "shared_ns".
        let graph_b = make_graph("my_graph", Some("different_ns"), &["proc_b"]);
        let result_b = manager.register_graph(&graph_b);
        assert!(
            result_b.is_err(),
            "Fail policy should reject namespace reassignment for the same graph name"
        );
    }

    #[test]
    fn test_namespace_conflict_auto_resolve_policy() {
        let mut manager = GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve);

        let graph_a = make_graph("my_graph", Some("ns_a"), &["proc_a"]);
        let result_a = manager.register_graph(&graph_a);
        assert!(result_a.is_ok());
        let ns_a = result_a.unwrap();

        // Second graph with same name but different namespace preference
        let graph_b = make_graph("my_graph", Some("ns_b"), &["proc_b"]);
        let result_b = manager.register_graph(&graph_b);
        assert!(result_b.is_ok());
        let ns_b = result_b.unwrap();

        // They should have different namespaces
        assert_ne!(ns_a, ns_b, "Auto-resolve should assign different namespaces");
    }

    #[test]
    fn test_namespace_conflict_version_suffix_policy() {
        let mut manager = GraphNamespaceManager::new(NamespaceConflictPolicy::VersionSuffix);

        let graph_a = make_graph("my_graph", Some("shared"), &["proc_a"]);
        let result_a = manager.register_graph(&graph_a);
        assert!(result_a.is_ok());

        // A second graph also wanting "shared" namespace
        let graph_b = make_graph("my_graph_v2", Some("shared"), &["proc_b"]);
        let result_b = manager.register_graph(&graph_b);
        // Should auto-resolve since "shared" is taken
        assert!(result_b.is_ok());
    }

    // === 5.6: Circular dependency detection tests ===

    #[test]
    fn test_dependency_resolver_no_deps() {
        let resolver = DependencyResolver::new();
        let graphs = vec![
            make_graph("a", None, &["p1"]),
            make_graph("b", None, &["p2"]),
        ];
        let result = resolver.resolve_dependencies(&graphs);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_dependency_resolver_linear_deps() {
        let resolver = DependencyResolver::new();
        let graphs = vec![
            make_graph_with_deps("c", None, &["p3"], &["b"]),
            make_graph_with_deps("b", None, &["p2"], &["a"]),
            make_graph("a", None, &["p1"]),
        ];
        let result = resolver.resolve_dependencies(&graphs);
        assert!(result.is_ok());
        let ordered = result.unwrap();
        assert_eq!(ordered.len(), 3);
        // "a" should come first, then "b", then "c"
        let names: Vec<String> = ordered
            .iter()
            .map(|g| {
                g.properties
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_dependency_resolver_circular_deps() {
        let resolver = DependencyResolver::new();
        let graphs = vec![
            make_graph_with_deps("a", None, &["p1"], &["c"]),
            make_graph_with_deps("b", None, &["p2"], &["a"]),
            make_graph_with_deps("c", None, &["p3"], &["b"]),
        ];
        let result = resolver.resolve_dependencies(&graphs);
        assert!(result.is_err(), "Circular dependency should be detected");
        match result.unwrap_err() {
            DependencyError::CircularDependency => {} // expected
            other => panic!("Expected CircularDependency, got: {:?}", other),
        }
    }

    // === 5.4: Distributed composition planning tests ===

    #[cfg(not(target_arch = "wasm32"))]
    mod distributed_composition_tests {
        use crate::distributed_composition::*;
        use crate::multi_graph::{CompositionConnection, CompositionEndpoint, GraphSource};
        use std::collections::HashMap;

        #[test]
        fn test_distributed_namespace_resolver_local() {
            let mut resolver = DistributedNamespaceResolver::new("local_net");
            let graph = super::make_graph("pipeline", Some("data"), &["reader", "writer"]);
            resolver.register_local_graph("data", &graph);

            let endpoint = DistributedEndpoint {
                network_id: Some("local_net".to_string()),
                process: "data/reader".to_string(),
                port: "output".to_string(),
                index: None,
            };
            let location = resolver.resolve(&endpoint);
            assert!(location.is_some());
            let loc = location.unwrap();
            assert!(loc.is_local);
            assert_eq!(loc.namespace, "data");
            assert_eq!(loc.process_name, "reader");
        }

        #[test]
        fn test_distributed_namespace_resolver_remote() {
            let mut resolver = DistributedNamespaceResolver::new("local_net");
            let graph = super::make_graph("ml_model", Some("ml"), &["trainer", "predictor"]);
            resolver.register_remote_graph("gpu_cluster", "ml", &graph);

            let endpoint = DistributedEndpoint {
                network_id: Some("gpu_cluster".to_string()),
                process: "ml/trainer".to_string(),
                port: "input".to_string(),
                index: None,
            };
            let location = resolver.resolve(&endpoint);
            assert!(location.is_some());
            let loc = location.unwrap();
            assert!(!loc.is_local);
            assert_eq!(loc.network_id, "gpu_cluster");
        }

        #[test]
        fn test_cross_network_edge_detection() {
            let mut resolver = DistributedNamespaceResolver::new("local_net");

            let local_graph = super::make_graph("pipeline", Some("data"), &["writer"]);
            resolver.register_local_graph("data", &local_graph);

            let remote_graph = super::make_graph("ml_model", Some("ml"), &["trainer"]);
            resolver.register_remote_graph("gpu_cluster", "ml", &remote_graph);

            let connections = vec![DistributedConnection {
                from: DistributedEndpoint {
                    network_id: Some("local_net".to_string()),
                    process: "data/writer".to_string(),
                    port: "output".to_string(),
                    index: None,
                },
                to: DistributedEndpoint {
                    network_id: Some("gpu_cluster".to_string()),
                    process: "ml/trainer".to_string(),
                    port: "input".to_string(),
                    index: None,
                },
                metadata: None,
            }];

            let edges = resolver.find_cross_network_connections(&connections);
            assert_eq!(edges.len(), 1);
            assert_eq!(edges[0].from_network, "local_net");
            assert_eq!(edges[0].to_network, "gpu_cluster");
            assert_eq!(edges[0].proxy_actor_name(), "ml/trainer@gpu_cluster");
        }

        #[test]
        fn test_plan_distributed_composition_creates_proxy_specs() {
            let composition = DistributedGraphComposition {
                local_sources: vec![],
                remote_sources: vec![],
                local_connections: vec![],
                distributed_connections: vec![DistributedConnection {
                    from: DistributedEndpoint {
                        network_id: Some("local".to_string()),
                        process: "data/writer".to_string(),
                        port: "output".to_string(),
                        index: None,
                    },
                    to: DistributedEndpoint {
                        network_id: Some("remote_gpu".to_string()),
                        process: "ml/trainer".to_string(),
                        port: "input".to_string(),
                        index: None,
                    },
                    metadata: None,
                }],
                properties: HashMap::from([(
                    "name".to_string(),
                    serde_json::Value::String("dist_composed".to_string()),
                )]),
                execution_targets: HashMap::new(),
            };

            let plan = plan_distributed_composition(&composition, "local");

            assert_eq!(plan.proxy_actors.len(), 1);
            assert_eq!(plan.proxy_actors[0].remote_network_id, "remote_gpu");
            assert_eq!(plan.proxy_actors[0].remote_actor_id, "ml/trainer");
            assert_eq!(plan.cross_network_edges.len(), 1);
        }

        #[test]
        fn test_plan_deduplicates_proxy_actors() {
            let composition = DistributedGraphComposition {
                local_sources: vec![],
                remote_sources: vec![],
                local_connections: vec![],
                distributed_connections: vec![
                    DistributedConnection {
                        from: DistributedEndpoint {
                            network_id: Some("local".to_string()),
                            process: "a/proc1".to_string(),
                            port: "out1".to_string(),
                            index: None,
                        },
                        to: DistributedEndpoint {
                            network_id: Some("remote".to_string()),
                            process: "b/proc2".to_string(),
                            port: "in1".to_string(),
                            index: None,
                        },
                        metadata: None,
                    },
                    DistributedConnection {
                        from: DistributedEndpoint {
                            network_id: Some("local".to_string()),
                            process: "a/proc3".to_string(),
                            port: "out2".to_string(),
                            index: None,
                        },
                        to: DistributedEndpoint {
                            network_id: Some("remote".to_string()),
                            process: "b/proc2".to_string(),
                            port: "in2".to_string(),
                            index: None,
                        },
                        metadata: None,
                    },
                ],
                properties: HashMap::from([(
                    "name".to_string(),
                    serde_json::Value::String("test".to_string()),
                )]),
                execution_targets: HashMap::new(),
            };

            let plan = plan_distributed_composition(&composition, "local");

            // Both connections target the same remote actor, so only one proxy should be created
            assert_eq!(
                plan.proxy_actors.len(),
                1,
                "Should deduplicate proxy actors for the same remote target"
            );
            assert_eq!(plan.cross_network_edges.len(), 2);
        }

        #[test]
        fn test_distributed_endpoint_qualified_name() {
            let remote = DistributedEndpoint {
                network_id: Some("gpu_cluster".to_string()),
                process: "ml/trainer".to_string(),
                port: "input".to_string(),
                index: None,
            };
            assert_eq!(remote.qualified_name(), "gpu_cluster/ml/trainer");
            assert!(remote.is_remote());

            let local = DistributedEndpoint {
                network_id: None,
                process: "data/reader".to_string(),
                port: "output".to_string(),
                index: None,
            };
            assert_eq!(local.qualified_name(), "data/reader");
            assert!(!local.is_remote());
        }
    }
}
