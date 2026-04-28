# `reflow_pack_loader`

Runtime that loads `.rflpack` bundles into a live Reflow process so
the template catalog can grow without a rebuild of the host. Used
internally by every Reflow SDK; the C ABI / Go / Python / Node /
JVM bindings all expose the same four-function surface
(`load_pack`, `inspect_pack`, `list_packs`, `pack_abi_version`).

## What's a `.rflpack`?

A self-contained zip bundle holding:

- One or more platform-specific cdylibs (one per supported triple)
  — the actual compiled actor templates.
- A manifest describing pack name, version, target triples, ABI
  version, and the list of templates the pack publishes.
- A canonical `entrypoint` symbol the loader resolves on every
  cdylib to retrieve the `(template_id, factory)` pairs.

Each first-party pack lives under `sdk/packs/<name>/`; release
artifacts are published to GitHub Releases under tags matching
`pack-v*`.

## ABI handshake

Every pack stamps the `reflow_pack_abi_version` hash it was built
against into its manifest. Every SDK runtime knows its own
`REFLOW_PACK_ABI_VERSION` (computed by this crate's `build.rs`).
`load_pack` rejects any pack whose ABI doesn't match the host:

```
RuntimeError: pack 'reflow.pack.api_services' ABI <a> != host ABI <b>
              — rebuild pack against current toolchain
```

Compatibility is therefore wave-based: a pack release pairs with a
specific wave of SDK releases all built from the same commit.
**See [`docs/pack-compatibility.md`](../../docs/pack-compatibility.md)
for the wave matrix and how to verify compatibility from code.**

## ABI hash construction

The hash is set via the `REFLOW_PACK_ABI_VERSION` env var at build
time (build.rs reads it). The publish workflow computes it from
`cargo metadata` over the loader's dep tree — any change in the
versions of `reflow_actor`, `reflow_pack_sdk`, `reflow_pack_loader`,
or their public-API-affecting dependencies bumps the hash.

`reflow-pack abi` prints the host's value; pack authors stamp the
same value into their bundles via `REFLOW_PACK_ABI_VERSION` at
`reflow-pack build` time.

## API surface (Rust)

```rust
use reflow_rt::pack_loader;

// Load a pack and register every template it carries.
let template_ids = pack_loader::load_pack(path)?;

// Read manifest without loading code (useful for UI/CI checks).
let manifest = pack_loader::inspect_pack(path)?;

// Currently-loaded packs.
let packs = pack_loader::PACK_REGISTRY.loaded_packs();

// All template ids reachable via `template_actor()` after loads.
let ids = pack_loader::PACK_REGISTRY.template_ids();

// Host's ABI version (compare with `manifest.reflow_pack_abi_version`).
let abi = pack_loader::REFLOW_PACK_ABI_VERSION;
```

The same four operations are exposed verbatim through every
language SDK — see each SDK's README for the language-idiomatic
spelling.

## Loading semantics

- `load_pack` is **idempotent per pack name**. Calling it twice
  with the same pack returns the existing template set without
  re-dlopening the cdylib.
- Templates registered by a pack become reachable via
  `reflow_components::get_actor_for_template(id)` (and therefore
  every SDK's `template_actor(id)`).
- Pack templates and built-in `tpl_*` templates share a flat
  namespace; pack ids should be prefixed (`api_*`, `gpu_*`, etc.)
  to avoid collisions.
- The cdylib stays loaded for the process lifetime — there is no
  `unload_pack`.

## Triple selection

Multi-triple `.rflpack` bundles carry one cdylib per supported
triple. The loader picks the bundle entry matching
`REFLOW_PACK_HOST_TRIPLE` (compiled into the loader at build
time) and rejects the load if no matching entry exists.

Slim-bundle packs (`<name>-<ver>-<triple>.rflpack`) carry one
triple only. They install identically — the loader still selects
on triple, just from a one-element manifest.

## See also

- [`docs/pack-compatibility.md`](../../docs/pack-compatibility.md) —
  pack ↔ SDK wave matrix
- [`sdk/packs/README.md`](../../sdk/packs/README.md) — list of
  first-party packs and their template surfaces
- [`crates/reflow_pack_cli/README.md`](../reflow_pack_cli/README.md)
  — `reflow-pack` CLI for building / inspecting / stripping
  bundles
