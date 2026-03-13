//! Operation extractor — maps OpenAPI paths+methods to standardized verb/resource format.
//!
//! Uses heuristics based on HTTP method, path structure, and operation metadata
//! to assign a verb (from api_tool_verbs_schema) and resource name.

use crate::openapi::{OpenApiSpec, OperationInfo, ParameterInfo, RequestBodyInfo, SchemaInfo};
use crate::registry::{Operation, Parameter};
use tracing::debug;

/// Extract operations from an OpenAPI spec into registry-compatible format.
pub fn extract_operations(spec: &OpenApiSpec) -> Vec<Operation> {
    let mut operations = Vec::new();

    for path_item in &spec.paths {
        for op in &path_item.operations {
            if op.deprecated {
                debug!(
                    "Skipping deprecated operation: {} {}",
                    op.method, path_item.path
                );
                continue;
            }

            let (verb, resource) = infer_verb_resource(&path_item.path, op);
            let description = op
                .summary
                .clone()
                .or_else(|| op.description.clone())
                .or_else(|| Some(format!("{} {}", verb, resource.replace('_', " "))));

            let mut parameters = extract_parameters(&op.parameters);

            // Extract body parameters from requestBody
            if let Some(ref body) = op.request_body {
                let body_params = extract_body_parameters(body);
                parameters.extend(body_params);
            }

            // Extract response schema from 200/201 response
            let response_schema = extract_response_schema(op);

            operations.push(Operation {
                verb: verb.clone(),
                resource: resource.clone(),
                method: op.method.clone(),
                endpoint_pattern: path_item.path.clone(),
                description,
                graphql_operation: None,
                parameters: if parameters.is_empty() {
                    None
                } else {
                    Some(parameters)
                },
                required_scopes: extract_required_scopes(op),
                rate_limit_cost: None,
                batch_capable: None,
                webhook_events: None,
                response_schema,
            });
        }
    }

    // Deduplicate by verb+resource (keep first, which is typically more specific)
    let mut seen = std::collections::HashSet::new();
    operations.retain(|op| {
        let key = format!("{}_{}", op.verb, op.resource);
        seen.insert(key)
    });

    operations
}

/// Infer verb and resource from HTTP method + path + operation metadata.
fn infer_verb_resource(path: &str, op: &OperationInfo) -> (String, String) {
    // If operation_id exists, try to parse it first
    if let Some(ref op_id) = op.operation_id {
        if let Some((verb, resource)) = parse_operation_id(op_id) {
            return (verb, resource);
        }
    }

    let segments: Vec<&str> = path
        .split('/')
        .filter(|s| !s.is_empty() && !s.starts_with('{'))
        .collect();

    let resource = infer_resource(&segments, op);
    let has_id_param = path.contains('{');
    let verb = infer_verb(&op.method, has_id_param, &segments, op);

    (verb, resource)
}

/// Try to parse an operationId like "listUsers", "createProject", "getUser"
fn parse_operation_id(op_id: &str) -> Option<(String, String)> {
    // Common verb prefixes in operationIds
    let verb_mappings = [
        ("list", "list"),
        ("get", "read"),
        ("create", "create"),
        ("post", "create"),
        ("update", "update"),
        ("patch", "update"),
        ("put", "update"),
        ("delete", "delete"),
        ("remove", "delete"),
        ("search", "search"),
        ("find", "search"),
        ("send", "send"),
        ("upload", "upload"),
        ("download", "download"),
        ("start", "start"),
        ("stop", "stop"),
        ("cancel", "cancel"),
        ("generate", "generate"),
        ("analyze", "analyze"),
        ("embed", "embed"),
        ("classify", "classify"),
    ];

    let normalized = op_id.to_lowercase();

    for (prefix, verb) in &verb_mappings {
        if normalized.starts_with(prefix) {
            let rest = &op_id[prefix.len()..];
            if rest.is_empty() {
                continue;
            }
            // camelCase: "listUsers" -> resource = "users"
            // snake_case: "list_users" -> resource = "users"
            let resource = if rest.starts_with('_') {
                to_snake_case(&rest[1..])
            } else {
                to_snake_case(rest)
            };
            if !resource.is_empty() {
                return Some((verb.to_string(), resource));
            }
        }
    }

    None
}

