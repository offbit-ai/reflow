//! OpenAPI spec fetcher and parser.
//!
//! Fetches OpenAPI 3.x / Swagger 2.x specs from URLs or local files,
//! normalizes them into a common intermediate representation for extraction.

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Parsed OpenAPI spec in a normalized form.
#[derive(Debug, Clone)]
pub struct OpenApiSpec {
    pub title: String,
    pub description: Option<String>,
    pub version: String,
    pub servers: Vec<ServerInfo>,
    pub paths: Vec<PathItem>,
    pub security_schemes: HashMap<String, SecurityScheme>,
    pub components_schemas: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub url: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PathItem {
    pub path: String,
    pub operations: Vec<OperationInfo>,
}

#[derive(Debug, Clone)]
pub struct OperationInfo {
    pub method: String,
    pub operation_id: Option<String>,
    pub summary: Option<String>,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub parameters: Vec<ParameterInfo>,
    pub request_body: Option<RequestBodyInfo>,
    pub responses: HashMap<String, ResponseInfo>,
    pub security: Vec<HashMap<String, Vec<String>>>,
    pub deprecated: bool,
}

#[derive(Debug, Clone)]
pub struct ParameterInfo {
    pub name: String,
    pub location: String, // path, query, header, cookie
    pub required: bool,
    pub description: Option<String>,
    pub schema: Option<SchemaInfo>,
    pub example: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct SchemaInfo {
    pub schema_type: Option<String>,
    pub format: Option<String>,
    pub description: Option<String>,
    pub enum_values: Option<Vec<String>>,
    pub default: Option<Value>,
    pub example: Option<Value>,
    pub items: Option<Box<SchemaInfo>>,
    pub properties: HashMap<String, SchemaInfo>,
    pub required: Vec<String>,
    pub pattern: Option<String>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub ref_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RequestBodyInfo {
    pub description: Option<String>,
    pub required: bool,
    pub content_types: Vec<String>,
    pub schema: Option<SchemaInfo>,
}

#[derive(Debug, Clone)]
pub struct ResponseInfo {
    pub description: Option<String>,
    pub schema: Option<SchemaInfo>,
}

#[derive(Debug, Clone)]
pub enum SecurityScheme {
    ApiKey {
        name: String,
        location: String,
    },
    Http {
        scheme: String,
        bearer_format: Option<String>,
    },
    OAuth2 {
        flows: HashMap<String, OAuth2Flow>,
    },
}

#[derive(Debug, Clone)]
pub struct OAuth2Flow {
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub scopes: HashMap<String, String>,
}

/// Intermediate serde types for flexible OpenAPI parsing.
#[derive(Debug, Deserialize)]
struct RawOpenApi {
    openapi: Option<String>,
    swagger: Option<String>,
    info: Option<RawInfo>,
    servers: Option<Vec<RawServer>>,
    host: Option<String>,
    #[serde(rename = "basePath")]
    base_path: Option<String>,
    schemes: Option<Vec<String>>,
    paths: Option<HashMap<String, HashMap<String, Value>>>,
    components: Option<RawComponents>,
    definitions: Option<HashMap<String, Value>>,
    #[serde(rename = "securityDefinitions")]
    security_definitions: Option<HashMap<String, Value>>,
}

#[derive(Debug, Deserialize)]
struct RawInfo {
    title: Option<String>,
    description: Option<String>,
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawServer {
    url: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawComponents {
    schemas: Option<HashMap<String, Value>>,
    #[serde(rename = "securitySchemes")]
    security_schemes: Option<HashMap<String, Value>>,
}

/// Fetch and parse an OpenAPI spec from a URL.
pub async fn fetch_openapi_spec(url: &str) -> Result<OpenApiSpec> {
    info!("Fetching OpenAPI spec from {}", url);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .user_agent("api-schema-gen/0.1.0")
        .build()?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to fetch spec from {}", url))?;

    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("HTTP {} fetching {}", status, url);
    }

    let text = response.text().await?;
    parse_openapi_spec(&text)
}

/// Parse an OpenAPI spec from a string (JSON or YAML).
pub fn parse_openapi_spec(content: &str) -> Result<OpenApiSpec> {
    // Try JSON first, then YAML
    let raw: RawOpenApi = serde_json::from_str(content)
        .or_else(|_| serde_yaml::from_str(content))
        .context("Failed to parse spec as JSON or YAML")?;

    let is_swagger = raw.swagger.is_some();
    let spec_version = raw
        .openapi
        .as_deref()
        .or(raw.swagger.as_deref())
        .unwrap_or("unknown");
    debug!("Parsed spec version: {}", spec_version);

    let info = raw.info.as_ref();
    let title = info
        .and_then(|i| i.title.as_deref())
        .unwrap_or("Unknown API")
        .to_string();
    let description = info.and_then(|i| i.description.clone());
    let version = info
        .and_then(|i| i.version.as_deref())
        .unwrap_or("unknown")
        .to_string();

    // Parse servers (OpenAPI 3.x) or construct from host/basePath (Swagger 2.x)
    let servers = if let Some(svrs) = &raw.servers {
        svrs.iter()
            .filter_map(|s| {
                s.url.as_ref().map(|url| ServerInfo {
                    url: url.clone(),
                    description: s.description.clone(),
                })
            })
            .collect()
    } else if is_swagger {
        let scheme = raw
            .schemes
            .as_ref()
            .and_then(|s| s.first())
            .map(|s| s.as_str())
            .unwrap_or("https");
        let host = raw.host.as_deref().unwrap_or("localhost");
        let base = raw.base_path.as_deref().unwrap_or("");
        vec![ServerInfo {
            url: format!("{}://{}{}", scheme, host, base),
            description: None,
        }]
    } else {
        vec![]
    };

    // Parse component schemas
    let components_schemas = raw
        .components
        .as_ref()
        .and_then(|c| c.schemas.clone())
        .or(raw.definitions.clone())
        .unwrap_or_default();

    // Parse security schemes
    let raw_security_schemes = raw
        .components
        .as_ref()
        .and_then(|c| c.security_schemes.clone())
        .or(raw.security_definitions.clone())
        .unwrap_or_default();

    let security_schemes = parse_security_schemes(&raw_security_schemes);

    // Parse paths
    let paths = parse_paths(raw.paths.as_ref(), &components_schemas);

    let total_ops: usize = paths.iter().map(|p| p.operations.len()).sum();
    info!(
        "Parsed spec: {} — {} paths, {} operations",
        title,
        paths.len(),
        total_ops
    );

    Ok(OpenApiSpec {
        title,
        description,
        version,
        servers,
        paths,
        security_schemes,
        components_schemas,
    })
}

fn parse_paths(
    raw_paths: Option<&HashMap<String, HashMap<String, Value>>>,
    schemas: &HashMap<String, Value>,
) -> Vec<PathItem> {
    let Some(paths) = raw_paths else {
        return vec![];
    };

    let http_methods = ["get", "post", "put", "patch", "delete", "head", "options"];

    let mut result = Vec::new();
    for (path, methods) in paths {
        let mut operations = Vec::new();
        for (method, op_value) in methods {
            if !http_methods.contains(&method.as_str()) {
                continue;
            }
            if let Some(op) = parse_operation(method, op_value, schemas) {
                operations.push(op);
            }
        }
        if !operations.is_empty() {
            result.push(PathItem {
                path: path.clone(),
                operations,
            });
        }
    }
    // Sort paths for deterministic output
    result.sort_by(|a, b| a.path.cmp(&b.path));
    result
}

fn parse_operation(
    method: &str,
    value: &Value,
    schemas: &HashMap<String, Value>,
) -> Option<OperationInfo> {
    let obj = value.as_object()?;

    let operation_id = obj
        .get("operationId")
        .and_then(|v| v.as_str())
        .map(String::from);
    let summary = obj
        .get("summary")
        .and_then(|v| v.as_str())
        .map(String::from);
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);
    let deprecated = obj
        .get("deprecated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let tags: Vec<String> = obj
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Parse inline parameters
    let mut parameters: Vec<ParameterInfo> = obj
        .get("parameters")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|p| parse_parameter(p, schemas))
                .collect()
        })
        .unwrap_or_default();

