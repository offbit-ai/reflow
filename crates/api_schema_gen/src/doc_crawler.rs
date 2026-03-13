//! HTML API documentation crawler.
//!
//! For services that don't publish raw OpenAPI specs, this module fetches
//! their HTML documentation pages and extracts API endpoint information
//! into a structured format that can be fed into an LLM for schema generation.
//!
//! The flow is:
//! 1. Fetch HTML doc pages for a service
//! 2. Extract text content (strip HTML)
//! 3. Output structured prompts for LLM-based schema extraction
//! 4. Parse LLM output into registry Operations

use crate::registry::{Operation, Parameter, Service};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Extracted doc content ready for LLM processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocContent {
    pub service_id: String,
    pub service_name: String,
    pub base_url: String,
    pub doc_url: String,
    pub text_content: String,
    pub auth_method: String,
}

/// Crawl result with extracted operations (or raw content for LLM processing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    pub service_id: String,
    pub doc_contents: Vec<DocContent>,
    pub operations: Vec<Operation>,
    pub error: Option<String>,
}

/// Get the documentation URLs for services that need crawling.
/// These are services where schema_url points to HTML docs, not raw specs.
pub fn get_doc_urls_for_service(service: &Service) -> Vec<String> {
    let mut urls = Vec::new();

    // Check documentation.api_reference first (most likely to have endpoint details)
    if let Some(ref docs) = service.documentation {
        if let Some(ref api_ref) = docs.api_reference {
            urls.push(api_ref.clone());
        }
        if let Some(ref main) = docs.main_docs {
            if !urls.contains(main) {
                urls.push(main.clone());
            }
        }
    }

    // Fall back to schema_url from api_specs
    for spec in &service.api_specs {
        if let Some(ref url) = spec.schema_url {
            if !urls.contains(url) {
                urls.push(url.clone());
            }
        }
    }

    urls
}

/// Fetch and extract text content from an HTML documentation page.
pub async fn fetch_doc_page(url: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("Mozilla/5.0 (compatible; api-schema-gen/0.1.0)")
        .build()?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch {}", url))?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {} fetching {}", status, url);
    }

    let html = response.text().await?;
    Ok(extract_text_from_html(&html))
}

/// Strip HTML tags and extract meaningful text content.
/// Focuses on API-relevant content: endpoints, methods, parameters.
fn extract_text_from_html(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;
    let mut tag_name = String::new();
    let mut collecting_tag = false;

    for c in html.chars() {
        if c == '<' {
            in_tag = true;
            collecting_tag = true;
            tag_name.clear();
            continue;
        }
        if c == '>' {
            in_tag = false;
            collecting_tag = false;
            let tag_lower = tag_name.to_lowercase();
            if tag_lower.starts_with("script") {
                in_script = true;
            } else if tag_lower.starts_with("/script") {
                in_script = false;
            } else if tag_lower.starts_with("style") {
                in_style = true;
            } else if tag_lower.starts_with("/style") {
                in_style = false;
            }
            // Add newlines for block elements
            if matches!(
                tag_lower.trim_start_matches('/').split_whitespace().next(),
                Some(
                    "p" | "div"
                        | "br"
                        | "h1"
                        | "h2"
                        | "h3"
                        | "h4"
                        | "h5"
                        | "h6"
                        | "li"
                        | "tr"
                        | "td"
                        | "th"
                        | "pre"
                        | "code"
                )
            ) {
                text.push('\n');
            }
            continue;
        }
        if collecting_tag {
            tag_name.push(c);
            continue;
        }
        if !in_tag && !in_script && !in_style {
            text.push(c);
        }
    }

    // Decode common HTML entities
    let text = text
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");

    // Collapse whitespace while preserving structure
    let mut result = String::new();
    let mut prev_blank = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
                prev_blank = true;
            }
            continue;
        }
        prev_blank = false;
        result.push_str(trimmed);
        result.push('\n');
    }

    // Truncate to reasonable size for LLM context
    if result.len() > 50_000 {
        result.truncate(50_000);
        result.push_str("\n... [truncated]");
    }

    result
}

/// Build an LLM prompt for extracting API operations from doc text.
pub fn build_extraction_prompt(doc: &DocContent) -> String {
    format!(
        r#"Extract all API endpoints from this documentation for the "{service_name}" API.
Base URL: {base_url}
Authentication: {auth_method}

For each endpoint, output a JSON array of operations in this exact format:
```json
[
  {{
    "verb": "create|read|update|delete|list|search|send|upload|download|start|stop|generate|analyze",
    "resource": "snake_case_resource_name",
    "method": "GET|POST|PUT|PATCH|DELETE",
    "endpoint_pattern": "/path/with/{{param_placeholders}}",
    "description": "Brief description of what this endpoint does",
    "parameters": [
      {{
        "name": "param_name",
        "type": "string|integer|number|boolean|array|object",
        "location": "path|query|header|body",
        "required": true,
        "description": "What this parameter does"
      }}
    ]
  }}
]
```

Rules:
- Use curly braces for path parameters: /users/{{user_id}}
- verb should match the api_tool_verbs_schema conventions
- resource should be singular for CRUD on one item, plural for list/search
- Include ALL endpoints you can find, not just a few examples
- For body parameters, extract individual fields, not the whole body as one param

Documentation text:
---
{text}
---

Output ONLY the JSON array, no other text."#,
        service_name = doc.service_name,
        base_url = doc.base_url,
        auth_method = doc.auth_method,
        text = doc.text_content,
    )
}

