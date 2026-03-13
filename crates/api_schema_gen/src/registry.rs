//! Service Registry types — mirrors api_kit.rs structs for standalone use.
//! These types are kept in sync with reflow_network::api_kit but don't depend on it,
//! so this tool can run independently as a build-time code generator.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRegistry {
    pub schema_version: String,
    pub services: IndexMap<String, Service>,
    pub global_settings: GlobalSettings,
}

impl ServiceRegistry {
    pub fn from_json_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let registry: ServiceRegistry = serde_json::from_str(&content)?;
        Ok(registry)
    }

    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let registry: ServiceRegistry = serde_yaml::from_str(&content)?;
        Ok(registry)
    }

    pub fn to_json(&self) -> Result<String> {
        let json = serde_json::to_string_pretty(self)?;
        Ok(json)
    }

    pub fn to_json_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = self.to_json()?;
        fs::write(path, json)?;
        Ok(())
    }

    pub fn to_yaml(&self) -> Result<String> {
        let yaml = serde_yaml::to_string(self)?;
        Ok(yaml)
    }

    pub fn to_yaml_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let yaml = self.to_yaml()?;
        fs::write(path, yaml)?;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhooks: Option<WebhookConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sdks: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<Documentation>,
    #[serde(default = "default_status")]
    pub status: ServiceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pricing_model: Option<PricingModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    SocialMedia,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSpec {
    #[serde(rename = "type")]
    pub api_type: ApiType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub base_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative_methods: Option<Vec<AuthMethod>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth2_config: Option<OAuth2Config>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_config: Option<ApiKeyConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scopes_url: Option<String>,
    pub flows: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pkce_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub common_scopes: Option<Vec<Scope>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scope {
    pub scope: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_for: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyConfig {
    pub location: String,
    pub parameter_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAuth {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implementation_guide: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimits {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_limits: Option<RateLimit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier_based_limits: Option<HashMap<String, RateLimit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_specific: Option<HashMap<String, RateLimit>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_headers: Option<RateLimitHeaders>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub burst_allowance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_second: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_minute: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_hour: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_day: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_month: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_100_seconds: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrent_requests: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens_per_minute: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_per_minute: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitHeaders {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_header: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_header: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Operation {
    pub verb: String,
    pub resource: String,
    pub method: String,
    pub endpoint_pattern: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphql_operation: Option<GraphQLOperation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Vec<Parameter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_scopes: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_cost: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_capable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_events: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<serde_json::Value>,
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_values: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub supported: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub setup_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<Vec<WebhookEvent>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub security: Option<WebhookSecurity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_policy: Option<WebhookRetryPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub event_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_schema: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSecurity {
    pub signature_method: String,
    pub header_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookRetryPolicy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_attempts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff_strategy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Documentation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_docs: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tutorials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postman_collection: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceStatus {
    #[default]
    Active,
    Beta,
    Deprecated,
    Legacy,
    Unknown,
}

fn default_status() -> ServiceStatus {
    ServiceStatus::Active
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_timeout: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global_rate_limit: Option<GlobalRateLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRateLimit {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requests_per_second: Option<f64>,
}
