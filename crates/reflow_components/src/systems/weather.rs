//! Weather system — reads :weather components, outputs particle/fog/wind data.
//!
//! Weather is data. The system computes per-frame particle positions,
//! fog density, wind vectors. The renderer reads the output components.
//!
//! ## Component schema: `entity:weather`
//!
//! ```json
//! {
//!   "type": "rain",
//!   "intensity": 0.7,
//!   "windDirection": [0.3, 0.0, 0.1],
//!   "windStrength": 2.0,
//!   "fog": {
//!     "enabled": true,
//!     "density": 0.02,
//!     "color": [0.5, 0.55, 0.6],
//!     "start": 10.0,
//!     "end": 200.0
//!   },
//!   "particleCount": 5000,
//!   "area": [100, 50, 100],
//!   "particleSize": 0.02,
//!   "particleColor": [0.7, 0.75, 0.8, 0.6],
//!   "particleSpeed": 8.0,
//!   "followCamera": true
//! }
//! ```
//!
//! ## Weather types
//!
//! - `"rain"` — vertical streaks, fast, affected by wind
//! - `"snow"` — slow, drifting, slight wobble
//! - `"hail"` — faster than rain, larger particles
//! - `"dust"` — horizontal drift, low opacity
//! - `"fog"` — fog only, no particles
//! - `"sandstorm"` — dense horizontal particles, reduced visibility
//! - `"fireflies"` — slow, glowing, random paths
//! - `"leaves"` — tumbling, wind-driven, larger sprites
//!
//! ## Output
//!
//! Writes `:weather_state` with particle positions and fog params.
//! The renderer uses this for particle rendering + fog post-process.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_assets::get_or_create_db;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

static WEATHER_TICKS: AtomicU64 = AtomicU64::new(0);

