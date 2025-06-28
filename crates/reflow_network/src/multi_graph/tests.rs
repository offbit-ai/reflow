use super::*;
use crate::graph::types::{GraphExport, GraphNode, GraphConnection, GraphEdge};
use std::path::PathBuf;
use std::collections::HashMap;
use tokio::fs;
use tempfile::TempDir;

async fn create_test_graph(name: &str, namespace: Option<&str>) -> GraphExport {
    let mut properties = HashMap::new();
    properties.insert("name".to_string(), serde_json::Value::String(name.to_string()));
    
    if let Some(ns) = namespace {
        properties.insert("namespace".to_string(), serde_json::Value::String(ns.to_string()));
    }

    let mut processes = HashMap::new();
    processes.insert(
        "processor".to_string(),
        GraphNode {
            id: "processor".to_string(),
            component: format!("{}Processor", name),
            metadata: Some(HashMap::new()),
        },
    );

    GraphExport {
        case_sensitive: false,
        properties,
        processes,
        connections: vec![],
        inports: HashMap::new(),
        outports: HashMap::new(),
        groups: vec![],
    }
}

async fn create_test_workspace(temp_dir: &TempDir) -> anyhow::Result<PathBuf> {
    let workspace_root = temp_dir.path().to_path_buf();

    // Create directory structure
    fs::create_dir_all(workspace_root.join("data/ingestion")).await?;
    fs::create_dir_all(workspace_root.join("data/processing")).await?;
    fs::create_dir_all(workspace_root.join("ml/training")).await?;
    fs::create_dir_all(workspace_root.join("monitoring")).await?;

    // Create graph files
    let collector_graph = create_test_graph("collector", Some("data/ingestion")).await;
    let collector_json = serde_json::to_string_pretty(&collector_graph)?;
    fs::write(
        workspace_root.join("data/ingestion/collector.graph.json"),
        collector_json,
    ).await?;

    let transformer_graph = create_test_graph("transformer", Some("data/processing")).await;
    let transformer_json = serde_json::to_string_pretty(&transformer_graph)?;
    fs::write(
        workspace_root.join("data/processing/transformer.graph.json"),
        transformer_json,
    ).await?;

    let trainer_graph = create_test_graph("trainer", Some("ml/training")).await;
    let trainer_json = serde_json::to_string_pretty(&trainer_graph)?;
    fs::write(
        workspace_root.join("ml/training/trainer.graph.json"),
        trainer_json,
    ).await?;

    let monitor_graph = create_test_graph("monitor", Some("monitoring")).await;
    let monitor_json = serde_json::to_string_pretty(&monitor_graph)?;
    fs::write(
        workspace_root.join("monitoring/system_monitor.graph.json"),
        monitor_json,
    ).await?;

    Ok(workspace_root)
}

#[tokio::test]
async fn test_graph_loader() {
    let loader = GraphLoader::new();

    // Test loading from GraphExport
    let graph_export = create_test_graph("test", None).await;
    let source = GraphSource::GraphExport(graph_export.clone());
    
    let loaded_graph = loader.load_graph(source).await.unwrap();
    assert_eq!(
        loaded_graph.properties.get("name").unwrap().as_str().unwrap(),
        "test"
    );

    // Test loading from JSON content
    let json_content = serde_json::to_string(&graph_export).unwrap();
    let source = GraphSource::JsonContent(json_content);
    
    let loaded_graph = loader.load_graph(source).await.unwrap();
    assert_eq!(
        loaded_graph.properties.get("name").unwrap().as_str().unwrap(),
        "test"
    );
}

