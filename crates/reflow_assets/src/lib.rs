//! File-based asset database for Reflow workflows.
//!
//! Inspired by CastleDB — a structured, queryable asset store that persists
//! across actor invocations and workflow runs. Binary data is content-addressed
//! for automatic deduplication.
//!
//! ## Storage backends
//!
//! - **FileBackend** (native) — directory with manifest.json + blobs/
//! - **MemoryBackend** (wasm/testing) — in-memory HashMap
//! - **S3Backend** (remote workflows) — to be implemented
//!
//! ## Directory structure (FileBackend)
//!
//! ```text
//! assets.db/
//!   manifest.json       ← asset index
//!   blobs/
//!     ab/abcdef1234...  ← binary data keyed by content hash
//! ```

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

pub type AssetId = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AssetType {
    Mesh,
    Texture,
    Skeleton,
    Animation,
    SkinWeights,
    Material,
    Scene,
    Audio,
    Video,
    Generic,
}

impl AssetType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "mesh" => Self::Mesh,
            "texture" | "image" => Self::Texture,
            "skeleton" => Self::Skeleton,
            "animation" | "clip" => Self::Animation,
            "skinweights" | "skin_weights" | "skin" => Self::SkinWeights,
            "material" => Self::Material,
            "scene" => Self::Scene,
            "audio" => Self::Audio,
            "video" => Self::Video,
            _ => Self::Generic,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mesh => "mesh",
            Self::Texture => "texture",
            Self::Skeleton => "skeleton",
            Self::Animation => "animation",
            Self::SkinWeights => "skinweights",
            Self::Material => "material",
            Self::Scene => "scene",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetEntry {
    pub id: AssetId,
    pub name: String,
    #[serde(rename = "assetType")]
    pub asset_type: AssetType,
    #[serde(rename = "blobHash")]
    pub blob_hash: String,
    #[serde(rename = "blobSize")]
    pub blob_size: u64,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<Value>,
}

pub struct Asset {
    pub entry: AssetEntry,
    pub data: Vec<u8>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Storage backend trait
// ═══════════════════════════════════════════════════════════════════════════

/// Pluggable storage backend for AssetDB.
/// Implementations: FileBackend (native), MemoryBackend (wasm), S3Backend (remote).
pub trait StorageBackend: Send + Sync {
    fn read_manifest(&self) -> Result<Vec<AssetEntry>>;
    fn write_manifest(&self, entries: &[AssetEntry]) -> Result<()>;
    fn read_blob(&self, hash: &str) -> Result<Vec<u8>>;
    fn write_blob(&self, hash: &str, data: &[u8]) -> Result<()>;
    fn blob_exists(&self, hash: &str) -> bool;
    fn delete_blob(&self, hash: &str) -> Result<()>;
    fn backend_name(&self) -> &'static str;
}

// ═══════════════════════════════════════════════════════════════════════════
// FileBackend (native)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(not(target_arch = "wasm32"))]
pub struct FileBackend {
    root: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl FileBackend {
    pub fn new(root: impl AsRef<std::path::Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        std::fs::create_dir_all(root.join("blobs"))?;

        // Ensure manifest exists
        let manifest_path = root.join("manifest.json");
        if !manifest_path.exists() {
            std::fs::write(&manifest_path, "[]")?;
        }
        Ok(Self { root })
    }

    fn blob_path(&self, hash: &str) -> std::path::PathBuf {
        let prefix = &hash[..2.min(hash.len())];
        self.root.join("blobs").join(prefix).join(hash)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl StorageBackend for FileBackend {
    fn read_manifest(&self) -> Result<Vec<AssetEntry>> {
        let path = self.root.join("manifest.json");
        let data = std::fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&data).unwrap_or_default())
    }

    fn write_manifest(&self, entries: &[AssetEntry]) -> Result<()> {
        let data = serde_json::to_string_pretty(entries)?;
        let tmp = self.root.join(".manifest.tmp");
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, self.root.join("manifest.json"))?;
        Ok(())
    }

    fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        Ok(std::fs::read(self.blob_path(hash))?)
    }

