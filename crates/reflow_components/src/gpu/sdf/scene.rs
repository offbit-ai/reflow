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

fn parse_shade_packet(msg: &Message) -> Option<(String, String, String)> {
    let Message::Object(value) = msg else {
        return None;
    };
    let json: serde_json::Value = value.as_ref().clone().into();
    let slot = json.get("slot")?.as_str()?.to_string();
    let wgsl = json.get("wgsl")?.as_str()?.to_string();
    let function_name = json.get("functionName")?.as_str()?.to_string();
    Some((slot, function_name, wgsl))
}

fn split_slot_shade_wgsl<'a>(wgsl: &'a str, function_name: &str) -> Option<(&'a str, &'a str)> {
    let probe_marker = format!("fn {function_name}_probe(");
    let fn_marker = format!("fn {function_name}(");
    let idx = match (wgsl.find(&probe_marker), wgsl.find(&fn_marker)) {
        (Some(probe_idx), Some(fn_idx)) => probe_idx.min(fn_idx),
        (Some(probe_idx), None) => probe_idx,
        (None, Some(fn_idx)) => fn_idx,
        (None, None) => return None,
    };
    Some(wgsl.split_at(idx))
}

fn slot_prelude_uses_noise(prelude: &str) -> bool {
    prelude.contains("fn shade_noise3d(")
        || prelude.contains("fn shade_fbm_noise(")
        || prelude.contains("fn shade_voronoi_noise(")
}

fn build_slot_dispatch_wgsl(
    slots: &[String],
    shade_packets: &HashMap<String, (String, String)>,
) -> Option<String> {
    if slots.is_empty() {
        return None;
    }

    let parsed_slots: Vec<(&String, &String, String, &str, &str)> = slots
        .iter()
        .map(|slot| {
            let (function_name, slot_wgsl) = shade_packets.get(slot)?;
            let (prelude, function_wgsl) = split_slot_shade_wgsl(slot_wgsl, function_name)?;
            Some((
                slot,
                function_name,
                format!("{function_name}_probe"),
                prelude,
                function_wgsl,
            ))
        })
        .collect::<Option<Vec<_>>>()?;

    let selected_prelude = parsed_slots
        .iter()
        .find(|(_, _, _, prelude, _)| slot_prelude_uses_noise(prelude))
        .map(|(_, _, _, prelude, _)| *prelude)
        .or_else(|| parsed_slots.first().map(|(_, _, _, prelude, _)| *prelude))?;

    let mut wgsl = String::new();
    wgsl.push_str(selected_prelude);
    if !selected_prelude.ends_with('\n') {
        wgsl.push('\n');
    }
    for (_, _, _, _, function_wgsl) in &parsed_slots {
        wgsl.push_str(function_wgsl);
        if !function_wgsl.ends_with('\n') {
            wgsl.push('\n');
        }
    }

    wgsl.push_str("fn shade_probe(ro: vec3f, rd: vec3f, t: f32) -> vec3f {\n");
    wgsl.push_str("  let p = ro + rd * t;\n");
    wgsl.push_str("  let material_id = sdf_material_id(p);\n");
    for (idx, (_, _, probe_name, _, _)) in parsed_slots.iter().enumerate() {
        if idx == 0 {
            wgsl.push_str(&format!(
                "  if material_id == {idx} {{ return {probe_name}(ro, rd, t); }}\n"
            ));
        } else {
            wgsl.push_str(&format!(
                "  else if material_id == {idx} {{ return {probe_name}(ro, rd, t); }}\n"
            ));
        }
    }
    let probe_fallback = parsed_slots
        .first()
        .map(|(_, _, probe_name, _, _)| probe_name.clone())?;
    wgsl.push_str(&format!("  return {probe_fallback}(ro, rd, t);\n"));
    wgsl.push_str("}\n");

    wgsl.push_str("fn shade(ro: vec3f, rd: vec3f, t: f32) -> vec3f {\n");
    wgsl.push_str("  let p = ro + rd * t;\n");
    wgsl.push_str("  let material_id = sdf_material_id(p);\n");
    for (idx, (_, function_name, _, _, _)) in parsed_slots.iter().enumerate() {
        if idx == 0 {
            wgsl.push_str(&format!(
                "  if material_id == {idx} {{ return {function_name}(ro, rd, t); }}\n"
            ));
        } else {
            wgsl.push_str(&format!(
                "  else if material_id == {idx} {{ return {function_name}(ro, rd, t); }}\n"
            ));
        }
    }
    let fallback = parsed_slots
        .first()
        .map(|(_, function_name, _, _, _)| (*function_name).clone())?;
    wgsl.push_str(&format!("  return {fallback}(ro, rd, t);\n"));
    wgsl.push_str("}\n");
    Some(wgsl)
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
    let slot = config
        .get("slot")
        .and_then(|v| v.as_str())
        .map(ToOwned::to_owned);

    let mat = SdfMaterial {
        slot,
        color: [r, g, b],
        roughness,
        metallic,
        ..Default::default()
    };

    Ok(sdf_output(&child.with_material(mat)))
}

