//! Material system — collects PBR material components from AssetDB.
//!
//! ## Component schema: `entity:material`
//!
//! ```json
//! {
//!   "albedo": [0.8, 0.2, 0.1],
//!   "metallic": 0.0,
//!   "roughness": 0.5,
//!   "emissive": [0.0, 0.0, 0.0],
//!   "emissiveStrength": 0.0,
//!   "normalScale": 1.0,
//!   "ao": 1.0,
//!   "alphaMode": "opaque",
//!   "alphaCutoff": 0.5,
//!   "doubleSided": false,
//!   "albedoTexture": "wood:texture",
//!   "normalTexture": "wood_normal:texture",
//!   "metallicRoughnessTexture": "wood_mr:texture"
//! }
//! ```
//!
//! Output: packed material array for the renderer. Texture references are
//! asset IDs that the render system resolves.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Packed material for GPU: 64 bytes
/// albedo(12) + metallic(4) + roughness(4) + ao(4) + emissive(12) +
/// emissiveStrength(4) + normalScale(4) + alphaMode(4) + alphaCutoff(4) +
/// doubleSided(4) + pad(8)
const MATERIAL_STRIDE: usize = 64;

#[actor(
    SceneMaterialSystemActor,
    inports::<10>(tick),
    outports::<1>(materials, material_map, metadata),
    state(MemoryState)
)]
pub async fn material_system_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let db = get_or_create_db(db_path)?;

    let material_entities = db.entities_with(&["material"])?;

    let mut material_buffer = Vec::new();
    let mut material_map = serde_json::Map::new();
    let mut material_meta = Vec::new();

    for (idx, entity) in material_entities.iter().enumerate() {
        let mat_asset = match db.get_component(entity, "material") {
            Ok(a) => a,
            Err(_) => continue,
        };

        let mat: Value = if let Some(ref inline) = mat_asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&mat_asset.data).unwrap_or(json!({}))
        };

        let albedo = read_vec3(&mat, "albedo", [1.0, 1.0, 1.0]);
        let metallic = mat.get("metallic").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let roughness = mat.get("roughness").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        let ao = mat.get("ao").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let emissive = read_vec3(&mat, "emissive", [0.0, 0.0, 0.0]);
        let emissive_str = mat.get("emissiveStrength").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let normal_scale = mat.get("normalScale").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let alpha_mode = match mat.get("alphaMode").and_then(|v| v.as_str()).unwrap_or("opaque") {
            "mask" => 1.0f32,
            "blend" => 2.0,
            _ => 0.0, // opaque
        };
        let alpha_cutoff = mat.get("alphaCutoff").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        let double_sided = if mat.get("doubleSided").and_then(|v| v.as_bool()).unwrap_or(false) {
            1.0f32
        } else {
            0.0
        };

        // Pack into buffer
        let mut packed = [0u8; MATERIAL_STRIDE];
        let mut w = |off: usize, val: f32| {
            packed[off..off + 4].copy_from_slice(&val.to_le_bytes());
        };

        w(0, albedo[0]); w(4, albedo[1]); w(8, albedo[2]);
        w(12, metallic);
        w(16, roughness);
        w(20, ao);
        w(24, emissive[0]); w(28, emissive[1]); w(32, emissive[2]);
        w(36, emissive_str);
        w(40, normal_scale);
        w(44, alpha_mode);
        w(48, alpha_cutoff);
        w(52, double_sided);

        material_buffer.extend_from_slice(&packed);

        // Map entity name → material index (for render system)
        material_map.insert(entity.clone(), json!(idx));

        material_meta.push(json!({
            "entity": entity,
            "index": idx,
            "albedo": albedo.to_vec(),
            "metallic": metallic,
            "roughness": roughness,
            "textures": {
                "albedo": mat.get("albedoTexture"),
                "normal": mat.get("normalTexture"),
                "metallicRoughness": mat.get("metallicRoughnessTexture"),
            }
        }));
    }

    let mut out = HashMap::new();
    out.insert("materials".to_string(), Message::bytes(material_buffer));
    out.insert(
        "material_map".to_string(),
        Message::object(EncodableValue::from(Value::Object(material_map))),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "count": material_meta.len(),
            "materials": material_meta,
        }))),
    );
    Ok(out)
}

fn read_vec3(v: &Value, key: &str, default: [f32; 3]) -> [f32; 3] {
    v.get(key)
        .and_then(|a| a.as_array())
        .map(|a| {
            [
                a.get(0).and_then(|v| v.as_f64()).unwrap_or(default[0] as f64) as f32,
                a.get(1).and_then(|v| v.as_f64()).unwrap_or(default[1] as f64) as f32,
                a.get(2).and_then(|v| v.as_f64()).unwrap_or(default[2] as f64) as f32,
            ]
        })
        .unwrap_or(default)
}
