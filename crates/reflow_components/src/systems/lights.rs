//! Light collector system — gathers all light components from AssetDB.
//!
//! ## Component schema: `entity:light`
//!
//! ```json
//! {
//!   "type": "directional",
//!   "color": [1.0, 1.0, 0.9],
//!   "intensity": 2.0,
//!   "direction": [0.0, -1.0, 0.5],
//!   "castShadow": true,
//!   "shadowBias": 0.005
//! }
//! ```
//!
//! Light types: "directional", "point", "spot", "ambient"
//!
//! Point lights read position from the entity's :transform component.
//! Spot lights additionally use "innerAngle" and "outerAngle".
//!
//! Output: packed light array for the renderer (up to 16 lights).

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

const MAX_LIGHTS: usize = 16;

/// Packed light for GPU uniform buffer.
/// 64 bytes per light: position(12) + type(4) + direction(12) + intensity(4) +
///                     color(12) + range(4) + angles(8) + shadow(4) + pad(4)
const LIGHT_STRIDE: usize = 64;

#[actor(
    SceneLightCollectorActor,
    inports::<10>(tick, entity_id),
    outports::<1>(lights, light_count, metadata),
    state(MemoryState)
)]
pub async fn light_collector_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");

    let db = get_or_create_db(db_path)?;

    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let light_entities = if selected.is_empty() {
        db.entities_with(&["light"])?
    } else {
        selected.into_iter().filter(|e| db.has_component(e, "light")).collect()
    };

    let mut light_buffer = vec![0u8; MAX_LIGHTS * LIGHT_STRIDE];
    let mut light_count = 0usize;
    let mut light_meta = Vec::new();

    for entity in &light_entities {
        if light_count >= MAX_LIGHTS {
            break;
        }

        let light_asset = match db.get_component(entity, "light") {
            Ok(a) => a,
            Err(_) => continue,
        };

        let light: Value = if let Some(ref inline) = light_asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&light_asset.data).unwrap_or(json!({}))
        };

        let light_type = light.get("type").and_then(|v| v.as_str()).unwrap_or("point");
        let color = read_vec3(&light, "color", [1.0, 1.0, 1.0]);
        let intensity = light.get("intensity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let cast_shadow = light.get("castShadow").and_then(|v| v.as_bool()).unwrap_or(false);

        let type_id: f32 = match light_type {
            "directional" => 0.0,
            "point" => 1.0,
            "spot" => 2.0,
            "ambient" => 3.0,
            _ => 1.0,
        };

        // Position from transform component (for point/spot lights)
        let position = if light_type == "point" || light_type == "spot" {
            read_entity_position(&db, entity)
        } else {
            [0.0; 3]
        };

        // Direction (for directional/spot)
        let direction = read_vec3(&light, "direction", [0.0, -1.0, 0.0]);
        let range = light.get("range").and_then(|v| v.as_f64()).unwrap_or(20.0) as f32;
        let inner_angle = light.get("innerAngle").and_then(|v| v.as_f64()).unwrap_or(30.0) as f32;
        let outer_angle = light.get("outerAngle").and_then(|v| v.as_f64()).unwrap_or(45.0) as f32;

        // Pack into buffer
        let offset = light_count * LIGHT_STRIDE;
        let mut write = |off: usize, val: f32| {
            light_buffer[offset + off..offset + off + 4]
                .copy_from_slice(&val.to_le_bytes());
        };

        // position (0..12)
        write(0, position[0]);
        write(4, position[1]);
        write(8, position[2]);
        // type (12..16)
        write(12, type_id);
        // direction (16..28)
        write(16, direction[0]);
        write(20, direction[1]);
        write(24, direction[2]);
        // intensity (28..32)
        write(28, intensity);
        // color (32..44)
        write(32, color[0]);
        write(36, color[1]);
        write(40, color[2]);
        // range (44..48)
        write(44, range);
        // angles (48..56)
        write(48, inner_angle.to_radians().cos());
        write(52, outer_angle.to_radians().cos());
        // shadow flag (56..60)
        write(56, if cast_shadow { 1.0 } else { 0.0 });
        // pad (60..64)
        write(60, 0.0);

        light_meta.push(json!({
            "entity": entity,
            "type": light_type,
            "color": color.to_vec(),
            "intensity": intensity,
            "castShadow": cast_shadow,
        }));

        light_count += 1;
    }

    // Trim buffer to actual count
    light_buffer.truncate(light_count * LIGHT_STRIDE);

    let mut out = HashMap::new();
    out.insert("lights".to_string(), Message::bytes(light_buffer));
    out.insert("light_count".to_string(), Message::Integer(light_count as i64));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "count": light_count,
            "lights": light_meta,
        }))),
    );
    Ok(out)
}

fn read_entity_position(db: &std::sync::Arc<reflow_assets::AssetDB>, entity: &str) -> [f32; 3] {
    match db.get_component(entity, "transform") {
        Ok(asset) => {
            let v: Value = if let Some(ref inline) = asset.entry.inline_data {
                inline.clone()
            } else {
                serde_json::from_slice(&asset.data).unwrap_or(json!({}))
            };
            read_vec3(&v, "position", [0.0, 0.0, 0.0])
        }
        Err(_) => [0.0, 0.0, 0.0],
    }
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
