//! Billboard system — generates camera-facing quads from :billboard components.
//!
//! ## Component schema: `entity:billboard`
//!
//! ```json
//! {
//!   "mode": "world",
//!   "sprite": "pine_tree:texture",
//!   "width": 3.0,
//!   "height": 5.0,
//!   "anchor": [0.5, 0.0],
//!   "opacity": 1.0,
//!   "tint": [1.0, 1.0, 1.0],
//!   "flipX": false,
//!   "atlas": { "cols": 4, "rows": 4, "frame": 0 }
//! }
//! ```
//!
//! ## Modes
//!
//! - `"screen"` — always faces camera, fixed pixel size (HUD, name tags)
//! - `"world"` — always faces camera, scales with distance (signs, speech bubbles)
//! - `"axisY"` — rotates around Y only (trees, grass, vegetation)
//! - `"velocity"` — aligns to entity's velocity vector (sparks, rain, tracers)
//!
//! ## Text billboards
//!
//! If `text` is set instead of `sprite`, renders as a text glyph:
//!
//! ```json
//! {
//!   "mode": "screen",
//!   "text": "PlayerOne",
//!   "fontSize": 14,
//!   "color": [1, 1, 1],
//!   "offset": [0, 2.2, 0],
//!   "target": "player",
//!   "background": [0, 0, 0, 0.5]
//! }
//! ```
//!
//! ## Output
//!
//! Writes `:billboard_quad` component per entity with computed vertices
//! ready for the renderer. The render system collects these for batched
//! alpha-blended rendering.

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    SceneBillboardSystemActor,
    inports::<10>(tick, entity_id, camera_position),
    outports::<1>(quads, metadata),
    state(MemoryState)
)]
pub async fn scene_billboard_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let db = get_or_create_db(db_path)?;

    // Camera position (from CameraSystem output or config)
    let cam_pos = match payload.get("camera_position") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            read_vec3(&v, [0.0, 5.0, 10.0])
        }
        _ => {
            // Try reading from active camera component
            let cam_entity = config
                .get("camera")
                .and_then(|v| v.as_str())
                .unwrap_or("main");
            match db.get_component(cam_entity, "camera_matrices") {
                Ok(a) => {
                    let v: Value = a.entry.inline_data.unwrap_or(json!({}));
                    read_vec3_field(&v, "eye", [0.0, 5.0, 10.0])
                }
                Err(_) => [0.0, 5.0, 10.0],
            }
        }
    };

    // Entity selection
    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let billboard_entities = if selected.is_empty() {
        db.entities_with(&["billboard"])?
    } else {
        selected
            .into_iter()
            .filter(|e| db.has_component(e, "billboard"))
            .collect()
    };

    let mut quad_data: Vec<Value> = Vec::new();
    let mut processed = 0;

    for entity in &billboard_entities {
        let bb_asset = match db.get_component(entity, "billboard") {
            Ok(a) => a,
            Err(_) => continue,
        };
        let bb: Value = bb_asset
            .entry
            .inline_data
            .unwrap_or_else(|| serde_json::from_slice(&bb_asset.data).unwrap_or(json!({})));

        let mode = bb.get("mode").and_then(|v| v.as_str()).unwrap_or("world");
        let width = bb.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let height = bb.get("height").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let anchor = bb
            .get("anchor")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.first().and_then(|v| v.as_f64()).unwrap_or(0.5) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(0.5) as f32,
                ]
            })
            .unwrap_or([0.5, 0.5]);
        let opacity = bb.get("opacity").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;
        let tint = bb
            .get("tint")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    a.first().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                    a.get(2).and_then(|v| v.as_f64()).unwrap_or(1.0) as f32,
                ]
            })
            .unwrap_or([1.0, 1.0, 1.0]);
        let offset = read_vec3_field(&bb, "offset", [0.0, 0.0, 0.0]);

        // Entity world position (from transform or target entity)
        let world_pos = if let Some(target) = bb.get("target").and_then(|v| v.as_str()) {
            read_entity_position(&db, target)
        } else {
            read_entity_position(&db, entity)
        };

        let pos = [
            world_pos[0] + offset[0],
            world_pos[1] + offset[1],
            world_pos[2] + offset[2],
        ];

        // Compute billboard orientation
        let (right, up) = compute_billboard_axes(mode, pos, cam_pos, &db, entity);

        // Compute quad corners (anchor-adjusted)
        let hw = width * 0.5;
        let hh = height * 0.5;
        let ax = anchor[0] - 0.5; // -0.5 to 0.5
        let ay = anchor[1] - 0.5;

        let corners = [
            // bottom-left, bottom-right, top-right, top-left
            vadd(
                pos,
                vadd(
                    vscale(right, -hw - ax * width),
                    vscale(up, -hh - ay * height),
                ),
            ),
            vadd(
                pos,
                vadd(
                    vscale(right, hw - ax * width),
                    vscale(up, -hh - ay * height),
                ),
            ),
            vadd(
                pos,
                vadd(vscale(right, hw - ax * width), vscale(up, hh - ay * height)),
            ),
            vadd(
                pos,
                vadd(
                    vscale(right, -hw - ax * width),
                    vscale(up, hh - ay * height),
                ),
            ),
        ];

        // Sprite atlas UVs
        let (uv_min, uv_max) = if let Some(atlas) = bb.get("atlas") {
            let cols = atlas.get("cols").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let rows = atlas.get("rows").and_then(|v| v.as_u64()).unwrap_or(1) as u32;
            let frame = atlas.get("frame").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let col = frame % cols;
            let row = frame / cols;
            let u0 = col as f32 / cols as f32;
            let v0 = row as f32 / rows as f32;
            let u1 = (col + 1) as f32 / cols as f32;
            let v1 = (row + 1) as f32 / rows as f32;
            ([u0, v0], [u1, v1])
        } else {
            ([0.0f32, 0.0], [1.0, 1.0])
        };

        let flip_x = bb.get("flipX").and_then(|v| v.as_bool()).unwrap_or(false);
        let uv_l = if flip_x { uv_max[0] } else { uv_min[0] };
        let uv_r = if flip_x { uv_min[0] } else { uv_max[0] };

        // Write computed quad back as a component for the renderer
        let quad = json!({
            "entity": entity,
            "mode": mode,
            "corners": [
                corners[0].to_vec(), corners[1].to_vec(),
                corners[2].to_vec(), corners[3].to_vec(),
            ],
            "uvs": [
                [uv_l, uv_max[1]], [uv_r, uv_max[1]],
                [uv_r, uv_min[1]], [uv_l, uv_min[1]],
            ],
            "opacity": opacity,
            "tint": tint.to_vec(),
            "sprite": bb.get("sprite"),
            "text": bb.get("text"),
            "fontSize": bb.get("fontSize"),
            "color": bb.get("color"),
            "background": bb.get("background"),
            "distance": vlen(vsub(pos, cam_pos)),
        });

        // Quads are ephemeral — flow through DAG outport, not persisted.
        quad_data.push(quad);
        processed += 1;
    }

    // Sort back-to-front for alpha blending
    quad_data.sort_by(|a, b| {
        let da = a["distance"].as_f64().unwrap_or(0.0);
        let db_val = b["distance"].as_f64().unwrap_or(0.0);
        db_val.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out = HashMap::new();
    out.insert(
        "quads".to_string(),
        Message::object(EncodableValue::from(json!(quad_data))),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "billboardCount": processed,
        }))),
    );
    Ok(out)
}

