//! Model manifest helpers layered on top of `reflow_assets`.

use anyhow::{bail, Result};
use reflow_assets::{get_or_create_db, AssetDB, AssetEntry};
use reflow_litert::{ModelInfo, TensorSpec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub model_id: String,
    pub task_kind: String,
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub input_specs: Vec<TensorSpec>,
    #[serde(default)]
    pub output_specs: Vec<TensorSpec>,
    pub license: String,
    pub source_url: String,
    pub checksum_sha256: String,
    #[serde(default)]
    pub attribution_required: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub metadata: HashMap<String, Value>,
}

impl ModelManifest {
    pub fn to_model_info(&self) -> ModelInfo {
        let mut metadata = self.metadata.clone();
        if let Some(asset_id) = &self.asset_id {
            metadata
                .entry("assetId".to_string())
                .or_insert_with(|| json!(asset_id));
        }
        metadata
            .entry("license".to_string())
            .or_insert_with(|| json!(self.license));
        metadata
            .entry("sourceUrl".to_string())
            .or_insert_with(|| json!(self.source_url));
        metadata
            .entry("checksumSha256".to_string())
            .or_insert_with(|| json!(self.checksum_sha256));
        metadata
            .entry("attributionRequired".to_string())
            .or_insert_with(|| json!(self.attribution_required));
        metadata
            .entry("tags".to_string())
            .or_insert_with(|| json!(self.tags));

        ModelInfo {
            id: self.model_id.clone(),
            backend: self.backend.clone(),
            task: self.task_kind.clone(),
            inputs: self.input_specs.clone(),
            outputs: self.output_specs.clone(),
            metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoadedModelAsset {
    pub asset_id: String,
    pub manifest: ModelManifest,
    pub data: Arc<Vec<u8>>,
}

pub fn allowed_licenses() -> HashSet<&'static str> {
    HashSet::from(["apache-2.0", "mit", "bsd-2-clause", "bsd-3-clause"])
}

pub fn allowed_backends() -> HashSet<&'static str> {
    HashSet::from(["mock", "litert", "onnx", "tract"])
}

pub fn validate_manifest(manifest: &ModelManifest) -> Result<()> {
    if manifest.model_id.trim().is_empty() {
        bail!("model manifest is missing model_id");
    }
    if manifest.task_kind.trim().is_empty() {
        bail!("model manifest is missing task_kind");
    }
    if !allowed_backends().contains(manifest.backend.to_ascii_lowercase().as_str()) {
        bail!("unsupported model backend '{}'", manifest.backend);
    }
    if !allowed_licenses().contains(manifest.license.to_ascii_lowercase().as_str()) {
        bail!("unsupported model license '{}'", manifest.license);
    }
    if manifest.source_url.trim().is_empty() {
        bail!("model manifest is missing source_url");
    }
    if !looks_like_sha256(&manifest.checksum_sha256) {
        bail!("model manifest checksum_sha256 must be a 64-character hex SHA-256");
    }
    if manifest.input_specs.is_empty() {
        bail!("model manifest must declare at least one input tensor spec");
    }
    if manifest.output_specs.is_empty() {
        bail!("model manifest must declare at least one output tensor spec");
    }
    Ok(())
}

pub fn manifest_to_metadata(manifest: &ModelManifest) -> Result<Value> {
    validate_manifest(manifest)?;
    Ok(json!({
        "kind": "reflow.modelManifest",
        "version": 1,
        "manifest": manifest,
    }))
}

pub fn manifest_from_metadata(metadata: &Value) -> Result<ModelManifest> {
    let manifest_value = metadata
        .get("manifest")
        .cloned()
        .unwrap_or_else(|| metadata.clone());
    let manifest: ModelManifest = serde_json::from_value(manifest_value)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

pub fn manifest_from_entry(entry: &AssetEntry) -> Result<ModelManifest> {
    manifest_from_metadata(&entry.metadata)
}

pub fn store_model_asset(
    db: &Arc<AssetDB>,
    asset_id: &str,
    model_bytes: &[u8],
    manifest: &ModelManifest,
) -> Result<()> {
    validate_manifest(manifest)?;
    let actual = sha256_hex(model_bytes);
    if actual != manifest.checksum_sha256.to_ascii_lowercase() {
        bail!(
            "model checksum mismatch for '{}': expected {}, got {}",
            asset_id,
            manifest.checksum_sha256,
            actual
        );
    }
    db.put(asset_id, model_bytes, manifest_to_metadata(manifest)?)?;
    let tag_refs = manifest.tags.iter().map(String::as_str).collect::<Vec<_>>();
    db.tag(asset_id, &tag_refs)?;
    Ok(())
}

pub fn store_model_asset_at_path(
    db_path: &str,
    asset_id: &str,
    model_bytes: &[u8],
    manifest: &ModelManifest,
) -> Result<()> {
    let db = get_or_create_db(db_path)?;
    store_model_asset(&db, asset_id, model_bytes, manifest)
}

pub fn load_model_manifest(db: &Arc<AssetDB>, asset_id: &str) -> Result<ModelManifest> {
    let entry = db.get_entry(asset_id)?;
    manifest_from_entry(&entry)
}

pub fn load_model_asset(db: &Arc<AssetDB>, asset_id: &str) -> Result<LoadedModelAsset> {
    let manifest_entry_asset = db.get(asset_id)?;
    let manifest = manifest_from_entry(&manifest_entry_asset.entry)?;
    let data_asset_id = manifest.asset_id.as_deref().unwrap_or(asset_id);
    let data = if data_asset_id == asset_id {
        manifest_entry_asset.data
    } else {
        db.get(data_asset_id)?.data
    };
    validate_model_bytes(&manifest, &data)?;

    Ok(LoadedModelAsset {
        asset_id: data_asset_id.to_string(),
        manifest,
        data: Arc::new(data),
    })
}

pub fn load_model_asset_from_path(db_path: &str, asset_id: &str) -> Result<LoadedModelAsset> {
    let db = get_or_create_db(db_path)?;
    load_model_asset(&db, asset_id)
}

pub fn validate_local_model_asset(db: &Arc<AssetDB>, manifest: &ModelManifest) -> Result<()> {
    validate_manifest(manifest)?;
    if let Some(asset_id) = &manifest.asset_id {
        let asset = db.get(asset_id)?;
        validate_model_bytes(manifest, &asset.data)?;
    }
    Ok(())
}

pub fn validate_model_bytes(manifest: &ModelManifest, model_bytes: &[u8]) -> Result<()> {
    validate_manifest(manifest)?;
    let actual = sha256_hex(model_bytes);
    if actual != manifest.checksum_sha256.to_ascii_lowercase() {
        bail!(
            "model checksum mismatch for '{}': expected {}, got {}",
            manifest.model_id,
            manifest.checksum_sha256,
            actual
        );
    }
    Ok(())
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn looks_like_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reflow_litert::TensorSpec;
    use reflow_media_types::{TensorDType, TensorShape};

    fn manifest(bytes: &[u8]) -> ModelManifest {
        ModelManifest {
            model_id: "demo".to_string(),
            task_kind: "classification".to_string(),
            backend: "mock".to_string(),
            asset_id: Some("demo:model".to_string()),
            input_specs: vec![TensorSpec {
                name: "input".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 4]),
            }],
            output_specs: vec![TensorSpec {
                name: "output".to_string(),
                dtype: TensorDType::F32,
                shape: TensorShape::new([1, 2]),
            }],
            license: "MIT".to_string(),
            source_url: "https://example.test/model".to_string(),
            checksum_sha256: sha256_hex(bytes),
            attribution_required: false,
            tags: vec!["ml".to_string()],
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn validates_supported_manifest() {
        validate_manifest(&manifest(b"abc")).unwrap();
    }

    #[test]
    fn rejects_unknown_license() {
        let mut manifest = manifest(b"abc");
        manifest.license = "unknown".to_string();
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn stores_and_loads_manifest_from_asset_db() {
        let db = AssetDB::in_memory().unwrap();
        let bytes = b"abc";
        let manifest = manifest(bytes);

        store_model_asset(&db, "demo:model", bytes, &manifest).unwrap();
        let loaded = load_model_manifest(&db, "demo:model").unwrap();

        assert_eq!(loaded.model_id, "demo");
        validate_local_model_asset(&db, &loaded).unwrap();
    }
}