/// Parse LLM output (JSON array of operations) into registry Operations.
pub fn parse_llm_operations(json_str: &str) -> Result<Vec<Operation>> {
    // Extract JSON from potential markdown code blocks
    let json = if let Some(start) = json_str.find('[') {
        if let Some(end) = json_str.rfind(']') {
            &json_str[start..=end]
        } else {
            json_str
        }
    } else {
        json_str
    };

    #[derive(Deserialize)]
    struct RawOp {
        verb: String,
        resource: String,
        method: String,
        endpoint_pattern: String,
        description: Option<String>,
        parameters: Option<Vec<RawParam>>,
    }

    #[derive(Deserialize)]
    struct RawParam {
        name: String,
        #[serde(rename = "type", default = "default_type")]
        param_type: String,
        #[serde(default = "default_location")]
        location: String,
        #[serde(default)]
        required: Option<bool>,
        description: Option<String>,
    }

    fn default_type() -> String {
        "string".to_string()
    }
    fn default_location() -> String {
        "query".to_string()
    }

    let raw_ops: Vec<RawOp> = serde_json::from_str(json)
        .context("Failed to parse LLM output as JSON array of operations")?;

    Ok(raw_ops
        .into_iter()
        .map(|raw| Operation {
            verb: raw.verb,
            resource: raw.resource,
            method: raw.method,
            endpoint_pattern: raw.endpoint_pattern,
            description: raw.description,
            graphql_operation: None,
            parameters: raw.parameters.map(|params| {
                params
                    .into_iter()
                    .map(|p| Parameter {
                        name: p.name,
                        param_type: p.param_type,
                        location: p.location,
                        required: p.required,
                        description: p.description,
                        example: None,
                        enum_values: None,
                        format: None,
                        pattern: None,
                        default: None,
                    })
                    .collect()
            }),
            required_scopes: None,
            rate_limit_cost: None,
            batch_capable: None,
            webhook_events: None,
            response_schema: None,
        })
        .collect())
}

/// Crawl a service's documentation pages and prepare for LLM extraction.
pub async fn crawl_service(service_id: &str, service: &Service) -> CrawlResult {
    let doc_urls = get_doc_urls_for_service(service);
    if doc_urls.is_empty() {
        return CrawlResult {
            service_id: service_id.to_string(),
            doc_contents: vec![],
            operations: vec![],
            error: Some("No documentation URLs found".to_string()),
        };
    }

    let base_url = service
        .api_specs
        .first()
        .map(|s| s.base_url.clone())
        .unwrap_or_default();

    let auth_method = format!("{:?}", service.authentication.primary_method);

    let mut doc_contents = Vec::new();
    let mut errors = Vec::new();

    for url in &doc_urls {
        match fetch_doc_page(url).await {
            Ok(text) => {
                if text.trim().len() < 100 {
                    debug!("Skipping {} — too little content", url);
                    continue;
                }
                doc_contents.push(DocContent {
                    service_id: service_id.to_string(),
                    service_name: service.name.clone(),
                    base_url: base_url.clone(),
                    doc_url: url.clone(),
                    text_content: text,
                    auth_method: auth_method.clone(),
                });
            }
            Err(e) => {
                warn!("Failed to fetch {}: {}", url, e);
                errors.push(format!("{}: {}", url, e));
            }
        }
    }

    CrawlResult {
        service_id: service_id.to_string(),
        doc_contents,
        operations: vec![],
        error: if errors.is_empty() {
            None
        } else {
            Some(errors.join("; "))
        },
    }
}

/// Crawl all services that failed spec-based enrichment.
/// Returns doc contents ready for parallel LLM processing.
pub async fn crawl_unenriched_services(
    services: &HashMap<String, (String, Service)>,
    max_concurrent: usize,
) -> Vec<CrawlResult> {
    let service_ids: Vec<&String> = services.keys().collect();
    let mut results = Vec::new();

    for chunk in service_ids.chunks(max_concurrent) {
        let mut handles = Vec::new();
        for service_id in chunk {
            let (_, service) = &services[*service_id];
            let sid = (*service_id).clone();
            let svc = service.clone();
            handles.push(async move { crawl_service(&sid, &svc).await });
        }
        let batch = futures::future::join_all(handles).await;
        results.extend(batch);
    }

    results
}