fn compute_billboard_axes(
    mode: &str,
    pos: [f32; 3],
    cam_pos: [f32; 3],
    db: &std::sync::Arc<reflow_assets::AssetDB>,
    entity: &str,
) -> ([f32; 3], [f32; 3]) {
    let world_up = [0.0f32, 1.0, 0.0];

    match mode {
        "screen" | "world" => {
            // Full camera-facing
            let forward = vnorm(vsub(cam_pos, pos));
            let right = vnorm(vcross(world_up, forward));
            let up = vcross(forward, right);
            (right, up)
        }
        "axisY" => {
            // Y-axis constrained — only rotates around Y
            let to_cam = vsub(cam_pos, pos);
            let forward = vnorm([to_cam[0], 0.0, to_cam[2]]);
            let right = vnorm(vcross(world_up, forward));
            (right, world_up)
        }
        "velocity" => {
            // Align to velocity direction
            let vel = match db.get_component(entity, "velocity") {
                Ok(a) => {
                    let v: Value = a.entry.inline_data.unwrap_or(json!({}));
                    read_vec3_field(&v, "linear", [0.0, 0.0, -1.0])
                }
                Err(_) => [0.0, 0.0, -1.0],
            };
            let forward = vnorm(vel);
            let right = vnorm(vcross(world_up, forward));
            let up = vcross(forward, right);
            (right, up)
        }
        _ => {
            let forward = vnorm(vsub(cam_pos, pos));
            let right = vnorm(vcross(world_up, forward));
            let up = vcross(forward, right);
            (right, up)
        }
    }
}

fn read_entity_position(db: &std::sync::Arc<reflow_assets::AssetDB>, entity: &str) -> [f32; 3] {
    match db.get_component(entity, "transform") {
        Ok(a) => {
            let v: Value = a
                .entry
                .inline_data
                .unwrap_or_else(|| serde_json::from_slice(&a.data).unwrap_or(json!({})));
            read_vec3_field(&v, "position", [0.0, 0.0, 0.0])
        }
        Err(_) => [0.0, 0.0, 0.0],
    }
}

fn read_vec3(v: &Value, default: [f32; 3]) -> [f32; 3] {
    v.as_array()
        .map(|a| {
            [
                a.first()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(default[0] as f64) as f32,
                a.get(1)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(default[1] as f64) as f32,
                a.get(2)
                    .and_then(|v| v.as_f64())
                    .unwrap_or(default[2] as f64) as f32,
            ]
        })
        .unwrap_or(default)
}

fn read_vec3_field(v: &Value, key: &str, default: [f32; 3]) -> [f32; 3] {
    v.get(key).map(|v| read_vec3(v, default)).unwrap_or(default)
}

fn vsub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn vadd(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn vscale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn vdot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn vlen(a: [f32; 3]) -> f32 {
    vdot(a, a).sqrt()
}
fn vcross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn vnorm(v: [f32; 3]) -> [f32; 3] {
    let l = vlen(v);
    if l > 1e-6 {
        [v[0] / l, v[1] / l, v[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}