#[tokio::test]
async fn test_graph_validator() {
    let validator = GraphValidator::new();

    // Test valid graph
    let valid_graph = create_test_graph("valid", None).await;
    assert!(validator.validate(&valid_graph).is_ok());

    // Test graph missing name
    let mut invalid_graph = create_test_graph("invalid", None).await;
    invalid_graph.properties.remove("name");
    assert!(matches!(
        validator.validate(&invalid_graph),
        Err(ValidationError::MissingProperty(_))
    ));

    // Test graph with invalid connection
    let mut invalid_conn_graph = create_test_graph("invalid_conn", None).await;
    invalid_conn_graph.connections.push(GraphConnection {
        from: GraphEdge {
            node_id: "nonexistent".to_string(),
            port_id: "out".to_string(),
            index: None,
            ..Default::default()
        },
        to: GraphEdge {
            node_id: "processor".to_string(),
            port_id: "in".to_string(),
            index: None,
            ..Default::default()
        },
        metadata: None,
        data: None,
    });
    
    assert!(matches!(
        validator.validate(&invalid_conn_graph),
        Err(ValidationError::InvalidConnection(_))
    ));
}

#[tokio::test]
async fn test_graph_normalizer() {
    let normalizer = GraphNormalizer::new();

    let mut graph = GraphExport {
        case_sensitive: false,
        properties: HashMap::new(), // Missing name
        processes: HashMap::new(),
        connections: vec![],
        inports: HashMap::new(),
        outports: HashMap::new(),
        groups: vec![],
    };

    normalizer.normalize(&mut graph).unwrap();

    // Should have added default name
    assert!(graph.properties.contains_key("name"));
    assert_eq!(
        graph.properties.get("name").unwrap().as_str().unwrap(),
        "unnamed_graph"
    );
}

#[tokio::test]
async fn test_namespace_manager() {
    let mut manager = GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve);

    // Test registering graphs
    let graph1 = create_test_graph("graph1", Some("namespace1")).await;
    let namespace1 = manager.register_graph(&graph1).unwrap();
    assert_eq!(namespace1, "namespace1");

    let graph2 = create_test_graph("graph2", Some("namespace2")).await;
    let namespace2 = manager.register_graph(&graph2).unwrap();
    assert_eq!(namespace2, "namespace2");

    // Test namespace conflict resolution
    let graph3 = create_test_graph("graph1", Some("namespace1")).await; // Same name, same namespace
    let namespace3 = manager.register_graph(&graph3).unwrap();
    assert_eq!(namespace3, "namespace1"); // Should be same namespace

    // Test resolving process paths
    let process_ref = manager.resolve_process_path("namespace1/processor").unwrap();
    assert_eq!(process_ref.qualified_name, "namespace1/processor");
    assert_eq!(process_ref.local_name, "processor");
}

#[tokio::test]
async fn test_dependency_resolver() {
    let resolver = DependencyResolver::new();

    // Create graphs with dependencies
    let mut graph1 = create_test_graph("graph1", None).await;
    graph1.properties.insert(
        "dependencies".to_string(),
        serde_json::Value::Array(vec![]),
    );

    let mut graph2 = create_test_graph("graph2", None).await;
    graph2.properties.insert(
        "dependencies".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String("graph1".to_string())]),
    );

    let graphs = vec![graph2.clone(), graph1.clone()]; // Intentionally unordered
    let ordered = resolver.resolve_dependencies(&graphs).unwrap();

    // graph1 should come before graph2
    assert_eq!(
        ordered[0].properties.get("name").unwrap().as_str().unwrap(),
        "graph1"
    );
    assert_eq!(
        ordered[1].properties.get("name").unwrap().as_str().unwrap(),
        "graph2"
    );
}

#[tokio::test]
async fn test_workspace_discovery_config() {
    let config = WorkspaceConfig {
        root_path: PathBuf::from("/test"),
        graph_patterns: vec!["**/*.graph.json".to_string()],
        excluded_paths: vec!["**/node_modules/**".to_string()],
        max_depth: Some(5),
        namespace_strategy: NamespaceStrategy::FolderStructure,
        auto_connect: true,
        dependency_resolution: DependencyResolutionStrategy::Automatic,
    };

    assert_eq!(config.root_path, PathBuf::from("/test"));
    assert_eq!(config.max_depth, Some(5));
    assert!(config.auto_connect);
}

