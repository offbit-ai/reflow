//! Workspace discovery system implementing the two-tier approach:
//! 1. Extended GraphExport (first tier) - backward compatible extensions
//! 2. WorkspaceGraphExport (second tier) - discovery metadata enrichment

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use glob::glob;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs;

use crate::graph::types::{GraphExport, WorkspaceGraphExport, WorkspaceMetadata, WorkspaceFileFormat, 
    ResolvedDependency, AutoDiscoveredConnection, InterfaceAnalysis, DependencyResolutionStatus,
    DiscoveryMethod, InterfaceTypeMismatch, MismatchSeverity};

use super::{GraphComposition, GraphLoader, GraphMetadata, GraphSource, LoadError};

/// Configuration for workspace discovery
#[derive(Debug, Clone)]
pub struct WorkspaceConfig {
    pub root_path: PathBuf,
    pub graph_patterns: Vec<String>,
    pub excluded_paths: Vec<String>,
    pub max_depth: Option<usize>,
    pub namespace_strategy: NamespaceStrategy,
    pub auto_connect: bool,
    pub dependency_resolution: DependencyResolutionStrategy,
}

#[derive(Debug, Clone)]
pub enum NamespaceStrategy {
    FolderStructure,        // Use folder structure as namespace
    Flatten,               // Flatten all graphs to root namespace
    FileBasedPrefix,       // Use filename prefix as namespace
}

#[derive(Debug, Clone)]
pub enum DependencyResolutionStrategy {
    Automatic,             // Automatically detect and resolve dependencies
    Explicit,              // Only use explicitly declared dependencies
    None,                  // No dependency resolution
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

/// Main workspace discovery service
pub struct WorkspaceDiscovery {
    config: WorkspaceConfig,
    loader: GraphLoader,
    namespace_builder: NamespaceBuilder,
    dependency_analyzer: DependencyAnalyzer,
    interface_analyzer: InterfaceAnalyzer,
    auto_connector: AutoConnector,
}

impl WorkspaceDiscovery {
    pub fn new(config: WorkspaceConfig) -> Self {
        WorkspaceDiscovery {
            config,
            loader: GraphLoader::new(),
            namespace_builder: NamespaceBuilder::new(),
            dependency_analyzer: DependencyAnalyzer::new(),
            interface_analyzer: InterfaceAnalyzer::new(),
            auto_connector: AutoConnector::new(),
        }
    }

    /// Discover all graphs in the workspace and return enriched composition
    pub async fn discover_workspace(&self) -> Result<WorkspaceComposition, DiscoveryError> {
        println!("🔍 Discovering graphs in workspace: {}", self.config.root_path.display());
        
        // 1. Discover all graph files
        let discovered_files = self.discover_graph_files().await?;
        println!("📁 Found {} graph files", discovered_files.len());
        
        // 2. Load base graphs (first tier - GraphExport)
        let loaded_graphs = self.load_base_graphs(&discovered_files).await?;
        println!("📊 Successfully loaded {} graphs", loaded_graphs.len());
        
        // 3. Enrich with workspace metadata (second tier - WorkspaceGraphExport)
        let workspace_graphs = self.enrich_with_workspace_metadata(loaded_graphs, &discovered_files).await?;
        println!("✨ Enriched graphs with workspace metadata");
        
        // 4. Analyze dependencies and interfaces
        let analysis = self.analyze_workspace(&workspace_graphs).await?;
        println!("🔍 Completed workspace analysis");
        
        // 5. Build namespace mappings
        let namespace_mappings = self.build_namespace_mappings(&workspace_graphs)?;
        
        // 6. Create composition from discovered graphs
        let composition = self.build_graph_composition(&workspace_graphs, &analysis).await?;
        
        let workspace_composition = WorkspaceComposition {
            composition,
            discovered_graphs: workspace_graphs,
            discovered_namespaces: namespace_mappings,
            analysis,
            workspace_root: self.config.root_path.clone(),
        };
        
        println!("✅ Workspace discovery completed with {} namespaces", 
            workspace_composition.discovered_namespaces.len());
        
        Ok(workspace_composition)
    }

