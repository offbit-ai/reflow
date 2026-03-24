//! SDF material and scene composition actors.
//!
//! - SdfMaterialActor: assigns color/PBR properties to a child SDF
//! - SdfSceneActor: wraps root SDF + scene settings, compiles to WGSL

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_sdf::ir::{SceneSettings, SdfMaterial, SdfNode};
use serde_json::json;
use std::collections::HashMap;

fn parse_sdf(msg: Option<&Message>) -> Option<SdfNode> {
    match msg {
        Some(Message::Object(v)) => {
            let json: serde_json::Value = v.as_ref().clone().into();
            serde_json::from_value(json).ok()
        }
        _ => None,
    }
}

fn sdf_output(node: &SdfNode) -> HashMap<String, Message> {
    let json = serde_json::to_value(node).unwrap_or_default();
    let mut out = HashMap::new();
    out.insert(
        "sdf".to_string(),
        Message::object(EncodableValue::from(json)),
    );
    out
}

// ── Material ────────────────────────────────────────────────────

#[actor(SdfMaterialActor, inports::<10>(sdf), outports::<1>(sdf, error), state(MemoryState))]
pub async fn sdf_material_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let child =
        parse_sdf(payload.get("sdf")).ok_or_else(|| anyhow::anyhow!("Missing sdf input"))?;

    let r = config.get("colorR").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
    let g = config.get("colorG").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
    let b = config.get("colorB").and_then(|v| v.as_f64()).unwrap_or(0.8) as f32;
    let roughness = config
        .get("roughness")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5) as f32;
    let metallic = config
        .get("metallic")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as f32;

    let mat = SdfMaterial {
        color: [r, g, b],
        roughness,
        metallic,
        ..Default::default()
    };

    Ok(sdf_output(&child.with_material(mat)))
}

// ── Scene (Compose + Compile) ───────────────────────────────────

/// Takes a root SDF and scene config, wraps into Scene node, and
/// compiles to WGSL. Outputs the WGSL source on `wgsl` port and
/// the IR tree on `sdf` port.
#[actor(SdfSceneActor, inports::<10>(sdf, shade), outports::<1>(wgsl, sdf, stats, error), state(MemoryState))]
pub async fn sdf_scene_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();

    // Cache shade WGSL from shader graph compiler
    if let Some(Message::String(s)) = payload.get("shade") {
        context.pool_upsert("_scene", "shade_wgsl", serde_json::json!(s.to_string()));
    }

    // Cache SDF IR when it arrives
    if let Some(sdf_msg) = payload.get("sdf") {
        if let Some(root) = parse_sdf(Some(sdf_msg)) {
            let ir_json = serde_json::to_value(&root).unwrap_or(serde_json::json!(null));
            context.pool_upsert("_scene", "sdf_ir", ir_json);
        }
    }

    // Read cached SDF + shade — proceed only when SDF is available
    let cache: HashMap<String, serde_json::Value> = context.get_pool("_scene").into_iter().collect();
    let root: SdfNode = match cache.get("sdf_ir") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone())
            .map_err(|e| anyhow::anyhow!("SDF IR: {}", e))?,
        _ => return Ok(HashMap::new()),
    };
    let custom_shade: String = cache.get("shade_wgsl")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    eprintln!("[sdf_scene] custom_shade={} bytes", custom_shade.len());

    let settings = SceneSettings {
        width: config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32,
        height: config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32,
        max_steps: config
            .get("maxSteps")
            .and_then(|v| v.as_u64())
            .unwrap_or(128) as u32,
        fov: config.get("fov").and_then(|v| v.as_f64()).unwrap_or(45.0) as f32,
        camera_pos: [
            config
                .get("cameraPosX")
                .and_then(|v| v.as_f64())
                .unwrap_or(3.0) as f32,
            config
                .get("cameraPosY")
                .and_then(|v| v.as_f64())
                .unwrap_or(2.0) as f32,
            config
                .get("cameraPosZ")
                .and_then(|v| v.as_f64())
                .unwrap_or(4.0) as f32,
        ],
        camera_target: [
            config
                .get("cameraTargetX")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
            config
                .get("cameraTargetY")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
            config
                .get("cameraTargetZ")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0) as f32,
        ],
        soft_shadows: config
            .get("softShadows")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        ao: config.get("ao").and_then(|v| v.as_bool()).unwrap_or(true),
        ambient: config
            .get("ambient")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.15) as f32,
        custom_shade_wgsl: custom_shade,
        ..Default::default()
    };

    let scene = root.into_scene_with(settings);
    let compiled = reflow_sdf::codegen::compile(&scene);

    let mut results = HashMap::new();
    let shader_size = compiled.wgsl.len();
    results.insert("wgsl".to_string(), Message::String(compiled.wgsl.into()));
    results.insert(
        "sdf".to_string(),
        Message::object(EncodableValue::from(
            serde_json::to_value(&scene).unwrap_or_default(),
        )),
    );
    results.insert(
        "stats".to_string(),
        Message::object(EncodableValue::from(json!({
            "nodeCount": compiled.node_count,
            "usesNoise": compiled.uses_noise,
            "usesSmoothOps": compiled.uses_smooth_ops,
            "shaderSize": shader_size,
        }))),
    );
    Ok(results)
}