#[tokio::test]
async fn test_workspace_discovery_full_flow() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = create_test_workspace(&temp_dir).await.unwrap();

    let config = WorkspaceConfig {
        root_path: workspace_root.clone(),
        ..WorkspaceConfig::default()
    };

    let discovery = WorkspaceDiscovery::new(config);
    let workspace = discovery.discover_workspace().await.unwrap();

    // Should discover all 4 graphs
    assert_eq!(workspace.discovered_graphs.len(), 4);

    // Check namespaces
    assert!(workspace.discovered_namespaces.contains_key("data/ingestion"));
    assert!(workspace.discovered_namespaces.contains_key("data/processing"));
    assert!(workspace.discovered_namespaces.contains_key("ml/training"));
    assert!(workspace.discovered_namespaces.contains_key("monitoring"));

    // Check that graphs have correct metadata injected
    for graph_meta in &workspace.discovered_graphs {
        assert!(graph_meta.graph.base.properties.contains_key("workspace_namespace"));
        assert!(graph_meta.graph.base.properties.contains_key("workspace_path"));
        assert!(graph_meta.graph.base.properties.contains_key("discovered_by"));
    }
}

#[tokio::test]
async fn test_graph_composition() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = create_test_workspace(&temp_dir).await.unwrap();

    let config = WorkspaceConfig {
        root_path: workspace_root.clone(),
        ..WorkspaceConfig::default()
    };

    let discovery = WorkspaceDiscovery::new(config);
    let workspace = discovery.discover_workspace().await.unwrap();

    let mut composer = GraphComposer::new();
    let composed_graph = composer.compose_graphs(workspace.composition).await.unwrap();

    let exported = composed_graph.export();

    // Should have processes from all graphs with namespace prefixes
    assert!(exported.processes.contains_key("data/ingestion/processor"));
    assert!(exported.processes.contains_key("data/processing/processor"));
    assert!(exported.processes.contains_key("ml/training/processor"));
    assert!(exported.processes.contains_key("monitoring/processor"));
}

#[tokio::test]
async fn test_workspace_auto_composer() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = create_test_workspace(&temp_dir).await.unwrap();

    let workspace_config = WorkspaceConfig {
        root_path: workspace_root.clone(),
        ..WorkspaceConfig::default()
    };

    let composer_config = AutoComposerConfig {
        enable_auto_connections: true,
        validate_before_compose: true,
        output_path: Some(workspace_root.join("composed.graph.json")),
        ..AutoComposerConfig::default()
    };

    let mut auto_composer = WorkspaceAutoComposer::new(workspace_config, composer_config);
    let composed_graph = auto_composer.discover_and_compose().await.unwrap();

    let exported = composed_graph.export();
    assert!(!exported.processes.is_empty());

    // Check that output file was created
    assert!(workspace_root.join("composed.graph.json").exists());
}

#[tokio::test]
async fn test_graph_metadata() {
    let mut graph = create_test_graph("test", Some("test_namespace")).await;
    
    // Add some metadata properties
    graph.properties.insert(
        "version".to_string(),
        serde_json::Value::String("1.0.0".to_string()),
    );
    graph.properties.insert(
        "dependencies".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String("other_graph".to_string())]),
    );
    graph.properties.insert(
        "exports".to_string(),
        serde_json::Value::Array(vec![serde_json::Value::String("processor".to_string())]),
    );

    let metadata = GraphMetadata::from_graph_export(&graph);

    assert_eq!(metadata.namespace, Some("test_namespace".to_string()));
    assert_eq!(metadata.version, Some("1.0.0".to_string()));
    assert_eq!(metadata.dependencies, vec!["other_graph".to_string()]);
    assert_eq!(metadata.exports, vec!["processor".to_string()]);
}

