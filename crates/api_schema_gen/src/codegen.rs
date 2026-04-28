//! Rust actor code generation from enriched API service registry.
//!
//! Generates one module per service with typed actors per operation.
//! Each actor uses the `#[actor]` macro, reads API keys from env vars,
//! and produces pre-generated source files for source control.

use crate::registry::{
    AuthMethod, Operation, Parameter, Service, ServiceCategory, ServiceRegistry,
};
use anyhow::Result;
use std::fmt::Write;
use std::fs;
use std::path::Path;

/// Convert a service_id like "google_maps" to a PascalCase struct prefix like "GoogleMaps"
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|seg| !seg.is_empty())
        .map(|seg| {
            let mut c = seg.chars();
            match c.next() {
                Some(first) => {
                    let upper: String = first.to_uppercase().collect();
                    format!("{}{}", upper, c.as_str())
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Sanitize a resource/verb string to be a valid Rust identifier segment.
/// Replaces all non-alphanumeric/underscore chars with underscore, collapses runs.
fn sanitize_segment(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Collapse consecutive underscores
    let mut result = String::with_capacity(cleaned.len());
    let mut prev_underscore = false;
    for c in cleaned.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push('_');
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    // Trim trailing underscores
    result.trim_end_matches('_').to_string()
}

/// Convert a verb_resource pair to a PascalCase actor name.
/// Includes service prefix for graph-level uniqueness.
fn actor_name(service_id: &str, op: &Operation) -> String {
    let service_prefix = to_pascal_case(service_id);
    let verb_part = to_pascal_case(&sanitize_segment(&op.verb));
    let resource_part = to_pascal_case(&sanitize_segment(&op.resource));
    format!("{}{}{}Actor", service_prefix, verb_part, resource_part)
}

/// Convert a verb_resource pair to a snake_case function name.
/// Includes service prefix for graph-level uniqueness.
fn fn_name(service_id: &str, op: &Operation) -> String {
    let mut name = format!(
        "{}_{}_{}",
        sanitize_segment(service_id),
        sanitize_segment(&op.verb),
        sanitize_segment(&op.resource)
    );
    // Collapse any consecutive underscores from joining segments
    while name.contains("__") {
        name = name.replace("__", "_");
    }
    name
}

/// Template ID for registry mapping
fn template_id(service_id: &str, op: &Operation) -> String {
    let mut id = format!(
        "api_{}_{}_{}",
        service_id.replace('-', "_"),
        op.verb.replace('-', "_"),
        op.resource.replace('-', "_")
    );
    while id.contains("__") {
        id = id.replace("__", "_");
    }
    id.trim_end_matches('_').to_string()
}

/// Env var name for a service's API credential
fn env_var_name(service_id: &str) -> String {
    format!("{}_API_KEY", service_id.to_uppercase().replace('-', "_"))
}

/// Map a service_id to a Zeal-compatible icon name.
/// Uses brand icons from Zeal's react-icons registry where available,
/// falls back to "plug" (lucide) for unrecognized services.
fn service_icon(service_id: &str) -> &'static str {
    match service_id {
        // Brand icons available in Zeal's FlatBrandIconRegistry
        "github" => "github",
        "gitlab" => "gitlab",
        "slack" => "slack",
        "discord" => "discord",
        "telegram" => "telegram",
        "whatsapp_business" => "whatsapp",
        "stripe" => "stripe",
        "paypal" => "paypal",
        "shopify" => "shopify",
        "salesforce" => "salesforce",
        "hubspot" => "hubspot",
        "openai" => "openai",
        "anthropic" => "anthropic",
        "cohere" | "replicate" | "stability_ai" => "brain",
        "hugging_face" => "huggingface",
        "google_sheets" => "google-sheets",
        "google_maps" | "google_analytics" => "google",
        "dropbox" => "dropbox",
        "trello" => "trello",
        "asana" => "asana",
        "notion" => "notion",
        "airtable" => "airtable",
        "clickup" => "clickup",
        "jira" | "bitbucket" => "atlassian",
        "monday" => "calendar",
        "linear" => "layout-list",
        "spotify" => "spotify",
        "youtube" => "youtube",
        "twitch" => "twitch",
        "twitter" => "twitter",
        "facebook" | "instagram" => "facebook",
        "linkedin" => "linkedin",
        "pinterest" => "pinterest",
        "microsoft_teams" => "microsoft",
        "zoom" => "zoom",
        "firebase" | "supabase" => "database",
        "mongodb_atlas" => "mongodb",
        "aws_s3" | "aws_dynamodb" => "aws",
        "cloudflare" => "cloud",
        "vercel" => "vercel",
        "netlify" => "netlify",
        "circleci" => "circleci",
        "docker" => "docker",
        "twilio" | "vonage" => "phone",
        "sendgrid" | "mailgun" => "mail",
        "mapbox" | "here" | "tomtom" => "map-pin",
        "openweathermap" | "weatherbit" | "accuweather" => "cloud-sun",
        "nasa" => "rocket",
        "newsapi" | "nytimes" => "newspaper",
        "coinbase" | "binance" | "alphavantage" | "currencyapi" => "dollar-sign",
        "pexels" | "unsplash" | "giphy" => "image",
        "tmdb" | "omdb" => "film",
        "auth0" | "okta" => "shield",
        "mixpanel" | "segment" | "amplitude" | "posthog" => "bar-chart",
        "adyen" | "braintree" | "razorpay" | "square" => "credit-card",
        "pipedrive" | "intercom" | "freshworks" => "users",
        "ipgeolocation" => "globe",
        "spoonacular" => "utensils",
        _ => "plug",
    }
}

/// Map ServiceCategory to a Zeal template category string
fn category_string(cat: &ServiceCategory) -> &'static str {
    match cat {
        ServiceCategory::Communication => "communication",
        ServiceCategory::CrmSales => "crm",
        ServiceCategory::DataStorage => "data",
        ServiceCategory::AiServices => "ai",
        ServiceCategory::Development => "development",
        ServiceCategory::ProjectManagement => "productivity",
        ServiceCategory::Payment => "payment",
        ServiceCategory::Analytics => "analytics",
        ServiceCategory::Security => "security",
        ServiceCategory::Weather => "data",
        ServiceCategory::Finance => "finance",
        ServiceCategory::Location => "data",
        ServiceCategory::News => "data",
        ServiceCategory::Entertainment => "media",
        ServiceCategory::SocialMedia => "social",
        ServiceCategory::Other => "integration",
    }
}

/// Sanitize a parameter name to be a valid Rust identifier.
/// Handles Rust keywords and invalid chars.
fn sanitize_ident(name: &str) -> String {
    // Replace known operator suffixes with meaningful names
    let name = name.replace('<', "_before").replace('>', "_after");
    let mut s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    // Collapse consecutive underscores
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    s = s.trim_matches('_').to_string();
    // Remove leading digits
    while s.starts_with(|c: char| c.is_ascii_digit()) {
        s = s[1..].to_string();
    }
    if s.is_empty() {
        s = "param".to_string();
    }
    // Rust keywords get a trailing underscore
    match s.as_str() {
        "type" | "fn" | "let" | "mut" | "ref" | "self" | "super" | "crate" | "mod" | "pub"
        | "use" | "as" | "in" | "for" | "if" | "else" | "loop" | "while" | "match" | "return"
        | "break" | "continue" | "struct" | "enum" | "trait" | "impl" | "where" | "async"
        | "await" | "move" | "static" | "const" | "true" | "false" | "abstract" | "become"
        | "box" | "do" | "final" | "macro" | "override" | "priv" | "try" | "typeof" | "unsafe"
        | "unsized" | "virtual" | "yield" | "dyn" | "extern" => {
            s.push('_');
        }
        _ => {}
    }
    s
}

/// Determine the inport names from an operation's parameters
fn inport_names(op: &Operation) -> Vec<String> {
    let mut ports = Vec::new();
    if let Some(params) = &op.parameters {
        for param in params {
            let port_name = sanitize_ident(&param.name);
            if !ports.contains(&port_name) {
                ports.push(port_name);
            }
        }
    }
    // Always have a trigger port for parameterless operations
    if ports.is_empty() {
        ports.push("trigger".to_string());
    }
    ports
}

/// Escape a string for use in Rust source code
fn escape_str(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

/// Generate the Rust source code for a single service module
fn generate_service_module(service_id: &str, service: &Service) -> Result<String> {
    let mut out = String::with_capacity(8192);

    let env_key = env_var_name(service_id);
    let _pascal_service = to_pascal_case(service_id);

    // Suppress clippy and rustfmt on generated code
    writeln!(out, "#![allow(clippy::all, unused_imports, dead_code)]")?;
    writeln!(out)?;

    // Module header
    writeln!(out, "//! Auto-generated API actors for {}", service.name)?;
    writeln!(out, "//!")?;
    writeln!(out, "//! Service: {} ({})", service.name, service_id)?;
    if let Some(desc) = &service.description {
        writeln!(out, "//! {}", desc)?;
    }
    writeln!(out, "//!")?;
    writeln!(
        out,
        "//! Required env var: {} ({})",
        env_key,
        match service.authentication.primary_method {
            AuthMethod::ApiKey => "API key",
            AuthMethod::BearerToken => "Bearer token",
            AuthMethod::OAuth2 => "OAuth2 access token",
            AuthMethod::BasicAuth => "Basic auth credentials (user:pass)",
            AuthMethod::Jwt => "JWT token",
            _ => "auth credential",
        }
    )?;
    writeln!(out, "//!")?;
    writeln!(
        out,
        "//! Generated by api-schema-gen codegen — do not edit manually."
    )?;
    writeln!(out)?;

    // Imports
    // ClientBuilderExt::timeout_compat lives on `crate` so generated
    // modules pick it up via the same import. The trait is no-op on
    // wasm and a passthrough to `.timeout(d)` on native — see
    // reflow_api_services/src/lib.rs.
    writeln!(
        out,
        "use crate::{{Actor, ActorBehavior, ClientBuilderExt, Message, Port}};"
    )?;
    writeln!(out, "use reflow_actor_macro::actor;")?;
    writeln!(out, "use anyhow::{{Error, Result}};")?;
    writeln!(
        out,
        "use reflow_actor::{{message::EncodableValue, ActorContext}};"
    )?;
    writeln!(out, "use serde_json::{{json, Value}};")?;
    writeln!(out, "use std::collections::HashMap;")?;
    writeln!(out, "use std::time::Duration;")?;
    writeln!(out)?;

    // Base URL constant
    let base_url = service
        .api_specs
        .first()
        .map(|s| s.base_url.as_str())
        .unwrap_or("");
    writeln!(out, "const BASE_URL: &str = \"{}\";", escape_str(base_url))?;
    writeln!(out, "const ENV_KEY: &str = \"{}\";", escape_str(&env_key))?;
    writeln!(out)?;

    // Auth helper
    generate_auth_helper(&mut out, service)?;

    // Generate each operation actor, deduplicating by function name
    let mut seen_fns = std::collections::HashSet::new();
    for op in &service.operations {
        let func = fn_name(service_id, op);
        if seen_fns.contains(&func) {
            // Skip duplicate verb+resource combinations (different endpoints, same mapping)
            continue;
        }
        seen_fns.insert(func);
        generate_operation_actor(&mut out, service_id, service, op)?;
    }

    Ok(out)
}

/// Generate the auth helper function for a service
fn generate_auth_helper(out: &mut String, service: &Service) -> Result<()> {
    writeln!(out, "/// Apply authentication to the request builder.")?;
    writeln!(out, "fn apply_auth(config: &reflow_actor::ActorConfig, mut builder: reqwest::RequestBuilder) -> Result<reqwest::RequestBuilder> {{")?;
    writeln!(
        out,
        "    let credential = config.get_config_or_env(ENV_KEY)"
    )?;
    writeln!(
        out,
        "        .ok_or_else(|| anyhow::anyhow!(\"Missing env var: {{}}\", ENV_KEY))?;"
    )?;

    match service.authentication.primary_method {
        AuthMethod::ApiKey => {
            if let Some(api_key_config) = &service.authentication.api_key_config {
                match api_key_config.location.as_str() {
                    "header" => {
                        let header_name = escape_str(&api_key_config.parameter_name);
                        if let Some(prefix) = &api_key_config.prefix {
                            writeln!(out, "    builder = builder.header(\"{}\", format!(\"{{}} {{}}\", \"{}\", credential));", header_name, escape_str(prefix))?;
                        } else {
                            writeln!(
                                out,
                                "    builder = builder.header(\"{}\", &credential);",
                                header_name
                            )?;
                        }
                    }
                    "query" => {
                        let param_name = escape_str(&api_key_config.parameter_name);
                        writeln!(
                            out,
                            "    builder = builder.query(&[(\"{}\", &credential)]);",
                            param_name
                        )?;
                    }
                    _ => {
                        writeln!(out, "    builder = builder.header(\"Authorization\", format!(\"Bearer {{}}\", credential));")?;
                    }
                }
            } else {
                writeln!(out, "    builder = builder.header(\"Authorization\", format!(\"Bearer {{}}\", credential));")?;
            }
        }
        AuthMethod::BearerToken | AuthMethod::OAuth2 | AuthMethod::Jwt => {
            writeln!(out, "    builder = builder.header(\"Authorization\", format!(\"Bearer {{}}\", credential));")?;
        }
        AuthMethod::BasicAuth => {
            writeln!(out, "    builder = builder.header(\"Authorization\", format!(\"Basic {{}}\", credential));")?;
        }
        _ => {
            writeln!(
                out,
                "    builder = builder.header(\"Authorization\", &credential);"
            )?;
        }
    }

    writeln!(out, "    Ok(builder)")?;
    writeln!(out, "}}")?;
    writeln!(out)?;
    Ok(())
}

/// Generate a single operation actor
fn generate_operation_actor(
    out: &mut String,
    service_id: &str,
    service: &Service,
    op: &Operation,
) -> Result<()> {
    let actor = actor_name(service_id, op);
    let func = fn_name(service_id, op);
    let inports = inport_names(op);
    let inport_list = inports.join(", ");

    // Doc comment
    if let Some(desc) = &op.description {
        writeln!(out, "/// {}", escape_str(desc))?;
    } else {
        writeln!(
            out,
            "/// {} {} via {} API",
            op.verb, op.resource, service.name
        )?;
    }
    writeln!(out, "///")?;
    writeln!(out, "/// Method: {} {}", op.method, op.endpoint_pattern)?;

    // Actor macro
    writeln!(out, "#[actor(")?;
    writeln!(out, "    {},", actor)?;
    writeln!(out, "    inports::<100>({}),", inport_list)?;
    writeln!(out, "    outports::<50>(response, error),")?;
    writeln!(out, "    state(MemoryState)")?;
    writeln!(out, ")]")?;

    // Function signature
    writeln!(
        out,
        "pub async fn {}(context: ActorContext) -> Result<HashMap<String, Message>, Error> {{",
        func
    )?;

    // Body
    writeln!(out, "    let inputs = context.get_payload();")?;
    writeln!(out, "    let actor_config = context.get_config();")?;
    writeln!(out)?;

    // Build URL with path param substitution
    let has_path_params = op
        .parameters
        .as_ref()
        .map(|ps| ps.iter().any(|p| p.location == "path"))
        .unwrap_or(false);
    let endpoint_let = if has_path_params {
        "let mut endpoint"
    } else {
        "let endpoint"
    };
    writeln!(
        out,
        "    {} = \"{}\".to_string();",
        endpoint_let,
        escape_str(&op.endpoint_pattern)
    )?;

    // Path params
    if let Some(params) = &op.parameters {
        for param in params {
            if param.location == "path" {
                let port_name = sanitize_ident(&param.name);
                writeln!(
                    out,
                    "    if let Some(val) = inputs.get(\"{}\") {{",
                    port_name
                )?;
                writeln!(
                    out,
                    "        endpoint = endpoint.replace(\"{{{{{}}}}}\", &message_to_str(val));",
                    param.name
                )?;
                writeln!(out, "    }}")?;
            }
        }
    }

    writeln!(out)?;
    writeln!(
        out,
        "    let url = format!(\"{{}}{{}}\", BASE_URL.trim_end_matches('/'), endpoint);"
    )?;
    writeln!(out)?;

    // Build HTTP client + request
    writeln!(out, "    let client = reqwest::Client::builder()")?;
    writeln!(out, "        .timeout_compat(Duration::from_secs(30))")?;
    writeln!(out, "        .build()?;")?;
    writeln!(out)?;

    let method_call = match op.method.as_str() {
        "GET" => "get",
        "POST" => "post",
        "PUT" => "put",
        "PATCH" => "patch",
        "DELETE" => "delete",
        "HEAD" => "head",
        _ => "get",
    };
    writeln!(out, "    let mut builder = client.{}(&url);", method_call)?;
    writeln!(
        out,
        "    builder = builder.header(\"Content-Type\", \"application/json\");"
    )?;
    writeln!(out, "    builder = apply_auth(actor_config, builder)?;")?;
    writeln!(out)?;

    // Query params
    let query_params: Vec<&Parameter> = op
        .parameters
        .as_ref()
        .map(|ps| ps.iter().filter(|p| p.location == "query").collect())
        .unwrap_or_default();
    if !query_params.is_empty() {
        writeln!(
            out,
            "    let mut query_pairs: Vec<(&str, String)> = Vec::new();"
        )?;
        for param in &query_params {
            let port_name = sanitize_ident(&param.name);
            writeln!(
                out,
                "    if let Some(val) = inputs.get(\"{}\") {{",
                port_name
            )?;
            writeln!(
                out,
                "        query_pairs.push((\"{}\", message_to_str(val)));",
                escape_str(&param.name)
            )?;
            writeln!(out, "    }}")?;
        }
        writeln!(out, "    if !query_pairs.is_empty() {{")?;
        writeln!(out, "        builder = builder.query(&query_pairs);")?;
        writeln!(out, "    }}")?;
        writeln!(out)?;
    }

    // Header params
    let header_params: Vec<&Parameter> = op
        .parameters
        .as_ref()
        .map(|ps| ps.iter().filter(|p| p.location == "header").collect())
        .unwrap_or_default();
    for param in &header_params {
        let port_name = param.name.replace(['-', '.'], "_");
        writeln!(
            out,
            "    if let Some(val) = inputs.get(\"{}\") {{",
            port_name
        )?;
        writeln!(
            out,
            "        builder = builder.header(\"{}\", message_to_str(val));",
            escape_str(&param.name)
        )?;
        writeln!(out, "    }}")?;
    }

    // Body params (for POST/PUT/PATCH)
    let body_params: Vec<&Parameter> = op
        .parameters
        .as_ref()
        .map(|ps| ps.iter().filter(|p| p.location == "body").collect())
        .unwrap_or_default();
    if !body_params.is_empty() && matches!(op.method.as_str(), "POST" | "PUT" | "PATCH") {
        writeln!(out, "    let mut body = serde_json::Map::new();")?;
        for param in &body_params {
            let port_name = sanitize_ident(&param.name);
            writeln!(
                out,
                "    if let Some(val) = inputs.get(\"{}\") {{",
                port_name
            )?;
            writeln!(
                out,
                "        body.insert(\"{}\".to_string(), val.clone().into());",
                escape_str(&param.name)
            )?;
            writeln!(out, "    }}")?;
        }
        writeln!(out, "    if !body.is_empty() {{")?;
        writeln!(
            out,
            "        builder = builder.json(&serde_json::Value::Object(body));"
        )?;
        writeln!(out, "    }}")?;
        writeln!(out)?;
    }

    // Execute request + handle response
    writeln!(out, "    let mut output = HashMap::new();")?;
    writeln!(out, "    match builder.send().await {{")?;
    writeln!(out, "        Ok(resp) => {{")?;
    writeln!(out, "            let status = resp.status().as_u16();")?;
    writeln!(
        out,
        "            let headers: HashMap<String, String> = resp.headers()"
    )?;
    writeln!(out, "                .iter()")?;
    writeln!(out, "                .filter_map(|(k, v)| v.to_str().ok().map(|val| (k.to_string(), val.to_string())))")?;
    writeln!(out, "                .collect();")?;
    writeln!(
        out,
        "            let body_text = resp.text().await.unwrap_or_default();"
    )?;
    writeln!(out, "            let body_value: Value = serde_json::from_str(&body_text).unwrap_or(Value::String(body_text));")?;
    writeln!(out, "            output.insert(\"response\".to_string(), Message::object(EncodableValue::from(json!({{")?;
    writeln!(out, "                \"status\": status,")?;
    writeln!(out, "                \"headers\": headers,")?;
    writeln!(out, "                \"body\": body_value,")?;
    writeln!(out, "            }}))));")?;
    writeln!(out, "        }}")?;
    writeln!(out, "        Err(e) => {{")?;
    // Escape curly braces in endpoint pattern for use in format!() string
    let escaped_endpoint = op.endpoint_pattern.replace('{', "{{").replace('}', "}}");
    writeln!(out, "            output.insert(\"error\".to_string(), Message::Error(format!(\"{} {} failed: {{}}\", e).into()));",
        op.method, escape_str(&escaped_endpoint)
    )?;
    writeln!(out, "        }}")?;
    writeln!(out, "    }}")?;
    writeln!(out)?;
    writeln!(out, "    Ok(output)")?;
    writeln!(out, "}}")?;
    writeln!(out)?;

    Ok(())
}

/// Generate the mod.rs that declares all service modules and the registry function
fn generate_mod_rs(service_ids: &[String]) -> Result<String> {
    let mut out = String::with_capacity(4096);

    writeln!(out, "#![allow(clippy::all, unused_imports, dead_code)]")?;
    writeln!(out)?;
    writeln!(out, "//! Auto-generated API actor modules.")?;
    writeln!(out, "//!")?;
    writeln!(
        out,
        "//! Generated by api-schema-gen codegen — do not edit manually."
    )?;
    writeln!(out)?;

    // api_registry module (template info + actor lookup)
    writeln!(out, "pub mod api_registry;")?;
    writeln!(out)?;

    for sid in service_ids {
        writeln!(out, "pub mod {};", sid.replace('-', "_"))?;
    }

    writeln!(out)?;

    // message_to_str helper (shared across all service modules via crate-level)
    writeln!(
        out,
        "/// Convert a Message to a string for use in query params / path params."
    )?;
    writeln!(
        out,
        "pub(crate) fn message_to_str(msg: &crate::Message) -> String {{"
    )?;
    writeln!(out, "    match msg {{")?;
    writeln!(
        out,
        "        crate::Message::String(s) => s.as_ref().clone(),"
    )?;
    writeln!(out, "        crate::Message::Integer(i) => i.to_string(),")?;
    writeln!(out, "        crate::Message::Float(f) => f.to_string(),")?;
    writeln!(out, "        crate::Message::Boolean(b) => b.to_string(),")?;
    writeln!(
        out,
        "        other => serde_json::to_string(other).unwrap_or_default(),"
    )?;
    writeln!(out, "    }}")?;
    writeln!(out, "}}")?;

    Ok(out)
}

/// Generate the registry mapping additions
fn generate_registry_entries(registry: &ServiceRegistry) -> Result<String> {
    let mut out = String::with_capacity(8192);

    writeln!(out, "#![allow(clippy::all, unused_imports, dead_code)]")?;
    writeln!(out)?;
    writeln!(
        out,
        "//! Auto-generated template ID → actor mapping for API actors."
    )?;
    writeln!(out, "//!")?;
    writeln!(
        out,
        "//! Generated by api-schema-gen codegen — do not edit manually."
    )?;
    writeln!(out)?;
    writeln!(out, "use std::sync::Arc;")?;
    writeln!(out, "use crate::Actor;")?;
    writeln!(out)?;

    // ApiTemplateInfo struct
    writeln!(
        out,
        "/// Metadata for an API template, used during ZIP registration."
    )?;
    writeln!(out, "#[derive(Debug, Clone)]")?;
    writeln!(out, "pub struct ApiTemplateInfo {{")?;
    writeln!(out, "    pub template_id: &'static str,")?;
    writeln!(out, "    pub title: &'static str,")?;
    writeln!(out, "    pub description: &'static str,")?;
    writeln!(out, "    pub icon: &'static str,")?;
    writeln!(out, "    pub category: &'static str,")?;
    writeln!(out, "    pub subcategory: &'static str,")?;
    writeln!(out, "    pub env_var: &'static str,")?;
    writeln!(out, "    pub inports: &'static [&'static str],")?;
    writeln!(out, "    pub outports: &'static [&'static str],")?;
    writeln!(out, "}}")?;
    writeln!(out)?;

    // get_api_actor_for_template (existing)
    writeln!(out, "/// Get an API actor by template ID.")?;
    writeln!(
        out,
        "pub fn get_api_actor_for_template(template_id: &str) -> Option<Arc<dyn Actor>> {{"
    )?;
    writeln!(out, "    match template_id {{")?;

    for (service_id, service) in &registry.services {
        let mod_name = service_id.replace('-', "_");
        let mut seen_fns = std::collections::HashSet::new();
        for op in &service.operations {
            let func = fn_name(service_id, op);
            if seen_fns.contains(&func) {
                continue;
            }
            seen_fns.insert(func);
            let tpl_id = template_id(service_id, op);
            let actor = actor_name(service_id, op);
            writeln!(
                out,
                "        \"{}\" => Some(Arc::new(super::{}::{}::new())),",
                tpl_id, mod_name, actor
            )?;
        }
    }

    writeln!(out, "        _ => None,")?;
    writeln!(out, "    }}")?;
    writeln!(out, "}}")?;
    writeln!(out)?;

    // get_api_template_infos — returns all template metadata for ZIP registration
    writeln!(
        out,
        "/// Get metadata for all API templates (for ZIP registration with Zeal)."
    )?;
    writeln!(
        out,
        "pub fn get_api_template_infos() -> &'static [ApiTemplateInfo] {{"
    )?;
    writeln!(out, "    static INFOS: &[ApiTemplateInfo] = &[")?;

    for (service_id, service) in &registry.services {
        let icon = service_icon(service_id);
        let category = category_string(&service.category);
        let env_key = env_var_name(service_id);
        let mut seen_fns = std::collections::HashSet::new();

        for op in &service.operations {
            let func = fn_name(service_id, op);
            if seen_fns.contains(&func) {
                continue;
            }
            seen_fns.insert(func);

            let tpl_id = template_id(service_id, op);
            let inports = inport_names(op);
            let title = format!("{}: {} {}", service.name, op.verb, op.resource);
            // Truncate raw description first, then escape for Rust string literal
            let raw_desc = op
                .description
                .as_deref()
                .unwrap_or("")
                .replace(['\n', '\r'], " ");
            let raw_desc = if raw_desc.len() > 200 {
                format!("{}...", &raw_desc[..197])
            } else {
                raw_desc
            };

            writeln!(out, "        ApiTemplateInfo {{")?;
            writeln!(out, "            template_id: \"{}\",", escape_str(&tpl_id))?;
            writeln!(out, "            title: \"{}\",", escape_str(&title))?;
            writeln!(
                out,
                "            description: \"{}\",",
                escape_str(&raw_desc)
            )?;
            writeln!(out, "            icon: \"{}\",", icon)?;
            writeln!(out, "            category: \"{}\",", category)?;
            writeln!(
                out,
                "            subcategory: \"{}\",",
                escape_str(&service.name)
            )?;
            writeln!(out, "            env_var: \"{}\",", escape_str(&env_key))?;
            write!(out, "            inports: &[")?;
            for (i, port) in inports.iter().enumerate() {
                if i > 0 {
                    write!(out, ", ")?;
                }
                write!(out, "\"{}\"", escape_str(port))?;
            }
            writeln!(out, "],")?;
            writeln!(out, "            outports: &[\"response\", \"error\"],")?;
            writeln!(out, "        }},")?;
        }
    }

    writeln!(out, "    ];")?;
    writeln!(out, "    INFOS")?;
    writeln!(out, "}}")?;

    Ok(out)
}

/// Main codegen entry point: reads registry, writes generated Rust files
pub fn generate_actors(registry_path: &Path, output_dir: &Path) -> Result<CodegenStats> {
    let registry = ServiceRegistry::from_json_file(registry_path)?;

    // Create output directory
    fs::create_dir_all(output_dir)?;

    let mut stats = CodegenStats::default();
    let mut service_ids = Vec::new();

    for (service_id, service) in &registry.services {
        if service.operations.is_empty() {
            continue;
        }

        let module_name = service_id.replace('-', "_");
        let module_code = generate_service_module(service_id, service)?;

        // Each service module uses message_to_str from parent
        // Replace the bare `message_to_str` calls with `super::message_to_str`
        let module_code = module_code.replace("message_to_str(", "super::message_to_str(");

        let file_path = output_dir.join(format!("{}.rs", module_name));
        fs::write(&file_path, &module_code)?;

        stats.services += 1;
        stats.actors += service.operations.len();
        stats.files.push(file_path.display().to_string());
        service_ids.push(module_name);
    }

    // Generate mod.rs
    let mod_rs = generate_mod_rs(&service_ids)?;
    let mod_path = output_dir.join("mod.rs");
    fs::write(&mod_path, &mod_rs)?;

    // Generate api_registry.rs (template mapping)
    let registry_rs = generate_registry_entries(&registry)?;
    let registry_path = output_dir.join("api_registry.rs");
    fs::write(&registry_path, &registry_rs)?;

    stats.files.push(mod_path.display().to_string());
    stats.files.push(registry_path.display().to_string());

    Ok(stats)
}

#[derive(Default)]
pub struct CodegenStats {
    pub services: usize,
    pub actors: usize,
    pub files: Vec<String>,
}
