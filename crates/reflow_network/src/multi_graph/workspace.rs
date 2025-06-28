// Workspace discovery system for automatic graph composition
use glob::glob;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::{
    fmt::Debug,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;

use crate::{
    graph::types::GraphExport,
    multi_graph::{
        CompositionConnection, CompositionEndpoint, GraphComposition, GraphLoader, GraphSource,
        SharedResource,
    },
};

#[derive(Serialize, Deserialize)]
pub struct WorkspaceDiscovery {
    config: WorkspaceConfig,
    loader: GraphLoader,
    namespace_builder: NamespaceBuilder,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    pub root_path: PathBuf,
    pub graph_patterns: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub max_depth: Option<usize>,
    pub namespace_strategy: NamespaceStrategy,
    pub auto_connect: bool,
    pub dependency_resolution: DependencyResolutionStrategy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NamespaceStrategy {
    FolderStructure, // Use folder structure as namespace
    Flatten,         // Flatten all graphs to root namespace
    FileBasedPrefix, // Use filename prefix as namespace
    // Custom(Arc<dyn Fn(PathBuf) -> String + Send + Sync>), // Custom namespace function
}







#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DependencyResolutionStrategy {
    Automatic, // Automatically detect and resolve dependencies
    Explicit,  // Only use explicitly declared dependencies
    None,      // No dependency resolution
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        WorkspaceConfig {
            root_path: PathBuf::from("."),
            graph_patterns: vec![
                "**/*.graph.json".to_string(),
                "**/*.graph.yaml".to_string(),
                "**/*.graph.yml".to_string(),
            ],
            excluded_paths: vec![
                "**/node_modules/**".to_string(),
                "**/target/**".to_string(),
                "**/.git/**".to_string(),
                "**/.*/**".to_string(),
            ],
            max_depth: Some(10),
            namespace_strategy: NamespaceStrategy::FolderStructure,
            auto_connect: true,
            dependency_resolution: DependencyResolutionStrategy::Automatic,
        }
    }
}

impl WorkspaceDiscovery {
    pub fn new(config: WorkspaceConfig) -> Self {
        WorkspaceDiscovery {
            config,
            loader: GraphLoader::new(),
            namespace_builder: NamespaceBuilder::new(),
        }
    }

    pub async fn discover_workspace(&self) -> Result<WorkspaceComposition, DiscoveryError> {
        println!(
            "🔍 Discovering graphs in workspace: {}",
            self.config.root_path.display()
        );

        // 1. Discover all graph files
        let discovered_files = self.discover_graph_files().await?;
        println!("📁 Found {} graph files", discovered_files.len());

        // 2. Build namespace mappings
        let namespace_mappings = self.build_namespace_mappings(&discovered_files)?;

        // 3. Load all graphs
        let loaded_graphs = self.load_discovered_graphs(&discovered_files).await?;
        println!("📊 Successfully loaded {} graphs", loaded_graphs.len());

        // 4. Detect dependencies and interfaces
        let graph_analysis = self
            .analyze_graphs(&loaded_graphs, &namespace_mappings)
            .await?;

        // 5. Build composition
        let composition = self
            .build_workspace_composition(loaded_graphs, namespace_mappings, graph_analysis)
            .await?;

        println!(
            "✅ Workspace composition ready with {} namespaces",
            composition.discovered_namespaces.len()
        );

        Ok(composition)
    }

    async fn discover_graph_files(&self) -> Result<Vec<DiscoveredGraphFile>, DiscoveryError> {
        let mut discovered_files = Vec::new();

        for pattern in &self.config.graph_patterns {
            let full_pattern = self.config.root_path.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();

            for entry in glob(&pattern_str).map_err(|e| DiscoveryError::GlobError(e.to_string()))? {
                match entry {
                    Ok(path) => {
                        // Check if path should be excluded
                        if self.should_exclude_path(&path)? {
                            continue;
                        }

                        // Check depth limit
                        if let Some(max_depth) = self.config.max_depth {
                            let relative_path =
                                path.strip_prefix(&self.config.root_path).unwrap_or(&path);
                            if relative_path.components().count() > max_depth {
                                continue;
                            }
                        }

                        let file_info = self.analyze_graph_file(&path).await?;
                        discovered_files.push(file_info);
                    }
                    Err(e) => {
                        eprintln!("⚠️  Warning: Error reading path: {}", e);
                    }
                }
            }
        }

        // Sort by namespace for consistent ordering
        discovered_files.sort_by(|a, b| a.namespace.cmp(&b.namespace));

        Ok(discovered_files)
    }