#[actor(SdfShadeSlotActor, inports::<10>(shade), outports::<1>(shade, error), state(MemoryState))]
pub async fn sdf_shade_slot_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let slot = config
        .get("slot")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing slot config"))?;
    let function_name = config
        .get("functionName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing functionName config"))?;
    let shade = match payload.get("shade") {
        Some(Message::String(s)) => s.to_string(),
        _ => return Ok(HashMap::new()),
    };

    let mut out = HashMap::new();
    out.insert(
        "shade".to_string(),
        Message::object(EncodableValue::from(json!({
            "slot": slot,
            "functionName": function_name,
            "wgsl": shade,
        }))),
    );
    Ok(out)
}

// ── Scene (Compose + Compile) ───────────────────────────────────

/// Takes a root SDF and scene config, wraps into Scene node, and
/// compiles to WGSL. Outputs the WGSL source on `wgsl` port and
/// the IR tree on `sdf` port.
#[actor(SdfSceneActor, inports::<10>(sdf, shade), outports::<1>(wgsl, sdf, stats, error), state(MemoryState))]
pub async fn sdf_scene_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let log_progress = config
        .get("logProgress")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let require_shade = config
        .get("requireShade")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // Cache shade WGSL from shader graph compiler
    if let Some(Message::String(s)) = payload.get("shade") {
        context.pool_upsert("_scene", "shade_wgsl", serde_json::json!(s.to_string()));
    } else if let Some(shade_msg) = payload.get("shade") {
        if let Some((slot, function_name, wgsl)) = parse_shade_packet(shade_msg) {
            context.pool_upsert(
                "_scene",
                &format!("shade_slot::{slot}"),
                serde_json::json!({
                    "functionName": function_name,
                    "wgsl": wgsl,
                }),
            );
        }
    }

    // Cache SDF IR when it arrives
    if let Some(sdf_msg) = payload.get("sdf") {
        if let Some(root) = parse_sdf(Some(sdf_msg)) {
            let ir_json = serde_json::to_value(&root).unwrap_or(serde_json::json!(null));
            context.pool_upsert("_scene", "sdf_ir", ir_json);
        }
    }

    // Read cached SDF + shade — proceed only when SDF is available
    let cache: HashMap<String, serde_json::Value> =
        context.get_pool("_scene").into_iter().collect();
    let root: SdfNode = match cache.get("sdf_ir") {
        Some(v) if !v.is_null() => {
            serde_json::from_value(v.clone()).map_err(|e| anyhow::anyhow!("SDF IR: {}", e))?
        }
        _ => return Ok(HashMap::new()),
    };
    let slot_names = reflow_sdf::codegen::collect_material_slots(&root);
    let slot_shades: HashMap<String, (String, String)> = slot_names
        .iter()
        .filter_map(|slot| {
            let key = format!("shade_slot::{slot}");
            let value = cache.get(&key)?;
            let function_name = value.get("functionName")?.as_str()?.to_string();
            let wgsl = value.get("wgsl")?.as_str()?.to_string();
            Some((slot.clone(), (function_name, wgsl)))
        })
        .collect();

    let custom_shade = if slot_names.is_empty() {
        cache
            .get("shade_wgsl")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    } else {
        if slot_shades.len() != slot_names.len() {
            if log_progress {
                let missing: Vec<String> = slot_names
                    .iter()
                    .filter(|slot| !slot_shades.contains_key(*slot))
                    .cloned()
                    .collect();
                let missing_json = serde_json::to_string(&missing).unwrap_or_default();
                let last_missing = cache
                    .get("waiting_slots")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if last_missing != missing_json {
                    eprintln!("[sdf_scene] waiting for shade slots: {:?}", missing);
                    context.pool_upsert("_scene", "waiting_slots", serde_json::json!(missing_json));
                }
            }
            return Ok(HashMap::new());
        }
        if log_progress && cache.get("waiting_slots").is_some() {
            eprintln!("[sdf_scene] all shade slots ready");
            context.pool_upsert("_scene", "waiting_slots", serde_json::Value::Null);
        }
        build_slot_dispatch_wgsl(&slot_names, &slot_shades).unwrap_or_default()
    };
    if require_shade && custom_shade.is_empty() {
        return Ok(HashMap::new());
    }
    let shade_len = custom_shade.len() as u64;
    let last_shade_len = cache.get("shade_len").and_then(|v| v.as_u64());
    if last_shade_len != Some(shade_len) {
        eprintln!("[sdf_scene] custom_shade={} bytes", shade_len);
        context.pool_upsert("_scene", "shade_len", serde_json::json!(shade_len));
    }

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
        shadow_k: config
            .get("shadowK")
            .and_then(|v| v.as_f64())
            .unwrap_or(32.0) as f32,
        light_dir: config
            .get("lightDir")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.577) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.577) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(-0.577) as f32,
                ]
            })
            .unwrap_or([0.577, 0.577, -0.577]),
        light_color: config
            .get("lightColor")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.get(0).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                ]
            })
            .unwrap_or([1.0, 1.0, 1.0]),
        background: config
            .get("background")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.get(0).and_then(|v| v.as_f64()).unwrap_or(0.1) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.1) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(0.15) as f32,
                ]
            })
            .unwrap_or([0.1, 0.1, 0.15]),
        time: config.get("time").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
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

