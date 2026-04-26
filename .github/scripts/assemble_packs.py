#!/usr/bin/env python3
"""Assemble per-pack `.rflpack` bundles from the per-triple cdylibs
uploaded by publish-packs.yml's `build` job.

For each pack listed in PACKS:
  1. Write a fresh `Reflow.pack.generated.toml` in the pack's source
     directory that references the cdylibs downloaded into
     `$GITHUB_WORKSPACE/artifacts/pack-cdylibs-<triple>/`.
  2. Invoke `target/release/reflow-pack build` against that TOML to
     produce `packs/<name>-<version>.rflpack`.

Missing triples are silently skipped — the pack author's static
`Reflow.pack.toml` may advertise templates for triples we don't yet
build. The host loader will error clearly at `rfl_pack_load` time if
the current triple is missing from the shipped manifest.
"""

from __future__ import annotations
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

PACKS = [
    ("reflow_pack_browser",       "reflow.pack.browser",       "sdk/packs/browser"),
    ("reflow_pack_video_encode",  "reflow.pack.video_encode",  "sdk/packs/video_encode"),
    ("reflow_pack_ml",            "reflow.pack.ml",            "sdk/packs/ml"),
    ("reflow_pack_gpu",           "reflow.pack.gpu",           "sdk/packs/gpu"),
    ("reflow_pack_window_events", "reflow.pack.window_events", "sdk/packs/window_events"),
    ("reflow_pack_api_services",  "reflow.pack.api_services",  "sdk/packs/api_services"),
]

TRIPLE_EXT = {
    "aarch64-apple-darwin":      "dylib",
    "x86_64-apple-darwin":       "dylib",
    "x86_64-unknown-linux-gnu":  "so",
    "aarch64-unknown-linux-gnu": "so",
    "x86_64-pc-windows-msvc":    "dll",
    # Browser bundle. Only present for packs that compile to
    # wasm32-unknown-unknown — see the wasm_packs allow-list in
    # publish-packs.yml. Native loaders skip this entry; the
    # browser-side pack loader uses WebAssembly.instantiate
    # against the .wasm bytes.
    "wasm32-unknown-unknown":    "wasm",
}

ROOT = Path(os.environ.get("GITHUB_WORKSPACE", os.getcwd())).resolve()
OUT = ROOT / "packs"
OUT.mkdir(exist_ok=True)

reflow_pack = ROOT / "target" / "release" / "reflow-pack"
if not reflow_pack.exists():
    sys.exit(f"reflow-pack binary missing at {reflow_pack}")


def collect_targets(crate: str) -> list[tuple[str, Path]]:
    """Return [(triple, cdylib_path)] for every triple whose artifact
    was downloaded and contains a cdylib for `crate`."""
    found: list[tuple[str, Path]] = []
    for triple, ext in TRIPLE_EXT.items():
        candidate = ROOT / "artifacts" / f"pack-cdylibs-{triple}" / f"{crate}.{ext}"
        if candidate.exists():
            found.append((triple, candidate))
    return found


def write_generated_toml(src_toml: Path, out_toml: Path, targets: list[tuple[str, Path]]) -> None:
    """Copy `src_toml` but replace its `[targets.files]` table with one
    that points at the downloaded absolute paths (made relative to the
    TOML for cleaner logs)."""
    text = src_toml.read_text()
    # Strip any existing [targets.files] block — everything up to the
    # next top-level header or EOF.
    text = re.sub(r"\n\[targets\.files\][^\[]*", "\n", text, flags=re.DOTALL)

    lines = [text.rstrip(), "", "[targets.files]"]
    for triple, path in targets:
        rel = os.path.relpath(path, out_toml.parent)
        lines.append(f'{triple} = "{rel}"')
    out_toml.write_text("\n".join(lines) + "\n")


for crate, pack_name, rel_dir in PACKS:
    pack_dir = ROOT / rel_dir
    src_toml = pack_dir / "Reflow.pack.toml"
    out_toml = pack_dir / "Reflow.pack.generated.toml"

    targets = collect_targets(crate)
    if not targets:
        print(f"[skip] {pack_name}: no cdylibs found for any triple", flush=True)
        continue

    print(f"[pack] {pack_name}: {len(targets)} triple(s)", flush=True)
    for triple, path in targets:
        print(f"       {triple:30s} <- {path}", flush=True)

    write_generated_toml(src_toml, out_toml, targets)
    subprocess.run(
        [str(reflow_pack), "build", "--manifest", str(out_toml), "--out-dir", str(OUT)],
        check=True,
    )

# Final listing for CI logs.
for p in sorted(OUT.glob("*.rflpack")):
    size_mib = p.stat().st_size / (1024 * 1024)
    print(f"built {p.name}  ({size_mib:.2f} MiB)")
