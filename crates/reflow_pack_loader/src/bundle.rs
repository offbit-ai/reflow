//! `.rflpack` bundle format: zip archive containing `manifest.json` and
//! per-triple cdylibs under `lib/<triple>/`.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Current manifest schema version the loader understands.
pub const MANIFEST_VERSION: u32 = 1;

/// Default symbol name for the registration entrypoint.
pub const DEFAULT_ENTRYPOINT: &str = "reflow_pack_register";

/// Symbol name for the ABI handshake.
pub const ABI_VERSION_SYMBOL: &str = "reflow_pack_abi_version";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackManifest {
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    pub reflow_pack_abi_version: u32,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    pub targets: std::collections::BTreeMap<String, PackTarget>,
    #[serde(default)]
    pub templates: Vec<String>,
}

fn default_entrypoint() -> String {
    DEFAULT_ENTRYPOINT.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackTarget {
    pub file: String,
}

/// Where a loaded bundle's extracted dylib lives after unpacking.
pub struct ExtractedBundle {
    pub manifest: PackManifest,
    pub dylib_path: PathBuf,
    pub cache_dir: PathBuf,
}

/// Determine whether the file at `path` is a `.rflpack` zip archive.
/// Sniffs the first four bytes rather than trusting the extension.
pub fn is_bundle_file(path: &Path) -> Result<bool> {
    let mut f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut magic = [0u8; 4];
    let n = f.read(&mut magic)?;
    Ok(n == 4 && &magic == b"PK\x03\x04")
}

/// Extract a `.rflpack` into a per-bundle cache directory, validate the
/// manifest, and return the dylib path for the current triple.
pub fn extract_bundle(path: &Path, host_abi: u32, host_triple: &str) -> Result<ExtractedBundle> {
    let bundle_bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bundle_bytes);
    let digest = hex::encode(hasher.finalize());
    let short = &digest[..16];

    let cache_root = cache_root()?;
    let cache_dir = cache_root.join(short);
    let manifest_path = cache_dir.join("manifest.json");

    // Fast path: already extracted and manifest readable.
    if manifest_path.exists() {
        if let Ok(s) = fs::read_to_string(&manifest_path) {
            if let Ok(m) = serde_json::from_str::<PackManifest>(&s) {
                if m.reflow_pack_abi_version == host_abi && m.manifest_version == MANIFEST_VERSION {
                    if let Some(target) = m.targets.get(host_triple) {
                        let dylib = cache_dir.join(&target.file);
                        if dylib.exists() {
                            return Ok(ExtractedBundle {
                                manifest: m,
                                dylib_path: dylib,
                                cache_dir,
                            });
                        }
                    }
                }
            }
        }
    }

    fs::create_dir_all(&cache_dir).with_context(|| format!("mkdir {}", cache_dir.display()))?;

    let reader = std::io::Cursor::new(&bundle_bytes);
    let mut archive = zip::ZipArchive::new(reader).context("parse .rflpack zip")?;

    // Locate and parse manifest first so we can reject on metadata before
    // paying the cost of extracting dylibs.
    let manifest: PackManifest = {
        let mut m = archive
            .by_name("manifest.json")
            .context("manifest.json missing from .rflpack")?;
        let mut buf = String::new();
        m.read_to_string(&mut buf)?;
        serde_json::from_str(&buf).context("parse manifest.json")?
    };

    if manifest.manifest_version != MANIFEST_VERSION {
        bail!(
            "manifest_version {} not supported (host expects {})",
            manifest.manifest_version,
            MANIFEST_VERSION
        );
    }
    if manifest.reflow_pack_abi_version != host_abi {
        bail!(
            "pack '{}' ABI {} != host ABI {} — rebuild pack against current toolchain",
            manifest.name,
            manifest.reflow_pack_abi_version,
            host_abi
        );
    }
    let target = manifest.targets.get(host_triple).cloned().ok_or_else(|| {
        let have: Vec<&str> = manifest.targets.keys().map(String::as_str).collect();
        anyhow!(
            "pack '{}' has no build for triple {} (bundle includes: {})",
            manifest.name,
            host_triple,
            have.join(", ")
        )
    })?;

    // Extract just the bits we need: manifest + the one dylib.
    fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    {
        let mut entry = archive
            .by_name(&target.file)
            .with_context(|| format!("entry {} missing from .rflpack", target.file))?;
        let out_path = cache_dir.join(&target.file);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out =
            File::create(&out_path).with_context(|| format!("create {}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out)?;

        // Mark executable on unix so dlopen works.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&out_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&out_path, perms)?;
        }
    }

    let dylib_path = cache_dir.join(&target.file);
    Ok(ExtractedBundle {
        manifest,
        dylib_path,
        cache_dir,
    })
}

/// In-memory result of extracting a `.rflpack`'s wasm32 entry.
///
/// Used by the browser-side pack loader, which fetches a `.rflpack`
/// over HTTP and instantiates the embedded wasm via
/// `WebAssembly.instantiate` — there is no filesystem on the path.
pub struct ExtractedWasm {
    pub manifest: PackManifest,
    /// Raw wasm module bytes from `manifest.targets["wasm32-unknown-unknown"].file`.
    pub wasm: Vec<u8>,
}

/// Extract the wasm32 binary from a `.rflpack` byte buffer.
///
/// Performs the same manifest validation as [`extract_bundle`] but
/// keeps everything in memory and only returns the `wasm32-unknown-unknown`
/// target entry. Returns an error if the pack doesn't carry a wasm
/// build, the manifest version doesn't match, or the ABI version
/// disagrees with `host_abi`.
pub fn extract_wasm_from_bytes(bundle_bytes: &[u8], host_abi: u32) -> Result<ExtractedWasm> {
    let reader = std::io::Cursor::new(bundle_bytes);
    let mut archive = zip::ZipArchive::new(reader).context("parse .rflpack zip")?;

    let manifest: PackManifest = {
        let mut m = archive
            .by_name("manifest.json")
            .context("manifest.json missing from .rflpack")?;
        let mut buf = String::new();
        m.read_to_string(&mut buf)?;
        serde_json::from_str(&buf).context("parse manifest.json")?
    };

    if manifest.manifest_version != MANIFEST_VERSION {
        bail!(
            "manifest_version {} not supported (host expects {})",
            manifest.manifest_version,
            MANIFEST_VERSION
        );
    }
    if manifest.reflow_pack_abi_version != host_abi {
        bail!(
            "pack '{}' ABI {} != host ABI {} — rebuild pack against current toolchain",
            manifest.name,
            manifest.reflow_pack_abi_version,
            host_abi
        );
    }

    const WASM_TRIPLE: &str = "wasm32-unknown-unknown";
    let target = manifest.targets.get(WASM_TRIPLE).cloned().ok_or_else(|| {
        let have: Vec<&str> = manifest.targets.keys().map(String::as_str).collect();
        anyhow!(
            "pack '{}' has no wasm32 build (bundle includes: {})",
            manifest.name,
            have.join(", ")
        )
    })?;

    let mut entry = archive
        .by_name(&target.file)
        .with_context(|| format!("entry {} missing from .rflpack", target.file))?;
    let mut wasm = Vec::with_capacity(entry.size() as usize);
    std::io::copy(&mut entry, &mut wasm)
        .with_context(|| format!("read {} from .rflpack", target.file))?;

    Ok(ExtractedWasm { manifest, wasm })
}

/// Read just the manifest without extracting dylibs. For `rfl_pack_inspect`.
pub fn read_manifest(path: &Path) -> Result<PackManifest> {
    let f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut reader = std::io::BufReader::new(f);
    reader.seek(SeekFrom::Start(0))?;
    let mut archive = zip::ZipArchive::new(reader).context("parse .rflpack zip")?;
    let mut m = archive
        .by_name("manifest.json")
        .context("manifest.json missing from .rflpack")?;
    let mut buf = String::new();
    m.read_to_string(&mut buf)?;
    serde_json::from_str(&buf).context("parse manifest.json")
}

fn cache_root() -> Result<PathBuf> {
    let base = std::env::var_os("REFLOW_PACK_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("reflow-packs"));
    fs::create_dir_all(&base).with_context(|| format!("mkdir {}", base.display()))?;
    Ok(base)
}
