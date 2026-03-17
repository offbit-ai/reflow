//! Scene camera system — processes all camera components from AssetDB.
//!
//! Iterates every entity with a `:camera` component, computes view/projection
//! matrices based on mode and target, writes results back as `:camera_matrices`
//! components. The renderer then queries `:camera_matrices` to render.
//!
//! ## Component schema: `entity:camera`
//!
//! ```json
//! {
//!   "fov": 60.0,
//!   "near": 0.1,
//!   "far": 1000.0,
//!   "mode": "thirdPerson",
//!   "target": "player",
//!   "distance": 5.0,
//!   "height": 2.0,
//!   "orbitYaw": 0.0,
//!   "orbitPitch": 0.3,
//!   "active": true
//! }
//! ```
//!
//! Modes: "fixed", "firstPerson", "thirdPerson", "orbit"
//!
//! ## Written component: `entity:camera_matrices`
//! ```json
//! { "view": [...16 floats], "projection": [...], "vp": [...], "eye": [x,y,z] }
//! ```

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    SceneCameraSystemActor,
    inports::<10>(tick, camera_tag),
    outports::<1>(active_camera, metadata),
    state(MemoryState)
)]
pub async fn scene_camera_system_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let aspect = config
        .get("aspect")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0) as f32;

    // Camera tag selects which camera to activate — from inport or config
    let camera_tag = match payload.get("camera_tag") {
        Some(Message::String(s)) => Some(s.to_string()),
        _ => config.get("cameraTag").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };

    let db = get_or_create_db(db_path)?;

    let camera_entities = db.entities_with(&["camera"])?;

    let mut active_entity = String::new();
    let mut active_eye = [0.0f32; 3];
    let mut cameras_processed = 0;

    for entity in &camera_entities {
        let cam_asset = match db.get_component(entity, "camera") {
            Ok(a) => a,
            Err(_) => continue,
        };

        let cam: Value = if let Some(ref inline) = cam_asset.entry.inline_data {
            inline.clone()
        } else {
            serde_json::from_slice(&cam_asset.data).unwrap_or(json!({}))
        };

        let fov = cam.get("fov").and_then(|v| v.as_f64()).unwrap_or(60.0) as f32;
        let near = cam.get("near").and_then(|v| v.as_f64()).unwrap_or(0.1) as f32;
        let far = cam.get("far").and_then(|v| v.as_f64()).unwrap_or(1000.0) as f32;
        let mode = cam.get("mode").and_then(|v| v.as_str()).unwrap_or("fixed");
        let is_active = cam.get("active").and_then(|v| v.as_bool()).unwrap_or(true);

        let (eye, target) = compute_camera(mode, &cam, &db);

        let view = look_at(eye, target, [0.0, 1.0, 0.0]);
        let proj = perspective(fov.to_radians(), aspect, near, far);
        let vp = mat4_mul(&proj, &view);

        // Write matrices back as a component
        let _ = db.set_component_json(
            entity,
            "camera_matrices",
            json!({
                "view": view.to_vec(),
                "projection": proj.to_vec(),
                "vp": vp.to_vec(),
                "eye": eye.to_vec(),
                "target": target.to_vec(),
                "active": is_active,
            }),
            json!({}),
        );

        // Active camera: matches tag, or first with active=true
        let tag_match = camera_tag
            .as_ref()
            .map(|tag| {
                db.has(&format!("{}:camera", entity))
                    && db
                        .get_entry(&format!("{}:camera", entity))
                        .ok()
                        .and_then(|e| e.tags.contains(&tag.to_string()).then_some(()))
                        .is_some()
                    || entity == tag
            })
            .unwrap_or(false);

        if active_entity.is_empty() && (tag_match || (camera_tag.is_none() && is_active)) {
            active_entity = entity.clone();
            active_eye = eye;
        }

        cameras_processed += 1;
    }

    // Output the active camera's data for direct DAG wiring
    let mut out = HashMap::new();

    if !active_entity.is_empty() {
        if let Ok(matrices) = db.get_component(&active_entity, "camera_matrices") {
            let v: Value = if let Some(ref inline) = matrices.entry.inline_data {
                inline.clone()
            } else {
                serde_json::from_slice(&matrices.data).unwrap_or(json!({}))
            };
            out.insert(
                "active_camera".to_string(),
                Message::object(EncodableValue::from(v)),
            );
        }
    }

    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "camerasProcessed": cameras_processed,
            "activeCamera": active_entity,
        }))),
    );
    Ok(out)
}