    async fn analyze_graph_file(&self, path: &Path) -> Result<DiscoveredGraphFile, DiscoveryError> {
        let metadata = fs::metadata(path)
            .await
            .map_err(|e| DiscoveryError::FileAccessError(path.to_path_buf(), e.to_string()))?;

        // Build namespace from folder structure
        let namespace = self.namespace_builder.build_namespace(path, &self.config)?;

        // Determine file format
        let format = self.detect_file_format(path)?;

        // Extract graph name from filename
        let graph_name = self.extract_graph_name(path)?;

        Ok(DiscoveredGraphFile {
            path: path.to_path_buf(),
            namespace,
            graph_name,
            format,
            size_bytes: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }

    fn build_namespace_mappings(
        &self,
        files: &[DiscoveredGraphFile],
    ) -> Result<HashMap<String, String>, DiscoveryError> {
        let mut mappings: HashMap<String, String> = HashMap::new();
        let mut namespace_counts = HashMap::new();

        for file in files {
            // Count namespace usage for conflict detection
            *namespace_counts.entry(file.namespace.clone()).or_insert(0) += 1;

            // Create mapping from graph name to namespace
            if mappings.contains_key(&file.graph_name) {
                return Err(DiscoveryError::DuplicateGraphName(
                    file.graph_name.clone(),
                    vec![
                        mappings.get(&file.graph_name).unwrap().clone(),
                        file.namespace.clone(),
                    ],
                ));
            }

            mappings.insert(file.graph_name.clone(), file.namespace.clone());
        }

        // Report namespace statistics
        println!("📊 Namespace distribution:");
        for (namespace, count) in &namespace_counts {
            println!("  📁 {}: {} graphs", namespace, count);
        }

        Ok(mappings)
    }

    async fn load_discovered_graphs(
        &self,
        files: &[DiscoveredGraphFile],
    ) -> Result<Vec<GraphWithMetadata>, DiscoveryError> {
        let mut graphs = Vec::new();

        for file in files {
            println!(
                "📈 Loading graph: {} ({})",
                file.graph_name,
                file.path.display()
            );

            match self.load_graph_file(file).await {
                Ok(graph_with_meta) => {
                    graphs.push(graph_with_meta);
                }
                Err(e) => {
                    eprintln!("❌ Failed to load {}: {}", file.path.display(), e);
                    if self.should_fail_on_load_error()? {
                        return Err(e);
                    }
                    // Continue with other graphs if configured to be resilient
                }
            }
        }

        Ok(graphs)
    }

    async fn load_graph_file(
        &self,
        file: &DiscoveredGraphFile,
    ) -> Result<GraphWithMetadata, DiscoveryError> {
        let source = match file.format {
            GraphFileFormat::Json => GraphSource::JsonFile(file.path.to_string_lossy().to_string()),
            GraphFileFormat::Yaml => GraphSource::JsonFile(file.path.to_string_lossy().to_string()), // Loader handles YAML
        };

        let mut graph_export = self
            .loader
            .load_graph(source)
            .await
            .map_err(|e| DiscoveryError::LoadError(file.path.clone(), e.to_string()))?;

        // Inject namespace and discovery metadata
        self.inject_workspace_metadata(&mut graph_export, file)?;

        Ok(GraphWithMetadata {
            graph: graph_export,
            file_info: file.clone(),
            discovered_namespace: file.namespace.clone(),
        })
    }

    fn inject_workspace_metadata(
        &self,
        graph: &mut GraphExport,
        file: &DiscoveredGraphFile,
    ) -> Result<(), DiscoveryError> {
        // Ensure the graph has a name
        if !graph.properties.contains_key("name") {
            graph.properties.insert(
                "name".to_string(),
                serde_json::Value::String(file.graph_name.clone()),
            );
        }

        // Inject workspace metadata
        graph.properties.insert(
            "workspace_namespace".to_string(),
            serde_json::Value::String(file.namespace.clone()),
        );
        graph.properties.insert(
            "workspace_path".to_string(),
            serde_json::Value::String(file.path.to_string_lossy().to_string()),
        );
        graph.properties.insert(
            "discovered_by".to_string(),
            serde_json::Value::String("workspace_discovery".to_string()),
        );

        // Set namespace if not already set
        if !graph.properties.contains_key("namespace") {
            graph.properties.insert(
                "namespace".to_string(),
                serde_json::Value::String(file.namespace.clone()),
            );
        }

        Ok(())
    }

    async fn analyze_graphs(
        &self,
        graphs: &[GraphWithMetadata],
        namespace_mappings: &HashMap<String, String>,
    ) -> Result<WorkspaceAnalysis, DiscoveryError> {
        let mut analysis = WorkspaceAnalysis::new();

        for graph_meta in graphs {
            let graph = &graph_meta.graph;

            // Analyze exposed interfaces
            self.analyze_graph_interfaces(graph, &mut analysis)?;

            // Analyze dependencies
            self.analyze_graph_dependencies(graph, &mut analysis)?;

            // Analyze potential connections
            if self.config.auto_connect {
                self.analyze_auto_connections(graph, graphs, &mut analysis)
                    .await?;
            }
        }

        println!("🔗 Analysis summary:");
        println!(
            "  📤 Exposed interfaces: {}",
            analysis.exposed_interfaces.len()
        );
        println!(
            "  📥 Required interfaces: {}",
            analysis.required_interfaces.len()
        );
        println!(
            "  🔄 Auto-connections found: {}",
            analysis.auto_connections.len()
        );

        Ok(analysis)
    }

    fn analyze_graph_interfaces(
        &self,
        graph: &GraphExport,
        analysis: &mut WorkspaceAnalysis,
    ) -> Result<(), DiscoveryError> {
        let graph_name = graph
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed");
        let namespace = graph
            .properties
            .get("workspace_namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        // Analyze outports as exposed interfaces
        for (outport_name, outport_def) in &graph.outports {
            let interface = ExposedInterface {
                graph_name: graph_name.to_string(),
                namespace: namespace.to_string(),
                interface_name: outport_name.clone(),
                process_name: outport_def.node_id.clone(),
                port_name: outport_def.port_id.clone(),
                interface_type: InterfaceType::Output,
            };
            analysis.exposed_interfaces.push(interface);
        }

        // Analyze inports as required interfaces
        for (inport_name, inport_def) in &graph.inports {
            let interface = RequiredInterface {
                graph_name: graph_name.to_string(),
                namespace: namespace.to_string(),
                interface_name: inport_name.clone(),
                process_name: inport_def.node_id.clone(),
                port_name: inport_def.port_id.clone(),
                interface_type: InterfaceType::Input,
                required: true, // Assume inports are required
            };
            analysis.required_interfaces.push(interface);
        }

        Ok(())
    }

    fn analyze_graph_dependencies(
        &self,
        graph: &GraphExport,
        analysis: &mut WorkspaceAnalysis,
    ) -> Result<(), DiscoveryError> {
        let graph_name = graph
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed");

        // Check for explicit dependencies in properties
        if let Some(deps) = graph
            .properties
            .get("dependencies")
            .and_then(|v| v.as_array())
        {
            for dep in deps {
                if let Some(dep_name) = dep.as_str() {
                    analysis.dependencies.push(GraphDependency {
                        dependent: graph_name.to_string(),
                        dependency: dep_name.to_string(),
                        dependency_type: DependencyType::Explicit,
                    });
                }
            }
        }

        Ok(())
    }

    async fn analyze_auto_connections(
        &self,
        graph: &GraphExport,
        all_graphs: &[GraphWithMetadata],
        analysis: &mut WorkspaceAnalysis,
    ) -> Result<(), DiscoveryError> {
        let graph_name = graph
            .properties
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed");

        // Find potential connections based on port name matching
        for (outport_name, outport_def) in &graph.outports {
            for other_graph_meta in all_graphs {
                let other_graph = &other_graph_meta.graph;
                let other_name = other_graph
                    .properties
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unnamed");

                if graph_name == other_name {
                    continue; // Skip self
                }

                // Look for matching inport names
                for (inport_name, inport_def) in &other_graph.inports {
                    if self.ports_are_compatible(outport_name, inport_name) {
                        let connection = AutoConnection {
                            from_graph: graph_name.to_string(),
                            from_interface: outport_name.clone(),
                            to_graph: other_name.to_string(),
                            to_interface: inport_name.clone(),
                            confidence: self
                                .calculate_connection_confidence(outport_name, inport_name),
                            connection_type: AutoConnectionType::PortNameMatch,
                        };
                        analysis.auto_connections.push(connection);
                    }
                }
            }
        }

        Ok(())
    }

    fn ports_are_compatible(&self, outport_name: &str, inport_name: &str) -> bool {
        // Simple heuristic: exact match or semantic similarity
        if outport_name == inport_name {
            return true;
        }

        // Check for common patterns
        let common_mappings = [
            ("output", "input"),
            ("out", "in"),
            ("data", "data"),
            ("result", "input"),
            ("processed", "raw"),
        ];

        for (out_pattern, in_pattern) in &common_mappings {
            if outport_name.to_lowercase().contains(out_pattern)
                && inport_name.to_lowercase().contains(in_pattern)
            {
                return true;
            }
        }

        false
    }

    fn calculate_connection_confidence(&self, outport_name: &str, inport_name: &str) -> f32 {
        if outport_name == inport_name {
            return 1.0;
        }

        // Simple string similarity calculation
        let similarity = self.string_similarity(outport_name, inport_name);
        if similarity > 0.7 {
            similarity
        } else {
            0.5 // Default confidence for pattern matches
        }
    }

    fn string_similarity(&self, s1: &str, s2: &str) -> f32 {
        // Simple Levenshtein distance-based similarity
        let len1 = s1.len();
        let len2 = s2.len();

        if len1 == 0 || len2 == 0 {
            return 0.0;
        }

        let mut matrix = vec![vec![0; len2 + 1]; len1 + 1];

        for i in 0..=len1 {
            matrix[i][0] = i;
        }
        for j in 0..=len2 {
            matrix[0][j] = j;
        }

        for (i, c1) in s1.chars().enumerate() {
            for (j, c2) in s2.chars().enumerate() {
                let cost = if c1 == c2 { 0 } else { 1 };
                matrix[i + 1][j + 1] = std::cmp::min(
                    std::cmp::min(matrix[i][j + 1] + 1, matrix[i + 1][j] + 1),
                    matrix[i][j] + cost,
                );
            }
        }

        let distance = matrix[len1][len2];
        let max_len = std::cmp::max(len1, len2);
        1.0 - (distance as f32 / max_len as f32)
    }

    async fn build_workspace_composition(
        &self,
        graphs: Vec<GraphWithMetadata>,
        namespace_mappings: HashMap<String, String>,
        analysis: WorkspaceAnalysis,
    ) -> Result<WorkspaceComposition, DiscoveryError> {
        // Create sources from loaded graphs
        let sources: Vec<GraphSource> = graphs
            .iter()
            .map(|g| GraphSource::GraphExport(g.graph.clone()))
            .collect();

        // Build auto-connections based on analysis
        let auto_connections = self.build_auto_connections(&analysis)?;

        // Create shared resources if any are detected
        let shared_resources = self.build_shared_resources(&graphs)?;

        let composition = GraphComposition {
            sources,
            connections: auto_connections,
            shared_resources,
            properties: HashMap::from([
                (
                    "name".to_string(),
                    serde_json::Value::String("workspace_composition".to_string()),
                ),
                (
                    "generated_by".to_string(),
                    serde_json::Value::String("workspace_discovery".to_string()),
                ),
                (
                    "discovery_timestamp".to_string(),
                    serde_json::Value::String(chrono::Utc::now().to_rfc3339()),
                ),
                (
                    "total_graphs".to_string(),
                    serde_json::Value::Number(graphs.len().into()),
                ),
                (
                    "total_namespaces".to_string(),
                    serde_json::Value::Number(namespace_mappings.len().into()),
                ),
            ]),
            case_sensitive: Some(false),
            metadata: Some(HashMap::from([
                (
                    "workspace_root".to_string(),
                    serde_json::Value::String(
                        (&self.config.root_path).to_string_lossy().to_string(),
                    ),
                ),
                (
                    "discovery_config".to_string(),
                    serde_json::to_value(&self.config).unwrap_or_default(),
                ),
            ])),
        };

        let discovered_namespaces: HashMap<String, NamespaceInfo> = namespace_mappings
            .into_iter()
            .map(|(graph_name, namespace)| {
                let graph_meta = graphs
                    .iter()
                    .find(|g| {
                        g.graph
                            .properties
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            == graph_name
                    })
                    .unwrap();

                (
                    namespace.clone(),
                    NamespaceInfo {
                        namespace: namespace.clone(),
                        graphs: vec![graph_name],
                        path: graph_meta
                            .file_info
                            .path
                            .parent()
                            .unwrap_or(&graph_meta.file_info.path)
                            .to_path_buf(),
                        graph_count: 1,
                    },
                )
            })
            .collect();

        Ok(WorkspaceComposition {
            composition,
            discovered_graphs: graphs,
            discovered_namespaces,
            analysis,
            workspace_root: self.config.root_path.clone(),
        })
    }

    fn build_auto_connections(
        &self,
        analysis: &WorkspaceAnalysis,
    ) -> Result<Vec<CompositionConnection>, DiscoveryError> {
        let mut connections = Vec::new();

        for auto_conn in &analysis.auto_connections {
            if auto_conn.confidence >= 0.7 {
                // Only include high-confidence connections
                connections.push(CompositionConnection {
                    from: CompositionEndpoint {
                        process: format!("{}/{}", auto_conn.from_graph, auto_conn.from_interface),
                        port: "Output".to_string(), // Would be more sophisticated in real implementation
                        index: None,
                    },
                    to: CompositionEndpoint {
                        process: format!("{}/{}", auto_conn.to_graph, auto_conn.to_interface),
                        port: "Input".to_string(),
                        index: None,
                    },
                    metadata: Some(HashMap::from([
                        ("auto_generated".to_string(), serde_json::Value::Bool(true)),
                        (
                            "confidence".to_string(),
                            serde_json::Value::Number(
                                serde_json::Number::from_f64(auto_conn.confidence as f64).unwrap(),
                            ),
                        ),
                        (
                            "connection_type".to_string(),
                            serde_json::Value::String(format!("{:?}", auto_conn.connection_type)),
                        ),
                    ])),
                });
            }
        }

        Ok(connections)
    }

    fn build_shared_resources(
        &self,
        graphs: &[GraphWithMetadata],
    ) -> Result<Vec<SharedResource>, DiscoveryError> {
        let mut shared_resources = Vec::new();

        // Look for processes marked as shared across graphs
        for graph_meta in graphs {
            for (process_name, process_def) in &graph_meta.graph.processes {
                if let Some(metadata) = &process_def.metadata {
                    if metadata
                        .get("shared")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        shared_resources.push(SharedResource {
                            name: format!("shared_{}", process_name),
                            component: process_def.component.clone(),
                            metadata: process_def.metadata.clone(),
                        });
                    }
                }
            }
        }

        Ok(shared_resources)
    }

    // Helper methods
    fn should_exclude_path(&self, path: &Path) -> Result<bool, DiscoveryError> {
        let path_str = path.to_string_lossy();

        for exclude_pattern in &self.config.excluded_paths {
            if glob::Pattern::new(exclude_pattern)
                .map_err(|e| DiscoveryError::GlobError(e.to_string()))?
                .matches(&path_str)
            {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn detect_file_format(&self, path: &Path) -> Result<GraphFileFormat, DiscoveryError> {
        match path.extension().and_then(|s| s.to_str()) {
            Some("json") => Ok(GraphFileFormat::Json),
            Some("yaml") | Some("yml") => Ok(GraphFileFormat::Yaml),
            _ => Err(DiscoveryError::UnsupportedFormat(path.to_path_buf())),
        }
    }

    fn extract_graph_name(&self, path: &Path) -> Result<String, DiscoveryError> {
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| DiscoveryError::InvalidFileName(path.to_path_buf()))?;

        // Remove .graph suffix if present
        let name = if filename.ends_with(".graph") {
            &filename[..filename.len() - 6]
        } else {
            filename
        };

        Ok(name.to_string())
    }

    fn should_fail_on_load_error(&self) -> Result<bool, DiscoveryError> {
        // For now, be resilient and continue on load errors
        // This could be configurable
        Ok(false)
    }
}

// Namespace builder for folder structure
#[derive(Serialize, Deserialize)]
pub struct NamespaceBuilder;

impl NamespaceBuilder {
    pub fn new() -> Self {
        NamespaceBuilder
    }

    pub fn build_namespace(
        &self,
        file_path: &Path,
        config: &WorkspaceConfig,
    ) -> Result<String, DiscoveryError> {
        match &config.namespace_strategy {
            NamespaceStrategy::FolderStructure => {
                self.build_folder_based_namespace(file_path, &config.root_path)
            }
            NamespaceStrategy::Flatten => Ok("root".to_string()),
            NamespaceStrategy::FileBasedPrefix => self.build_file_based_namespace(file_path),
            // NamespaceStrategy::Custom(func) => Ok(func(file_path.to_path_buf())),
        }
    }

    fn build_folder_based_namespace(
        &self,
        file_path: &Path,
        root_path: &Path,
    ) -> Result<String, DiscoveryError> {
        let relative_path = file_path.strip_prefix(root_path).map_err(|_| {
            DiscoveryError::PathError(format!(
                "Path {} is not under root {}",
                file_path.display(),
                root_path.display()
            ))
        })?;

        let parent_dir = relative_path.parent().unwrap_or_else(|| Path::new(""));

        if parent_dir == Path::new("") {
            Ok("root".to_string())
        } else {
            Ok(parent_dir.to_string_lossy().replace('\\', "/"))
        }
    }

    fn build_file_based_namespace(&self, file_path: &Path) -> Result<String, DiscoveryError> {
        let filename = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| DiscoveryError::InvalidFileName(file_path.to_path_buf()))?;

        // Extract prefix before first underscore or dot
        let namespace = filename
            .split('_')
            .next()
            .or_else(|| filename.split('.').next())
            .unwrap_or("default");

        Ok(namespace.to_string())
    }
}

// Data structures for discovered information
#[derive(Debug, Clone)]
pub struct DiscoveredGraphFile {
    pub path: PathBuf,
    pub namespace: String,
    pub graph_name: String,
    pub format: GraphFileFormat,
    pub size_bytes: u64,
    pub modified: Option<std::time::SystemTime>,
}

#[derive(Debug, Clone)]
pub enum GraphFileFormat {
    Json,
    Yaml,
}

#[derive(Debug, Clone)]
pub struct GraphWithMetadata {
    pub graph: GraphExport,
    pub file_info: DiscoveredGraphFile,
    pub discovered_namespace: String,
}

#[derive(Debug)]
pub struct WorkspaceComposition {
    pub composition: GraphComposition,
    pub discovered_graphs: Vec<GraphWithMetadata>,
    pub discovered_namespaces: HashMap<String, NamespaceInfo>,
    pub analysis: WorkspaceAnalysis,
    pub workspace_root: PathBuf,
}

#[derive(Debug)]
pub struct NamespaceInfo {
    pub namespace: String,
    pub graphs: Vec<String>,
    pub path: PathBuf,
    pub graph_count: usize,
}

#[derive(Debug)]
pub struct WorkspaceAnalysis {
    pub exposed_interfaces: Vec<ExposedInterface>,
    pub required_interfaces: Vec<RequiredInterface>,
    pub dependencies: Vec<GraphDependency>,
    pub auto_connections: Vec<AutoConnection>,
}

impl WorkspaceAnalysis {
    fn new() -> Self {
        WorkspaceAnalysis {
            exposed_interfaces: Vec::new(),
            required_interfaces: Vec::new(),
            dependencies: Vec::new(),
            auto_connections: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct ExposedInterface {
    pub graph_name: String,
    pub namespace: String,
    pub interface_name: String,
    pub process_name: String,
    pub port_name: String,
    pub interface_type: InterfaceType,
}

#[derive(Debug)]
pub struct RequiredInterface {
    pub graph_name: String,
    pub namespace: String,
    pub interface_name: String,
    pub process_name: String,
    pub port_name: String,
    pub interface_type: InterfaceType,
    pub required: bool,
}

#[derive(Debug)]
pub enum InterfaceType {
    Input,
    Output,
}

#[derive(Debug)]
pub struct GraphDependency {
    pub dependent: String,
    pub dependency: String,
    pub dependency_type: DependencyType,
}

#[derive(Debug)]
pub enum DependencyType {
    Explicit,
    Inferred,
}

#[derive(Debug)]
pub struct AutoConnection {
    pub from_graph: String,
    pub from_interface: String,
    pub to_graph: String,
    pub to_interface: String,
    pub confidence: f32,
    pub connection_type: AutoConnectionType,
}

#[derive(Debug)]
pub enum AutoConnectionType {
    PortNameMatch,
    SemanticMatch,
    PatternMatch,
}

// Error types
#[derive(Debug, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Glob pattern error: {0}")]
    GlobError(String),
    #[error("File access error for {0}: {1}")]
    FileAccessError(PathBuf, String),
    #[error("Load error for {0}: {1}")]
    LoadError(PathBuf, String),
    #[error("Unsupported file format: {0}")]
    UnsupportedFormat(PathBuf),
    #[error("Invalid file name: {0}")]
    InvalidFileName(PathBuf),
    #[error("Path error: {0}")]
    PathError(String),
    #[error("Duplicate graph name '{0}' found in namespaces: {1:?}")]
    DuplicateGraphName(String, Vec<String>),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}
