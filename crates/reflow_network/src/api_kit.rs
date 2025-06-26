use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use tracing::{error, info, warn};

use crate::actor::{Actor, ActorBehavior, ActorContext, ActorState, MemoryState, Port};
use crate::message::Message;

// ============================================================================
// Service Registry Types (matching our JSON schema)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistry {
    pub schema_version: String,
    pub services: HashMap<String, Service>,
    pub global_settings: GlobalSettings,
}

impl ServiceRegistry {
    /// Load a ServiceRegistry from a JSON file
    pub fn from_json_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_content = fs::read_to_string(path)?;
        let registry: ServiceRegistry = serde_json::from_str(&file_content)?;
        Ok(registry)
    }

    /// Load a ServiceRegistry from a YAML file
    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file_content = fs::read_to_string(path)?;
        let registry: ServiceRegistry = serde_yaml::from_str(&file_content)?;
        Ok(registry)
    }

    /// Convert a ServiceRegistry to YAML format
    pub fn to_yaml(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(self)?;
        Ok(yaml)
    }

    /// Save a ServiceRegistry to a YAML file
    pub fn to_yaml_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let yaml = self.to_yaml()?;
        fs::write(path, yaml)?;
        Ok(())
    }

    /// Convert a ServiceRegistry to JSON format
    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::to_string_pretty(self)?;
        Ok(json)
    }

    /// Save a ServiceRegistry to a JSON file
    pub fn to_json_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = self.to_json()?;
        fs::write(path, json)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    pub description: Option<String>,
    pub category: ServiceCategory,
    pub vendor: Option<String>,
    pub api_specs: Vec<ApiSpec>,
    pub authentication: Authentication,
    pub rate_limits: RateLimits,
    pub operations: Vec<Operation>,
    pub webhooks: Option<WebhookConfig>,
    pub sdks: Option<HashMap<String, String>>,
    pub documentation: Option<Documentation>,
    pub status: ServiceStatus,
    pub pricing_model: Option<PricingModel>,
    pub compliance: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCategory {
    Communication,
    CrmSales,
    DataStorage,
    AiServices,
    Development,
    ProjectManagement,
    Payment,
    Analytics,
    Security,
    Weather,
    Finance,
    Location,
    News,
    Entertainment,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSpec {
    #[serde(rename = "type")]
    pub api_type: ApiType,
    pub version: Option<String>,
    pub base_url: String,
    pub schema_url: Option<String>,
    pub sandbox_url: Option<String>,
    pub spec_format: Option<String>,
    pub transport: Option<String>,
    pub content_types: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiType {
    #[serde(rename = "openapi")]
    OpenApi,
    #[serde(rename = "graphql")]
    GraphQL,
    #[serde(rename = "websocket")]
    WebSocket,
    #[serde(rename = "grpc")]
    Grpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authentication {
    pub primary_method: AuthMethod,
    pub alternative_methods: Option<Vec<AuthMethod>>,
    pub oauth2_config: Option<OAuth2Config>,
    pub api_key_config: Option<ApiKeyConfig>,
    pub custom_auth: Option<CustomAuth>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    #[serde(rename = "oauth2")]
    OAuth2,
    ApiKey,
    BearerToken,
    BasicAuth,
    Jwt,
    Hmac,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2Config {
    pub authorization_url: String,
    pub token_url: String,
    pub scopes_url: Option<String>,
    pub flows: Vec<String>,
    pub pkce_required: Option<bool>,
    pub common_scopes: Option<Vec<Scope>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    pub scope: String,
    pub description: String,
    pub required_for: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    pub location: String,
    pub parameter_name: String,
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAuth {
    pub description: String,
    pub implementation_guide: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    pub default_limits: Option<RateLimit>,
    pub tier_based_limits: Option<HashMap<String, RateLimit>>,
    pub endpoint_specific: Option<HashMap<String, RateLimit>>,
    pub rate_limit_headers: Option<RateLimitHeaders>,
    pub burst_allowance: Option<f64>,
    pub backoff_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    pub requests_per_second: Option<f64>,
    pub requests_per_minute: Option<f64>,
    pub requests_per_hour: Option<f64>,
    pub requests_per_day: Option<f64>,
    pub concurrent_requests: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitHeaders {
    pub limit_header: Option<String>,
    pub remaining_header: Option<String>,
    pub reset_header: Option<String>,
    pub retry_after_header: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub verb: String,
    pub resource: String,
    pub method: String,
    pub endpoint_pattern: String,
    pub graphql_operation: Option<GraphQLOperation>,
    pub parameters: Option<Vec<Parameter>>,
    pub required_scopes: Option<Vec<String>>,
    pub rate_limit_cost: Option<u32>,
    pub batch_capable: Option<bool>,
    pub webhook_events: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphQLOperation {
    #[serde(rename = "type")]
    pub operation_type: String,
    pub operation_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    pub location: String,
    pub required: Option<bool>,
    pub description: Option<String>,
    pub example: Option<serde_json::Value>,
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    pub format: Option<String>,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub supported: bool,
    pub setup_url: Option<String>,
    pub events: Option<Vec<WebhookEvent>>,
    pub security: Option<WebhookSecurity>,
    pub retry_policy: Option<WebhookRetryPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub event_type: String,
    pub description: String,
    pub payload_schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSecurity {
    pub signature_method: String,
    pub header_name: String,
    pub verification_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookRetryPolicy {
    pub max_attempts: Option<u32>,
    pub backoff_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Documentation {
    pub main_docs: Option<String>,
    pub api_reference: Option<String>,
    pub tutorials: Option<String>,
    pub postman_collection: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    Active,
    Beta,
    Deprecated,
    Legacy,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PricingModel {
    Free,
    Freemium,
    Subscription,
    UsageBased,
    Enterprise,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSettings {
    pub default_timeout: Option<u32>,
    pub default_retries: Option<u32>,
    pub user_agent: Option<String>,
    pub global_rate_limit: Option<GlobalRateLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRateLimit {
    pub enabled: bool,
    pub requests_per_second: Option<f64>,
}

// ============================================================================
// API Tool Generation System
// ============================================================================

#[derive(Debug)]
pub struct ApiToolGenerator {
    registry: ServiceRegistry,
    auth_manager: Arc<dyn AuthManager>,
    rate_limiter: Arc<dyn RateLimiter>,
    http_client: reqwest::Client,
}

impl ApiToolGenerator {
    pub fn new(
        registry: ServiceRegistry,
        auth_manager: Arc<dyn AuthManager>,
        rate_limiter: Arc<dyn RateLimiter>,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            registry,
            auth_manager,
            rate_limiter,
            http_client,
        }
    }

    pub fn generate_all_actors(&self) -> Result<Vec<Box<dyn Actor>>> {
        let mut actors = Vec::new();

        for (service_id, service) in &self.registry.services {
            let service_actors = self.generate_service_actors(service_id, service)?;
            actors.extend(service_actors);
        }

        Ok(actors)
    }

    pub fn generate_service_actors(
        &self,
        service_id: &str,
        service: &Service,
    ) -> Result<Vec<Box<dyn Actor>>> {
        let mut actors = Vec::new();

        for operation in &service.operations {
            let actor = self.generate_operation_actor(service_id, service, operation)?;
            actors.push(actor);
        }

        Ok(actors)
    }

    fn generate_operation_actor(
        &self,
        service_id: &str,
        service: &Service,
        operation: &Operation,
    ) -> Result<Box<dyn Actor>> {
        let actor_id = format!("{}_{}_{}", service_id, operation.verb, operation.resource);

        let base_url = service
            .api_specs
            .first()
            .map(|spec| spec.base_url.clone())
            .unwrap_or_default();

        let actor = ApiOperationActor::new(
            actor_id,
            service_id.to_string(),
            service.clone(),
            operation.clone(),
            base_url,
            self.auth_manager.clone(),
            self.rate_limiter.clone(),
            self.http_client.clone(),
        );

        Ok(Box::new(actor))
    }

    pub fn generate_openai_functions(&self) -> Result<Vec<serde_json::Value>> {
        let mut functions = Vec::new();

        for (service_id, service) in &self.registry.services {
            for operation in &service.operations {
                let function = self.generate_openai_function(service_id, service, operation)?;
                functions.push(function);
            }
        }

        Ok(functions)
    }

    fn generate_openai_function(
        &self,
        service_id: &str,
        service: &Service,
        operation: &Operation,
    ) -> Result<serde_json::Value> {
        let function_name = format!("{}_{}_{}", service_id, operation.verb, operation.resource);

        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        if let Some(parameters) = &operation.parameters {
            for param in parameters {
                let param_schema = self.parameter_to_json_schema(param);
                properties.insert(param.name.clone(), param_schema);

                if param.required.unwrap_or(false) {
                    required.push(param.name.clone());
                }
            }
        }

        Ok(serde_json::json!({
            "type": "function",
            "function": {
                "name": function_name,
                "description": format!("{} {} using {} API",
                    operation.verb, operation.resource, service.name),
                "parameters": {
                    "type": "object",
                    "properties": properties,
                    "required": required
                }
            }
        }))
    }

    fn parameter_to_json_schema(&self, param: &Parameter) -> serde_json::Value {
        let mut schema = serde_json::Map::new();

        let json_type = match param.param_type.as_str() {
            "string" => "string",
            "integer" => "integer",
            "number" => "number",
            "boolean" => "boolean",
            "array" => "array",
            "object" => "object",
            "file" => "string",
            _ => "string",
        };

        schema.insert(
            "type".to_string(),
            serde_json::Value::String(json_type.to_string()),
        );

        if let Some(description) = &param.description {
            schema.insert(
                "description".to_string(),
                serde_json::Value::String(description.clone()),
            );
        }

        if let Some(enum_values) = &param.enum_values {
            let enum_json: Vec<serde_json::Value> = enum_values
                .iter()
                .map(|v| serde_json::Value::String(v.clone()))
                .collect();
            schema.insert("enum".to_string(), serde_json::Value::Array(enum_json));
        }

        if let Some(example) = &param.example {
            schema.insert("example".to_string(), example.clone());
        }

        serde_json::Value::Object(schema)
    }

    pub fn generate_mcp_schema(&self) -> Result<serde_json::Value> {
        let mut tools = Vec::new();

        for (service_id, service) in &self.registry.services {
            for operation in &service.operations {
                let tool = self.generate_mcp_tool(service_id, service, operation)?;
                tools.push(tool);
            }
        }

        Ok(serde_json::json!({
            "version": "2024-11-05",
            "tools": tools
        }))
    }

    fn generate_mcp_tool(
        &self,
        service_id: &str,
        service: &Service,
        operation: &Operation,
    ) -> Result<serde_json::Value> {
        let tool_name = format!("{}_{}_{}", service_id, operation.verb, operation.resource);

        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        if let Some(parameters) = &operation.parameters {
            for param in parameters {
                let param_schema = self.parameter_to_json_schema(param);
                properties.insert(param.name.clone(), param_schema);

                if param.required.unwrap_or(false) {
                    required.push(param.name.clone());
                }
            }
        }

        Ok(serde_json::json!({
            "name": tool_name,
            "description": format!("{} {} using {} API",
                operation.verb, operation.resource, service.name),
            "inputSchema": {
                "type": "object",
                "properties": properties,
                "required": required
            }
        }))
    }
}

// ============================================================================
// API Operation Actor Implementation
// ============================================================================

#[derive(Clone)]
pub struct ApiOperationActor {
    pub id: String,
    pub service_id: String,
    pub service: Service,
    pub operation: Operation,
    pub base_url: String,
    pub auth_manager: Arc<dyn AuthManager>,
    pub rate_limiter: Arc<dyn RateLimiter>,
    pub http_client: reqwest::Client,
    pub inports: Port,
    pub outports: Port,
}

impl ApiOperationActor {
    pub fn new(
        id: String,
        service_id: String,
        service: Service,
        operation: Operation,
        base_url: String,
        auth_manager: Arc<dyn AuthManager>,
        rate_limiter: Arc<dyn RateLimiter>,
        http_client: reqwest::Client,
    ) -> Self {
        let inports = flume::unbounded();
        let outports = flume::unbounded();

        Self {
            id,
            service_id,
            service,
            operation,
            base_url,
            auth_manager,
            rate_limiter,
            http_client,
            inports,
            outports,
        }
    }

    async fn execute_api_call(&self, context: &ActorContext) -> Result<HashMap<String, Message>> {
        // Apply rate limiting
        self.rate_limiter
            .wait_if_needed(&self.service_id, &self.operation.verb)
            .await?;

        // Build the request URL
        let url = self.build_request_url(context)?;

        // Prepare headers
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        // Add authentication
        self.add_authentication(&mut headers).await?;

        // Build request
        let mut request_builder = match self.operation.method.as_str() {
            "GET" => self.http_client.get(&url),
            "POST" => self.http_client.post(&url),
            "PUT" => self.http_client.put(&url),
            "PATCH" => self.http_client.patch(&url),
            "DELETE" => self.http_client.delete(&url),
            _ => {
                return Err(anyhow!(
                    "Unsupported HTTP method: {}",
                    self.operation.method
                ));
            }
        };

        request_builder = request_builder.headers(headers);

        // Add query parameters and body
        if self.operation.method == "GET" || self.operation.method == "DELETE" {
            let query_params = self.extract_query_parameters(context)?;
            if !query_params.is_empty() {
                request_builder = request_builder.query(&query_params);
            }
        } else {
            let body = self.build_request_body(context)?;
            if let Some(body) = body {
                request_builder = request_builder.json(&body);
            }
        }

        // Execute request
        let response = request_builder.send().await?;

        // Handle response
        self.handle_response(response).await
    }

    fn build_request_url(&self, context: &ActorContext) -> Result<String> {
        let mut endpoint = self.operation.endpoint_pattern.clone();

        // Replace path parameters
        if let Some(parameters) = &self.operation.parameters {
            for param in parameters {
                if param.location == "path" {
                    if let Some(value) = context.payload.get(&param.name) {
                        let string_value = self.message_to_string(value)?;
                        endpoint = endpoint.replace(&format!("{{{}}}", param.name), &string_value);
                    }
                }
            }
        }

        let url = if self.base_url.ends_with('/') && endpoint.starts_with('/') {
            format!("{}{}", &self.base_url[..self.base_url.len() - 1], endpoint)
        } else if !self.base_url.ends_with('/') && !endpoint.starts_with('/') {
            format!("{}/{}", self.base_url, endpoint)
        } else {
            format!("{}{}", self.base_url, endpoint)
        };

        Ok(url)
    }

    async fn add_authentication(&self, headers: &mut reqwest::header::HeaderMap) -> Result<()> {
        match self.service.authentication.primary_method {
            AuthMethod::OAuth2 => {
                let token = self.auth_manager.get_oauth_token(&self.service_id).await?;
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token))?,
                );
            }
            AuthMethod::BearerToken => {
                let token = self.auth_manager.get_bearer_token(&self.service_id).await?;
                headers.insert(
                    reqwest::header::AUTHORIZATION,
                    reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token))?,
                );
            }
            AuthMethod::ApiKey => {
                if let Some(config) = &self.service.authentication.api_key_config {
                    let api_key = self.auth_manager.get_api_key(&self.service_id).await?;
                    let header_value = if let Some(prefix) = &config.prefix {
                        format!("{} {}", prefix, api_key)
                    } else {
                        api_key
                    };

                    if config.location == "header" {
                        headers.insert(
                            reqwest::header::HeaderName::from_bytes(
                                config.parameter_name.as_bytes(),
                            )?,
                            reqwest::header::HeaderValue::from_str(&header_value)?,
                        );
                    }
                }
            }
            _ => {
                warn!(
                    "Unsupported authentication method: {:?}",
                    self.service.authentication.primary_method
                );
            }
        }

        Ok(())
    }

    fn extract_query_parameters(&self, context: &ActorContext) -> Result<Vec<(String, String)>> {
        let mut params = Vec::new();

        if let Some(parameters) = &self.operation.parameters {
            for param in parameters {
                if param.location == "query" {
                    if let Some(value) = context.payload.get(&param.name) {
                        let string_value = self.message_to_string(value)?;
                        params.push((param.name.clone(), string_value));
                    }
                }
            }
        }

        Ok(params)
    }

    fn build_request_body(&self, context: &ActorContext) -> Result<Option<serde_json::Value>> {
        let mut body = serde_json::Map::new();
        let mut has_body_params = false;

        if let Some(parameters) = &self.operation.parameters {
            for param in parameters {
                if param.location == "body" {
                    if let Some(value) = context.payload.get(&param.name) {
                        body.insert(param.name.clone(), value.clone().into());
                        has_body_params = true;
                    }
                }
            }
        }

        if has_body_params {
            Ok(Some(serde_json::Value::Object(body)))
        } else {
            Ok(None)
        }
    }

    async fn handle_response(
        &self,
        response: reqwest::Response,
    ) -> Result<HashMap<String, Message>> {
        let status = response.status();
        let mut result = HashMap::new();

        // Handle rate limiting
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(60);

            return Err(anyhow!(
                "Rate limit exceeded. Retry after {} seconds",
                retry_after
            ));
        }

        // Handle authentication errors
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(anyhow!("Authentication failed"));
        }

        // Handle other client/server errors
        if status.is_client_error() || status.is_server_error() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow!("API request failed: {} - {}", status, error_text));
        }

        // Parse successful response
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|h| h.to_str().ok())
            .unwrap_or("");

        if content_type.contains("application/json") {
            let json_response: serde_json::Value = response.json().await?;
            result.insert("data".to_string(), Message::object(json_response.into()));
        } else {
            let text_response = response.text().await?;
            result.insert("data".to_string(), Message::string(text_response));
        }

        result.insert(
            "status_code".to_string(),
            Message::integer(status.as_u16() as i64),
        );

        Ok(result)
    }

    fn message_to_string(&self, message: &Message) -> Result<String> {
        match message {
            Message::String(s) => Ok(s.as_ref().clone()),
            Message::Integer(i) => Ok(i.to_string()),
            Message::Float(f) => Ok(f.to_string()),
            Message::Boolean(b) => Ok(b.to_string()),
            _ => Err(anyhow!("Cannot convert message to string: {:?}", message)),
        }
    }
}

#[async_trait::async_trait]
impl Actor for ApiOperationActor {
    fn get_behavior(&self) -> ActorBehavior {
        let actor = self.clone();

        Box::new(move |context: ActorContext| {
            let actor = actor.clone();
            Box::pin(async move {
                info!("Executing API operation: {}", actor.id);

                let result = actor.execute_api_call(&context).await;

                match result {
                    Ok(response) => {
                        info!("API operation {} completed successfully", actor.id);
                        Ok(response)
                    }
                    Err(e) => {
                        error!("API operation {} failed: {}", actor.id, e);
                        Ok(HashMap::from([
                            ("error".to_string(), Message::error(e.to_string())),
                            ("actor_id".to_string(), Message::string(actor.id.clone())),
                        ]))
                    }
                }
            })
        })
    }

    fn get_inports(&self) -> Port {
        self.inports.clone()
    }

    fn get_outports(&self) -> Port {
        self.outports.clone()
    }

    fn create_process(&self) -> Pin<Box<dyn Future<Output = ()> + 'static + Send>> {
        let inports = self.get_inports();
        let behavior = self.get_behavior();
        let state = Arc::new(parking_lot::Mutex::new(MemoryState::default()));
        let outports = self.get_outports();
        let load = self.load_count();

        Box::pin(async move {
            while let Ok(payload) = inports.1.recv_async().await {
                {
                    let mut load_guard = load.lock();
                    load_guard.inc();
                }

                let context = ActorContext::new(
                    payload,
                    outports.clone(),
                    state.clone(),
                    HashMap::new(),
                    load.clone(),
                );

                match behavior(context).await {
                    Ok(result) => {
                        if !result.is_empty() {
                            let _ = outports.0.send_async(result).await;
                        }
                    }
                    Err(e) => {
                        let error_msg =
                            HashMap::from([("error".to_string(), Message::error(e.to_string()))]);
                        let _ = outports.0.send_async(error_msg).await;
                    }
                }

                {
                    let mut load_guard = load.lock();
                    load_guard.dec();
                }
            }
        })
    }
}

// ============================================================================
// Authentication Management
// ============================================================================

#[async_trait::async_trait]
pub trait AuthManager: Send + Sync + std::fmt::Debug {
    async fn get_oauth_token(&self, service_id: &str) -> Result<String>;
    async fn get_bearer_token(&self, service_id: &str) -> Result<String>;
    async fn get_api_key(&self, service_id: &str) -> Result<String>;
    async fn refresh_token(&self, service_id: &str) -> Result<String>;
}

#[derive(Debug)]
pub struct DefaultAuthManager {
    tokens: Arc<Mutex<HashMap<String, TokenInfo>>>,
    credentials: HashMap<String, ServiceCredentials>,
}

#[derive(Debug, Clone)]
struct TokenInfo {
    token: String,
    expires_at: Option<std::time::SystemTime>,
    refresh_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServiceCredentials {
    pub api_key: Option<String>,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub oauth_config: Option<OAuth2Config>,
}

impl DefaultAuthManager {
    pub fn new(credentials: HashMap<String, ServiceCredentials>) -> Self {
        Self {
            tokens: Arc::new(Mutex::new(HashMap::new())),
            credentials,
        }
    }
}

#[async_trait::async_trait]
impl AuthManager for DefaultAuthManager {
    async fn get_oauth_token(&self, service_id: &str) -> Result<String> {
        // Check for cached token
        {
            let tokens = self.tokens.lock().unwrap();
            if let Some(token_info) = tokens.get(service_id) {
                if let Some(expires_at) = token_info.expires_at {
                    if std::time::SystemTime::now() < expires_at {
                        return Ok(token_info.token.clone());
                    }
                } else {
                    return Ok(token_info.token.clone());
                }
            }
        }

        // Token expired or doesn't exist, refresh it
        self.refresh_token(service_id).await
    }

    async fn get_bearer_token(&self, service_id: &str) -> Result<String> {
        self.get_oauth_token(service_id).await
    }

    async fn get_api_key(&self, service_id: &str) -> Result<String> {
        self.credentials
            .get(service_id)
            .and_then(|creds| creds.api_key.clone())
            .ok_or_else(|| anyhow!("No API key found for service: {}", service_id))
    }

    async fn refresh_token(&self, service_id: &str) -> Result<String> {
        let credentials = self
            .credentials
            .get(service_id)
            .ok_or_else(|| anyhow!("No credentials found for service: {}", service_id))?;

        let oauth_config = credentials
            .oauth_config
            .as_ref()
            .ok_or_else(|| anyhow!("No OAuth config found for service: {}", service_id))?;

        let client_id = credentials
            .client_id
            .as_ref()
            .ok_or_else(|| anyhow!("No client ID found for service: {}", service_id))?;

        let client_secret = credentials
            .client_secret
            .as_ref()
            .ok_or_else(|| anyhow!("No client secret found for service: {}", service_id))?;

        // Implement OAuth2 client credentials flow
        let client = reqwest::Client::new();
        let response = client
            .post(&oauth_config.token_url)
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", client_id),
                ("client_secret", client_secret),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow!("OAuth token request failed: {}", response.status()));
        }

        let token_response: serde_json::Value = response.json::<serde_json::Value>().await?;
        let access_token = token_response["access_token"]
            .as_str()
            .ok_or_else(|| anyhow!("No access token in response"))?;

        let expires_in = token_response["expires_in"]
            .as_u64()
            .map(|secs| std::time::SystemTime::now() + std::time::Duration::from_secs(secs));

        let refresh_token = token_response["refresh_token"]
            .as_str()
            .map(|s| s.to_string());

        // Cache the token
        {
            let mut tokens = self.tokens.lock().unwrap();
            tokens.insert(
                service_id.to_string(),
                TokenInfo {
                    token: access_token.to_string(),
                    expires_at: expires_in,
                    refresh_token,
                },
            );
        }

        Ok(access_token.to_string())
    }
}

