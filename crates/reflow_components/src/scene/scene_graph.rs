//! Scene graph actor — collects objects via the input pool and outputs
//! the complete scene tree.
//!
//! Multiple Instance/Terrain actors connect to the `object` inport.
//! Each invocation upserts the incoming object by its `id` into the
//! pool. The actor outputs the full scene on every update.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;

#[actor(
    SceneGraphActor,
    inports::<100>(object),
    outports::<10>(scene, objects, count),
    state(MemoryState)
)]
pub async fn scene_graph_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let scene_name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("scene")
        .to_string();

    // Upsert incoming object into the pool by its ID
    if let Some(Message::Object(obj)) = payload.get("object") {
        let data: serde_json::Value = obj.as_ref().clone().into();
        let id = data
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        ctx.pool_upsert("objects", &id, data);
    }

    // Read the full pool
    let all_objects = ctx.get_pool("objects");
    let count = all_objects.len();

    // If expectedObjects is set, suppress output until pool reaches that count
    let expected = config
        .get("expectedObjects")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as usize;
    if expected > 0 && count < expected {
        return Ok(HashMap::new());
    }

    // Build scene descriptor
    let objects_list: Vec<serde_json::Value> = all_objects.iter().map(|(_, v)| v.clone()).collect();

    let scene = json!({
        "name": scene_name,
        "objectCount": count,
        "objects": objects_list,
    });

    // Also output as array of EncodableValues
    let objects_array: Vec<EncodableValue> = objects_list
        .iter()
        .map(|v| EncodableValue::from(v.clone()))
        .collect();

    let mut out = HashMap::new();
    out.insert(
        "scene".to_string(),
        Message::object(EncodableValue::from(scene)),
    );
    out.insert(
        "objects".to_string(),
        Message::Array(Arc::new(objects_array)),
    );
    out.insert("count".to_string(), Message::Integer(count as i64));
    Ok(out)
}
