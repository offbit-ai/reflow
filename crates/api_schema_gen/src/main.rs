//! api-schema-gen — OpenAPI spec discovery and registry enrichment tool.
//!
//! Fetches OpenAPI specs for services in the api_service_registry,
//! extracts operations with full parameter/response metadata,
//! and merges them into the registry without losing manual overrides.
//!
//! Usage:
//!   api-schema-gen enrich --registry path/to/registry.json [--services stripe,github]
//!   api-schema-gen enrich-url --url <spec-url> --service-id <id>
//!   api-schema-gen validate --registry path/to/registry.json
//!   api-schema-gen discover --output discovered.json
//!   api-schema-gen stats --registry path/to/registry.json

mod codegen;
mod discovery;
mod doc_crawler;
mod enricher;
mod extractor;
mod openapi;
mod registry;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tracing::{info, warn};

#[derive(Parser)]
#[command(
    name = "api-schema-gen",
    about = "OpenAPI spec discovery and registry enrichment"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Enrich existing registry services by fetching their OpenAPI specs
    Enrich {
        /// Path to the service registry (JSON or YAML)
        #[arg(short, long)]
        registry: PathBuf,

        /// Output path (defaults to overwriting input)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Comma-separated list of service IDs to enrich (default: all)
        #[arg(short, long)]
        services: Option<String>,

        /// Max concurrent spec fetches
        #[arg(long, default_value = "5")]
        concurrency: usize,

        /// Also output YAML
        #[arg(long)]
        yaml: bool,
    },

    /// Enrich from a specific OpenAPI spec URL
    EnrichUrl {
        /// Path to the service registry
        #[arg(short, long)]
        registry: PathBuf,

        /// URL to the OpenAPI spec
        #[arg(long)]
        url: String,

        /// Service ID to enrich or create
        #[arg(long)]
        service_id: String,

        /// Output path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Validate registry for common issues
    Validate {
        /// Path to the service registry
        #[arg(short, long)]
        registry: PathBuf,
    },

    /// Discover new APIs from public directories
    Discover {
        /// Output path for discovered APIs
        #[arg(short, long, default_value = "discovered_apis.json")]
        output: PathBuf,

        /// Max APIs to discover
        #[arg(long, default_value = "100")]
        limit: usize,
    },

    /// Show registry statistics
    Stats {
        /// Path to the service registry
        #[arg(short, long)]
        registry: PathBuf,
    },

    /// Parse a spec URL and print extracted operations (dry run)
    Inspect {
        /// URL to the OpenAPI spec
        #[arg(long)]
        url: String,
    },

    /// Crawl HTML doc pages for services without raw specs, output LLM-ready prompts
    Crawl {
        /// Path to the service registry
        #[arg(short, long)]
        registry: PathBuf,

        /// Output directory for crawl results (one JSON per service)
        #[arg(short, long, default_value = "crawl_output")]
        output: PathBuf,

        /// Comma-separated list of service IDs to crawl (default: all unenriched)
        #[arg(short, long)]
        services: Option<String>,

        /// Max concurrent fetches
        #[arg(long, default_value = "10")]
        concurrency: usize,
    },

    /// Apply LLM-extracted operations from crawl output back into the registry
    ApplyCrawl {
        /// Path to the service registry
        #[arg(short, long)]
        registry: PathBuf,

        /// Directory containing LLM output files (service_id.json)
        #[arg(short, long)]
        input: PathBuf,

        /// Output path for updated registry
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Generate Rust actor source files from the enriched registry
    Codegen {
        /// Path to the enriched service registry (JSON)
        #[arg(short, long)]
        registry: PathBuf,

        /// Output directory for generated Rust source files
        #[arg(short, long, default_value = "../reflow_components/src/api")]
        output: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "api_schema_gen=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Enrich {
            registry,
            output,
            services,
            concurrency,
            yaml,
        } => cmd_enrich(registry, output, services, concurrency, yaml).await,

        Commands::EnrichUrl {
            registry,
            url,
            service_id,
            output,
        } => cmd_enrich_url(registry, url, service_id, output).await,

        Commands::Validate { registry } => cmd_validate(registry),

        Commands::Discover { output, limit } => cmd_discover(output, limit).await,

        Commands::Stats { registry } => cmd_stats(registry),

        Commands::Inspect { url } => cmd_inspect(url).await,

        Commands::Crawl {
            registry,
            output,
            services,
            concurrency,
        } => cmd_crawl(registry, output, services, concurrency).await,

        Commands::ApplyCrawl {
            registry,
            input,
            output,
        } => cmd_apply_crawl(registry, input, output),

        Commands::Codegen { registry, output } => cmd_codegen(registry, output),
    }
}

