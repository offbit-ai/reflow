//! Browser-WASM distribution of the Reflow runtime.
//!
//! This crate wires up `reflow_graph`, `reflow_actor`, and
//! `reflow_network` for the wasm32-unknown-unknown target via
//! `wasm-bindgen`. Building it produces a `.wasm` + JS glue that an
//! npm package can ship as `@offbit-ai/reflow-wasm`.
//!
//! Usage in JS:
//!
//! ```js
//! import init, { Graph, Network, Message } from "@offbit-ai/reflow-wasm";
//! await init();
//!
//! const g = new Graph("demo");
//! g.addNode("a", "tpl_doubler");
//! g.addNode("b", "tpl_collector");
//! g.addConnection("a", "out", "b", "in");
//!
//! const net = Network.fromGraph(g);
//! net.registerActorJs("tpl_doubler", { /* JS class with run(ctx) */ });
//! net.start();
//! ```
//!
//! For now the surface is intentionally narrow — Graph + Network +
//! Message + the existing `_*` wasm-bindgen exports inherited from
//! `reflow_graph`/`reflow_network`. The bundled component catalog
//! (`reflow_components`) is excluded because its native-only deps
//! (rquickjs, openh264, wgpu native backends, …) don't compile to
//! wasm32-unknown-unknown without significant feature-gating work.
//! Author actors as JS classes via `Network.registerActorJs(...)`.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

/// Wire up `console.error_panic_hook` so Rust panics surface as
/// JS exceptions instead of "RuntimeError: unreachable executed".
/// Call once from your JS bootstrap.
#[wasm_bindgen(start)]
pub fn __reflow_rt_wasm_init() {
    console_error_panic_hook::set_once();
}

/// Re-export the `Graph` and `GraphNetwork` wasm-bindgen surfaces
/// inherited from `reflow_graph` and `reflow_network`. wasm-bindgen
/// doesn't propagate `#[wasm_bindgen]` exports across crates
/// automatically — re-exporting here makes them visible in this
/// crate's generated JS module.
pub use reflow_graph::Graph;
pub use reflow_network::network::GraphNetwork;

/// Library version string — useful for debug overlays.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Initialize the shared GPU context from a `<canvas>` element.
///
/// Must be awaited once during workflow startup before any GPU-backed
/// actor runs (SDF renderer, scene renderer, marching cubes, …).
/// `canvasSelector` is a CSS selector pointing at the `HTMLCanvasElement`
/// Reflow should render to. Pass `null`/`undefined` for off-screen-only
/// workloads where the result is read back to CPU and displayed
/// outside the wgpu pipeline.
///
/// On success, `wgpu::Adapter`, `Device`, and `Queue` are cached for
/// the life of the runtime; subsequent calls are no-ops.
#[wasm_bindgen(js_name = initGpuContext)]
pub async fn init_gpu_context(canvas_selector: Option<String>) -> Result<(), JsValue> {
    reflow_components::gpu::context::init_gpu_context(canvas_selector.as_deref())
        .await
        .map_err(|e| JsValue::from_str(&e))
}

// ─── .rflpack loading ──────────────────────────────────────────────────────

/// Extract the wasm32 binary from a `.rflpack` byte buffer.
///
/// Used by the JS `loadPack(url)` shim. Validates the manifest
/// (`manifest_version` + `reflow_pack_abi_version`) and pulls the
/// `wasm32-unknown-unknown` entry out of the zip. Returns a JS
/// object `{ manifestJson: string, wasm: Uint8Array }` so the
/// caller can compile via `WebAssembly.compile(result.wasm)`.
///
/// Errors:
/// - The zip can't be parsed.
/// - `manifest.json` is missing or malformed.
/// - The pack was built against a different `reflow_pack_abi_version`.
/// - The pack has no `wasm32-unknown-unknown` target (its `Reflow.pack.toml`
///   author didn't opt-in to a wasm build, or the build failed in CI).
#[wasm_bindgen(js_name = extractPackWasm)]
pub fn extract_pack_wasm(bytes: &[u8]) -> Result<JsValue, JsValue> {
    let host_abi = reflow_pack_loader::REFLOW_PACK_ABI_VERSION;
    let extracted = reflow_pack_loader::bundle::extract_wasm_from_bytes(bytes, host_abi)
        .map_err(|e| JsValue::from_str(&e.to_string()))?;

    let manifest_json = serde_json::to_string(&extracted.manifest)
        .map_err(|e| JsValue::from_str(&format!("serialize manifest: {}", e)))?;

    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("manifestJson"),
        &JsValue::from_str(&manifest_json),
    )?;
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("wasm"),
        &js_sys::Uint8Array::from(extracted.wasm.as_slice()),
    )?;
    Ok(obj.into())
}

/// The pack ABI version this runtime was built against. Packs whose
/// `manifest.reflow_pack_abi_version` doesn't match cannot be loaded.
/// Surfaced for diagnostics — `loadPack` already enforces it.
#[wasm_bindgen(js_name = packAbiVersion)]
pub fn pack_abi_version() -> u32 {
    reflow_pack_loader::REFLOW_PACK_ABI_VERSION
}