    async fn discover_graph_files(&self) -> Result<Vec<DiscoveredGraphFile>, DiscoveryError> {
        let mut discovered_files = Vec::new();
        
        for pattern in &self.config.graph_patterns {
            let full_pattern = self.config.root_path.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();
            
            for entry in glob(&pattern_str).map_err(|e| DiscoveryError::GlobError(e.to_string()))? {
                match entry {
                    Ok(path) => {
                        if self.should_exclude_path(&path)? {
                            continue;
                        }
                        
                        if let Some(max_depth) = self.config.max_depth {
                            let relative_path = path.strip_prefix(&self.config.root_path)
                                .unwrap_or(&path);
                            if relative_path.components().count() > max_depth {
                                continue;
                            }
                        }
                        
                        let file_info = self.analyze_graph_file(&path).await?;
                        discovered_files.push(file_info);
                    },
                    Err(e) => {
                        eprintln!("⚠️  Warning: Error reading path: {}", e);
                    }
                }
            }
        }
        
        discovered_files.sort_by(|a, b| a.namespace.cmp(&b.namespace));
        Ok(discovered_files)
    }

    async fn load_base_graphs(&self, files: &[DiscoveredGraphFile]) -> Result<Vec<GraphWithFileInfo>, DiscoveryError> {
        let mut graphs = Vec::new();
        
        for file in files {
            println!("📈 Loading graph: {} ({})", file.graph_name, file.path.display());
            
            match self.load_base_graph_file(file).await {
                Ok(graph_with_info) => {
                    graphs.push(graph_with_info);
                },
                Err(e) => {
                    eprintln!("❌ Failed to load {}: {}", file.path.display(), e);
                    // Continue with other graphs - be resilient
                }
            }
        }
        
        Ok(graphs)
    }

    async fn load_base_graph_file(&self, file: &DiscoveredGraphFile) -> Result<GraphWithFileInfo, DiscoveryError> {
        let source = GraphSource::JsonFile(file.path.to_string_lossy().to_string());
        let mut graph_export = self.loader.load_graph(source).await
            .map_err(|e| DiscoveryError::LoadError(file.path.clone(), e.to_string()))?;
        
        // Inject basic workspace metadata into the GraphExport properties
        self.inject_basic_metadata(&mut graph_export, file)?;
        
        Ok(GraphWithFileInfo {
            graph: graph_export,
            file_info: file.clone(),
        })
    }

    fn inject_basic_metadata(&self, graph: &mut GraphExport, file: &DiscoveredGraphFile) -> Result<(), DiscoveryError> {
        // Ensure the graph has a name
        if !graph.properties.contains_key("name") {
            graph.properties.insert("name".to_string(), 
                serde_json::Value::String(file.graph_name.clone()));
        }
        
        // Inject namespace if not already set
        if !graph.properties.contains_key("namespace") {
            graph.properties.insert("namespace".to_string(), 
                serde_json::Value::String(file.namespace.clone()));
        }
        
        Ok(())
    }

    async fn enrich_with_workspace_metadata(
        &self, 
        base_graphs: Vec<GraphWithFileInfo>, 
        discovered_files: &[DiscoveredGraphFile]
    ) -> Result<Vec<WorkspaceGraphExport>, DiscoveryError> {
        let mut workspace_graphs = Vec::new();
        
        for graph_with_info in base_graphs {
            let workspace_metadata = self.create_workspace_metadata(&graph_with_info.file_info).await?;
            
            let workspace_graph = WorkspaceGraphExport::from_graph_export(
                graph_with_info.graph,
                workspace_metadata
            );
            
            workspace_graphs.push(workspace_graph);
        }
        
        Ok(workspace_graphs)
    }

    async fn create_workspace_metadata(&self, file_info: &DiscoveredGraphFile) -> Result<WorkspaceMetadata, DiscoveryError> {
        let last_modified = file_info.modified
            .map(|time| time.duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_else(|| "unknown".to_string()))
                .unwrap_or_else(|_| "unknown".to_string()));