#[tokio::test]
async fn test_namespace_conflict_resolution() {
    let mut manager = GraphNamespaceManager::new(NamespaceConflictPolicy::AutoResolve);

    // Register first graph
    let graph1 = create_test_graph("conflicted", Some("same_namespace")).await;
    let namespace1 = manager.register_graph(&graph1).unwrap();
    assert_eq!(namespace1, "same_namespace");

    // Try to register second graph with same namespace but different content
    let mut graph2 = create_test_graph("different", Some("same_namespace")).await;
    graph2.processes.insert(
        "different_processor".to_string(),
        GraphNode {
            id: "different_processor".to_string(),
            component: "DifferentProcessor".to_string(),
            metadata: Some(HashMap::new()),
        },
    );

    let namespace2 = manager.register_graph(&graph2).unwrap();
    // Should get a different namespace due to conflict resolution
    assert_ne!(namespace2, "same_namespace");
}

#[tokio::test]
async fn test_file_format_detection() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create YAML graph file
    let yaml_graph = create_test_graph("yaml_test", None).await;
    let yaml_content = serde_yaml::to_string(&yaml_graph).unwrap();
    fs::write(workspace_root.join("test.graph.yaml"), yaml_content).await.unwrap();

    // Create JSON graph file  
    let json_graph = create_test_graph("json_test", None).await;
    let json_content = serde_json::to_string_pretty(&json_graph).unwrap();
    fs::write(workspace_root.join("test.graph.json"), json_content).await.unwrap();

    let config = WorkspaceConfig {
        root_path: workspace_root.to_path_buf(),
        ..WorkspaceConfig::default()
    };

    let discovery = WorkspaceDiscovery::new(config);
    let workspace = discovery.discover_workspace().await.unwrap();

    // Should discover both files
    assert_eq!(workspace.discovered_graphs.len(), 2);

    // Check that both formats were loaded
    let graph_names: Vec<String> = workspace.discovered_graphs
        .iter()
        .map(|g| g.graph.base.properties.get("name").unwrap().as_str().unwrap().to_string())
        .collect();
    
    assert!(graph_names.contains(&"yaml_test".to_string()));
    assert!(graph_names.contains(&"json_test".to_string()));
}

#[tokio::test]
async fn test_excluded_paths() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create directories
    fs::create_dir_all(workspace_root.join("valid")).await.unwrap();
    fs::create_dir_all(workspace_root.join("node_modules")).await.unwrap();

    // Create graph files
    let valid_graph = create_test_graph("valid", None).await;
    let valid_json = serde_json::to_string_pretty(&valid_graph).unwrap();
    fs::write(workspace_root.join("valid/graph.graph.json"), valid_json).await.unwrap();

    let excluded_graph = create_test_graph("excluded", None).await;
    let excluded_json = serde_json::to_string_pretty(&excluded_graph).unwrap();
    fs::write(workspace_root.join("node_modules/graph.graph.json"), excluded_json).await.unwrap();

    let config = WorkspaceConfig {
        root_path: workspace_root.to_path_buf(),
        excluded_paths: vec!["**/node_modules/**".to_string()],
        ..WorkspaceConfig::default()
    };

    let discovery = WorkspaceDiscovery::new(config);
    let workspace = discovery.discover_workspace().await.unwrap();

    // Should only discover the valid graph, not the excluded one
    assert_eq!(workspace.discovered_graphs.len(), 1);
    assert_eq!(
        workspace.discovered_graphs[0].graph.base.properties.get("name").unwrap().as_str().unwrap(),
        "valid"
    );
}

#[tokio::test]
async fn test_max_depth_limit() {
    let temp_dir = TempDir::new().unwrap();
    let workspace_root = temp_dir.path();

    // Create nested directory structure
    fs::create_dir_all(workspace_root.join("level1/level2/level3")).await.unwrap();

    // Create graph at depth 3
    let deep_graph = create_test_graph("deep", None).await;
    let deep_json = serde_json::to_string_pretty(&deep_graph).unwrap();
    fs::write(
        workspace_root.join("level1/level2/level3/deep.graph.json"),
        deep_json,
    ).await.unwrap();

    // Create config with max_depth of 2
    let config = WorkspaceConfig {
        root_path: workspace_root.to_path_buf(),
        max_depth: Some(2),
        ..WorkspaceConfig::default()
    };

    let discovery = WorkspaceDiscovery::new(config);
    let workspace = discovery.discover_workspace().await.unwrap();

    // Should not discover the deep graph due to max_depth limit
    assert_eq!(workspace.discovered_graphs.len(), 0);
}

