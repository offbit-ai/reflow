//! Emits `REFLOW_PACK_ABI_VERSION`, a u32 derived from the host's rustc
//! version + a manually-bumped source revision constant. Packs and the host
//! both compute this at build time from the same `reflow_pack_loader`
//! source tree, so a mismatched rustc or a bumped ABI revision yields a
//! different number and the loader refuses the load.

use std::process::Command;

/// Bump this whenever the pack vtable (`rfl_pack_host`, factory signature,
/// entry-point names, manifest schema) changes in a way that would break an
/// older pack.
const PACK_ABI_REVISION: u32 = 1;

fn main() {
    // Include rustc's verbose version so a host and pack built with
    // different compilers never accidentally agree.
    let rustc_version = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("-vV")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_else(|_| "unknown-rustc".to_string());

    let mut h: u32 = 0x811c9dc5; // FNV-1a offset
    for b in rustc_version.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    for b in PACK_ABI_REVISION.to_le_bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }

    let triple = std::env::var("TARGET").unwrap_or_else(|_| "unknown".into());

    println!("cargo:rustc-env=REFLOW_PACK_ABI_VERSION={h}");
    println!("cargo:rustc-env=REFLOW_PACK_HOST_TRIPLE={triple}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=RUSTC");
}
