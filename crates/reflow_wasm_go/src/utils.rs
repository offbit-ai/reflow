//! Utilities for building and testing Go WASM plugins

use std::path::{Path, PathBuf};
use std::process::Command;

/// Build a Go WASM plugin using TinyGo
pub fn build_go_plugin<P: AsRef<Path>>(go_file: P, output: P) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new("tinygo")
        .args(&[
            "build",
            "-o", output.as_ref().to_str().unwrap(),
            "-target", "wasi",
            "-no-debug",
            go_file.as_ref().to_str().unwrap(),
        ])
        .status()?;

    if !status.success() {
        return Err(format!("TinyGo build failed with status: {}", status).into());
    }

    Ok(())
}

/// Get the SDK directory path
pub fn sdk_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sdk")
}

/// Get the examples directory path
pub fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}