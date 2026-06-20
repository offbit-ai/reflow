//! Integration tests for the `reflow` binary, driven as a subprocess.

use std::process::{Command, Stdio};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_reflow");

fn fixture() -> String {
    format!("{}/tests/fixtures/smoke.graph.json", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn graph_validate_reports_nodes_and_components() {
    let out = Command::new(BIN)
        .args(["graph", "validate", &fixture()])
        .output()
        .expect("run reflow graph validate");
    assert!(out.status.success(), "validate failed: {:?}", out);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("1 nodes"), "unexpected output: {stdout}");
    // tpl_signal resolves against the bundled catalog, so no "unresolved" warning.
    assert!(!stdout.contains("unresolved"), "unexpected unresolved: {stdout}");
}

#[test]
fn graph_inspect_lists_the_component() {
    let out = Command::new(BIN)
        .args(["graph", "inspect", &fixture()])
        .output()
        .expect("run reflow graph inspect");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("tpl_signal"), "unexpected output: {stdout}");
}

#[test]
fn run_loads_resolves_and_starts() {
    let mut child = Command::new(BIN)
        .args(["run", &fixture()])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn reflow run");

    // Give it a beat to load, resolve actors against the catalog, and start.
    std::thread::sleep(Duration::from_millis(1500));
    let still_running = child.try_wait().expect("try_wait").is_none();

    let _ = child.kill();
    let out = child.wait_with_output().expect("collect output");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(still_running, "process exited early; stderr:\n{stderr}");
    assert!(
        stderr.contains("running") || stderr.contains("started"),
        "did not see a start message; stderr:\n{stderr}"
    );
}