async fn cmd_enrich(
    registry_path: PathBuf,
    output: Option<PathBuf>,
    services: Option<String>,
    concurrency: usize,
    yaml: bool,
) -> Result<()> {
    let mut registry = load_registry(&registry_path)?;

    let service_filter: Option<Vec<String>> =
        services.map(|s| s.split(',').map(|s| s.trim().to_string()).collect());

    let results =
        enricher::enrich_registry(&mut registry, service_filter.as_deref(), concurrency).await;

    // Print summary
    println!("\n{}", "=".repeat(60));
    println!("Enrichment Results");
    println!("{}", "=".repeat(60));

    let mut total_added = 0;
    let mut success_count = 0;
    let mut error_count = 0;

    for result in &results {
        let added = result.new_op_count.saturating_sub(result.original_op_count);
        total_added += added;

        if let Some(ref err) = result.error {
            error_count += 1;
            println!("  FAIL  {:<25} — {}", result.service_id, err);
        } else {
            success_count += 1;
            println!(
                "  OK    {:<25} — {} ops ({} → {})",
                result.service_id,
                if added > 0 {
                    format!("+{}", added)
                } else {
                    "no change".to_string()
                },
                result.original_op_count,
                result.new_op_count,
            );
        }
    }

    println!("{}", "-".repeat(60));
    println!(
        "Total: {} succeeded, {} failed, +{} operations added",
        success_count, error_count, total_added
    );

    // Save
    let out_path = output.unwrap_or(registry_path);
    let ext = out_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("json");
    if ext == "yaml" || ext == "yml" {
        registry.to_yaml_file(&out_path)?;
    } else {
        registry.to_json_file(&out_path)?;
    }
    println!("\nSaved to {}", out_path.display());

    if yaml {
        let yaml_path = out_path.with_extension("yaml");
        registry.to_yaml_file(&yaml_path)?;
        println!("Saved YAML to {}", yaml_path.display());
    }

    Ok(())
}

async fn cmd_enrich_url(
    registry_path: PathBuf,
    url: String,
    service_id: String,
    output: Option<PathBuf>,
) -> Result<()> {
    let mut registry = load_registry(&registry_path)?;

    info!("Fetching spec from {}", url);
    let spec = openapi::fetch_openapi_spec(&url).await?;
    let operations = extractor::extract_operations(&spec);

    println!(
        "Extracted {} operations from {}",
        operations.len(),
        spec.title
    );

    if let Some(service) = registry.services.get_mut(&service_id) {
        let original = service.operations.len();
        // Merge
        let mut new_ops = operations;
        let existing_keys: std::collections::HashSet<String> = service
            .operations
            .iter()
            .map(|op| format!("{}_{}", op.verb, op.resource))
            .collect();
        new_ops.retain(|op| !existing_keys.contains(&format!("{}_{}", op.verb, op.resource)));
        service.operations.extend(new_ops);
        println!(
            "Enriched '{}': {} → {} operations",
            service_id,
            original,
            service.operations.len()
        );
    } else {
        warn!(
            "Service '{}' not found in registry. Creating new entry.",
            service_id
        );
        println!(
            "Service '{}' not in registry — use 'enrich' with --services to add",
            service_id
        );
        return Ok(());
    }

    let out_path = output.unwrap_or(registry_path);
    registry.to_json_file(&out_path)?;
    println!("Saved to {}", out_path.display());

    Ok(())
}