    // Parse requestBody (OpenAPI 3.x)
    let request_body = obj
        .get("requestBody")
        .and_then(|rb| parse_request_body(rb, schemas));

    // For Swagger 2.x, body parameters are in the parameters array with `in: body`
    // Extract them into request_body if present
    let request_body = if request_body.is_none() {
        let body_desc = parameters
            .iter()
            .find(|p| p.location == "body")
            .and_then(|p| p.description.clone());
        let body_required = parameters
            .iter()
            .find(|p| p.location == "body")
            .map(|p| p.required)
            .unwrap_or(false);
        let body_schema = parameters
            .iter()
            .find(|p| p.location == "body")
            .and_then(|p| p.schema.clone());
        let has_body = parameters.iter().any(|p| p.location == "body");
        parameters.retain(|p| p.location != "body");
        if has_body {
            Some(RequestBodyInfo {
                description: body_desc,
                required: body_required,
                content_types: vec!["application/json".to_string()],
                schema: body_schema,
            })
        } else {
            None
        }
    } else {
        request_body
    };

    // Parse responses
    let responses = obj
        .get("responses")
        .and_then(|v| v.as_object())
        .map(|resp_map| {
            resp_map
                .iter()
                .filter_map(|(code, resp)| {
                    let desc = resp
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let schema = resp
                        .get("content")
                        .and_then(|c| c.get("application/json"))
                        .and_then(|ct| ct.get("schema"))
                        .and_then(|s| parse_schema(s, schemas))
                        .or_else(|| {
                            // Swagger 2.x
                            resp.get("schema").and_then(|s| parse_schema(s, schemas))
                        });
                    Some((
                        code.clone(),
                        ResponseInfo {
                            description: desc,
                            schema,
                        },
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    // Parse security requirements
    let security = obj
        .get("security")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|s| {
                    s.as_object().map(|obj| {
                        obj.iter()
                            .map(|(k, v)| {
                                let scopes: Vec<String> = v
                                    .as_array()
                                    .map(|a| {
                                        a.iter()
                                            .filter_map(|s| s.as_str().map(String::from))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                (k.clone(), scopes)
                            })
                            .collect()
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    Some(OperationInfo {
        method: method.to_uppercase(),
        operation_id,
        summary,
        description,
        tags,
        parameters,
        request_body,
        responses,
        security,
        deprecated,
    })
}

fn parse_parameter(value: &Value, schemas: &HashMap<String, Value>) -> Option<ParameterInfo> {
    let obj = value.as_object()?;

    // Handle $ref to parameter definitions
    if let Some(ref_path) = obj.get("$ref").and_then(|v| v.as_str()) {
        debug!("Skipping parameter $ref: {}", ref_path);
        return None;
    }

    let name = obj.get("name").and_then(|v| v.as_str())?.to_string();
    let location = obj
        .get("in")
        .and_then(|v| v.as_str())
        .unwrap_or("query")
        .to_string();
    let required = obj
        .get("required")
        .and_then(|v| v.as_bool())
        .unwrap_or(location == "path");
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);

    let schema = obj
        .get("schema")
        .and_then(|s| parse_schema(s, schemas))
        .or_else(|| {
            // Swagger 2.x: type is inline
            let param_type = obj.get("type").and_then(|v| v.as_str()).map(String::from);
            param_type.map(|t| SchemaInfo {
                schema_type: Some(t),
                format: obj.get("format").and_then(|v| v.as_str()).map(String::from),
                description: None,
                enum_values: obj.get("enum").and_then(|v| {
                    v.as_array().map(|a| {
                        a.iter()
                            .filter_map(|e| e.as_str().map(String::from))
                            .collect()
                    })
                }),
                default: obj.get("default").cloned(),
                example: obj.get("example").cloned(),
                items: None,
                properties: HashMap::new(),
                required: vec![],
                pattern: obj
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                minimum: obj.get("minimum").and_then(|v| v.as_f64()),
                maximum: obj.get("maximum").and_then(|v| v.as_f64()),
                ref_path: None,
            })
        });

    let example = obj
        .get("example")
        .cloned()
        .or_else(|| schema.as_ref().and_then(|s| s.example.clone()));

    Some(ParameterInfo {
        name,
        location,
        required,
        description,
        schema,
        example,
    })
}

fn parse_request_body(value: &Value, schemas: &HashMap<String, Value>) -> Option<RequestBodyInfo> {
    let obj = value.as_object()?;

    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);
    let required = obj
        .get("required")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let content = obj.get("content").and_then(|v| v.as_object())?;
    let content_types: Vec<String> = content.keys().cloned().collect();

    // Prefer application/json, fall back to first content type
    let schema_value = content
        .get("application/json")
        .or_else(|| content.values().next());

    let schema = schema_value
        .and_then(|ct| ct.get("schema"))
        .and_then(|s| parse_schema(s, schemas));

    Some(RequestBodyInfo {
        description,
        required,
        content_types,
        schema,
    })
}

fn parse_schema(value: &Value, schemas: &HashMap<String, Value>) -> Option<SchemaInfo> {
    let obj = value.as_object()?;

    // Handle $ref — resolve one level
    if let Some(ref_path) = obj.get("$ref").and_then(|v| v.as_str()) {
        let ref_name = ref_path.rsplit('/').next().unwrap_or(ref_path);
        if let Some(resolved) = schemas.get(ref_name) {
            let mut info = parse_schema(resolved, &HashMap::new()).unwrap_or(SchemaInfo {
                schema_type: Some("object".to_string()),
                format: None,
                description: None,
                enum_values: None,
                default: None,
                example: None,
                items: None,
                properties: HashMap::new(),
                required: vec![],
                pattern: None,
                minimum: None,
                maximum: None,
                ref_path: Some(ref_path.to_string()),
            });
            info.ref_path = Some(ref_path.to_string());
            return Some(info);
        }
        return Some(SchemaInfo {
            schema_type: Some("object".to_string()),
            format: None,
            description: None,
            enum_values: None,
            default: None,
            example: None,
            items: None,
            properties: HashMap::new(),
            required: vec![],
            pattern: None,
            minimum: None,
            maximum: None,
            ref_path: Some(ref_path.to_string()),
        });
    }

    let schema_type = obj.get("type").and_then(|v| v.as_str()).map(String::from);
    let format = obj.get("format").and_then(|v| v.as_str()).map(String::from);
    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .map(String::from);
    let pattern = obj
        .get("pattern")
        .and_then(|v| v.as_str())
        .map(String::from);
    let minimum = obj.get("minimum").and_then(|v| v.as_f64());
    let maximum = obj.get("maximum").and_then(|v| v.as_f64());
    let default = obj.get("default").cloned();
    let example = obj.get("example").cloned();

    let enum_values = obj.get("enum").and_then(|v| {
        v.as_array().map(|a| {
            a.iter()
                .filter_map(|e| e.as_str().map(String::from).or_else(|| Some(e.to_string())))
                .collect()
        })
    });

    let items = obj
        .get("items")
        .and_then(|i| parse_schema(i, schemas))
        .map(Box::new);

    let required_fields: Vec<String> = obj
        .get("required")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let properties: HashMap<String, SchemaInfo> = obj
        .get("properties")
        .and_then(|v| v.as_object())
        .map(|props| {
            props
                .iter()
                .filter_map(|(k, v)| parse_schema(v, schemas).map(|s| (k.clone(), s)))
                .collect()
        })
        .unwrap_or_default();

    Some(SchemaInfo {
        schema_type,
        format,
        description,
        enum_values,
        default,
        example,
        items,
        properties,
        required: required_fields,
        pattern,
        minimum,
        maximum,
        ref_path: None,
    })
}

fn parse_security_schemes(raw: &HashMap<String, Value>) -> HashMap<String, SecurityScheme> {
    let mut result = HashMap::new();
    for (name, value) in raw {
        let Some(obj) = value.as_object() else {
            continue;
        };
        let scheme_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("");

        let scheme = match scheme_type {
            "apiKey" => {
                let param_name = obj
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let location = obj
                    .get("in")
                    .and_then(|v| v.as_str())
                    .unwrap_or("header")
                    .to_string();
                SecurityScheme::ApiKey {
                    name: param_name,
                    location,
                }
            }
            "http" => {
                let scheme_name = obj
                    .get("scheme")
                    .and_then(|v| v.as_str())
                    .unwrap_or("bearer")
                    .to_string();
                let bearer_format = obj
                    .get("bearerFormat")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                SecurityScheme::Http {
                    scheme: scheme_name,
                    bearer_format,
                }
            }
            "oauth2" => {
                let mut flows = HashMap::new();
                if let Some(flows_obj) = obj.get("flows").and_then(|v| v.as_object()) {
                    for (flow_name, flow_value) in flows_obj {
                        if let Some(flow) = parse_oauth2_flow(flow_value) {
                            flows.insert(flow_name.clone(), flow);
                        }
                    }
                }
                SecurityScheme::OAuth2 { flows }
            }
            _ => {
                warn!("Unknown security scheme type: {}", scheme_type);
                continue;
            }
        };
        result.insert(name.clone(), scheme);
    }
    result
}

fn parse_oauth2_flow(value: &Value) -> Option<OAuth2Flow> {
    let obj = value.as_object()?;
    let authorization_url = obj
        .get("authorizationUrl")
        .and_then(|v| v.as_str())
        .map(String::from);
    let token_url = obj
        .get("tokenUrl")
        .and_then(|v| v.as_str())
        .map(String::from);
    let scopes: HashMap<String, String> = obj
        .get("scopes")
        .and_then(|v| v.as_object())
        .map(|s| {
            s.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string()))
                .collect()
        })
        .unwrap_or_default();

    Some(OAuth2Flow {
        authorization_url,
        token_url,
        scopes,
    })
}