/// Infer the resource name from path segments and tags.
fn infer_resource(segments: &[&str], op: &OperationInfo) -> String {
    // Prefer tag if available and clean
    if let Some(tag) = op.tags.first() {
        let cleaned = to_snake_case(tag);
        if !cleaned.is_empty() && cleaned.len() < 30 {
            return cleaned;
        }
    }

    // Use last non-parameter path segment
    if let Some(last) = segments.last() {
        return to_snake_case(last);
    }

    "resource".to_string()
}

/// Infer the verb from HTTP method and path context.
fn infer_verb(method: &str, has_id: bool, segments: &[&str], op: &OperationInfo) -> String {
    let last_segment = segments.last().map(|s| s.to_lowercase());

    // Check for action-like endpoints: /users/{id}/activate, /builds/{id}/retry
    if let Some(ref seg) = last_segment {
        let action_verbs = [
            "activate",
            "deactivate",
            "approve",
            "reject",
            "cancel",
            "retry",
            "start",
            "stop",
            "pause",
            "resume",
            "send",
            "upload",
            "download",
            "search",
            "invite",
            "assign",
            "unassign",
            "share",
            "copy",
            "move",
            "archive",
            "restore",
            "publish",
            "unpublish",
            "subscribe",
            "unsubscribe",
            "watch",
            "unwatch",
            "generate",
            "analyze",
            "translate",
            "summarize",
            "classify",
            "embed",
            "verify",
            "validate",
            "export",
            "import",
            "refresh",
            "sync",
            "merge",
            "split",
            "lock",
            "unlock",
        ];
        if action_verbs.contains(&seg.as_str()) {
            return seg.clone();
        }
    }

    match method {
        "GET" => {
            if has_id {
                "read".to_string()
            } else {
                // Could be list or search based on query params
                let has_search_params = op.parameters.iter().any(|p| {
                    let name_lower = p.name.to_lowercase();
                    name_lower == "q"
                        || name_lower == "query"
                        || name_lower == "search"
                        || name_lower == "keywords"
                });
                if has_search_params {
                    "search".to_string()
                } else {
                    "list".to_string()
                }
            }
        }
        "POST" => "create".to_string(),
        "PUT" => "update".to_string(),
        "PATCH" => "update".to_string(),
        "DELETE" => "delete".to_string(),
        _ => "read".to_string(),
    }
}

/// Convert camelCase or PascalCase or kebab-case to snake_case.
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut prev_lower = false;

    for (i, c) in s.chars().enumerate() {
        if c == '-' || c == ' ' || c == '.' {
            result.push('_');
            prev_lower = false;
            continue;
        }
        if c == '_' {
            result.push('_');
            prev_lower = false;
            continue;
        }
        if c.is_uppercase() {
            if prev_lower && i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap_or(c));
            prev_lower = false;
        } else {
            result.push(c);
            prev_lower = true;
        }
    }

    // Clean up double underscores
    while result.contains("__") {
        result = result.replace("__", "_");
    }
    result.trim_matches('_').to_string()
}

/// Extract parameters from OpenAPI parameter objects.
fn extract_parameters(params: &[ParameterInfo]) -> Vec<Parameter> {
    params
        .iter()
        .map(|p| {
            let (param_type, format, enum_values, pattern, default) =
                if let Some(ref schema) = p.schema {
                    (
                        schema
                            .schema_type
                            .clone()
                            .unwrap_or_else(|| "string".to_string()),
                        schema.format.clone(),
                        schema.enum_values.clone(),
                        schema.pattern.clone(),
                        schema.default.clone(),
                    )
                } else {
                    ("string".to_string(), None, None, None, None)
                };

            Parameter {
                name: p.name.clone(),
                param_type,
                location: p.location.clone(),
                required: Some(p.required),
                description: p.description.clone(),
                example: p.example.clone(),
                enum_values,
                format,
                pattern,
                default,
            }
        })
        .collect()
}

