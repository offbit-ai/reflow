//! API discovery — find OpenAPI specs from well-known sources.
//!
//! Crawls public API directories to discover new services and their specs.

use anyhow::Result;
use tracing::info;

/// A discovered API service with its spec URL.
#[derive(Debug, Clone)]
pub struct DiscoveredApi {
    pub name: String,
    pub description: Option<String>,
    pub spec_url: String,
    pub spec_format: String,
    pub source: String,
    pub category: Option<String>,
}

/// Discover APIs from the APIs.guru directory.
/// APIs.guru maintains a comprehensive list of publicly available APIs with their OpenAPI specs.
pub async fn discover_from_apis_guru() -> Result<Vec<DiscoveredApi>> {
    info!("Fetching API directory from APIs.guru...");

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("api-schema-gen/0.1.0")
        .build()?;

    let response = client
        .get("https://api.apis.guru/v2/list.json")
        .send()
        .await?;

    if !response.status().is_success() {
        anyhow::bail!("APIs.guru returned {}", response.status());
    }

    let data: serde_json::Value = response.json().await?;
    let apis = data
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Expected object"))?;

    let mut discovered = Vec::new();

    for (api_id, api_data) in apis {
        let versions = api_data.get("versions").and_then(|v| v.as_object());

        let Some(versions) = versions else { continue };

        // Get the preferred/latest version
        let preferred = api_data
            .get("preferred")
            .and_then(|v| v.as_str())
            .and_then(|pref| versions.get(pref))
            .or_else(|| versions.values().next_back());

        let Some(version_data) = preferred else {
            continue;
        };

        let spec_url = version_data
            .get("swaggerUrl")
            .and_then(|v| v.as_str())
            .map(String::from);

        let Some(url) = spec_url else { continue };

        let name = version_data
            .get("info")
            .and_then(|i| i.get("title"))
            .and_then(|t| t.as_str())
            .unwrap_or(api_id)
            .to_string();

        let description = version_data
            .get("info")
            .and_then(|i| i.get("description"))
            .and_then(|d| d.as_str())
            .map(String::from);

        let category = version_data
            .get("info")
            .and_then(|i| i.get("x-apisguru-categories"))
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .map(String::from);

        discovered.push(DiscoveredApi {
            name,
            description,
            spec_url: url,
            spec_format: "openapi".to_string(),
            source: "apis_guru".to_string(),
            category,
        });
    }

    info!("Discovered {} APIs from APIs.guru", discovered.len());
    Ok(discovered)
}

/// Generate a service ID from an API name.
pub fn name_to_service_id(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}