fn compute_camera(
    mode: &str,
    cam: &Value,
    db: &std::sync::Arc<reflow_assets::AssetDB>,
) -> ([f32; 3], [f32; 3]) {
    match mode {
        "thirdPerson" => {
            let target_entity = cam.get("target").and_then(|v| v.as_str()).unwrap_or("player");
            let distance = cam.get("distance").and_then(|v| v.as_f64()).unwrap_or(5.0) as f32;
            let height = cam.get("height").and_then(|v| v.as_f64()).unwrap_or(2.0) as f32;
            let yaw = cam.get("orbitYaw").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let pitch = cam.get("orbitPitch").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
            let target_pos = read_entity_position(db, target_entity);
            let eye = [
                target_pos[0] + distance * yaw.cos() * pitch.cos(),
                target_pos[1] + distance * pitch.sin() + height,
                target_pos[2] + distance * yaw.sin() * pitch.cos(),
            ];
            (eye, target_pos)
        }
        "firstPerson" => {
            let target_entity = cam.get("target").and_then(|v| v.as_str()).unwrap_or("player");
            let eye_offset = read_vec3(cam, "eyeOffset", [0.0, 1.6, 0.0]);
            let look_dir = read_vec3(cam, "lookDirection", [0.0, 0.0, -1.0]);
            let pos = read_entity_position(db, target_entity);
            let eye = [pos[0] + eye_offset[0], pos[1] + eye_offset[1], pos[2] + eye_offset[2]];
            let target = [eye[0] + look_dir[0], eye[1] + look_dir[1], eye[2] + look_dir[2]];
            (eye, target)
        }
        "orbit" => {
            let center = read_vec3(cam, "center", [0.0, 0.0, 0.0]);
            let distance = cam.get("distance").and_then(|v| v.as_f64()).unwrap_or(5.0) as f32;
            let yaw = cam.get("orbitYaw").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
            let pitch = cam.get("orbitPitch").and_then(|v| v.as_f64()).unwrap_or(0.3) as f32;
            let eye = [
                center[0] + distance * yaw.cos() * pitch.cos(),
                center[1] + distance * pitch.sin(),
                center[2] + distance * yaw.sin() * pitch.cos(),
            ];
            (eye, center)
        }
        _ => {
            let eye = read_vec3(cam, "position", [0.0, 5.0, 10.0]);
            let target = read_vec3(cam, "target", [0.0, 0.0, 0.0]);
            (eye, target)
        }
    }
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

fn look_at(eye: [f32; 3], target: [f32; 3], up: [f32; 3]) -> [f32; 16] {
    let f = normalize(sub(target, eye));
    let s = normalize(cross(f, up));
    let u = cross(s, f);
    [
        s[0], u[0], -f[0], 0.0,
        s[1], u[1], -f[1], 0.0,
        s[2], u[2], -f[2], 0.0,
        -dot(s, eye), -dot(u, eye), dot(f, eye), 1.0,
    ]
}

fn perspective(fov_rad: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_rad / 2.0).tan();
    let nf = 1.0 / (near - far);
    [
        f / aspect, 0.0, 0.0, 0.0,
        0.0, f, 0.0, 0.0,
        0.0, 0.0, (far + near) * nf, -1.0,
        0.0, 0.0, 2.0 * far * near * nf, 0.0,
    ]
}

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                out[j * 4 + i] += a[k * 4 + i] * b[j * 4 + k];
            }
        }
    }
    out
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] { [a[0]-b[0], a[1]-b[1], a[2]-b[2]] }
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1]*b[2]-a[2]*b[1], a[2]*b[0]-a[0]*b[2], a[0]*b[1]-a[1]*b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 { a[0]*b[0]+a[1]*b[1]+a[2]*b[2] }
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = dot(v, v).sqrt();
    if l > 1e-6 { [v[0]/l, v[1]/l, v[2]/l] } else { [0.0, 0.0, 1.0] }
}