#[cfg(test)]
mod tests {
    use super::build_slot_dispatch_wgsl;
    use reflow_shader::{
        codegen::compile_sdf_shade_named,
        ir::ShaderNode::{ConstFloat, ConstVec3, MaterialOutput, PrincipledBsdf},
    };
    use std::collections::HashMap;

    fn test_material(alpha: f32, transmission: f32, ior: f32) -> reflow_shader::ir::ShaderNode {
        MaterialOutput {
            surface: Box::new(PrincipledBsdf {
                base_color: Box::new(ConstVec3 {
                    c: [0.9, 0.97, 1.0],
                }),
                metallic: Box::new(ConstFloat { c: 0.0 }),
                roughness: Box::new(ConstFloat { c: 0.01 }),
                normal: None,
                emission: Box::new(ConstVec3 { c: [0.0, 0.0, 0.0] }),
                emission_strength: Box::new(ConstFloat { c: 0.0 }),
                ao: None,
                alpha: Box::new(ConstFloat { c: alpha }),
                subsurface: None,
                subsurface_color: None,
                clearcoat: None,
                clearcoat_roughness: None,
                anisotropic: None,
                anisotropic_rotation: None,
                sheen: None,
                sheen_tint: None,
                transmission: Some(Box::new(ConstFloat { c: transmission })),
                ior: Some(Box::new(ConstFloat { c: ior })),
            }),
        }
    }

    #[test]
    fn slot_dispatch_wgsl_deduplicates_shared_helpers() {
        let slot_a = compile_sdf_shade_named(&test_material(0.02, 0.985, 1.309), "shade_slot_a");
        let slot_b = compile_sdf_shade_named(&test_material(0.002, 0.998, 1.333), "shade_slot_b");

        let mut packets = HashMap::new();
        packets.insert("slot_a".to_string(), ("shade_slot_a".to_string(), slot_a));
        packets.insert("slot_b".to_string(), ("shade_slot_b".to_string(), slot_b));

        let merged =
            build_slot_dispatch_wgsl(&["slot_a".to_string(), "slot_b".to_string()], &packets)
                .expect("merged wgsl");

        assert_eq!(merged.matches("fn D_GGX_sdf(").count(), 1);
        assert_eq!(merged.matches("fn F_Schlick_sdf(").count(), 1);
        assert_eq!(merged.matches("fn pbr_shade_sdf(").count(), 1);
        assert_eq!(merged.matches("fn shade_slot_a_probe(").count(), 1);
        assert_eq!(merged.matches("fn shade_slot_b_probe(").count(), 1);
        assert_eq!(merged.matches("fn shade_slot_a(").count(), 1);
        assert_eq!(merged.matches("fn shade_slot_b(").count(), 1);
        assert_eq!(merged.matches("fn shade_probe(").count(), 1);
        assert_eq!(merged.matches("fn shade(").count(), 1);
    }
}