    fn write_blob(&self, hash: &str, data: &[u8]) -> Result<()> {
        let path = self.blob_path(hash);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, data)?;
        Ok(())
    }

    fn blob_exists(&self, hash: &str) -> bool {
        self.blob_path(hash).exists()
    }

    fn delete_blob(&self, hash: &str) -> Result<()> {
        let _ = std::fs::remove_file(self.blob_path(hash));
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "file"
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MemoryBackend (wasm + testing)
// ═══════════════════════════════════════════════════════════════════════════

pub struct MemoryBackend {
    manifest: RwLock<Vec<AssetEntry>>,
    blobs: RwLock<HashMap<String, Vec<u8>>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            manifest: RwLock::new(Vec::new()),
            blobs: RwLock::new(HashMap::new()),
        }
    }
}

impl StorageBackend for MemoryBackend {
    fn read_manifest(&self) -> Result<Vec<AssetEntry>> {
        Ok(self.manifest.read().map_err(|e| anyhow::anyhow!("{}", e))?.clone())
    }

    fn write_manifest(&self, entries: &[AssetEntry]) -> Result<()> {
        let mut m = self.manifest.write().map_err(|e| anyhow::anyhow!("{}", e))?;
        *m = entries.to_vec();
        Ok(())
    }

    fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        self.blobs
            .read()
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .get(hash)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Blob not found: {}", hash))
    }

    fn write_blob(&self, hash: &str, data: &[u8]) -> Result<()> {
        self.blobs
            .write()
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .insert(hash.to_string(), data.to_vec());
        Ok(())
    }

    fn blob_exists(&self, hash: &str) -> bool {
        self.blobs
            .read()
            .map(|b| b.contains_key(hash))
            .unwrap_or(false)
    }

    fn delete_blob(&self, hash: &str) -> Result<()> {
        self.blobs
            .write()
            .map_err(|e| anyhow::anyhow!("{}", e))?
            .remove(hash);
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "memory"
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// IndexedDbBackend (wasm — persistent browser storage)
// ═══════════════════════════════════════════════════════════════════════════

/// IndexedDB-backed storage for wasm. Uses in-memory cache for sync trait
/// methods and flushes writes to IndexedDB asynchronously.
///
/// Two object stores: "manifest" (single entry, key "entries") and "blobs" (keyed by hash).
///
/// Call `IndexedDbBackend::open(name).await` to create + hydrate from IDB.
#[cfg(target_arch = "wasm32")]
pub struct IndexedDbBackend {
    memory: MemoryBackend,
    db_name: String,
}

#[cfg(target_arch = "wasm32")]
impl IndexedDbBackend {
    /// Open (or create) an IndexedDB database and hydrate the in-memory cache.
    pub async fn open(name: &str) -> Result<Self> {
        use js_sys::{Array, Uint8Array};
        use wasm_bindgen::prelude::*;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::{IdbTransactionMode, IdbObjectStoreParameters};

        let window = web_sys::window().ok_or_else(|| anyhow::anyhow!("No window"))?;
        let idb_factory = window
            .indexed_db()
            .map_err(|_| anyhow::anyhow!("IndexedDB not available"))?
            .ok_or_else(|| anyhow::anyhow!("IndexedDB not available"))?;

        // Open DB (version 1)
        let open_req = idb_factory
            .open_with_u32(name, 1)
            .map_err(|_| anyhow::anyhow!("Failed to open IndexedDB"))?;

        // Handle upgrade (create object stores)
        let on_upgrade = Closure::once(move |event: web_sys::IdbVersionChangeEvent| {
            let db: web_sys::IdbDatabase = event
                .target()
                .unwrap()
                .dyn_into::<web_sys::IdbOpenDbRequest>()
                .unwrap()
                .result()
                .unwrap()
                .dyn_into()
                .unwrap();

            if !db.object_store_names().contains(&"manifest".into()) {
                db.create_object_store("manifest").unwrap();
            }
            if !db.object_store_names().contains(&"blobs".into()) {
                db.create_object_store("blobs").unwrap();
            }
        });
        open_req.set_onupgradeneeded(Some(on_upgrade.as_ref().unchecked_ref()));
        on_upgrade.forget();

        let db: web_sys::IdbDatabase = JsFuture::from(open_req)
            .await
            .map_err(|_| anyhow::anyhow!("IndexedDB open failed"))?
            .dyn_into()
            .map_err(|_| anyhow::anyhow!("IndexedDB cast failed"))?;

        // Hydrate manifest
        let tx = db
            .transaction_with_str_and_mode("manifest", IdbTransactionMode::Readonly)
            .map_err(|_| anyhow::anyhow!("Failed to create transaction"))?;
        let store = tx.object_store("manifest").map_err(|_| anyhow::anyhow!("No manifest store"))?;
        let get_req = store.get(&"entries".into()).map_err(|_| anyhow::anyhow!("get failed"))?;
        let result = JsFuture::from(get_req).await;

        let memory = MemoryBackend::new();

        if let Ok(val) = result {
            if !val.is_undefined() && !val.is_null() {
                if let Some(s) = val.as_string() {
                    if let Ok(entries) = serde_json::from_str::<Vec<AssetEntry>>(&s) {
                        let mut m = memory.manifest.write().map_err(|e| anyhow::anyhow!("{}", e))?;
                        *m = entries;
                    }
                }
            }
        }

        // Hydrate blobs — read all keys and values
        let tx = db
            .transaction_with_str_and_mode("blobs", IdbTransactionMode::Readonly)
            .map_err(|_| anyhow::anyhow!("Failed to create blob transaction"))?;
        let store = tx.object_store("blobs").map_err(|_| anyhow::anyhow!("No blobs store"))?;
        let keys_req = store.get_all_keys().map_err(|_| anyhow::anyhow!("getAllKeys failed"))?;
        let keys_result = JsFuture::from(keys_req).await;

        if let Ok(keys_val) = keys_result {
            let keys: Array = keys_val.dyn_into().unwrap_or_else(|_| Array::new());
            for i in 0..keys.length() {
                let key = keys.get(i);
                if let Some(hash) = key.as_string() {
                    let get_req = store.get(&key).ok();
                    if let Some(req) = get_req {
                        if let Ok(blob_val) = JsFuture::from(req).await {
                            if let Ok(arr) = blob_val.dyn_into::<Uint8Array>() {
                                let data = arr.to_vec();
                                let _ = memory.write_blob(&hash, &data);
                            }
                        }
                    }
                }
            }
        }

        db.close();

        Ok(Self {
            memory,
            db_name: name.to_string(),
        })
    }

    /// Flush a blob write to IndexedDB (fire-and-forget).
    fn flush_blob(&self, hash: &str, data: &[u8]) {
        let db_name = self.db_name.clone();
        let hash = hash.to_string();
        let data = data.to_vec();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = Self::idb_put_blob(&db_name, &hash, &data).await;
        });
    }

    /// Flush manifest to IndexedDB (fire-and-forget).
    fn flush_manifest(&self, entries: &[AssetEntry]) {
        let db_name = self.db_name.clone();
        let json = serde_json::to_string(entries).unwrap_or_default();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = Self::idb_put_manifest(&db_name, &json).await;
        });
    }

    async fn idb_put_blob(db_name: &str, hash: &str, data: &[u8]) -> Result<()> {
        use js_sys::Uint8Array;
        use wasm_bindgen::prelude::*;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::IdbTransactionMode;

        let window = web_sys::window().ok_or_else(|| anyhow::anyhow!("No window"))?;
        let factory = window.indexed_db().ok().flatten().ok_or_else(|| anyhow::anyhow!("No IDB"))?;
        let open_req = factory.open(db_name).map_err(|_| anyhow::anyhow!("open failed"))?;
        let db: web_sys::IdbDatabase = JsFuture::from(open_req)
            .await
            .map_err(|_| anyhow::anyhow!("open await failed"))?
            .dyn_into()
            .map_err(|_| anyhow::anyhow!("cast failed"))?;

        let tx = db.transaction_with_str_and_mode("blobs", IdbTransactionMode::Readwrite)
            .map_err(|_| anyhow::anyhow!("tx failed"))?;
        let store = tx.object_store("blobs").map_err(|_| anyhow::anyhow!("store failed"))?;
        let arr = Uint8Array::from(data);
        store.put_with_key(&arr, &hash.into()).map_err(|_| anyhow::anyhow!("put failed"))?;

        db.close();
        Ok(())
    }

    async fn idb_put_manifest(db_name: &str, json: &str) -> Result<()> {
        use wasm_bindgen::prelude::*;
        use wasm_bindgen_futures::JsFuture;
        use web_sys::IdbTransactionMode;

        let window = web_sys::window().ok_or_else(|| anyhow::anyhow!("No window"))?;
        let factory = window.indexed_db().ok().flatten().ok_or_else(|| anyhow::anyhow!("No IDB"))?;
        let open_req = factory.open(db_name).map_err(|_| anyhow::anyhow!("open failed"))?;
        let db: web_sys::IdbDatabase = JsFuture::from(open_req)
            .await
            .map_err(|_| anyhow::anyhow!("open await failed"))?
            .dyn_into()
            .map_err(|_| anyhow::anyhow!("cast failed"))?;

        let tx = db.transaction_with_str_and_mode("manifest", IdbTransactionMode::Readwrite)
            .map_err(|_| anyhow::anyhow!("tx failed"))?;
        let store = tx.object_store("manifest").map_err(|_| anyhow::anyhow!("store failed"))?;
        store.put_with_key(&json.into(), &"entries".into()).map_err(|_| anyhow::anyhow!("put failed"))?;

        db.close();
        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl StorageBackend for IndexedDbBackend {
    fn read_manifest(&self) -> Result<Vec<AssetEntry>> {
        self.memory.read_manifest()
    }

    fn write_manifest(&self, entries: &[AssetEntry]) -> Result<()> {
        self.memory.write_manifest(entries)?;
        self.flush_manifest(entries);
        Ok(())
    }

    fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        self.memory.read_blob(hash)
    }

    fn write_blob(&self, hash: &str, data: &[u8]) -> Result<()> {
        self.memory.write_blob(hash, data)?;
        self.flush_blob(hash, data);
        Ok(())
    }

    fn blob_exists(&self, hash: &str) -> bool {
        self.memory.blob_exists(hash)
    }

    fn delete_blob(&self, hash: &str) -> Result<()> {
        self.memory.delete_blob(hash)?;
        // IDB delete is fire-and-forget
        let db_name = self.db_name.clone();
        let hash = hash.to_string();
        wasm_bindgen_futures::spawn_local(async move {
            let _ = Self::idb_put_blob(&db_name, &hash, &[]).await; // overwrite with empty
        });
        Ok(())
    }

    fn backend_name(&self) -> &'static str {
        "indexeddb"
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// AssetDB
// ═══════════════════════════════════════════════════════════════════════════

pub struct AssetDB {
    backend: Box<dyn StorageBackend>,
    cache: RwLock<Vec<AssetEntry>>,
}

impl AssetDB {
    /// Create a new AssetDB with the given storage backend.
    pub fn with_backend(backend: Box<dyn StorageBackend>) -> Result<Arc<Self>> {
        let entries = backend.read_manifest()?;
        Ok(Arc::new(Self {
            backend,
            cache: RwLock::new(entries),
        }))
    }

    /// Open a file-backed AssetDB at the given path (native only).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Arc<Self>> {
        let backend = FileBackend::new(path)?;
        Self::with_backend(Box::new(backend))
    }

    /// Create an in-memory AssetDB (wasm or testing).
    pub fn in_memory() -> Result<Arc<Self>> {
        Self::with_backend(Box::new(MemoryBackend::new()))
    }

    /// Store binary data. Returns the asset ID.
    pub fn store(
        &self,
        asset_type: &str,
        name: &str,
        data: &[u8],
        metadata: Value,
        tags: &[&str],
    ) -> Result<AssetId> {
        let hash = content_hash(data);

        if !self.backend.blob_exists(&hash) {
            self.backend.write_blob(&hash, data)?;
        }

        let now = now_iso();
        let id = gen_id();

        let entry = AssetEntry {
            id: id.clone(),
            name: name.to_string(),
            asset_type: AssetType::from_str(asset_type),
            blob_hash: hash,
            blob_size: data.len() as u64,
            metadata,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            created_at: now.clone(),
            updated_at: now,
            inline_data: None,
        };

        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{}", e))?;
        cache.push(entry);
        self.backend.write_manifest(&cache)?;
        Ok(id)
    }

    /// Store JSON inline (materials, skeletons, small configs).
    pub fn store_json(
        &self,
        asset_type: &str,
        name: &str,
        json_data: Value,
        metadata: Value,
        tags: &[&str],
    ) -> Result<AssetId> {
        let serialized = serde_json::to_vec(&json_data)?;
        let hash = content_hash(&serialized);
        let now = now_iso();
        let id = gen_id();

        let entry = AssetEntry {
            id: id.clone(),
            name: name.to_string(),
            asset_type: AssetType::from_str(asset_type),
            blob_hash: hash,
            blob_size: serialized.len() as u64,
            metadata,
            tags: tags.iter().map(|s| s.to_string()).collect(),
            created_at: now.clone(),
            updated_at: now,
            inline_data: Some(json_data),
        };

        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{}", e))?;
        cache.push(entry);
        self.backend.write_manifest(&cache)?;
        Ok(id)
    }

    /// Load by ID.
    pub fn load(&self, id: &str) -> Result<Asset> {
        let cache = self.cache.read().map_err(|e| anyhow::anyhow!("{}", e))?;
        let entry = cache
            .iter()
            .find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", id))?
            .clone();
        drop(cache);
        self.load_entry(&entry)
    }

    /// Load by name (first match).
    pub fn load_by_name(&self, name: &str) -> Result<Asset> {
        let cache = self.cache.read().map_err(|e| anyhow::anyhow!("{}", e))?;
        let entry = cache
            .iter()
            .find(|a| a.name == name)
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", name))?
            .clone();
        drop(cache);
        self.load_entry(&entry)
    }

    /// Load by name + type.
    pub fn load_by_name_and_type(&self, name: &str, asset_type: &str) -> Result<Asset> {
        let at = AssetType::from_str(asset_type);
        let cache = self.cache.read().map_err(|e| anyhow::anyhow!("{}", e))?;
        let entry = cache
            .iter()
            .find(|a| a.name == name && a.asset_type == at)
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {} ({})", name, asset_type))?
            .clone();
        drop(cache);
        self.load_entry(&entry)
    }

    /// Query metadata (no blob data).
    pub fn query(&self, q: &AssetQuery) -> Result<Vec<AssetEntry>> {
        let cache = self.cache.read().map_err(|e| anyhow::anyhow!("{}", e))?;

        let mut results: Vec<AssetEntry> = cache
            .iter()
            .filter(|a| {
                if let Some(ref at) = q.asset_type {
                    if &a.asset_type != at {
                        return false;
                    }
                }
                if let Some(ref name) = q.name_contains {
                    if !a.name.to_lowercase().contains(name) {
                        return false;
                    }
                }
                if !q.tags.is_empty() {
                    match q.tag_match {
                        TagMatch::Any => {
                            if !q.tags.iter().any(|qt| a.tags.contains(qt)) {
                                return false;
                            }
                        }
                        TagMatch::All => {
                            if !q.tags.iter().all(|qt| a.tags.contains(qt)) {
                                return false;
                            }
                        }
                    }
                }
                true
            })
            .cloned()
            .collect();

        if let Some(limit) = q.limit {
            results.truncate(limit);
        }
        Ok(results)
    }

    /// Delete by ID.
    pub fn delete(&self, id: &str) -> Result<()> {
        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{}", e))?;
        let idx = cache
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", id))?;

        let entry = cache.remove(idx);
        let hash_still_used = cache.iter().any(|a| a.blob_hash == entry.blob_hash);
        if !hash_still_used && entry.inline_data.is_none() {
            let _ = self.backend.delete_blob(&entry.blob_hash);
        }
        self.backend.write_manifest(&cache)?;
        Ok(())
    }

    /// Update metadata/tags.
    pub fn update_metadata(
        &self,
        id: &str,
        metadata: Option<Value>,
        tags: Option<Vec<String>>,
    ) -> Result<()> {
        let mut cache = self.cache.write().map_err(|e| anyhow::anyhow!("{}", e))?;
        let entry = cache
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("Asset not found: {}", id))?;

        if let Some(meta) = metadata {
            entry.metadata = meta;
        }
        if let Some(t) = tags {
            entry.tags = t;
        }
        entry.updated_at = now_iso();
        self.backend.write_manifest(&cache)?;
        Ok(())
    }

    /// Stats summary.
    pub fn stats(&self) -> Result<Value> {
        let cache = self.cache.read().map_err(|e| anyhow::anyhow!("{}", e))?;
        let mut type_counts: HashMap<String, usize> = HashMap::new();
        let mut total_bytes: u64 = 0;

        for a in cache.iter() {
            *type_counts.entry(a.asset_type.as_str().to_string()).or_default() += 1;
            total_bytes += a.blob_size;
        }

        Ok(json!({
            "assetCount": cache.len(),
            "totalBytes": total_bytes,
            "types": type_counts,
            "backend": self.backend.backend_name(),
        }))
    }

    fn load_entry(&self, entry: &AssetEntry) -> Result<Asset> {
        let data = if let Some(ref inline) = entry.inline_data {
            serde_json::to_vec(inline)?
        } else {
            self.backend.read_blob(&entry.blob_hash)?
        };
        Ok(Asset {
            entry: entry.clone(),
            data,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Query builder
// ═══════════════════════════════════════════════════════════════════════════

#[derive(Default)]
pub struct AssetQuery {
    pub asset_type: Option<AssetType>,
    pub name_contains: Option<String>,
    pub tags: Vec<String>,
    pub tag_match: TagMatch,
    pub limit: Option<usize>,
}

#[derive(Default, Clone)]
pub enum TagMatch {
    #[default]
    Any,
    All,
}

impl AssetQuery {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn asset_type(mut self, t: &str) -> Self {
        self.asset_type = Some(AssetType::from_str(t));
        self
    }
    pub fn name_contains(mut self, n: &str) -> Self {
        self.name_contains = Some(n.to_lowercase());
        self
    }
    pub fn tag(mut self, t: &str) -> Self {
        self.tags.push(t.to_string());
        self
    }
    pub fn tags(mut self, tags: &[&str]) -> Self {
        self.tags.extend(tags.iter().map(|s| s.to_string()));
        self
    }
    pub fn match_all_tags(mut self) -> Self {
        self.tag_match = TagMatch::All;
        self
    }
    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Global DB registry
// ═══════════════════════════════════════════════════════════════════════════

static DB_REGISTRY: std::sync::OnceLock<Mutex<HashMap<String, Arc<AssetDB>>>> =
    std::sync::OnceLock::new();

fn registry() -> &'static Mutex<HashMap<String, Arc<AssetDB>>> {
    DB_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Get or create a file-backed AssetDB (native).
#[cfg(not(target_arch = "wasm32"))]
pub fn get_or_create_db(path: &str) -> Result<Arc<AssetDB>> {
    let mut reg = registry().lock().map_err(|e| anyhow::anyhow!("{}", e))?;
    if let Some(db) = reg.get(path) {
        return Ok(Arc::clone(db));
    }
    let db = AssetDB::open(path)?;
    reg.insert(path.to_string(), Arc::clone(&db));
    Ok(db)
}

/// Get or create an AssetDB (wasm — returns in-memory if not yet hydrated).
/// For full IndexedDB persistence, use `get_or_create_db_async` instead.
#[cfg(target_arch = "wasm32")]
pub fn get_or_create_db(path: &str) -> Result<Arc<AssetDB>> {
    let mut reg = registry().lock().map_err(|e| anyhow::anyhow!("{}", e))?;
    if let Some(db) = reg.get(path) {
        return Ok(Arc::clone(db));
    }
    // Sync fallback: in-memory only. Use get_or_create_db_async for IndexedDB.
    let db = AssetDB::in_memory()?;
    reg.insert(path.to_string(), Arc::clone(&db));
    Ok(db)
}

/// Get or create an IndexedDB-backed AssetDB (wasm, async).
/// Hydrates from IndexedDB on first call, returns cached instance after.
#[cfg(target_arch = "wasm32")]
pub async fn get_or_create_db_async(name: &str) -> Result<Arc<AssetDB>> {
    // Check cache first (sync)
    {
        let reg = registry().lock().map_err(|e| anyhow::anyhow!("{}", e))?;
        if let Some(db) = reg.get(name) {
            return Ok(Arc::clone(db));
        }
    }
    // Hydrate from IndexedDB (async)
    let backend = IndexedDbBackend::open(name).await?;
    let db = AssetDB::with_backend(Box::new(backend))?;
    let mut reg = registry().lock().map_err(|e| anyhow::anyhow!("{}", e))?;
    reg.insert(name.to_string(), Arc::clone(&db));
    Ok(db)
}

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn content_hash(data: &[u8]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut h1 = DefaultHasher::new();
    data.hash(&mut h1);
    let a = h1.finish();

    let mut h2 = DefaultHasher::new();
    data.len().hash(&mut h2);
    data.hash(&mut h2);
    let b = h2.finish();

    format!("{:016x}{:016x}", a, b)
}

fn gen_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::SystemTime;
    static CTR: AtomicU64 = AtomicU64::new(0);

    let ts = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let c = CTR.fetch_add(1, Ordering::Relaxed);
    let a = ts ^ c.wrapping_mul(6364136223846793005);
    let b = ts.wrapping_mul(2654435761) ^ c;

    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (a >> 32) as u32,
        (a >> 16) as u16 & 0xFFFF,
        a as u16 & 0x0FFF,
        (b >> 48) as u16 & 0x3FFF | 0x8000,
        b & 0xFFFF_FFFF_FFFF,
    )
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
