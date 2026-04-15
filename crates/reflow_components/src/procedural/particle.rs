//! Particle emitter actor.
//!
//! Generates a set of particles with position, velocity, lifetime.
//! Useful for particle systems, point clouds, scatter distribution.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    ParticleEmitterActor,
    inports::<10>(count),
    outports::<1>(particles, metadata),
    state(MemoryState)
)]
pub async fn particle_emitter_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let c = ctx.get_config_hashmap();
    let p = ctx.get_payload();

    let count = match p.get("count") {
        Some(Message::Integer(v)) => *v as usize,
        Some(Message::Float(v)) => *v as usize,
        _ => c.get("count").and_then(|v| v.as_u64()).unwrap_or(1000) as usize,
    };

    let shape = c
        .get("shape")
        .and_then(|v| v.as_str())
        .unwrap_or("sphere")
        .to_string();
    let radius = c.get("radius").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let speed = c.get("speed").and_then(|v| v.as_f64()).unwrap_or(1.0);
    let lifetime = c.get("lifetime").and_then(|v| v.as_f64()).unwrap_or(5.0);
    let seed = c.get("seed").and_then(|v| v.as_f64()).unwrap_or(0.0);

    // Generate particles: [pos.x, pos.y, pos.z, vel.x, vel.y, vel.z, life, age] per particle
    let floats_per_particle = 8;
    let mut data = Vec::with_capacity(count * floats_per_particle);

    for i in 0..count {
        let fi = i as f64;

        // Random position based on shape
        let r1 = reflow_sdf::noise::value_noise(fi * 1.37 + seed, 0.0, 0.0);
        let r2 = reflow_sdf::noise::value_noise(0.0, fi * 2.71 + seed, 0.0);
        let r3 = reflow_sdf::noise::value_noise(0.0, 0.0, fi * std::f64::consts::PI + seed);

        let (px, py, pz) = match shape.as_str() {
            "cube" => (
                (r1 - 0.5) * 2.0 * radius,
                (r2 - 0.5) * 2.0 * radius,
                (r3 - 0.5) * 2.0 * radius,
            ),
            "disc" => {
                let angle = r1 * std::f64::consts::TAU;
                let r = r2.sqrt() * radius;
                (angle.cos() * r, 0.0, angle.sin() * r)
            }
            _ => {
                // Sphere (uniform distribution)
                let theta = r1 * std::f64::consts::TAU;
                let phi = (1.0 - 2.0 * r2).acos();
                let r = r3.cbrt() * radius;
                (
                    r * phi.sin() * theta.cos(),
                    r * phi.cos(),
                    r * phi.sin() * theta.sin(),
                )
            }
        };

        // Random velocity (outward from center)
        let len = (px * px + py * py + pz * pz).sqrt().max(0.001);
        let vx = (px / len) * speed * (0.5 + r1 * 0.5);
        let vy = (py / len) * speed * (0.5 + r2 * 0.5);
        let vz = (pz / len) * speed * (0.5 + r3 * 0.5);

        let life = lifetime * (0.5 + r1 * 0.5);
        let age = 0.0;

        data.extend_from_slice(&[
            px as f32,
            py as f32,
            pz as f32,
            vx as f32,
            vy as f32,
            vz as f32,
            life as f32,
            age as f32,
        ]);
    }

    let bytes: Vec<u8> = data.iter().flat_map(|v| v.to_le_bytes()).collect();

    let mut out = HashMap::new();
    out.insert("particles".to_string(), Message::bytes(bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "count": count,
            "shape": shape,
            "radius": radius,
            "speed": speed,
            "lifetime": lifetime,
            "format": "pos3_vel3_life_age_f32",
            "stride": floats_per_particle * 4,
        }))),
    );
    Ok(out)
}