        Ok(WorkspaceMetadata {
            discovered_namespace: file_info.namespace.clone(),
            source_path: file_info.path.strip_prefix(&self.config.root_path)
                .unwrap_or(&file_info.path)
                .to_string_lossy()
                .to_string(),
            source_format: match file_info.format {
                GraphFileFormat::Json => WorkspaceFileFormat::Json,
                GraphFileFormat::Yaml => WorkspaceFileFormat::Yaml,
            },
            discovered_at: chrono::Utc::now().to_rfc3339(),
            file_size: file_info.size_bytes,
            last_modified,
            resolved_dependencies: Vec::new(), // Will be populated during analysis
            auto_connections: Vec::new(),       // Will be populated during analysis
            interface_analysis: InterfaceAnalysis::default(),
        })
    }

    async fn analyze_workspace(&self, workspace_graphs: &[WorkspaceGraphExport]) -> Result<WorkspaceAnalysis, DiscoveryError> {
        // Analyze dependencies
        let dependencies = self.dependency_analyzer.analyze_dependencies(workspace_graphs)?;
        
        // Analyze interfaces
        let (exposed_interfaces, required_interfaces) = self.interface_analyzer.analyze_interfaces(workspace_graphs)?;
        
        // Discover auto-connections
        let auto_connections = if self.config.auto_connect {
            self.auto_connector.discover_connections(workspace_graphs, &exposed_interfaces, &required_interfaces)?
        } else {
            Vec::new()
        };
        
        Ok(WorkspaceAnalysis {
            dependencies,
            exposed_interfaces,
            required_interfaces,
            auto_connections,
        })
    }

    async fn build_graph_composition(&self, workspace_graphs: &[WorkspaceGraphExport], analysis: &WorkspaceAnalysis) -> Result<GraphComposition, DiscoveryError> {
        let mut sources = Vec::new();
        
        // Convert workspace graphs back to sources
        for workspace_graph in workspace_graphs {
            let source = GraphSource::GraphExport(workspace_graph.graph.clone());
            sources.push(source);
        }
        
        // TODO: Convert auto-connections to composition connections
        let connections = Vec::new();
        
        let composition = GraphComposition {
            sources,
            connections,
            shared_resources: Vec::new(),
            properties: HashMap::from([
                ("name".to_string(), serde_json::Value::String("workspace_composition".to_string())),
                ("composed_from_workspace".to_string(), serde_json::Value::Bool(true)),
            ]),
            case_sensitive: Some(false),
            metadata: None,
        };
        
        Ok(composition)
    }

