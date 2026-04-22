//! Instance actor — spawns a prefab at a specific transform.
//!
//! Takes a prefab reference and transform (position, rotation, scale),
//! outputs a scene object with a unique ID for the scene graph pool.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

#[actor(
    InstanceActor,
    inports::<10>(prefab, position, rotation, scale),
    outports::<10>(object),
    state(MemoryState)
)]
pub async fn instance_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let id = config
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("instance_0")
        .to_string();

    // Prefab reference
    let prefab = match payload.get("prefab") {
        Some(Message::Object(obj)) => {
            let v: serde_json::Value = obj.as_ref().clone().into();
            v
        }
        _ => json!({ "id": "default", "type": "prefab" }),
    };

    // Transform from inports or config
    let pos = match payload.get("position") {
        Some(Message::Object(obj)) => {
            let v: serde_json::Value = obj.as_ref().clone().into();
            [
                v.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                v.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                v.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0),
            ]
        }
        _ => [
            config.get("posX").and_then(|v| v.as_f64()).unwrap_or(0.0),
            config.get("posY").and_then(|v| v.as_f64()).unwrap_or(0.0),
            config.get("posZ").and_then(|v| v.as_f64()).unwrap_or(0.0),
        ],
    };

    let rot = match payload.get("rotation") {
        Some(Message::Object(obj)) => {
            let v: serde_json::Value = obj.as_ref().clone().into();
            [
                v.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                v.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                v.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0),
            ]
        }
        _ => [
            config.get("rotX").and_then(|v| v.as_f64()).unwrap_or(0.0),
            config.get("rotY").and_then(|v| v.as_f64()).unwrap_or(0.0),
            config.get("rotZ").and_then(|v| v.as_f64()).unwrap_or(0.0),
        ],
    };

    let scl = match payload.get("scale") {
        Some(Message::Object(obj)) => {
            let v: serde_json::Value = obj.as_ref().clone().into();
            [
                v.get("x").and_then(|v| v.as_f64()).unwrap_or(1.0),
                v.get("y").and_then(|v| v.as_f64()).unwrap_or(1.0),
                v.get("z").and_then(|v| v.as_f64()).unwrap_or(1.0),
            ]
        }
        _ => {
            let s = config.get("scale").and_then(|v| v.as_f64()).unwrap_or(1.0);
            [s, s, s]
        }
    };

    // Pass material and stride from prefab into the scene object
    let material = prefab.get("material").cloned().unwrap_or(json!({
        "color": [0.8, 0.8, 0.8],
    }));
    let mesh_stride = prefab.get("stride").and_then(|v| v.as_u64()).unwrap_or(24);

    let object = json!({
        "id": id,
        "type": "instance",
        "prefab": prefab.get("id").and_then(|v| v.as_str()).unwrap_or("default"),
        "material": material,
        "meshStride": mesh_stride,
        "transform": {
            "position": pos,
            "rotation": rot,
            "scale": scl,
        },
    });

    Ok([(
        "object".to_string(),
        Message::object(EncodableValue::from(object)),
    )]
    .into())
}
