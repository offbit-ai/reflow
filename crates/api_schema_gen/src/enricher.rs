//! Registry enricher — fetches OpenAPI specs for existing services and enriches them.
//!
//! For each service in the registry that has a schema_url, fetches the spec,
//! extracts operations, and merges them with existing data. Manual overrides
//! (auth config, rate limits, webhooks) are preserved.

use crate::extractor::extract_operations;
use crate::openapi::{fetch_openapi_spec, OpenApiSpec, SecurityScheme};
use crate::registry::{
    ApiKeyConfig, AuthMethod, Authentication, OAuth2Config, Operation, Scope, Service,
    ServiceRegistry,
};
use anyhow::Result;
use std::collections::HashSet;
use tracing::{error, info, warn};

/// Well-known OpenAPI spec URLs for services that publish them.
/// Used as fallback when the registry's schema_url points to docs instead of raw spec.
pub fn get_known_spec_urls() -> Vec<(&'static str, &'static str)> {
    vec![
        // Tier 1: Official repos with verified raw specs
        ("stripe", "https://raw.githubusercontent.com/stripe/openapi/master/openapi/spec3.json"),
        ("github", "https://raw.githubusercontent.com/github/rest-api-description/main/descriptions/api.github.com/api.github.com.json"),
        ("slack", "https://raw.githubusercontent.com/slackapi/slack-api-specs/master/web-api/slack_web_openapi_v2.json"),
        ("twilio", "https://raw.githubusercontent.com/twilio/twilio-oai/main/spec/json/twilio_api_v2010.json"),
        ("discord", "https://raw.githubusercontent.com/discord/discord-api-spec/main/specs/openapi.json"),
        ("gitlab", "https://gitlab.com/gitlab-org/gitlab/-/raw/master/doc/api/openapi/openapi.yaml"),
        ("jira", "https://dac-static.atlassian.com/cloud/jira/platform/swagger-v3.v3.json"),
        ("cloudflare", "https://raw.githubusercontent.com/cloudflare/api-schemas/main/openapi.json"),
        ("spotify", "https://raw.githubusercontent.com/sonallux/spotify-web-api/main/official-spotify-open-api.yml"),
        ("square", "https://raw.githubusercontent.com/square/connect-api-specification/master/api.json"),
        ("netlify", "https://raw.githubusercontent.com/netlify/open-api/master/swagger.yml"),
        ("paypal", "https://raw.githubusercontent.com/paypal/paypal-rest-api-specifications/main/openapi/checkout_orders_v2.json"),
        ("intercom", "https://raw.githubusercontent.com/intercom/Intercom-OpenAPI/main/descriptions/2.11/api.intercom.io.yaml"),
        ("cohere", "https://raw.githubusercontent.com/cohere-ai/cohere-developer-experience/main/cohere-openapi.yaml"),
        ("pinterest", "https://raw.githubusercontent.com/pinterest/api-description/main/v5/openapi.yaml"),
        ("replicate", "https://api.replicate.com/openapi.json"),
        ("twitch", "https://raw.githubusercontent.com/DmitryScaletta/twitch-api-swagger/main/openapi.json"),

        // Tier 2: Live API-served specs
        ("circleci", "https://circleci.com/api/v2/openapi.json"),
        ("vercel", "https://openapi.vercel.sh/"),
        ("posthog", "https://app.posthog.com/api/schema/"),

        // Tier 3: Community-maintained specs
        ("bitbucket", "https://raw.githubusercontent.com/magmax/atlassian-openapi/master/spec/bitbucket.yaml"),
        ("auth0", "https://raw.githubusercontent.com/aminya/auth0-openapi/main/openapi.yaml"),
        ("stability_ai", "https://raw.githubusercontent.com/Stability-AI/rest-api-support/main/spec/v1alpha.yaml"),
        ("hugging_face", "https://huggingface.co/.well-known/openapi.json"),
        ("firebase", "https://raw.githubusercontent.com/APIs-guru/openapi-directory/main/APIs/googleapis.com/firebase/v1beta1/openapi.yaml"),
        ("clickup", "https://developer.clickup.com/openapi/ClickUp_PUBLIC_API_V3.yaml"),
    ]
}

