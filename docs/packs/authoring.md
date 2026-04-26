# Authoring a Pack

A pack is a Rust `cdylib` crate using [`reflow_pack_sdk`](https://github.com/offbit-ai/reflow/tree/main/crates/reflow_pack_sdk). The SDK provides:

- The `#[reflow_pack]` attribute macro that emits the C ABI entrypoints.
- A safe `PackHost` API for registering template factories.
- Re-exports of `Actor`, `Message`, `ActorContext`, etc.

You write Rust actors as you would for any Reflow runtime, then ship the resulting `.rflpack` from a CI workflow.

## Skeleton

```toml
# my_pack/Cargo.toml
[package]
name = "my_pack"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
reflow_pack_sdk = { version = "0.2.0", path = "../reflow/crates/reflow_pack_sdk" }
anyhow = "1"
flume = "0.11"
parking_lot = "0.12"
```

```rust
// my_pack/src/lib.rs
use reflow_pack_sdk::{
    reflow_pack, Actor, ActorBehavior, ActorContext, ActorLoad,
    ActorState, MemoryState, Message, PackHost, Port,
};

use parking_lot::Mutex;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

struct EchoActor {
    inports: Port,
    outports: Port,
    load: Arc<ActorLoad>,
}

impl EchoActor {
    fn new() -> Self {
        Self {
            inports: flume::bounded(16),
            outports: flume::bounded(16),
            load: Arc::new(ActorLoad::new(0)),
        }
    }
}

impl Actor for EchoActor {
    fn get_behavior(&self) -> ActorBehavior {
        Box::new(|ctx: ActorContext| -> Pin<
            Box<
                dyn Future<Output = Result<HashMap<String, Message>, anyhow::Error>>
                    + Send + 'static,
            >,
        > {
            Box::pin(async move {
                let payload = ctx.get_payload().clone();
                let input = payload.get("input").cloned().unwrap_or(Message::Flow);
                let mut out = HashMap::new();
                out.insert("output".to_string(), input);
                Ok(out)
            })
        })
    }
    fn get_inports(&self)  -> Port { self.inports.clone() }
    fn get_outports(&self) -> Port { self.outports.clone() }
    fn inport_names(&self)  -> Vec<String> { vec!["input".into()] }
    fn outport_names(&self) -> Vec<String> { vec!["output".into()] }
    fn create_state(&self)  -> Arc<Mutex<dyn ActorState>> {
        Arc::new(Mutex::new(MemoryState::default()))
    }
    fn load_count(&self) -> Arc<ActorLoad> { Arc::clone(&self.load) }
    fn create_instance(&self) -> Arc<dyn Actor> { Arc::new(EchoActor::new()) }
}

#[reflow_pack]
fn register(host: &mut PackHost) {
    host.register("my.pack.echo", || Arc::new(EchoActor::new()));
}
```

The `#[reflow_pack]` macro expands `register` into the two C symbols the loader looks up: `reflow_pack_abi_version` (returns the host ABI hash) and `reflow_pack_register` (calls back into the host vtable to register every template factory).

## Reflow.pack.toml

A small companion file that drives the [`reflow-pack`](https://github.com/offbit-ai/reflow/tree/main/crates/reflow_pack_cli) CLI, which assembles the multi-platform `.rflpack` zip:

```toml
[pack]
name = "my.pack.echo"
version = "0.1.0"
description = "Echo actor that copies input → output"
authors = ["Your Name"]
license = "MIT"
templates = ["my.pack.echo"]

# Paths are relative to this file. CI populates one entry per built triple.
[targets.files]
aarch64-apple-darwin = "../../target/release/libmy_pack.dylib"
```

## Build

```sh
# 1. Build the cdylib for each triple you want to ship.
cargo build --release -p my_pack --target aarch64-apple-darwin
cargo build --release -p my_pack --target x86_64-unknown-linux-gnu
# … plus any others you target

# 2. Build the packaging CLI.
cargo build --release -p reflow_pack_cli

# 3. Read the host ABI version. The pack must be stamped with the
#    same number, so build the CLI with the SAME rustc as the runtime
#    your users will load this pack into.
target/release/reflow-pack abi
# abi_version = 1380148208
# host_triple = aarch64-apple-darwin

# 4. Bundle.
REFLOW_PACK_ABI_VERSION=1380148208 target/release/reflow-pack build \
    --manifest my_pack/Reflow.pack.toml \
    --out-dir target/packs

# 5. Inspect.
target/release/reflow-pack inspect target/packs/my.pack.echo-0.1.0.rflpack
```

## CI: ship a multi-platform pack

[`.github/workflows/publish-packs.yml`](https://github.com/offbit-ai/reflow/blob/main/.github/workflows/publish-packs.yml) is a concrete example. The pattern:

1. **Matrix-build** the cdylib on all five supported runners (mac aarch64 / mac x86_64 / linux x86_64 / linux aarch64 / windows x86_64).
2. **Upload** the per-triple cdylib as a GitHub Actions artifact.
3. **Assembly job** downloads every artifact, generates a `Reflow.pack.toml` pointing at all of them, runs `reflow-pack build` to zip into a single `.rflpack`, attaches to a Release.

Pin `dtolnay/rust-toolchain@stable` everywhere so every cdylib in the bundle shares one ABI hash.

## Distribution options

- **GitHub Release attachment** — easiest; users `curl` the `.rflpack`.
- **Internal artifact registry** — anything that serves files works (Artifactory, Nexus, S3, GCS).
- **Bundled with your application** — drop the `.rflpack` next to your binary and `loadPack(__dirname + "/x.rflpack")` at startup.
- **Inside an npm / PyPI package** — ship the `.rflpack` as a data file; your install script runs `loadPack` on first import.

## ABI lockstep — what to communicate to users

A pack tied to runtime ABI version `X` only loads into SDK releases built against the same `X`. Make it easy on consumers:

- Tag your pack with the same `vX.Y.Z` as the SDK release it targets.
- Document the supported SDK versions in your README.
- If you maintain multiple SDK targets (e.g. one for `node-v0.2.0` and one for `node-v0.3.0`), publish two `.rflpack` files and label them clearly.

When the runtime bumps `PACK_ABI_REVISION` (vtable shape changes), all packs need to be rebuilt. Watch the [`reflow_pack_loader/build.rs`](https://github.com/offbit-ai/reflow/blob/main/crates/reflow_pack_loader/build.rs) constant for changes.

## Troubleshooting

- **"pack ABI X != host ABI Y"** — the pack was built with a different rustc or a different `PACK_ABI_REVISION`. Rebuild from the same toolchain as the host.
- **"pack has no build for triple T"** — the manifest doesn't include the user's platform. Add the missing triple to `[targets.files]` and re-bundle.
- **"unable to execute patch / no NASM"** — Linux cross-compile environments may need `apt install patch nasm gcc-aarch64-linux-gnu g++-aarch64-linux-gnu`. See `publish-packs.yml` for the working CI matrix.

## See also

- [Pack overview](README.md) — what packs are, how to load them.
- [`crates/reflow_pack_sdk`](https://github.com/offbit-ai/reflow/tree/main/crates/reflow_pack_sdk) — author-facing crate with the macro + safe types.
- [`crates/reflow_pack_loader`](https://github.com/offbit-ai/reflow/tree/main/crates/reflow_pack_loader) — runtime loader (read this if you're embedding Reflow in a custom host).
- [`sdk/packs/`](https://github.com/offbit-ai/reflow/tree/main/sdk/packs) — first-party pack source as worked examples.
