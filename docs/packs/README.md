# Actor Packs

A `.rflpack` is a **multi-platform native plugin bundle** that publishes additional actor templates into a Reflow runtime at load time. Every SDK can load packs the same way, via a single `loadPack(path)` call.

The default SDK install is intentionally lightweight — ~270 templates covering data, animation, math, scene, HTTP, and basic media. Heavier palettes (GPU renderers, ML inference, browser automation, ~6,700 SaaS API actors) ship as packs so you only pay for what you use.

## Why packs

Without packs, the only way to extend the SDK catalog is to recompile the runtime with new Cargo features and rebuild every per-platform native artifact. Packs make actor catalogs a runtime concern:

- **Smaller default install.** No GPU drivers, no LiteRT, no Chromium dependencies in the base SDK.
- **Modular delivery.** Ship third-party actor palettes as drop-in `.rflpack` files.
- **Faster iteration.** New actors → new pack → users `loadPack(...)` without bumping the SDK.
- **Same lifecycle as bundled actors.** `templateActor("tpl_pack_owned_id")` and graph references work identically once a pack is loaded.

## Bundle format

A `.rflpack` is a zip archive:

```
mypack-0.2.0.rflpack
├── manifest.json
└── lib/
    ├── aarch64-apple-darwin/libmypack.dylib
    ├── x86_64-apple-darwin/libmypack.dylib
    ├── x86_64-unknown-linux-gnu/libmypack.so
    ├── aarch64-unknown-linux-gnu/libmypack.so
    └── x86_64-pc-windows-msvc/mypack.dll
```

`manifest.json` declares the pack name, version, supported triples, advertised template ids, and the runtime ABI version it was built against:

```json
{
  "manifest_version": 1,
  "name": "offbit.ml",
  "version": "0.2.0",
  "reflow_pack_abi_version": 1380148208,
  "entrypoint": "reflow_pack_register",
  "targets": {
    "aarch64-apple-darwin":     { "file": "lib/aarch64-apple-darwin/libmypack.dylib" },
    "x86_64-pc-windows-msvc":   { "file": "lib/x86_64-pc-windows-msvc/mypack.dll" }
  },
  "templates": ["tpl_ml_run_inference", "tpl_cv_image_to_tensor", "..."]
}
```

The host validates the manifest, picks the dylib for the current triple, dlopens it, and asks the pack to register its templates with the runtime. Templates published by a pack are first-class — they participate in `templateList()`, subgraph resolution, network registration, and so on, identical to bundled templates.

## First-party packs

Six are published on every `pack-vX.Y.Z` GitHub Release:

| Pack | Templates | Pulls in |
|------|----------:|----------|
| `reflow.pack.browser`        | 1     | chromiumoxide |
| `reflow.pack.video_encode`   | 1     | openh264 |
| `reflow.pack.ml`             | 12    | CV ops + LiteRT inference |
| `reflow.pack.gpu`            | 6     | wgpu SDF / scene / 2D renderers |
| `reflow.pack.window_events`  | 5     | keyboard / mouse / gamepad / touch / window |
| `reflow.pack.api_services`   | ~6700 | generated Slack / Stripe / Jira / Notion / … |

Per-pack template inventories live in [`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md).

## Loading a pack

Every SDK exposes the same four entry points, named idiomatically:

| Operation | Node | Python | Go | JVM | C++ |
|---|---|---|---|---|---|
| Load | `loadPack(path)` | `load_pack(path)` | `LoadPack(path)` | `Packs.loadPack(path)` | `reflow::pack::load(path)` |
| Inspect manifest | `inspectPack(path)` | `inspect_pack(path)` | `InspectPack(path)` | `Packs.inspectPack(path)` | `reflow::pack::inspect_json(path)` |
| List loaded | `listPacks()` | `list_packs()` | `ListPacks()` | `Packs.listPacks()` | `reflow::pack::list_json()` |
| Host ABI version | `packAbiVersion()` | `pack_abi_version()` | `PackABIVersion()` | `Packs.packAbiVersion()` | `reflow::pack::abi_version()` |

`loadPack` is **idempotent** — repeated calls with the same pack name are no-ops and return the previously published template ids.

```ts
import { loadPack, templateList, templateActor } from "@offbit-ai/reflow";

const ids = loadPack("./reflow.pack.ml-0.2.0.rflpack");
console.log(ids);                                           // ["tpl_ml_run_inference", …]
console.log(templateList().filter(t => t.startsWith("tpl_ml")));
const infer = templateActor("tpl_ml_run_inference");        // resolves through the pack
```

Either ship the `.rflpack` alongside your application binary or download it on first run from a [GitHub Release](https://github.com/offbit-ai/reflow/releases?q=pack):

```sh
VER=0.2.0
curl -LO https://github.com/offbit-ai/reflow/releases/download/pack-v$VER/reflow.pack.ml-$VER.rflpack
```

## ABI lockstep

The pack handshake is **toolchain-locked**. Every pack stamps a `reflow_pack_abi_version` value at build time, computed from `fnv1a(rustc_verbose_version || PACK_ABI_REVISION)`. The host loader refuses to dlopen a pack whose ABI doesn't match the SDK release the user installed.

Practically:

- A `pack-v0.2.0` release is paired with `node-v0.2.0` / `python-v0.2.1` / `sdk/jvm/v0.2.0` / `sdk/go/v0.2.0`. CI builds them from the same workspace revision with the same rustc.
- Mixing pack and SDK versions (e.g. `pack-v0.2.0` with `node-v0.3.0`) errors at load with `pack ABI X != host ABI Y`.
- Third-party packs follow the same rule: the pack author rebuilds when the runtime upgrades. We document `reflow_pack_loader::REFLOW_PACK_ABI_VERSION` so authors can lock against a specific runtime.

This is the trade-off chosen for v1: `Arc<dyn Actor>` over the C ABI requires layout agreement, which means same rustc + same `reflow_actor` crate. A callback-only ABI that survives toolchain mismatches is a future option (see [`reflow_pack_loader`](https://github.com/offbit-ai/reflow/tree/main/crates/reflow_pack_loader)).

## See also

- [Authoring a pack](authoring.md) — write and ship your own.
- [`sdk/packs/README.md`](https://github.com/offbit-ai/reflow/blob/main/sdk/packs/README.md) — first-party pack catalog with template inventories.