/// Result of enriching a single service.
#[derive(Debug)]
pub struct EnrichmentResult {
    pub service_id: String,
    pub original_op_count: usize,
    pub new_op_count: usize,
    pub spec_title: Option<String>,
    pub error: Option<String>,
}

/// Enrich a single service by fetching its OpenAPI spec.
pub async fn enrich_service(service_id: &str, service: &mut Service) -> EnrichmentResult {
    let original_op_count = service.operations.len();

    // Find the best spec URL
    let spec_url = find_spec_url(service_id, service);
    let Some(url) = spec_url else {
        return EnrichmentResult {
            service_id: service_id.to_string(),
            original_op_count,
            new_op_count: original_op_count,
            spec_title: None,
            error: Some("No usable OpenAPI spec URL found".to_string()),
        };
    };

    info!("Enriching {} from {}", service_id, url);

    match fetch_openapi_spec(&url).await {
        Ok(spec) => {
            let spec_title = Some(spec.title.clone());
            merge_spec_into_service(service, &spec);
            let new_op_count = service.operations.len();

            EnrichmentResult {
                service_id: service_id.to_string(),
                original_op_count,
                new_op_count,
                spec_title,
                error: None,
            }
        }
        Err(e) => {
            error!("Failed to enrich {}: {}", service_id, e);
            EnrichmentResult {
                service_id: service_id.to_string(),
                original_op_count,
                new_op_count: original_op_count,
                spec_title: None,
                error: Some(e.to_string()),
            }
        }
    }
}

/// Enrich all services in the registry.
pub async fn enrich_registry(
    registry: &mut ServiceRegistry,
    service_filter: Option<&[String]>,
    max_concurrent: usize,
) -> Vec<EnrichmentResult> {
    let service_ids: Vec<String> = if let Some(filter) = service_filter {
        filter.to_vec()
    } else {
        registry.services.keys().cloned().collect()
    };

    let mut results = Vec::new();

    // Process in batches to limit concurrency
    for chunk in service_ids.chunks(max_concurrent) {
        let mut handles = Vec::new();

        for service_id in chunk {
            if let Some(service) = registry.services.get(service_id) {
                let sid = service_id.clone();
                let mut svc = service.clone();
                handles.push(async move {
                    let result = enrich_service(&sid, &mut svc).await;
                    (sid, svc, result)
                });
            }
        }

        let batch_results = futures::future::join_all(handles).await;
        for (sid, svc, result) in batch_results {
            if result.error.is_none() {
                registry.services.insert(sid, svc);
            }
            results.push(result);
        }
    }

    results
}

/// Find the best OpenAPI spec URL for a service.
fn find_spec_url(service_id: &str, service: &Service) -> Option<String> {
    // Check known spec URLs first (these are guaranteed to be raw OpenAPI specs)
    let known = get_known_spec_urls();
    if let Some((_, url)) = known.iter().find(|(id, _)| *id == service_id) {
        return Some(url.to_string());
    }

    // Check if any api_spec has a schema_url that looks like a raw spec
    for spec in &service.api_specs {
        if let Some(ref url) = spec.schema_url {
            if is_likely_raw_spec(url) {
                return Some(url.clone());
            }
        }
    }

    // Fall back to schema_url even if it might be docs
    for spec in &service.api_specs {
        if let Some(ref url) = spec.schema_url {
            return Some(url.clone());
        }
    }

    None
}

/// Heuristic: does this URL look like a raw OpenAPI spec (JSON/YAML)?
fn is_likely_raw_spec(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.ends_with(".json")
        || lower.ends_with(".yaml")
        || lower.ends_with(".yml")
        || lower.contains("openapi")
        || lower.contains("swagger")
        || lower.contains("spec3")
        || lower.contains("raw.githubusercontent.com")
}