#[actor(
    SceneWeatherSystemActor,
    inports::<10>(tick, dt, entity_id),
    outports::<1>(fog, wind, particle_data, metadata),
    state(MemoryState)
)]
pub async fn scene_weather_system_actor(
    ctx: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let db_path = config
        .get("$db")
        .and_then(|v| v.as_str())
        .unwrap_or("./assets.db");
    let dt = match payload.get("dt") {
        Some(Message::Float(f)) => *f as f32,
        _ => config
            .get("dt")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0 / 60.0) as f32,
    };

    let db = get_or_create_db(db_path)?;

    let selected = super::selector::resolve_entities(&payload, &config, &db);
    let weather_entities = if selected.is_empty() {
        db.entities_with(&["weather"])?
    } else {
        selected
            .into_iter()
            .filter(|e| db.has_component(e, "weather"))
            .collect()
    };

    let mut fog_data = json!(null);
    let mut wind_data = json!([0.0, 0.0, 0.0]);
    let mut total_particles = 0u64;

    for entity in &weather_entities {
        let w_asset = match db.get_component(entity, "weather") {
            Ok(a) => a,
            Err(_) => continue,
        };
        let w: Value = w_asset
            .entry
            .inline_data
            .unwrap_or_else(|| serde_json::from_slice(&w_asset.data).unwrap_or(json!({})));

        let weather_type = w.get("type").and_then(|v| v.as_str()).unwrap_or("rain");
        let intensity = w.get("intensity").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        let wind_dir = w
            .get("windDirection")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    fv(a, 0, 0.0) as f32,
                    fv(a, 1, 0.0) as f32,
                    fv(a, 2, 0.0) as f32,
                ]
            })
            .unwrap_or([0.0, 0.0, 0.0]);
        let wind_str = w
            .get("windStrength")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0) as f32;
        let particle_count = w
            .get("particleCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000) as u32;
        let area = w
            .get("area")
            .and_then(|v| v.as_array())
            .map(|a| {
                [
                    fv(a, 0, 50.0) as f32,
                    fv(a, 1, 30.0) as f32,
                    fv(a, 2, 50.0) as f32,
                ]
            })
            .unwrap_or([50.0, 30.0, 50.0]);
        let particle_speed = w
            .get("particleSpeed")
            .and_then(|v| v.as_f64())
            .unwrap_or(5.0) as f32;
        let particle_size = w
            .get("particleSize")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.02) as f32;
        let particle_color = w
            .get("particleColor")
            .and_then(|v| v.as_array())
            .map(|a| [fv(a, 0, 1.0), fv(a, 1, 1.0), fv(a, 2, 1.0), fv(a, 3, 0.5)])
            .unwrap_or([1.0, 1.0, 1.0, 0.5]);

        // Camera position for followCamera
        let cam_pos = if w
            .get("followCamera")
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
        {
            let cam_entity = config
                .get("camera")
                .and_then(|v| v.as_str())
                .unwrap_or("main");
            match db.get_component(cam_entity, "camera_matrices") {
                Ok(a) => {
                    let v: Value = a.entry.inline_data.unwrap_or(json!({}));
                    v.get("eye")
                        .and_then(|e| e.as_array())
                        .map(|a| {
                            [
                                fv(a, 0, 0.0) as f32,
                                fv(a, 1, 0.0) as f32,
                                fv(a, 2, 0.0) as f32,
                            ]
                        })
                        .unwrap_or([0.0, 0.0, 0.0])
                }
                Err(_) => [0.0, 0.0, 0.0],
            }
        } else {
            [0.0, 0.0, 0.0]
        };

        // Compute wind vector
        let wind = [
            wind_dir[0] * wind_str,
            wind_dir[1] * wind_str,
            wind_dir[2] * wind_str,
        ];
        wind_data = json!([wind[0], wind[1], wind[2]]);

        // Weather type parameters
        let (gravity, wobble, stretch) = match weather_type {
            "rain" => (
                [-0.1 * wind[0], -particle_speed, -0.1 * wind[2]],
                0.0f32,
                3.0f32,
            ),
            "snow" => (
                [-0.3 * wind[0], -particle_speed * 0.2, -0.3 * wind[2]],
                0.5,
                1.0,
            ),
            "hail" => (
                [wind[0] * 0.05, -particle_speed * 1.5, wind[2] * 0.05],
                0.0,
                2.0,
            ),
            "dust" => ([wind[0], -0.2, wind[2]], 0.3, 1.0),
            "sandstorm" => ([wind[0] * 2.0, -0.5, wind[2] * 2.0], 0.2, 1.5),
            "fireflies" => ([0.0, 0.1, 0.0], 2.0, 1.0),
            "leaves" => (
                [wind[0] * 0.5, -particle_speed * 0.3, wind[2] * 0.5],
                1.5,
                1.0,
            ),
            _ => ([0.0, -particle_speed, 0.0], 0.0, 1.0),
        };

        // Track elapsed time via tick counter
        let tick = WEATHER_TICKS.fetch_add(1, Ordering::Relaxed);
        let elapsed = tick as f32 * dt;

        // Generate particle positions (deterministic from seed + time)
        let active_count = ((particle_count as f32) * intensity) as u32;
        let mut particles: Vec<[f32; 4]> = Vec::with_capacity(active_count as usize);

        for i in 0..active_count {
            let seed = i as f32 * 1.618033988;
            let hash = (
                (seed * 43758.5453).fract() * 2.0 - 1.0,
                (seed * 22578.1459).fract() * 2.0 - 1.0,
                (seed * 31415.9265).fract() * 2.0 - 1.0,
            );

            // Base position in area around camera
            let base_x = cam_pos[0] + hash.0 * area[0] * 0.5;
            let base_z = cam_pos[2] + hash.2 * area[2] * 0.5;

            // Animated Y: wraps within area height
            let life_offset = (seed * 7919.0).fract();
            let t = (elapsed * 0.5 + life_offset) % 1.0;
            let base_y = cam_pos[1] + area[1] * 0.5 - t * area[1];

            // Apply gravity/wind displacement
            let phase = elapsed + seed * 100.0;
            let wx = gravity[0] * phase + wobble * (phase * 1.3 + seed).sin();
            let wy = gravity[1] * t * 2.0;
            let wz = gravity[2] * phase + wobble * (phase * 0.9 + seed * 2.0).cos();

            particles.push([
                base_x + wx,
                base_y + wy,
                base_z + wz,
                particle_size * stretch, // w = size with stretch factor
            ]);
        }

        // Pack particles as bytes (16 bytes per particle: x,y,z,size)
        let mut particle_bytes = Vec::with_capacity(particles.len() * 16);
        for p in &particles {
            for f in p {
                particle_bytes.extend_from_slice(&f.to_le_bytes());
            }
        }

        // Write weather state component
        let _ = db.set_component_json(
            entity,
            "weather_state",
            json!({
                "type": weather_type,
                "intensity": intensity,
                "activeParticles": active_count,
                "particleSize": particle_size,
                "particleColor": particle_color,
                "stretch": stretch,
                "wind": [wind[0], wind[1], wind[2]],
                "elapsed": elapsed,
            }),
            json!({}),
        );

        // Store particle buffer as binary component
        let _ = db.set_component(
            entity,
            "weather_particles",
            &particle_bytes,
            json!({
                "count": active_count,
                "stride": 16,
            }),
        );

        total_particles += active_count as u64;

        // Fog
        if let Some(fog) = w.get("fog") {
            if fog
                .get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                fog_data = json!({
                    "density": fog.get("density").and_then(|v| v.as_f64()).unwrap_or(0.02),
                    "color": fog.get("color").unwrap_or(&json!([0.5, 0.55, 0.6])),
                    "start": fog.get("start").and_then(|v| v.as_f64()).unwrap_or(10.0),
                    "end": fog.get("end").and_then(|v| v.as_f64()).unwrap_or(200.0),
                });
            }
        }

        // Auto-fog for weather types that imply reduced visibility
        if fog_data.is_null() && matches!(weather_type, "sandstorm" | "dust") {
            fog_data = json!({
                "density": 0.03 * intensity as f64,
                "color": [0.6, 0.55, 0.45],
                "start": 5.0,
                "end": 80.0 / intensity as f64,
            });
        }
    }

    let mut out = HashMap::new();
    if !fog_data.is_null() {
        out.insert(
            "fog".to_string(),
            Message::object(EncodableValue::from(fog_data)),
        );
    }
    out.insert(
        "wind".to_string(),
        Message::object(EncodableValue::from(wind_data)),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "weatherEntities": weather_entities.len(),
            "totalParticles": total_particles,
        }))),
    );
    Ok(out)
}

fn fv(a: &[Value], idx: usize, default: f64) -> f64 {
    a.get(idx).and_then(|v| v.as_f64()).unwrap_or(default)
}
