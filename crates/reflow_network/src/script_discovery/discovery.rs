use super::types::*;
use super::extractor::MetadataExtractor;
use anyhow::Result;
use glob::glob;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn, debug};

pub struct ScriptActorDiscovery {
    config: ScriptDiscoveryConfig,
    metadata_extractor: MetadataExtractor,
}

impl ScriptActorDiscovery {
    pub fn new(config: ScriptDiscoveryConfig) -> Self {
        Self {
            config,
            metadata_extractor: MetadataExtractor::new(),
        }
    }
    
    /// Discover all script actors in the workspace
    pub async fn discover_actors(&self) -> Result<DiscoveredActors> {
        info!("Discovering script actors in: {}", self.config.root_path.display());
        
        // Find all actor files
        let actor_files = self.find_actor_files().await?;
        info!("Found {} potential actor files", actor_files.len());
        
        // Process each file
        let mut discovered_actors = Vec::new();
        let mut failed_actors = Vec::new();
        
        for file in actor_files {
            match self.process_actor_file(&file).await {
                Ok(actor) => {
                    info!("Discovered actor: {} ({})", 
                        actor.component, 
                        file.display()
                    );
                    discovered_actors.push(actor);
                }
                Err(e) => {
                    warn!("Failed to process {}: {}", file.display(), e);
                    failed_actors.push(FailedActor {
                        file_path: file,
                        error: e.to_string(),
                    });
                }
            }
        }
        
        // Build namespace mappings
        let namespaces = self.build_namespace_mappings(&discovered_actors)?;
        
        // Validate actors if configured
        if self.config.validate_metadata {
            self.validate_actors(&mut discovered_actors).await?;
        }
        
        info!("Discovery complete: {} actors found, {} failed", 
            discovered_actors.len(), 
            failed_actors.len()
        );
        
        Ok(DiscoveredActors {
            actors: discovered_actors,
            failed: failed_actors,
            namespaces,
            discovery_time: chrono::Utc::now(),
        })
    }
    
    /// Find all actor files matching the configured patterns
    async fn find_actor_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        
        for pattern in &self.config.patterns {
            let full_pattern = self.config.root_path.join(pattern);
            let pattern_str = full_pattern.to_string_lossy();
            
            debug!("Searching for pattern: {}", pattern_str);
            
            for entry in glob(&pattern_str)? {
                if let Ok(path) = entry {
                    if self.should_exclude(&path) {
                        debug!("Excluding: {}", path.display());
                        continue;
                    }
                    
                    if self.check_depth(&path) {
                        debug!("Found actor file: {}", path.display());
                        files.push(path);
                    }
                }
            }
        }
        
        Ok(files)
    }
    
    /// Check if a path should be excluded
    fn should_exclude(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        
        for exclude_pattern in &self.config.excluded_paths {
            if path_str.contains(&exclude_pattern.replace("**", "").replace("*", "")) {
                return true;
            }
        }
        
        false
    }
    
    /// Check if path depth is within configured limits
    fn check_depth(&self, path: &Path) -> bool {
        if let Some(max_depth) = self.config.max_depth {
            let depth = path.components().count();
            depth <= max_depth
        } else {
            true
        }
    }
    
    /// Process a single actor file
    async fn process_actor_file(&self, file: &Path) -> Result<DiscoveredScriptActor> {
        // Determine runtime from file extension
        let runtime = self.determine_runtime(file)?;
        
        // Extract metadata based on runtime
        let metadata = match runtime {
            ScriptRuntime::Python => {
                self.metadata_extractor.extract_python_metadata(file).await?
            }
            ScriptRuntime::JavaScript => {
                self.metadata_extractor.extract_javascript_metadata(file).await?
            }
        };
        
        // Build workspace metadata
        let workspace_metadata = self.build_workspace_metadata(file, &metadata)?;
        
        Ok(DiscoveredScriptActor {
            component: metadata.component,
            description: metadata.description,
            file_path: file.to_path_buf(),
            runtime,
            inports: metadata.inports,
            outports: metadata.outports,
            workspace_metadata,
        })
    }
    
    /// Determine script runtime from file extension
    fn determine_runtime(&self, path: &Path) -> Result<ScriptRuntime> {
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| anyhow::anyhow!("No file extension found"))?;
        
        ScriptRuntime::from_extension(ext)
            .ok_or_else(|| anyhow::anyhow!("Unknown script type: {}", ext))
    }
    
    /// Build workspace metadata for an actor
    fn build_workspace_metadata(
        &self,
        file: &Path,
        metadata: &ExtractedMetadata
    ) -> Result<ScriptActorMetadata> {
        // Generate namespace from file path
        let namespace = self.generate_namespace(file)?;
        
        // Calculate file hash
        let source_hash = self.calculate_file_hash(file)?;
        
        // Get file modification time
        let last_modified = std::fs::metadata(file)?
            .modified()?
            .into();
        
        Ok(ScriptActorMetadata {
            namespace,
            version: metadata.version.clone(),
            author: None, // TODO: Extract from metadata
            dependencies: metadata.dependencies.clone(),
            runtime_requirements: RuntimeRequirements {
                runtime_version: "latest".to_string(),
                memory_limit: "512MB".to_string(),
                cpu_limit: Some(0.5),
                timeout: 30,
                env_vars: HashMap::new(),
            },
            config_schema: metadata.config_schema.clone(),
            tags: metadata.tags.clone(),
            category: metadata.category.clone(),
            source_hash,
            last_modified,
        })
    }
    
    /// Generate namespace from file path
    fn generate_namespace(&self, file: &Path) -> Result<String> {
        let relative = file.strip_prefix(&self.config.root_path)
            .unwrap_or(file);
        
        let components: Vec<&str> = relative
            .parent()
            .unwrap_or(Path::new(""))
            .components()
            .filter_map(|c| {
                if let std::path::Component::Normal(s) = c {
                    s.to_str()
                } else {
                    None
                }
            })
            .collect();
        
        if components.is_empty() {
            Ok("default".to_string())
        } else {
            Ok(components.join("."))
        }
    }
    
    /// Calculate SHA256 hash of file contents
    fn calculate_file_hash(&self, file: &Path) -> Result<String> {
        use sha2::{Sha256, Digest};
        use std::fs;
        
        let contents = fs::read(file)?;
        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();
        
        Ok(format!("{:x}", result))
    }
    
    /// Build namespace mappings from discovered actors
    fn build_namespace_mappings(
        &self,
        actors: &[DiscoveredScriptActor]
    ) -> Result<HashMap<String, Vec<String>>> {
        let mut namespaces: HashMap<String, Vec<String>> = HashMap::new();
        
        for actor in actors {
            let namespace = &actor.workspace_metadata.namespace;
            namespaces
                .entry(namespace.clone())
                .or_insert_with(Vec::new)
                .push(actor.component.clone());
        }
        
        Ok(namespaces)
    }
    
    /// Validate discovered actors
    async fn validate_actors(&self, actors: &mut Vec<DiscoveredScriptActor>) -> Result<()> {
        for actor in actors.iter_mut() {
            // Validate port definitions
            for port in &actor.inports {
                if port.name.is_empty() {
                    warn!("Actor {} has unnamed input port", actor.component);
                }
            }
            
            for port in &actor.outports {
                if port.name.is_empty() {
                    warn!("Actor {} has unnamed output port", actor.component);
                }
            }
            
            // Validate metadata
            if actor.description.is_empty() {
                warn!("Actor {} has no description", actor.component);
            }
            
            // TODO: Add more validation rules
        }
        
        Ok(())
    }
}