/// Merge an OpenAPI spec into an existing service, preserving manual overrides.
fn merge_spec_into_service(service: &mut Service, spec: &OpenApiSpec) {
    // Extract operations from spec
    let spec_operations = extract_operations(spec);

    // Merge operations: keep existing manual ones, add new ones from spec
    merge_operations(&mut service.operations, spec_operations);

    // Enrich auth config from security schemes if current config looks incomplete
    merge_auth(&mut service.authentication, &spec.security_schemes);

    // Update description if empty
    if service.description.is_none() || service.description.as_deref() == Some("") {
        service.description = spec.description.clone();
    }

    // Update base_url from servers if available
    if let Some(server) = spec.servers.first() {
        if !server.url.contains('{') {
            // Only update if the spec server doesn't have template variables
            if let Some(api_spec) = service.api_specs.first_mut() {
                if api_spec.base_url.contains('{') || api_spec.base_url.is_empty() {
                    api_spec.base_url = server.url.clone();
                }
            }
        }
    }
}

/// Merge extracted operations into existing operations.
/// Strategy: keep existing manual operations, add new ones from spec,
/// enrich existing ones with missing descriptions/parameters.
fn merge_operations(existing: &mut Vec<Operation>, new_ops: Vec<Operation>) {
    let existing_keys: HashSet<String> = existing
        .iter()
        .map(|op| format!("{}_{}_{}", op.method, op.endpoint_pattern, op.verb))
        .collect();

    // Also track by verb+resource to avoid semantic duplicates
    let existing_semantic: HashSet<String> = existing
        .iter()
        .map(|op| format!("{}_{}", op.verb, op.resource))
        .collect();

    // Enrich existing operations with missing data from spec
    for existing_op in existing.iter_mut() {
        if let Some(spec_op) = new_ops.iter().find(|o| {
            o.endpoint_pattern == existing_op.endpoint_pattern && o.method == existing_op.method
        }) {
            // Add description if missing
            if existing_op.description.is_none() {
                existing_op.description = spec_op.description.clone();
            }
            // Add response schema if missing
            if existing_op.response_schema.is_none() {
                existing_op.response_schema = spec_op.response_schema.clone();
            }
            // Enrich parameters: add descriptions and metadata to existing params,
            // add new params that don't exist
            enrich_parameters(&mut existing_op.parameters, &spec_op.parameters);
        }
    }

    // Add new operations that don't exist yet
    for new_op in new_ops {
        let key = format!(
            "{}_{}_{}",
            new_op.method, new_op.endpoint_pattern, new_op.verb
        );
        let semantic_key = format!("{}_{}", new_op.verb, new_op.resource);

        if !existing_keys.contains(&key) && !existing_semantic.contains(&semantic_key) {
            existing.push(new_op);
        }
    }
}

/// Enrich existing parameters with data from spec parameters.
fn enrich_parameters(
    existing: &mut Option<Vec<crate::registry::Parameter>>,
    new_params: &Option<Vec<crate::registry::Parameter>>,
) {
    let Some(new) = new_params else { return };

    if let Some(ref mut existing_params) = existing {
        // Enrich existing params
        for ep in existing_params.iter_mut() {
            if let Some(np) = new.iter().find(|p| p.name == ep.name) {
                if ep.description.is_none() {
                    ep.description = np.description.clone();
                }
                if ep.example.is_none() {
                    ep.example = np.example.clone();
                }
                if ep.enum_values.is_none() {
                    ep.enum_values = np.enum_values.clone();
                }
                if ep.format.is_none() {
                    ep.format = np.format.clone();
                }
                if ep.default.is_none() {
                    ep.default = np.default.clone();
                }
            }
        }

        // Add new params that don't exist
        let existing_names: HashSet<String> =
            existing_params.iter().map(|p| p.name.clone()).collect();
        for np in new {
            if !existing_names.contains(&np.name) {
                existing_params.push(np.clone());
            }
        }
    } else {
        *existing = new_params.clone();
    }
}