/// Extract body parameters — flattens requestBody schema properties into individual parameters.
fn extract_body_parameters(body: &RequestBodyInfo) -> Vec<Parameter> {
    let Some(ref schema) = body.schema else {
        return vec![];
    };

    // If the schema has properties, extract each as a body parameter
    if !schema.properties.is_empty() {
        return schema
            .properties
            .iter()
            .map(|(name, prop)| Parameter {
                name: name.clone(),
                param_type: prop
                    .schema_type
                    .clone()
                    .unwrap_or_else(|| "string".to_string()),
                location: "body".to_string(),
                required: Some(schema.required.contains(name)),
                description: prop.description.clone(),
                example: prop.example.clone(),
                enum_values: prop.enum_values.clone(),
                format: prop.format.clone(),
                pattern: prop.pattern.clone(),
                default: prop.default.clone(),
            })
            .collect();
    }

    // If it's a simple schema (e.g., type: array), create a single body param
    if schema.schema_type.is_some() {
        return vec![Parameter {
            name: "body".to_string(),
            param_type: schema
                .schema_type
                .clone()
                .unwrap_or_else(|| "object".to_string()),
            location: "body".to_string(),
            required: Some(body.required),
            description: body
                .description
                .clone()
                .or_else(|| schema.description.clone()),
            example: schema.example.clone(),
            enum_values: None,
            format: None,
            pattern: None,
            default: None,
        }];
    }

    vec![]
}

/// Extract response schema from the success response (200 or 201).
fn extract_response_schema(op: &OperationInfo) -> Option<serde_json::Value> {
    let success_resp = op
        .responses
        .get("200")
        .or_else(|| op.responses.get("201"))
        .or_else(|| op.responses.get("202"));

    success_resp.and_then(|resp| resp.schema.as_ref().map(|s| schema_to_json_schema(s)))
}

/// Convert our SchemaInfo to a simplified JSON Schema representation.
fn schema_to_json_schema(schema: &SchemaInfo) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    if let Some(ref t) = schema.schema_type {
        obj.insert("type".to_string(), serde_json::Value::String(t.clone()));
    }
    if let Some(ref desc) = schema.description {
        obj.insert(
            "description".to_string(),
            serde_json::Value::String(desc.clone()),
        );
    }
    if let Some(ref items) = schema.items {
        obj.insert("items".to_string(), schema_to_json_schema(items));
    }
    if !schema.properties.is_empty() {
        let props: serde_json::Map<String, serde_json::Value> = schema
            .properties
            .iter()
            .map(|(k, v)| (k.clone(), schema_to_json_schema(v)))
            .collect();
        obj.insert("properties".to_string(), serde_json::Value::Object(props));
    }

    serde_json::Value::Object(obj)
}

/// Extract required OAuth scopes from operation security requirements.
fn extract_required_scopes(op: &OperationInfo) -> Option<Vec<String>> {
    let scopes: Vec<String> = op
        .security
        .iter()
        .flat_map(|sec_req| sec_req.values().flatten().cloned())
        .collect();

    if scopes.is_empty() {
        None
    } else {
        Some(scopes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("listUsers"), "list_users");
        assert_eq!(to_snake_case("PullRequest"), "pull_request");
        assert_eq!(to_snake_case("dns-records"), "dns_records");
        assert_eq!(to_snake_case("HTTPRequest"), "httprequest");
        assert_eq!(to_snake_case("simpleTest"), "simple_test");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn test_parse_operation_id() {
        assert_eq!(
            parse_operation_id("listUsers"),
            Some(("list".to_string(), "users".to_string()))
        );
        assert_eq!(
            parse_operation_id("createProject"),
            Some(("create".to_string(), "project".to_string()))
        );
        assert_eq!(
            parse_operation_id("getUser"),
            Some(("read".to_string(), "user".to_string()))
        );
        assert_eq!(
            parse_operation_id("deleteMessage"),
            Some(("delete".to_string(), "message".to_string()))
        );
        assert_eq!(
            parse_operation_id("searchFiles"),
            Some(("search".to_string(), "files".to_string()))
        );
    }
}
