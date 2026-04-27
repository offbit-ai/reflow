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
    /// Strip a `.rflpack` down to one or more triples.
    ///
    /// Produces a new `.rflpack` containing only the requested
    /// triples' binaries plus a manifest with `targets` reduced to
    /// match. Useful when distributing to a known platform — strip
    /// a 22 MiB six-triple bundle down to ~3-4 MiB before shipping.
    ///
    /// **Default**: if neither `--triple` nor `--current` is given,
    /// the host triple this CLI was built for is used automatically.
    /// Pass `--triple X --current` to keep both X and the host.
    Strip {
        /// Path to the input `.rflpack`.
        path: PathBuf,
        /// Triple to keep. Repeat for multiple.
        ///
        /// Examples: `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`,
        /// `wasm32-unknown-unknown`.
        #[arg(long = "triple", value_name = "TRIPLE")]
        triples: Vec<String>,
        /// Keep the binary for the host this CLI was built for.
        /// Implied when no `--triple` is given; pass explicitly to
        /// add the host alongside other explicit triples.
        #[arg(long)]
        current: bool,
        /// Output path. Defaults to
        /// `<name>-<version>-<joined-triples>.rflpack` next to the input.
        #[arg(long)]
        out: Option<PathBuf>,
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
        Command::Strip {
            path,
            triples,
            current,
            out,
        } => cmd_strip(&path, &triples, current, out.as_deref()),
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

    // Zstd at level 19 — chosen for the `.rflpack` distribution
    // story:
    //
    //   - 30–40% smaller than DEFLATE on stripped-but-Rust-heavy
    //     binaries (measured on reflow_pack_gpu: 28 MiB → ~18 MiB).
    //   - Single-shot compress is slow (seconds per pack); fine for
    //     CI release builds, never on the hot path.
    //   - Decompress is ~2× DEFLATE — `.rflpack` extracts in ms
    //     either way.
    //
    // The loader keeps DEFLATE enabled too so legacy bundles
    // produced before this switch still load.
    let options: SimpleFileOptions = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Zstd)
        .compression_level(Some(19))
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

// ─── strip ─────────────────────────────────────────────────────────────────

fn cmd_strip(
    path: &Path,
    triples_arg: &[String],
    current: bool,
    out_path: Option<&Path>,
) -> Result<()> {
    // Resolve which triples to keep:
    //   - `--triple X --triple Y`: keep X and Y
    //   - `--current`: keep the host triple
    //   - neither: implicit `--current` (the common case — strip a
    //     downloaded multi-triple bundle to whatever this machine runs)
    //   - both: keep host + all explicit triples
    //
    // Dedup but preserve insertion order so the output filename is
    // deterministic.
    let want_host = current || triples_arg.is_empty();
    let mut keep: Vec<String> = Vec::new();
    let mut seen = std::collections::BTreeSet::<String>::new();
    if want_host {
        let host = reflow_pack_loader::REFLOW_PACK_HOST_TRIPLE.to_string();
        if host == "unknown" {
            bail!(
                "REFLOW_PACK_HOST_TRIPLE is `unknown` — this CLI was built without \
                 a host triple stamp. Pass `--triple <triple>` explicitly."
            );
        }
        if seen.insert(host.clone()) {
            keep.push(host);
        }
    }
    for t in triples_arg {
        if seen.insert(t.clone()) {
            keep.push(t.clone());
        }
    }

    // Load the existing pack.
    let bundle_bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let reader = std::io::Cursor::new(&bundle_bytes);
    let mut archive = zip::ZipArchive::new(reader).context("parse .rflpack zip")?;

    let manifest: PackManifest = {
        let mut m = archive
            .by_name("manifest.json")
            .context("manifest.json missing from .rflpack")?;
        let mut buf = String::new();
        m.read_to_string(&mut buf)?;
        serde_json::from_str(&buf).context("parse manifest.json")?
    };

    // Validate that every requested triple is actually present.
    for t in &keep {
        if !manifest.targets.contains_key(t) {
            let have: Vec<&str> = manifest.targets.keys().map(String::as_str).collect();
            bail!(
                "pack `{}` has no build for triple `{}` (available: {})",
                manifest.name,
                t,
                have.join(", ")
            );
        }
    }

    // Build a slimmed manifest: same name/version/abi/etc, but
    // `targets` reduced to the intersection.
    let mut new_targets: std::collections::BTreeMap<String, PackTarget> =
        std::collections::BTreeMap::new();
    for t in &keep {
        if let Some(pt) = manifest.targets.get(t) {
            new_targets.insert(t.clone(), pt.clone());
        }
    }
    let new_manifest = PackManifest {
        targets: new_targets.clone(),
        ..manifest.clone()
    };

    // Compute the output path. Default sits next to the input.
    let suffix = if keep.len() == 1 {
        keep[0].clone()
    } else {
        format!("{}-triples", keep.len())
    };
    let default_out = path.with_file_name(format!(
        "{}-{}-{}.rflpack",
        manifest.name, manifest.version, suffix
    ));
    let out = out_path.map(Path::to_path_buf).unwrap_or(default_out);

    let out_file = File::create(&out).with_context(|| format!("create {}", out.display()))?;
    let mut zip = ZipWriter::new(out_file);
    let options: SimpleFileOptions = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Zstd)
        .compression_level(Some(19))
        .unix_permissions(0o755);

    // Write the slimmed manifest.
    zip.start_file("manifest.json", options)?;
    zip.write_all(&serde_json::to_vec_pretty(&new_manifest)?)?;

    // Stream every kept binary through to the output. We re-read
    // each via `by_name` rather than copying raw zip entries
    // because the source archive may be DEFLATE-compressed
    // (legacy bundles) and we always emit Zstd.
    let mut total_in: u64 = 0;
    for (_triple, pt) in new_targets.iter() {
        let mut entry = archive
            .by_name(&pt.file)
            .with_context(|| format!("entry `{}` missing from .rflpack", pt.file))?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        std::io::copy(&mut entry, &mut buf)?;
        total_in += buf.len() as u64;
        zip.start_file(&pt.file, options)?;
        zip.write_all(&buf)?;
    }
    zip.finish()?;

    let out_size = fs::metadata(&out)?.len();
    println!("wrote {}", out.display());
    println!(
        "  {} triple(s): {}",
        keep.len(),
        keep.iter().cloned().collect::<Vec<_>>().join(", ")
    );
    println!(
        "  input  {:.2} MiB ({} bytes)",
        bundle_bytes.len() as f64 / 1024.0 / 1024.0,
        bundle_bytes.len()
    );
    println!(
        "  payload {:.2} MiB (uncompressed across kept triples)",
        total_in as f64 / 1024.0 / 1024.0
    );
    println!(
        "  output {:.2} MiB ({:.0}% of input)",
        out_size as f64 / 1024.0 / 1024.0,
        (out_size as f64 / bundle_bytes.len() as f64) * 100.0
    );
    Ok(())
}