/// Merge security schemes from spec into service auth config.
fn merge_auth(
    auth: &mut Authentication,
    schemes: &std::collections::HashMap<String, SecurityScheme>,
) {
    for (_name, scheme) in schemes {
        match scheme {
            SecurityScheme::ApiKey { name, location } => {
                if auth.api_key_config.is_none() {
                    auth.api_key_config = Some(ApiKeyConfig {
                        location: location.clone(),
                        parameter_name: name.clone(),
                        prefix: None,
                    });
                }
            }
            SecurityScheme::Http { scheme, .. } => {
                if scheme == "bearer" && auth.api_key_config.is_none() {
                    // Many bearer token APIs don't set api_key_config
                    if !matches!(auth.primary_method, AuthMethod::BearerToken) {
                        // Don't override if already set to bearer
                    }
                }
            }
            SecurityScheme::OAuth2 { flows } => {
                if auth.oauth2_config.is_none() {
                    let mut auth_url = String::new();
                    let mut token_url = String::new();
                    let mut flow_names = Vec::new();
                    let mut scopes = Vec::new();

                    for (flow_name, flow) in flows {
                        flow_names.push(flow_name.clone());
                        if let Some(ref url) = flow.authorization_url {
                            auth_url = url.clone();
                        }
                        if let Some(ref url) = flow.token_url {
                            token_url = url.clone();
                        }
                        for (scope, desc) in &flow.scopes {
                            scopes.push(Scope {
                                scope: scope.clone(),
                                description: desc.clone(),
                                required_for: None,
                            });
                        }
                    }

                    if !token_url.is_empty() {
                        auth.oauth2_config = Some(OAuth2Config {
                            authorization_url: auth_url,
                            token_url,
                            scopes_url: None,
                            flows: flow_names,
                            pkce_required: None,
                            common_scopes: if scopes.is_empty() {
                                None
                            } else {
                                Some(scopes)
                            },
                        });
                    }
                }
            }
        }
    }
}

/// Validate a registry for common issues.
pub fn validate_registry(registry: &ServiceRegistry) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for (service_id, service) in &registry.services {
        // Check for empty operations
        if service.operations.is_empty() {
            issues.push(ValidationIssue {
                service_id: service_id.clone(),
                severity: Severity::Warning,
                message: "No operations defined".to_string(),
            });
        }

        // Check for operations without parameters on POST/PUT/PATCH
        for op in &service.operations {
            if matches!(op.method.as_str(), "POST" | "PUT" | "PATCH")
                && op.parameters.as_ref().map(|p| p.is_empty()).unwrap_or(true)
            {
                issues.push(ValidationIssue {
                    service_id: service_id.clone(),
                    severity: Severity::Info,
                    message: format!(
                        "Operation {} {} {} has no parameters",
                        op.verb, op.resource, op.method
                    ),
                });
            }

            // Check for parameters missing descriptions
            if let Some(ref params) = op.parameters {
                for param in params {
                    if param.description.is_none() && param.required == Some(true) {
                        issues.push(ValidationIssue {
                            service_id: service_id.clone(),
                            severity: Severity::Info,
                            message: format!(
                                "Required parameter '{}' in {} {} missing description",
                                param.name, op.verb, op.resource
                            ),
                        });
                    }
                }
            }

            // Check for missing endpoint pattern
            if op.endpoint_pattern.is_empty() {
                issues.push(ValidationIssue {
                    service_id: service_id.clone(),
                    severity: Severity::Error,
                    message: format!(
                        "Operation {} {} has empty endpoint_pattern",
                        op.verb, op.resource
                    ),
                });
            }
        }

        // Check for missing base_url
        if service.api_specs.is_empty() {
            issues.push(ValidationIssue {
                service_id: service_id.clone(),
                severity: Severity::Error,
                message: "No API specs defined".to_string(),
            });
        }

        // Check for auth config completeness
        match service.authentication.primary_method {
            AuthMethod::OAuth2 => {
                if service.authentication.oauth2_config.is_none() {
                    issues.push(ValidationIssue {
                        service_id: service_id.clone(),
                        severity: Severity::Warning,
                        message: "OAuth2 primary method but no oauth2_config".to_string(),
                    });
                }
            }
            AuthMethod::ApiKey => {
                if service.authentication.api_key_config.is_none() {
                    issues.push(ValidationIssue {
                        service_id: service_id.clone(),
                        severity: Severity::Warning,
                        message: "ApiKey primary method but no api_key_config".to_string(),
                    });
                }
            }
            _ => {}
        }
    }

    issues
}

#[derive(Debug)]
pub struct ValidationIssue {
    pub service_id: String,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "ERROR"),
            Severity::Warning => write!(f, "WARN"),
            Severity::Info => write!(f, "INFO"),
        }
    }
}