// ============================================================================
// Rate Limiting
// ============================================================================

#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync + std::fmt::Debug {
    async fn wait_if_needed(&self, service_id: &str, operation: &str) -> Result<()>;
    async fn record_request(&self, service_id: &str, operation: &str);
    async fn get_remaining(&self, service_id: &str) -> Option<u32>;
}

#[derive(Debug)]
pub struct DefaultRateLimiter {
    service_limits: HashMap<String, RateLimit>,
    request_timestamps: Arc<Mutex<HashMap<String, Vec<std::time::Instant>>>>,
}

impl DefaultRateLimiter {
    pub fn new(service_limits: HashMap<String, RateLimit>) -> Self {
        Self {
            service_limits,
            request_timestamps: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

unsafe impl Send for DefaultRateLimiter {}
unsafe impl Sync for DefaultRateLimiter {}

#[async_trait::async_trait]
impl RateLimiter for DefaultRateLimiter {
    async fn wait_if_needed(&self, service_id: &str, _operation: &str) -> Result<()> {
        // First, determine if we need to wait and for how long, without holding the lock across await
        let wait_time_option = if let Some(limit) = self.service_limits.get(service_id) {
            if let Some(requests_per_minute) = limit.requests_per_minute {
                let now = std::time::Instant::now();
                let one_minute_ago = now - std::time::Duration::from_secs(60);

                // Scope for mutex lock
                let mut timestamps = self.request_timestamps.lock().unwrap();
                let service_timestamps = timestamps
                    .entry(service_id.to_string())
                    .or_insert_with(Vec::new);

                // Clean old timestamps
                service_timestamps.retain(|&timestamp| timestamp > one_minute_ago);

                // Check if we need to wait
                let wait_duration = if service_timestamps.len() >= requests_per_minute as usize {
                    let oldest = service_timestamps[0];
                    Some(std::time::Duration::from_secs(60) - (now - oldest))
                } else {
                    None
                };

                // Release the lock before returning
                drop(timestamps);
                wait_duration
            } else {
                None
            }
        } else {
            None
        };

        // If we need to wait, do it after the lock is released
        if let Some(duration) = wait_time_option {
            tokio::time::sleep(duration).await;
        }

        Ok(())
    }

    async fn record_request(&self, service_id: &str, _operation: &str) {
        let mut timestamps = self.request_timestamps.lock().unwrap();
        let service_timestamps = timestamps
            .entry(service_id.to_string())
            .or_insert_with(Vec::new);
        service_timestamps.push(std::time::Instant::now());
    }

    async fn get_remaining(&self, service_id: &str) -> Option<u32> {
        if let Some(limit) = self.service_limits.get(service_id) {
            let timestamps = self.request_timestamps.lock().unwrap();
            if let Some(service_timestamps) = timestamps.get(service_id) {
                if let Some(requests_per_minute) = limit.requests_per_minute {
                    let now = std::time::Instant::now();
                    let one_minute_ago = now - std::time::Duration::from_secs(60);
                    let recent_requests = service_timestamps
                        .iter()
                        .filter(|&&timestamp| timestamp > one_minute_ago)
                        .count();

                    return Some(
                        (requests_per_minute as usize).saturating_sub(recent_requests) as u32,
                    );
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::{env, path::PathBuf};

    use crate::actor::ActorLoad;

    use super::*;

    #[tokio::test]
    async fn test_api_operation_actor() {
        let service = Service {
            name: "Test Service".to_string(),
            description: None,
            category: ServiceCategory::Other,
            vendor: None,
            api_specs: vec![ApiSpec {
                api_type: ApiType::OpenApi,
                version: Some("v1".to_string()),
                base_url: "https://api.test.com".to_string(),
                schema_url: None,
                sandbox_url: None,
                spec_format: None,
                transport: None,
                content_types: None,
            }],
            authentication: Authentication {
                primary_method: AuthMethod::ApiKey,
                alternative_methods: None,
                oauth2_config: None,
                api_key_config: Some(ApiKeyConfig {
                    location: "header".to_string(),
                    parameter_name: "X-API-Key".to_string(),
                    prefix: None,
                }),
                custom_auth: None,
            },
            rate_limits: RateLimits {
                default_limits: Some(RateLimit {
                    requests_per_second: Some(10.0),
                    requests_per_minute: Some(600.0),
                    requests_per_hour: None,
                    requests_per_day: None,
                    concurrent_requests: None,
                }),
                tier_based_limits: None,
                endpoint_specific: None,
                rate_limit_headers: None,
                burst_allowance: None,
                backoff_strategy: None,
            },
            operations: vec![],
            webhooks: None,
            sdks: None,
            documentation: None,
            status: ServiceStatus::Active,
            pricing_model: None,
            compliance: None,
        };

        let operation = Operation {
            verb: "get".to_string(),
            resource: "user".to_string(),
            method: "GET".to_string(),
            endpoint_pattern: "/users/{id}".to_string(),
            graphql_operation: None,
            parameters: Some(vec![Parameter {
                name: "id".to_string(),
                param_type: "string".to_string(),
                location: "path".to_string(),
                required: Some(true),
                description: Some("User ID".to_string()),
                example: None,
                enum_values: None,
                format: None,
                pattern: None,
            }]),
            required_scopes: None,
            rate_limit_cost: None,
            batch_capable: None,
            webhook_events: None,
        };

        let credentials = HashMap::from([(
            "test_service".to_string(),
            ServiceCredentials {
                api_key: Some("test-api-key".to_string()),
                client_id: None,
                client_secret: None,
                oauth_config: None,
            },
        )]);

        let auth_manager = Arc::new(DefaultAuthManager::new(credentials));
        let rate_limiter = Arc::new(DefaultRateLimiter::new(HashMap::new()));
        let http_client = reqwest::Client::new();

        let actor = ApiOperationActor::new(
            "test_get_user".to_string(),
            "test_service".to_string(),
            service,
            operation,
            "https://api.test.com".to_string(),
            auth_manager,
            rate_limiter,
            http_client,
        );

        // Test URL building
        let payload = HashMap::from([("id".to_string(), Message::string("123".to_string()))]);

        let context = ActorContext::new(
            payload,
            actor.get_outports(),
            Arc::new(parking_lot::Mutex::new(MemoryState::default())),
            HashMap::new(),
            Arc::new(parking_lot::Mutex::new(ActorLoad::new(0))),
        );

        let url = actor.build_request_url(&context).unwrap();
        assert_eq!(url, "https://api.test.com/users/123");
    }

    #[test]
    fn test_tool_schema_generation() {
        let service_registry_json_string = include_str!("../../../api_service_registry.json");
        let registry: ServiceRegistry = serde_json::from_str(service_registry_json_string).unwrap();

        let auth_manager = Arc::new(DefaultAuthManager::new(HashMap::new()));
        let rate_limiter = Arc::new(DefaultRateLimiter::new(HashMap::new()));

        let generator = ApiToolGenerator::new(registry, auth_manager, rate_limiter);

        let actors: Vec<Box<dyn Actor>> = generator.generate_all_actors().unwrap();
        assert!(!actors.is_empty());

        let openai_functions = generator.generate_openai_functions().unwrap();
        let mcp_schema = generator.generate_mcp_schema().unwrap();

        assert!(!openai_functions.is_empty());
        assert_eq!(mcp_schema["version"], "2024-11-05");
        assert!(!mcp_schema["tools"].is_null());

        println!(
            "{}",
            serde_json::to_string_pretty(&mcp_schema["tools"]).unwrap()
        );
    }

    #[test]
    fn test_yaml_converter() {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

        let service_registry_yaml_string = manifest_dir.join("../../api_service_registry.yaml");
        let registry: ServiceRegistry =
            ServiceRegistry::from_yaml_file(service_registry_yaml_string).unwrap();

        assert!(!registry.services.is_empty());

        let auth_manager = Arc::new(DefaultAuthManager::new(HashMap::new()));
        let rate_limiter = Arc::new(DefaultRateLimiter::new(HashMap::new()));

        let generator = ApiToolGenerator::new(registry, auth_manager, rate_limiter);

        let actors: Vec<Box<dyn Actor>> = generator.generate_all_actors().unwrap();
        assert!(!actors.is_empty());

        let openai_functions = generator.generate_openai_functions().unwrap();
        let mcp_schema = generator.generate_mcp_schema().unwrap();

        assert!(!openai_functions.is_empty());
        assert_eq!(mcp_schema["version"], "2024-11-05");
        assert!(!mcp_schema["tools"].is_null());

        println!(
            "{}",
            serde_yaml::to_string(&mcp_schema["tools"]).unwrap()
        );
    }
}
