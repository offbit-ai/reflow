# Pack ↔ SDK compatibility matrix

A `.rflpack` and a Reflow SDK are compatible **iff they were built
from the same source tree**. Both stamp a `REFLOW_PACK_ABI_VERSION`
hash at build time (computed by `reflow_pack_loader`'s `build.rs`
over its dependency tree), and the loader checks them on every
`load_pack` call:

```
RuntimeError: pack 'reflow.pack.api_services' ABI <a> != host ABI <b>
              — rebuild pack against current toolchain
```

Mismatches are caught loudly at startup, never silently at runtime.

## Compatibility waves

The release-publishing workflow pushes packs and SDKs in
**coordinated waves**: every tag in a wave points to the same git
commit, so all artifacts in the row share an ABI. Mixing within
a row is safe; mixing across rows is not.

| Wave | Date | pack | Go | JVM | Node | Python |
|---|---|---|---|---|---|---|
| **4** (current) | 2026-04-28 | `pack-v0.2.5` | `sdk/go/v0.2.6` | `sdk/jvm/v0.2.8` | `node-v0.2.10` | `python-v0.2.9` |
| 3 | 2026-04-28 | `pack-v0.2.4` | `sdk/go/v0.2.5` | `sdk/jvm/v0.2.7` | `node-v0.2.9` | `python-v0.2.8` |
| 2 | 2026-04-27 | `pack-v0.2.3` | `sdk/go/v0.2.3` | `sdk/jvm/v0.2.4` | `node-v0.2.6` | `python-v0.2.4` |
| 1 | 2026-04-26 | `pack-v0.2.1` | `sdk/go/v0.2.2` | `sdk/jvm/v0.2.3` | `node-v0.2.5` | `python-v0.2.3` |

**Reader rule of thumb**: pin a pack version → use the matching
wave's SDK row for every language you embed.

## Out-of-wave SDK releases

Several SDK releases shipped between waves to fix language-specific
issues (Python 0.2.7's GIL panic, JVM 0.2.5/0.2.6's `ctx.send` and
sundry helpers, Node 0.2.7/0.2.8 docs/tutorials, Go 0.2.4 missing-kick
runtime fix).

These do **not** have a paired pack release — installing any
`pack-v*` release against them will fail the ABI check. If you
need the pack catalog *and* one of these out-of-wave fixes, wait
for the next coordinated wave (every wave rolls forward all
unreleased SDK fixes).

| SDK | Out-of-wave releases | Compatible with pack |
|---|---|---|
| Go | `sdk/go/v0.2.4` | none — use the wave 3+ Go release |
| JVM | `sdk/jvm/v0.2.5`, `sdk/jvm/v0.2.6` | none — use the wave 3+ JVM release |
| Node | `node-v0.2.7`, `node-v0.2.8` | none — use the wave 3+ Node release |
| Python | `python-v0.2.5`, `python-v0.2.6`, `python-v0.2.7` | none — use the wave 3+ Python release |

## Verifying compatibility from code

Each SDK exposes the same four-function pack API. Check the host
ABI before loading:

```python
import offbit_reflow as reflow

print("host abi:", reflow.pack_abi_version())
manifest = reflow.inspect_pack("./reflow.pack.api_services-0.2.0.rflpack")
print("pack abi:", manifest["reflow_pack_abi_version"])

# Equivalent in other SDKs:
#   Go:    reflow.PackABIVersion()       reflow.InspectPack(path)
#   Node:  reflow.packAbiVersion()       reflow.inspectPack(path)
#   JVM:   Packs.packAbiVersion()        Packs.inspectPack(path)
```

If the values don't match, downgrade the SDK (or upgrade) until they
do.

## Building a pack against the current toolchain

If you can't wait for the next published wave, build the pack
locally from `sdk/packs/<name>/`:

```sh
# Build the cdylib for your platform
cargo build --release -p reflow_pack_<name>

# Build the reflow-pack CLI to produce a sealed .rflpack bundle
cargo build --release -p reflow_pack_cli

# Read the host ABI and stamp it into the pack
ABI=$(target/release/reflow-pack abi | awk '/abi_version/ {print $3}')
cd sdk/packs/<name>
REFLOW_PACK_ABI_VERSION=$ABI \
  ../../../target/release/reflow-pack build --out-dir ./target
```

The resulting `.rflpack` carries the same ABI as your locally-built
SDK, so `load_pack` succeeds.

## See also

- [`crates/reflow_pack_loader/README.md`](../crates/reflow_pack_loader/README.md)
  — pack loader internals, ABI hash construction, registry semantics
- [`sdk/packs/README.md`](../sdk/packs/README.md) — list of
  first-party packs and their template surface