fn cmd_validate(registry_path: PathBuf) -> Result<()> {
    let registry = load_registry(&registry_path)?;
    let issues = enricher::validate_registry(&registry);

    if issues.is_empty() {
        println!("No issues found.");
        return Ok(());
    }

    let mut errors = 0;
    let mut warnings = 0;
    let mut infos = 0;

    for issue in &issues {
        match issue.severity {
            enricher::Severity::Error => errors += 1,
            enricher::Severity::Warning => warnings += 1,
            enricher::Severity::Info => infos += 1,
        }
        println!(
            "  [{}] {}: {}",
            issue.severity, issue.service_id, issue.message
        );
    }

    println!(
        "\nTotal: {} errors, {} warnings, {} info",
        errors, warnings, infos
    );

    Ok(())
}

async fn cmd_discover(output: PathBuf, limit: usize) -> Result<()> {
    let discovered = discovery::discover_from_apis_guru().await?;

    let limited: Vec<_> = discovered.into_iter().take(limit).collect();
    println!("Discovered {} APIs", limited.len());

    // Convert to JSON
    let json: Vec<serde_json::Value> = limited
        .iter()
        .map(|api| {
            serde_json::json!({
                "name": api.name,
                "description": api.description,
                "spec_url": api.spec_url,
                "spec_format": api.spec_format,
                "source": api.source,
                "category": api.category,
                "suggested_id": discovery::name_to_service_id(&api.name),
            })
        })
        .collect();

    let content = serde_json::to_string_pretty(&json)?;
    std::fs::write(&output, content)?;
    println!("Saved to {}", output.display());

    Ok(())
}

