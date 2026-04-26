# Language SDKs

Reflow's runtime is Rust, but its native shape is a **language-agnostic actor model** — graphs of typed-port nodes connected by message edges. Each first-party SDK is a thin native binding plus an idiomatic surface in that language. They all link to (or embed) the same `reflow_rt_capi` C ABI, so behavior is identical across languages.

## At a glance

| SDK | Package | Native lib | Authoring | Notes |
|---|---|---|---|---|
| [Node.js](node.md) | `@offbit-ai/reflow` (npm) | `.node` addon (per-platform optional dep) | napi-rs | Install just works — npm picks the right `.node` |
| [Python](python.md) | `offbit-reflow` (PyPI) | inside the wheel (abi3-py39) | pyo3 | Pre-built wheels for darwin / linux / windows |
| [Go](go.md) | `github.com/offbit-ai/reflow/sdk/go` | external `libreflow_rt_capi` | cgo | Run `scripts/install_lib.sh` after `go get` to drop the platform tarball into the module |
| [JVM](jvm.md) | `ai.offbit:reflow` (Maven Central) | bundled in the fat jar (classpath resource) | JNI + Kotlin DSL | Single dep; loader extracts the right native lib at runtime |
| [C++](cpp.md) | header-only at [`sdk/cpp/`](https://github.com/offbit-ai/reflow/tree/main/sdk/cpp) | external `libreflow_rt_capi` | RAII wrapper | C++17, CMake `add_subdirectory` or `find_package` |

## What's the same across languages

Each SDK exposes the same five core types with idiomatic naming:

- **Message** — typed payload (Flow / Boolean / Integer / Float / String / Object / Array / Bytes / Stream).
- **Graph** — declarative DAG: add nodes, connect ports, register groups, expose subgraph ports.
- **Actor** — either a registered template id (the bundled catalog) or a callback-driven actor authored in the host language.
- **Network** — the executor that runs a graph; takes initial packets, emits a runtime event stream.
- **EventStream** — async event tap for tracing / observability.

Plus a [pack loader](../packs/README.md) that lets any SDK install ship optional actor palettes (GPU renderers, ML, browser automation, ~6,700 SaaS API actors) at runtime via `loadPack(path)`.

## What's slightly different

- **Concurrency model.** Node uses ThreadSafeFunction callbacks; Python uses GIL-aware shims; Go uses cgo callbacks; JVM uses `JNIEnv::attach_current_thread`; C++ pushes raw C ABI threading concerns to the user. The SDK READMEs cover the per-language gotchas.
- **JSON conversion.** Each SDK auto-converts where idiomatic (`Map`/`dict`/`map[string]any`/`Map<String,Object>`/`std::string`). C++ returns `std::string` and lets you pick the JSON parser.
- **Async patterns.** Node returns Promises, Python exposes `async`-friendly entry points, Go uses channels, JVM has Kotlin `suspend`/Flow adapters, C++ uses callbacks.

## Picking an SDK

- **Browser / Electron / VS Code extension** → Node.
- **ML pipelines, data engineering, CLI tooling** → Python.
- **Backend services, CLI binaries, embedded gateways** → Go.
- **Android, JVM-based platforms (Kafka, Spark, Flink), desktop with Compose** → JVM.
- **Embedded, native engines, plugins for existing C++ apps** → C++.
- **Roll your own runtime** → use `reflow_rt_capi` directly; every SDK above is built on it.

## Versioning

The SDKs ship independently but track the same runtime semantics. Tag schemes:

| Tag pattern | Triggers |
|---|---|
| `node-vX.Y.Z` | Builds + publishes `@offbit-ai/reflow@X.Y.Z` to npm |
| `python-vX.Y.Z` | Builds + publishes `offbit-reflow X.Y.Z` to PyPI |
| `sdk/go/vX.Y.Z` | Builds `libreflow_rt_capi` for 5 triples + GitHub Release |
| `sdk/jvm/vX.Y.Z` | Builds + publishes `ai.offbit:reflow:X.Y.Z` to Maven Central |
| `pack-vX.Y.Z` | Builds + publishes the 6 first-party `.rflpack` bundles |

Pack ABI versions are **toolchain-locked** — pair a `pack-v*` release with the SDK release built from the same workspace revision.