    // Helper methods
    async fn analyze_graph_file(&self, path: &Path) -> Result<DiscoveredGraphFile, DiscoveryError> {
        let metadata = fs::metadata(path).await
            .map_err(|e| DiscoveryError::FileAccessError(path.to_path_buf(), e.to_string()))?;
        
        let namespace = self.namespace_builder.build_namespace(path, &self.config)?;
        let format = self.detect_file_format(path)?;
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

    fn build_namespace_mappings(&self, workspace_graphs: &[WorkspaceGraphExport]) -> Result<HashMap<String, NamespaceInfo>, DiscoveryError> {
        let mut namespaces = HashMap::new();
        
        for workspace_graph in workspace_graphs {
            let namespace = &workspace_graph.workspace_metadata.discovered_namespace;
            let graph_name = workspace_graph.graph_name().unwrap_or("unnamed").to_string();
            let path = PathBuf::from(&workspace_graph.workspace_metadata.source_path);
            
            let entry = namespaces.entry(namespace.clone()).or_insert_with(|| NamespaceInfo {
                namespace: namespace.clone(),
                graphs: Vec::new(),
                path: path.parent().unwrap_or(&path).to_path_buf(),
                graph_count: 0,
            });
            
            entry.graphs.push(graph_name);
            entry.graph_count += 1;
        }
        
        println!("📊 Namespace distribution:");
        for (namespace, info) in &namespaces {
            println!("  📁 {}: {} graphs", namespace, info.graph_count);
        }
        
        Ok(namespaces)
    }

    fn should_exclude_path(&self, path: &Path) -> Result<bool, DiscoveryError> {
        let path_str = path.to_string_lossy();
        
        for exclude_pattern in &self.config.excluded_paths {
            if glob::Pattern::new(exclude_pattern)
                .map_err(|e| DiscoveryError::GlobError(e.to_string()))?
                .matches(&path_str) {
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
        let filename = path.file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| DiscoveryError::InvalidFileName(path.to_path_buf()))?;
        
        let name = if filename.ends_with(".graph") {
            &filename[..filename.len() - 6]
        } else {
            filename
        };
        
        Ok(name.to_string())
    }
}

// Supporting structures and implementations

struct NamespaceBuilder;

impl NamespaceBuilder {
    pub fn new() -> Self {
        NamespaceBuilder
    }

    pub fn build_namespace(&self, path: &Path, config: &WorkspaceConfig) -> Result<String, DiscoveryError> {
        match config.namespace_strategy {
            NamespaceStrategy::FolderStructure => {
                if let Some(parent) = path.parent() {
                    let relative_path = parent.strip_prefix(&config.root_path)
                        .unwrap_or(parent);
                    Ok(relative_path.to_string_lossy().replace('\\', "/"))
                } else {
                    Ok("root".to_string())
                }
            }
            NamespaceStrategy::Flatten => Ok("default".to_string()),
            NamespaceStrategy::FileBasedPrefix => {
                let filename = path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unnamed");
                let parts: Vec<&str> = filename.split('_').collect();
                if parts.len() > 1 {
                    Ok(parts[0].to_string())
                } else {
                    Ok("default".to_string())
                }
            }
        }
    }
}

struct DependencyAnalyzer;

impl DependencyAnalyzer {
    pub fn new() -> Self {
        DependencyAnalyzer
    }

    pub fn analyze_dependencies(&self, workspace_graphs: &[WorkspaceGraphExport]) -> Result<Vec<DependencyInfo>, DiscoveryError> {
        let mut dependencies = Vec::new();
        
        for workspace_graph in workspace_graphs {
            let graph_name = workspace_graph.graph_name().unwrap_or("unnamed").to_string();
            
            // Analyze explicit dependencies from graph_dependencies
            for dep in &workspace_graph.graph.graph_dependencies {
                dependencies.push(DependencyInfo {
                    dependent: graph_name.clone(),
                    dependency: dep.graph_name.clone(),
                    dependency_type: DependencyType::Explicit,
                });
            }
            
            // Analyze dependencies from properties
            if let Some(deps_value) = workspace_graph.graph.properties.get("dependencies") {
                if let Some(deps_array) = deps_value.as_array() {
                    for dep_value in deps_array {
                        if let Some(dep_name) = dep_value.as_str() {
                            dependencies.push(DependencyInfo {
                                dependent: graph_name.clone(),
                                dependency: dep_name.to_string(),
                                dependency_type: DependencyType::Explicit,
                            });
                        }
                    }
                }
            }
        }
        
        Ok(dependencies)
    }
}

struct InterfaceAnalyzer;

impl InterfaceAnalyzer {
    pub fn new() -> Self {
        InterfaceAnalyzer
    }

    pub fn analyze_interfaces(&self, workspace_graphs: &[WorkspaceGraphExport]) -> Result<(Vec<InterfaceInfo>, Vec<InterfaceInfo>), DiscoveryError> {
        let mut exposed_interfaces = Vec::new();
        let mut required_interfaces = Vec::new();
        
        for workspace_graph in workspace_graphs {
            let graph_name = workspace_graph.graph_name().unwrap_or("unnamed").to_string();
            let namespace = &workspace_graph.workspace_metadata.discovered_namespace;
            
            // Analyze provided interfaces
            for (interface_name, interface_def) in &workspace_graph.graph.provided_interfaces {
                exposed_interfaces.push(InterfaceInfo {
                    namespace: namespace.clone(),
                    graph_name: graph_name.clone(),
                    process_name: interface_def.process_name.clone(),
                    port_name: interface_def.port_name.clone(),
                    data_type: interface_def.data_type.clone(),
                });
            }
            
            // Analyze required interfaces
            for (interface_name, interface_def) in &workspace_graph.graph.required_interfaces {
                required_interfaces.push(InterfaceInfo {
                    namespace: namespace.clone(),
                    graph_name: graph_name.clone(),
                    process_name: interface_def.process_name.clone(),
                    port_name: interface_def.port_name.clone(),
                    data_type: interface_def.data_type.clone(),
                });
            }
        }
        
        Ok((exposed_interfaces, required_interfaces))
    }
}

struct AutoConnector;

impl AutoConnector {
    pub fn new() -> Self {
        AutoConnector
    }

    pub fn discover_connections(
        &self,
        _workspace_graphs: &[WorkspaceGraphExport],
        exposed_interfaces: &[InterfaceInfo],
        required_interfaces: &[InterfaceInfo],
    ) -> Result<Vec<AutoConnection>, DiscoveryError> {
        let mut auto_connections = Vec::new();
        
        // Simple matching by data type
        for required in required_interfaces {
            for exposed in exposed_interfaces {
                if required.graph_name != exposed.graph_name {
                    let confidence = self.calculate_compatibility_confidence(exposed, required);
                    
                    if confidence > 0.5 {
                        auto_connections.push(AutoConnection {
                            from_graph: exposed.graph_name.clone(),
                            from_interface: format!("{}.{}", exposed.process_name, exposed.port_name),
                            to_graph: required.graph_name.clone(),
                            to_interface: format!("{}.{}", required.process_name, required.port_name),
                            confidence,
                        });
                    }
                }
            }
        }
        
        Ok(auto_connections)
    }

    fn calculate_compatibility_confidence(&self, exposed: &InterfaceInfo, required: &InterfaceInfo) -> f64 {
        let mut confidence: f64 = 0.0;
        
        // Data type matching
        match (&exposed.data_type, &required.data_type) {
            (Some(exposed_type), Some(required_type)) => {
                if exposed_type == required_type {
                    confidence += 0.8;
                } else if self.are_compatible_types(exposed_type, required_type) {
                    confidence += 0.6;
                }
            }
            _ => confidence += 0.3, // Unknown types get some base score
        }
        
        // Port name similarity
        if exposed.port_name.to_lowercase().contains(&required.port_name.to_lowercase()) ||
           required.port_name.to_lowercase().contains(&exposed.port_name.to_lowercase()) {
            confidence += 0.2;
        }
        
        confidence.min(1.0)
    }

    fn are_compatible_types(&self, type1: &str, type2: &str) -> bool {
        // Simple type compatibility rules
        match (type1.to_lowercase().as_str(), type2.to_lowercase().as_str()) {
            ("string", "text") | ("text", "string") => true,
            ("int", "integer") | ("integer", "int") => true,
            ("float", "number") | ("number", "float") => true,
            _ => false,
        }
    }
}

// Data structures

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
pub struct GraphWithFileInfo {
    pub graph: GraphExport,
    pub file_info: DiscoveredGraphFile,
}

#[derive(Debug)]
pub struct WorkspaceComposition {
    pub composition: GraphComposition,
    pub discovered_graphs: Vec<WorkspaceGraphExport>,
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
    pub dependencies: Vec<DependencyInfo>,
    pub exposed_interfaces: Vec<InterfaceInfo>,
    pub required_interfaces: Vec<InterfaceInfo>,
    pub auto_connections: Vec<AutoConnection>,
}

#[derive(Debug)]
pub struct DependencyInfo {
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
pub struct InterfaceInfo {
    pub namespace: String,
    pub graph_name: String,
    pub process_name: String,
    pub port_name: String,
    pub data_type: Option<String>,
}

#[derive(Debug)]
pub struct AutoConnection {
    pub from_graph: String,
    pub from_interface: String,
    pub to_graph: String,
    pub to_interface: String,
    pub confidence: f64,
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