fn cmd_stats(registry_path: PathBuf) -> Result<()> {
    let registry = load_registry(&registry_path)?;

    let total_services = registry.services.len();
    let total_ops: usize = registry.services.values().map(|s| s.operations.len()).sum();
    let total_params: usize = registry
        .services
        .values()
        .flat_map(|s| &s.operations)
        .map(|op| op.parameters.as_ref().map(|p| p.len()).unwrap_or(0))
        .sum();

    let ops_with_desc = registry
        .services
        .values()
        .flat_map(|s| &s.operations)
        .filter(|op| op.description.is_some())
        .count();

    let params_with_desc = registry
        .services
        .values()
        .flat_map(|s| &s.operations)
        .flat_map(|op| op.parameters.as_ref().into_iter().flatten())
        .filter(|p| p.description.is_some())
        .count();

    let ops_with_response = registry
        .services
        .values()
        .flat_map(|s| &s.operations)
        .filter(|op| op.response_schema.is_some())
        .count();

    println!("{}", "=".repeat(50));
    println!("Registry Statistics");
    println!("{}", "=".repeat(50));
    println!("  Services:              {}", total_services);
    println!("  Total operations:      {}", total_ops);
    println!(
        "  Avg ops/service:       {:.1}",
        total_ops as f64 / total_services as f64
    );
    println!("  Total parameters:      {}", total_params);
    println!();
    println!("  Quality:");
    println!(
        "    Ops with description:    {:>4}/{:<4} ({:.0}%)",
        ops_with_desc,
        total_ops,
        if total_ops > 0 {
            ops_with_desc as f64 / total_ops as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "    Params with description: {:>4}/{:<4} ({:.0}%)",
        params_with_desc,
        total_params,
        if total_params > 0 {
            params_with_desc as f64 / total_params as f64 * 100.0
        } else {
            0.0
        }
    );
    println!(
        "    Ops with response schema:{:>4}/{:<4} ({:.0}%)",
        ops_with_response,
        total_ops,
        if total_ops > 0 {
            ops_with_response as f64 / total_ops as f64 * 100.0
        } else {
            0.0
        }
    );

    // Category breakdown
    println!();
    println!("  By category:");
    let mut categories: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for service in registry.services.values() {
        let cat = format!("{:?}", service.category).to_lowercase();
        *categories.entry(cat).or_default() += 1;
    }
    let mut cat_vec: Vec<_> = categories.into_iter().collect();
    cat_vec.sort_by(|a, b| b.1.cmp(&a.1));
    for (cat, count) in cat_vec {
        println!("    {:<25} {}", cat, count);
    }

    // Services by operation count
    println!();
    println!("  Ops per service:");
    let mut svc_ops: Vec<_> = registry
        .services
        .iter()
        .map(|(id, s)| (id.clone(), s.operations.len()))
        .collect();
    svc_ops.sort_by(|a, b| b.1.cmp(&a.1));
    for (id, count) in svc_ops.iter().take(15) {
        println!("    {:<30} {:>3} ops", id, count);
    }
    if svc_ops.len() > 15 {
        println!("    ... and {} more", svc_ops.len() - 15);
    }

    Ok(())
}

async fn cmd_inspect(url: String) -> Result<()> {
    let spec = openapi::fetch_openapi_spec(&url).await?;
    let operations = extractor::extract_operations(&spec);

    println!("{}", "=".repeat(60));
    println!("Spec: {} (v{})", spec.title, spec.version);
    if let Some(ref desc) = spec.description {
        println!("  {}", truncate(desc, 100));
    }
    println!("Servers:");
    for server in &spec.servers {
        println!("  - {}", server.url);
    }
    println!(
        "Security schemes: {:?}",
        spec.security_schemes.keys().collect::<Vec<_>>()
    );
    println!("{}", "=".repeat(60));
    println!("Extracted {} operations:", operations.len());
    println!();

    for op in &operations {
        let param_count = op.parameters.as_ref().map(|p| p.len()).unwrap_or(0);
        let has_response = if op.response_schema.is_some() {
            "+"
        } else {
            " "
        };
        println!(
            "  {:<8} {:<25} {:>6} {:<40} {}params, {}resp",
            op.verb, op.resource, op.method, op.endpoint_pattern, param_count, has_response
        );

        if let Some(ref desc) = op.description {
            println!("           {}", truncate(desc, 70));
        }

        if let Some(ref params) = op.parameters {
            for p in params.iter().take(5) {
                let req = if p.required == Some(true) { "*" } else { " " };
                println!(
                    "             {}{:<20} {:<8} {:<6} {}",
                    req,
                    p.name,
                    p.param_type,
                    p.location,
                    p.description
                        .as_deref()
                        .map(|d| truncate(d, 40))
                        .unwrap_or_default()
                );
            }
            if params.len() > 5 {
                println!("             ... +{} more", params.len() - 5);
            }
        }
        println!();
    }

    Ok(())
}

async fn cmd_crawl(
    registry_path: PathBuf,
    output_dir: PathBuf,
    services: Option<String>,
    concurrency: usize,
) -> Result<()> {
    let registry = load_registry(&registry_path)?;

    // Determine which services need crawling (those with <= 3 ops are likely unenriched)
    let known_spec_ids: std::collections::HashSet<&str> = enricher::get_known_spec_urls()
        .iter()
        .map(|(id, _)| *id)
        .collect();

    let target_services: Vec<(String, registry::Service)> = if let Some(ref filter) = services {
        let ids: Vec<&str> = filter.split(',').map(|s| s.trim()).collect();
        registry
            .services
            .iter()
            .filter(|(id, _)| ids.contains(&id.as_str()))
            .map(|(id, s)| (id.clone(), s.clone()))
            .collect()
    } else {
        // Auto-select: services without known spec URLs that still have minimal ops
        registry
            .services
            .iter()
            .filter(|(id, s)| !known_spec_ids.contains(id.as_str()) && s.operations.len() <= 3)
            .map(|(id, s)| (id.clone(), s.clone()))
            .collect()
    };

    println!(
        "Crawling {} services for HTML doc extraction...",
        target_services.len()
    );

    // Create output directory
    std::fs::create_dir_all(&output_dir)?;

    let mut success = 0;
    let mut failed = 0;

    for chunk in target_services.chunks(concurrency) {
        let mut handles = Vec::new();
        for (service_id, service) in chunk {
            let sid = service_id.clone();
            let svc = service.clone();
            handles.push(async move { doc_crawler::crawl_service(&sid, &svc).await });
        }

        let results = futures::future::join_all(handles).await;
        for result in results {
            if result.doc_contents.is_empty() {
                failed += 1;
                println!(
                    "  SKIP  {:<25} — {}",
                    result.service_id,
                    result.error.as_deref().unwrap_or("no doc content")
                );
                continue;
            }

            // Write prompt files for each service
            let prompts_dir = output_dir.join("prompts");
            std::fs::create_dir_all(&prompts_dir)?;

            let responses_dir = output_dir.join("responses");
            std::fs::create_dir_all(&responses_dir)?;

            for doc in &result.doc_contents {
                let prompt = doc_crawler::build_extraction_prompt(doc);
                let prompt_path = prompts_dir.join(format!("{}.txt", result.service_id));
                std::fs::write(&prompt_path, &prompt)?;
            }

            // Also write the raw doc content for reference
            let doc_json = serde_json::to_string_pretty(&result.doc_contents)?;
            let doc_path = output_dir.join(format!("{}_docs.json", result.service_id));
            std::fs::write(&doc_path, doc_json)?;

            success += 1;
            let total_chars: usize = result
                .doc_contents
                .iter()
                .map(|d| d.text_content.len())
                .sum();
            println!(
                "  OK    {:<25} — {} pages, {} chars of content",
                result.service_id,
                result.doc_contents.len(),
                total_chars
            );
        }
    }

    println!("{}", "-".repeat(60));
    println!("Crawled: {} succeeded, {} failed/skipped", success, failed);
    println!();
    println!("Next steps:");
    println!(
        "  1. Feed prompts from {}/prompts/ to an LLM (in parallel)",
        output_dir.display()
    );
    println!(
        "  2. Save LLM responses as JSON to {}/responses/{{service_id}}.json",
        output_dir.display()
    );
    println!(
        "  3. Run: api-schema-gen apply-crawl -r registry.json -i {}/responses/",
        output_dir.display()
    );

    Ok(())
}

fn cmd_apply_crawl(
    registry_path: PathBuf,
    input_dir: PathBuf,
    output: Option<PathBuf>,
) -> Result<()> {
    let mut registry = load_registry(&registry_path)?;

    let entries = std::fs::read_dir(&input_dir)?;
    let mut applied = 0;
    let mut errors = 0;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }

        let service_id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let content = std::fs::read_to_string(&path)?;
        match doc_crawler::parse_llm_operations(&content) {
            Ok(ops) => {
                if let Some(service) = registry.services.get_mut(&service_id) {
                    let before = service.operations.len();
                    let existing_keys: std::collections::HashSet<String> = service
                        .operations
                        .iter()
                        .map(|op| format!("{}_{}", op.verb, op.resource))
                        .collect();
                    let new_ops: Vec<_> = ops
                        .into_iter()
                        .filter(|op| {
                            !existing_keys.contains(&format!("{}_{}", op.verb, op.resource))
                        })
                        .collect();
                    let added = new_ops.len();
                    service.operations.extend(new_ops);
                    println!(
                        "  OK    {:<25} — +{} ops ({} → {})",
                        service_id,
                        added,
                        before,
                        service.operations.len()
                    );
                    applied += 1;
                } else {
                    println!("  SKIP  {:<25} — not found in registry", service_id);
                }
            }
            Err(e) => {
                println!("  FAIL  {:<25} — {}", service_id, e);
                errors += 1;
            }
        }
    }

    println!("{}", "-".repeat(60));
    println!("Applied: {}, Errors: {}", applied, errors);

    let out_path = output.unwrap_or(registry_path);
    registry.to_json_file(&out_path)?;
    println!("Saved to {}", out_path.display());

    Ok(())
}

fn cmd_codegen(registry_path: PathBuf, output_dir: PathBuf) -> Result<()> {
    info!("Generating Rust actors from {}", registry_path.display());

    let stats = codegen::generate_actors(&registry_path, &output_dir)?;

    println!("Code generation complete:");
    println!("  Services: {}", stats.services);
    println!("  Actors:   {}", stats.actors);
    println!("  Files:    {}", stats.files.len());
    println!("  Output:   {}", output_dir.display());

    Ok(())
}

fn load_registry(path: &PathBuf) -> Result<registry::ServiceRegistry> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
    if ext == "yaml" || ext == "yml" {
        registry::ServiceRegistry::from_yaml_file(path)
    } else {
        registry::ServiceRegistry::from_json_file(path)
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    let s = s.replace('\n', " ").replace('\r', "");
    if s.len() > max_len {
        format!("{}...", &s[..max_len - 3])
    } else {
        s
    }
}
