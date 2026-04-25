//! End-to-end pack loader test.
//!
//! Builds `reflow_pack_fixture` (a workspace-local cdylib pack), loads it
//! as both a raw dylib and a `.rflpack` bundle, and verifies:
//!   1. The pack's template id appears in the registry.
//!   2. A fresh actor instance can be obtained.
//!   3. A tampered manifest (wrong ABI) is rejected before dlopen.
//!   4. A manifest missing the current triple is rejected.
//!
//! The pack registry is process-global, so all scenarios are exercised
//! in one `#[test]` (scenario order encoded in comments) to avoid
//! inter-test interference.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use reflow_pack_loader::{
    PACK_REGISTRY, REFLOW_PACK_ABI_VERSION, REFLOW_PACK_HOST_TRIPLE,
    bundle::{DEFAULT_ENTRYPOINT, MANIFEST_VERSION, PackManifest, PackTarget},
    load_pack,
};

const FIXTURE_TEMPLATE: &str = "reflow.test.echo";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn dylib_name(base: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("lib{base}.dylib")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        format!("lib{base}.so")
    }
    #[cfg(windows)]
    {
        format!("{base}.dll")
    }
}

fn build_fixture() -> PathBuf {
    let root = workspace_root();
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "reflow_pack_fixture"])
        .current_dir(&root)
        .status()
        .expect("cargo build fixture");
    assert!(status.success(), "cargo build reflow_pack_fixture failed");

    let path = root
        .join("target")
        .join("debug")
        .join(dylib_name("reflow_pack_fixture"));
    assert!(path.exists(), "fixture dylib missing: {}", path.display());
    path
}

fn fresh_cache() -> PathBuf {
    let dir = std::env::temp_dir()
        .join("reflow-pack-test")
        .join(format!("{}", std::process::id()));
    if dir.exists() {
        let _ = fs::remove_dir_all(&dir);
    }
    fs::create_dir_all(&dir).unwrap();
    // SAFETY: edition-2024 requires an unsafe block for set_var. Tests
    // run before any threads that might read env vars.
    unsafe { std::env::set_var("REFLOW_PACK_CACHE", &dir) };
    dir
}

#[test]
fn pack_loader_roundtrip() {
    let cache = fresh_cache();
    let dylib = build_fixture();

    // ── scenario 1: reject bundle with wrong ABI *before* any successful
    // load, so the registry stays clean.
    let bad_abi_bundle = make_bundle_with_triple(
        &cache,
        "reflow.test.bad-abi",
        REFLOW_PACK_ABI_VERSION.wrapping_add(1),
        &dylib,
        REFLOW_PACK_HOST_TRIPLE,
    );
    let err = load_pack(&bad_abi_bundle).expect_err("wrong ABI must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("ABI") || msg.contains("abi"),
        "error should mention ABI, got: {msg}"
    );

    // ── scenario 2: reject bundle whose manifest has no entry for the
    // current triple.
    let wrong_triple_bundle = make_bundle_with_triple(
        &cache,
        "reflow.test.wrong-triple",
        REFLOW_PACK_ABI_VERSION,
        &dylib,
        "zzz-unknown-triple",
    );
    let err = load_pack(&wrong_triple_bundle).expect_err("wrong triple must be rejected");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("triple") || msg.contains("build for"),
        "error should mention triple, got: {msg}"
    );

    // ── scenario 3: raw-dylib load succeeds and publishes template.
    let templates = load_pack(&dylib).expect("load raw dylib");
    assert!(
        templates.iter().any(|t| t == FIXTURE_TEMPLATE),
        "fixture did not register expected template; got {templates:?}"
    );

    let actor: Arc<dyn reflow_actor::Actor> = PACK_REGISTRY
        .instantiate(FIXTURE_TEMPLATE)
        .expect("instantiate fixture actor");
    assert_eq!(actor.inport_names(), vec!["input".to_string()]);
    assert_eq!(actor.outport_names(), vec!["output".to_string()]);

    // ── scenario 4: reloading the same raw dylib is a no-op (idempotent).
    let again = load_pack(&dylib).expect("reload raw dylib");
    assert_eq!(again, templates);
}

// ─── helpers ───────────────────────────────────────────────────────────────

fn make_bundle_with_triple(
    cache: &Path,
    name: &str,
    abi: u32,
    dylib: &Path,
    triple: &str,
) -> PathBuf {
    let archive_name = format!(
        "lib/{triple}/{}",
        dylib.file_name().unwrap().to_string_lossy()
    );
    let manifest = PackManifest {
        manifest_version: MANIFEST_VERSION,
        name: name.to_string(),
        version: "0.0.1".into(),
        authors: vec!["test".into()],
        description: Some("fixture".into()),
        license: Some("MIT".into()),
        reflow_pack_abi_version: abi,
        entrypoint: DEFAULT_ENTRYPOINT.to_string(),
        targets: [(
            triple.to_string(),
            PackTarget {
                file: archive_name.clone(),
            },
        )]
        .into_iter()
        .collect(),
        templates: vec![FIXTURE_TEMPLATE.to_string()],
    };

    let bundle_path = cache.join(format!("{name}.rflpack"));
    let out = File::create(&bundle_path).unwrap();
    let mut zip = zip::ZipWriter::new(out);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("manifest.json", opts).unwrap();
    zip.write_all(&serde_json::to_vec_pretty(&manifest).unwrap())
        .unwrap();
    zip.start_file(&archive_name, opts).unwrap();
    let mut f = File::open(dylib).unwrap();
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).unwrap();
    zip.write_all(&buf).unwrap();
    zip.finish().unwrap();
    bundle_path
}