#[tokio::test]
async fn test_composition_with_connections() {
    let sources = vec![
        GraphSource::GraphExport(create_test_graph("source", None).await),
        GraphSource::GraphExport(create_test_graph("target", None).await),
    ];

    let connections = vec![CompositionConnection {
        from: CompositionEndpoint {
            process: "source/processor".to_string(),
            port: "output".to_string(),
            index: None,
        },
        to: CompositionEndpoint {
            process: "target/processor".to_string(),
            port: "input".to_string(),
            index: None,
        },
        metadata: None,
    }];

    let composition = GraphComposition {
        sources,
        connections,
        shared_resources: vec![],
        properties: HashMap::new(),
        case_sensitive: Some(false),
        metadata: None,
    };

    let mut composer = GraphComposer::new();
    let composed_graph = composer.compose_graphs(composition).await.unwrap();

    let exported = composed_graph.export();

    // Should have processes from both graphs
    assert!(exported.processes.contains_key("source/processor"));
    assert!(exported.processes.contains_key("target/processor"));

    // Should have the cross-graph connection
    assert!(!exported.connections.is_empty());
    let connection = &exported.connections[0];
    assert_eq!(connection.from.node_id, "source/processor");
    assert_eq!(connection.to.node_id, "target/processor");
}

#[tokio::test]
async fn test_shared_resources() {
    let sources = vec![
        GraphSource::GraphExport(create_test_graph("graph1", None).await),
    ];

    let shared_resources = vec![SharedResource {
        name: "shared_logger".to_string(),
        component: "LoggerActor".to_string(),
        metadata: Some(HashMap::from([
            ("level".to_string(), serde_json::Value::String("info".to_string())),
        ])),
    }];

    let composition = GraphComposition {
        sources,
        connections: vec![],
        shared_resources,
        properties: HashMap::new(),
        case_sensitive: Some(false),
        metadata: None,
    };

    let mut composer = GraphComposer::new();
    let composed_graph = composer.compose_graphs(composition).await.unwrap();

    let exported = composed_graph.export();

    // Should have the shared resource as a process
    assert!(exported.processes.contains_key("shared_logger"));
    let shared_process = &exported.processes["shared_logger"];
    assert_eq!(shared_process.component, "LoggerActor");
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_workspace_integration() {
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = create_test_workspace(&temp_dir).await.unwrap();

        // Test complete workflow: discovery -> composition -> execution
        let workspace_config = WorkspaceConfig {
            root_path: workspace_root.clone(),
            auto_connect: true,
            dependency_resolution: DependencyResolutionStrategy::Automatic,
            ..WorkspaceConfig::default()
        };

        let composer_config = AutoComposerConfig {
            enable_auto_connections: true,
            validate_before_compose: true,
            output_path: Some(workspace_root.join("integrated.graph.json")),
            ..AutoComposerConfig::default()
        };

        let mut auto_composer = WorkspaceAutoComposer::new(workspace_config, composer_config);
        let composed_graph = auto_composer.discover_and_compose().await.unwrap();

        // Verify the composed graph
        let exported = composed_graph.export();
        
        // Should have all namespaced processes
        assert!(exported.processes.len() >= 4);
        assert!(exported.processes.contains_key("data/ingestion/processor"));
        assert!(exported.processes.contains_key("data/processing/processor"));
        assert!(exported.processes.contains_key("ml/training/processor"));
        assert!(exported.processes.contains_key("monitoring/processor"));

        // Should have workspace metadata
        assert!(exported.properties.contains_key("workspace_root") || 
                exported.properties.get("name").is_some());

        // Output file should exist and be valid JSON
        let output_path = workspace_root.join("integrated.graph.json");
        assert!(output_path.exists());
        
        let output_content = fs::read_to_string(output_path).await.unwrap();
        let _parsed: serde_json::Value = serde_json::from_str(&output_content).unwrap();
    }
}
