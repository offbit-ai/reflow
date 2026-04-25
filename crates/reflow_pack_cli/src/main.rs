//! `reflow-pack` — build, inspect, and unpack `.rflpack` bundles.
//!
//! This is a build-time utility. It does not dlopen packs; the runtime
//! loader does that. Its job is to take one or more pre-built cdylibs and
//! an author-supplied `Reflow.pack.toml`, then produce a zip bundle with a
//! matching `manifest.json`.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use reflow_pack_loader::bundle::{
    read_manifest, PackManifest, PackTarget, DEFAULT_ENTRYPOINT, MANIFEST_VERSION,
};
use serde::Deserialize;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

#[derive(Parser)]
#[command(name = "reflow-pack", version, about = "Build .rflpack bundles")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a `.rflpack` from a `Reflow.pack.toml` + pre-built cdylibs.
    Build {
        /// Path to the pack manifest TOML. Defaults to `./Reflow.pack.toml`.
        #[arg(long, default_value = "Reflow.pack.toml")]
        manifest: PathBuf,
        /// Directory to write the resulting `.rflpack` into.
        #[arg(long, default_value = "target")]
        out_dir: PathBuf,
    },
    /// Print the manifest of an existing `.rflpack`.
    Inspect {
        /// Path to the `.rflpack` file.
        path: PathBuf,
    },
    /// Extract a `.rflpack` to a directory (debugging).
    Unpack {
        /// Path to the `.rflpack` file.
        path: PathBuf,
        /// Destination directory.
        out_dir: PathBuf,
    },
    /// Print the pack ABI version of the host this binary was built for.
    /// Use this value as `REFLOW_PACK_ABI_VERSION` when cross-compiling a
    /// pack — build the CLI on the same toolchain as your runtime.
    Abi,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Command::Build { manifest, out_dir } => cmd_build(&manifest, &out_dir),
        Command::Inspect { path } => cmd_inspect(&path),
        Command::Unpack { path, out_dir } => cmd_unpack(&path, &out_dir),
        Command::Abi => {
            println!(
                "abi_version = {}",
                reflow_pack_loader::REFLOW_PACK_ABI_VERSION
            );
            println!(
                "host_triple = {}",
                reflow_pack_loader::REFLOW_PACK_HOST_TRIPLE
            );
            Ok(())
        }
    }
}

// ─── build ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PackToml {
    pack: PackSection,
    #[serde(default)]
    targets: TargetsSection,
    #[serde(default)]
    abi: Option<AbiSection>,
}

#[derive(Deserialize)]
struct PackSection {
    name: String,
    version: String,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    entrypoint: Option<String>,
    /// Template ids the pack advertises. Stamped into manifest.json so
    /// SDKs can enumerate a pack's contents without dlopen-ing it.
    #[serde(default)]
    templates: Vec<String>,
}

#[derive(Deserialize, Default)]
struct TargetsSection {
    /// Map of triple → path to the cdylib. Paths are relative to the
    /// manifest file.
    #[serde(default)]
    files: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct AbiSection {
    /// Explicit ABI version to stamp into the manifest. Usually set from
    /// CI as `REFLOW_PACK_ABI_VERSION` env var; this table only exists to
    /// allow overrides.
    version: u32,
}

fn cmd_build(manifest_path: &Path, out_dir: &Path) -> Result<()> {
    let toml_str = fs::read_to_string(manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let cfg: PackToml =
        toml::from_str(&toml_str).with_context(|| format!("parse {}", manifest_path.display()))?;

    let abi = cfg
        .abi
        .as_ref()
        .map(|a| a.version)
        .or_else(|| {
            std::env::var("REFLOW_PACK_ABI_VERSION")
                .ok()
                .and_then(|s| s.parse().ok())
        })
        .context(
            "no abi version: set [abi] version in Reflow.pack.toml or \
             REFLOW_PACK_ABI_VERSION env var. Use \
             `cargo run -p reflow_pack_loader --example print-abi` to read \
             the host ABI for the toolchain you're targeting.",
        )?;

    if cfg.targets.files.is_empty() {
        bail!(
            "no [targets.files] entries — at least one triple must be listed, \
             e.g. `[targets.files]\\naarch64-apple-darwin = \"target/aarch64-apple-darwin/release/libmypack.dylib\"`"
        );
    }

    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));

    // Resolve every cdylib path relative to the manifest.
    let mut target_entries: BTreeMap<String, (PackTarget, PathBuf)> = BTreeMap::new();
    for (triple, rel_path) in &cfg.targets.files {
        let full = manifest_dir.join(rel_path);
        if !full.exists() {
            bail!(
                "target {triple}: file {} does not exist — build it first, e.g. \
                 `cargo build --release --target {triple}`",
                full.display()
            );
        }
        let archive_name = format!(
            "lib/{triple}/{}",
            full.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("pack.dylib")
        );
        target_entries.insert(triple.clone(), (PackTarget { file: archive_name }, full));
    }

    let manifest = PackManifest {
        manifest_version: MANIFEST_VERSION,
        name: cfg.pack.name.clone(),
        version: cfg.pack.version.clone(),
        authors: cfg.pack.authors.clone(),
        description: cfg.pack.description.clone(),
        license: cfg.pack.license.clone(),
        reflow_pack_abi_version: abi,
        entrypoint: cfg
            .pack
            .entrypoint
            .clone()
            .unwrap_or_else(|| DEFAULT_ENTRYPOINT.to_string()),
        targets: target_entries
            .iter()
            .map(|(t, (pt, _))| (t.clone(), pt.clone()))
            .collect(),
        templates: cfg.pack.templates.clone(),
    };

    fs::create_dir_all(out_dir).with_context(|| format!("mkdir {}", out_dir.display()))?;
    let out_path = out_dir.join(format!("{}-{}.rflpack", manifest.name, manifest.version));
    let out = File::create(&out_path).with_context(|| format!("create {}", out_path.display()))?;
    let mut zip = ZipWriter::new(out);

    let options: SimpleFileOptions = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o755);

    // Manifest first — makes `inspect` cheap (no full-archive scan).
    zip.start_file("manifest.json", options)?;
    zip.write_all(&serde_json::to_vec_pretty(&manifest)?)?;

    // Then every dylib under its archive path.
    for (pt, source) in target_entries.values() {
        let mut f = File::open(source).with_context(|| format!("open {}", source.display()))?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        zip.start_file(&pt.file, options)?;
        zip.write_all(&buf)?;
    }

    zip.finish()?;
    println!("wrote {}", out_path.display());
    println!(
        "  {} target(s): {}",
        manifest.targets.len(),
        manifest
            .targets
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    );
    if !manifest.templates.is_empty() {
        println!(
            "  {} template(s): {}",
            manifest.templates.len(),
            manifest.templates.join(", ")
        );
    }
    Ok(())
}

// ─── inspect / unpack ──────────────────────────────────────────────────────

fn cmd_inspect(path: &Path) -> Result<()> {
    let m = read_manifest(path)?;
    println!("{}", serde_json::to_string_pretty(&m)?);
    Ok(())
}

fn cmd_unpack(path: &Path, out_dir: &Path) -> Result<()> {
    let f = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(f)?;
    fs::create_dir_all(out_dir)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let rel = match entry.enclosed_name() {
            Some(p) => p.to_path_buf(),
            None => continue,
        };
        let dest = out_dir.join(&rel);
        if entry.is_dir() {
            fs::create_dir_all(&dest)?;
        } else {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&dest)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    println!("unpacked to {}", out_dir.display());
    Ok(())
}
