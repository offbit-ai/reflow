//! Animation clip actor — defines keyframed animation tracks per bone.
//!
//! Supports both explicit channel arrays (backward compatible) and high-level
//! generator types that produce keyframes from simple config parameters.
//!
//! ## Generator types
//!
//! | Type | Use case | Key params |
//! |------|----------|------------|
//! | `wave` | Snake, fish, tentacle, flag | speed, amplitude, waveLength, rotationAxis |
//! | `bounce` | Jump, walk bob, ball physics | height, gravity, restitution |
//! | `twist` | Drill, tornado, rope coil | speed, maxAngle, progressive |
//! | `pendulum` | Chain, rope, chandelier | length, damping, initialAngle |
//! | `spring` | Jiggle, wobble, settling | stiffness, damping, initialDisplacement |
//! | `breathe` | Idle chest, pulsing, heartbeat | scaleAmount, rate |
//! | `sway` | Tree, grass, hair in wind | windStrength, windFrequency, turbulence |
//! | `orbit` | Planet, carousel, spinning top | radius, speed, axis, tilt |
//! | `spiral` | DNA helix, rising smoke, screw | radius, rise, speed |
//! | `figure8` | Juggling, figure skating, Lissajous | scaleX, scaleY, speed |
//! | `tremble` | Fear, cold shiver, engine vibration | intensity, frequency |
//!
//! Sprite animation uses [`SpriteAnimationActor`] — separate from the bone pipeline.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::f64::consts::PI;

#[actor(
    AnimationClipActor,
    inports::<10>(clip_data),
    outports::<1>(clip, metadata),
    state(MemoryState)
)]
pub async fn animation_clip_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let name = config
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("clip")
        .to_string();

    let duration = config
        .get("duration")
        .and_then(|v| v.as_f64())
        .unwrap_or(1.0);

    let fps = config.get("fps").and_then(|v| v.as_u64()).unwrap_or(30) as u32;

    // High-level generator config OR explicit channels (backward compatible)
    let validated: Vec<Value> = if let Some(gen_type) = config.get("type").and_then(|v| v.as_str())
    {
        let bone_count = config
            .get("boneCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(1) as usize;

        let ctx = GenContext {
            config: &config,
            bone_count,
            duration,
            fps,
        };

        match gen_type {
            "wave" => generate_wave(&ctx),
            "bounce" => generate_bounce(&ctx),
            "twist" => generate_twist(&ctx),
            "pendulum" => generate_pendulum(&ctx),
            "spring" => generate_spring(&ctx),
            "breathe" => generate_breathe(&ctx),
            "sway" => generate_sway(&ctx),
            "orbit" => generate_orbit(&ctx),
            "spiral" => generate_spiral(&ctx),
            "figure8" => generate_figure8(&ctx),
            "tremble" => generate_tremble(&ctx),
            _ => {
                return Err(anyhow::anyhow!("unknown clip type: {}", gen_type));
            }
        }
    } else {
        // Explicit channels from config or inport
        let channels = if let Some(Message::Object(obj)) = payload.get("clip_data") {
            let v: Value = obj.as_ref().clone().into();
            v.get("channels").cloned().unwrap_or_else(|| json!([]))
        } else {
            config.get("channels").cloned().unwrap_or_else(|| json!([]))
        };

        let channel_list = channels.as_array().cloned().unwrap_or_default();
        let mut result = Vec::new();
        for ch in &channel_list {
            let bone_index = ch.get("boneIndex").and_then(|v| v.as_u64()).unwrap_or(0);
            let property = ch
                .get("property")
                .and_then(|v| v.as_str())
                .unwrap_or("rotation")
                .to_string();
            let interpolation = ch
                .get("interpolation")
                .and_then(|v| v.as_str())
                .unwrap_or("linear")
                .to_string();
            let times = ch.get("times").cloned().unwrap_or_else(|| json!([]));
            let values = ch.get("values").cloned().unwrap_or_else(|| json!([]));

            result.push(json!({
                "boneIndex": bone_index,
                "property": property,
                "interpolation": interpolation,
                "times": times,
                "values": values,
            }));
        }
        result
    };

    let clip = json!({
        "name": name,
        "duration": duration,
        "channelCount": validated.len(),
        "channels": validated,
    });

    let mut out = HashMap::new();
    out.insert(
        "clip".to_string(),
        Message::object(EncodableValue::from(clip)),
    );
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "name": name,
            "duration": duration,
            "channelCount": validated.len(),
        }))),
    );
    Ok(out)
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared helpers
// ═══════════════════════════════════════════════════════════════════════════

struct GenContext<'a> {
    config: &'a HashMap<String, Value>,
    bone_count: usize,
    duration: f64,
    fps: u32,
}

impl<'a> GenContext<'a> {
    fn frame_count(&self) -> usize {
        (self.fps as f64 * self.duration) as usize
    }

    fn times(&self) -> Vec<f64> {
        let n = self.frame_count();
        (0..=n).map(|i| i as f64 / self.fps as f64).collect()
    }

    fn f(&self, key: &str, default: f64) -> f64 {
        self.config
            .get(key)
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    }

    fn s<'b>(&'b self, key: &str, default: &'b str) -> &'b str
    where
        'a: 'b,
    {
        self.config
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
    }

    /// Normalized bone position [0..1] along the chain
    fn bone_t(&self, bone_idx: usize) -> f64 {
        if self.bone_count > 1 {
            bone_idx as f64 / (self.bone_count - 1) as f64
        } else {
            0.5
        }
    }

    /// Amplitude envelope based on amplitudeCurve config
    fn amplitude_envelope(&self, bone_idx: usize, base_amp: f64) -> f64 {
        let t = self.bone_t(bone_idx);
        let curve = self.s("amplitudeCurve", "uniform");
        base_amp
            * match curve {
                "tailHeavy" => 0.5 + 0.5 * t,
                "headHeavy" => 1.0 - 0.5 * t,
                "bellCurve" => 1.0 - (2.0 * t - 1.0).powi(2), // peak in middle
                "inverse" => 1.0 - t,
                _ => 1.0, // uniform
            }
    }
}

fn quat_axis_angle(axis: &str, angle: f64) -> [f64; 4] {
    let half = angle / 2.0;
    match axis {
        "x" | "X" => [half.sin(), 0.0, 0.0, half.cos()],
        "z" | "Z" => [0.0, 0.0, half.sin(), half.cos()],
        _ => [0.0, half.sin(), 0.0, half.cos()],
    }
}

/// Multiply two quaternions [x, y, z, w]
fn quat_mul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn rotation_channel(bone_idx: usize, times: &[f64], rotations: Vec<[f64; 4]>) -> Value {
    json!({
        "boneIndex": bone_idx,
        "property": "rotation",
        "interpolation": "linear",
        "times": times,
        "values": rotations,
    })
}

fn position_channel(bone_idx: usize, times: &[f64], positions: Vec<[f64; 3]>) -> Value {
    json!({
        "boneIndex": bone_idx,
        "property": "position",
        "interpolation": "linear",
        "times": times,
        "values": positions,
    })
}

fn scale_channel(bone_idx: usize, times: &[f64], scales: Vec<[f64; 3]>) -> Value {
    json!({
        "boneIndex": bone_idx,
        "property": "scale",
        "interpolation": "linear",
        "times": times,
        "values": scales,
    })
}

fn append_locomotion(
    channels: &mut Vec<Value>,
    config: &HashMap<String, Value>,
    duration: f64,
    fps: u32,
) {
    if let Some(loco) = config.get("locomotion") {
        let axis = loco.get("axis").and_then(|v| v.as_str()).unwrap_or("x");
        let speed = loco.get("speed").and_then(|v| v.as_f64()).unwrap_or(0.3);
        let start = loco
            .get("startPosition")
            .and_then(|v| {
                v.as_array().map(|a| {
                    [
                        a.first().and_then(|x| x.as_f64()).unwrap_or(0.0),
                        a.get(1).and_then(|x| x.as_f64()).unwrap_or(0.0),
                        a.get(2).and_then(|x| x.as_f64()).unwrap_or(0.0),
                    ]
                })
            })
            .unwrap_or([0.0; 3]);

        let n = (fps as f64 * duration) as usize;
        let mut times = Vec::with_capacity(n + 1);
        let mut positions = Vec::with_capacity(n + 1);

        for i in 0..=n {
            let t = i as f64 / fps as f64;
            times.push(t);
            let offset = t * speed;
            let pos = match axis {
                "y" | "Y" => [start[0], start[1] + offset, start[2]],
                "z" | "Z" => [start[0], start[1], start[2] + offset],
                _ => [start[0] + offset, start[1], start[2]],
            };
            positions.push(pos);
        }
        channels.push(position_channel(0, &times, positions));
    }
}

/// Simple deterministic hash-based pseudo-random for sway/tremble.
/// Returns value in [-1, 1].
fn noise_1d(seed: u64, t: f64) -> f64 {
    let bits =
        (seed.wrapping_mul(2654435761) ^ (t * 10000.0) as u64).wrapping_mul(0x517cc1b727220a95);
    let frac = (bits & 0x00FF_FFFF) as f64 / 0x00FF_FFFF as f64;
    frac * 2.0 - 1.0
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: wave — traveling sine wave along bone chain
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: snake slither, fish swim, tentacle, flag, seaweed
//
// Config:
//   speed (0.6) — wave frequency in cycles/sec
//   amplitude (0.15) — max rotation per bone (radians)
//   waveLength (0.5) — phase delay between bones
//   rotationAxis ("y") — x/y/z
//   amplitudeCurve ("tailHeavy") — uniform/tailHeavy/headHeavy/bellCurve/inverse
//   locomotion — optional { axis, speed, startPosition }

fn generate_wave(ctx: &GenContext) -> Vec<Value> {
    let speed = ctx.f("speed", 0.6);
    let amplitude = ctx.f("amplitude", 0.15);
    let wave_length = ctx.f("waveLength", 0.5);
    let rot_axis = ctx.s("rotationAxis", "y");
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let phase = bone_idx as f64 * wave_length;
        let bone_amp = ctx.amplitude_envelope(bone_idx, amplitude);

        let rotations: Vec<[f64; 4]> = times
            .iter()
            .map(|&t| {
                let angle = (t * PI * 2.0 * speed - phase).sin() * bone_amp;
                quat_axis_angle(rot_axis, angle)
            })
            .collect();
        channels.push(rotation_channel(bone_idx, &times, rotations));
    }

    append_locomotion(&mut channels, ctx.config, ctx.duration, ctx.fps);
    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: bounce — vertical oscillation with gravity
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: jumping character, walk cycle vertical bob, bouncing ball,
//            kangaroo hop, piston, jackhammer
//
// Config:
//   height (1.0) — peak bounce height
//   gravity (9.8) — gravity acceleration
//   restitution (0.7) — energy retained per bounce (0=no bounce, 1=perfect)
//   axis ("y") — bounce direction
//   squash (0.0) — squash-and-stretch amount on impact (0=none, 0.3=cartoon)
//   phaseOffset (0.0) — per-bone phase stagger (for walk cycles)
//   amplitudeCurve ("uniform")

fn generate_bounce(ctx: &GenContext) -> Vec<Value> {
    let height = ctx.f("height", 1.0);
    let gravity = ctx.f("gravity", 9.8);
    let restitution = ctx.f("restitution", 0.7).clamp(0.0, 1.0);
    let axis = ctx.s("axis", "y");
    let squash = ctx.f("squash", 0.0);
    let phase_offset = ctx.f("phaseOffset", 0.0);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let bone_amp = ctx.amplitude_envelope(bone_idx, height);
        let phase = bone_idx as f64 * phase_offset;

        let mut positions: Vec<[f64; 3]> = Vec::with_capacity(times.len());
        let mut scales: Vec<[f64; 3]> = Vec::with_capacity(times.len());

        for &t in &times {
            let t_shifted = t + phase;
            let y = bounce_height(t_shifted, bone_amp, gravity, restitution);

            let pos = match axis {
                "x" | "X" => [y, 0.0, 0.0],
                "z" | "Z" => [0.0, 0.0, y],
                _ => [0.0, y, 0.0],
            };
            positions.push(pos);

            // Squash & stretch: compress on ground contact, stretch at peak
            if squash > 0.0 {
                let stretch = if y < 0.01 * bone_amp {
                    // On ground — squash
                    let s = 1.0 - squash;
                    match axis {
                        "x" | "X" => [s, 1.0 + squash * 0.5, 1.0 + squash * 0.5],
                        "z" | "Z" => [1.0 + squash * 0.5, 1.0 + squash * 0.5, s],
                        _ => [1.0 + squash * 0.5, s, 1.0 + squash * 0.5],
                    }
                } else {
                    // In air — stretch proportional to height
                    let h_norm = (y / bone_amp).clamp(0.0, 1.0);
                    let s = 1.0 + squash * 0.3 * h_norm;
                    let c = 1.0 - squash * 0.15 * h_norm;
                    match axis {
                        "x" | "X" => [s, c, c],
                        "z" | "Z" => [c, c, s],
                        _ => [c, s, c],
                    }
                };
                scales.push(stretch);
            }
        }

        channels.push(position_channel(bone_idx, &times, positions));
        if squash > 0.0 {
            channels.push(scale_channel(bone_idx, &times, scales));
        }
    }

    append_locomotion(&mut channels, ctx.config, ctx.duration, ctx.fps);
    channels
}

/// Compute bounce height at time t with energy loss per bounce.
fn bounce_height(t: f64, h0: f64, g: f64, restitution: f64) -> f64 {
    if g <= 0.0 || h0 <= 0.0 {
        return 0.0;
    }
    // Time to fall from h0: t_fall = sqrt(2*h/g)
    let mut h = h0;
    let mut elapsed = 0.0;

    loop {
        let t_bounce = 2.0 * (2.0 * h / g).sqrt(); // full up+down period
        if t_bounce < 1e-6 {
            return 0.0;
        }
        if elapsed + t_bounce > t {
            // Within this bounce
            let local_t = t - elapsed;
            let half = t_bounce / 2.0;
            let v0 = (2.0 * g * h).sqrt();
            if local_t <= half {
                // Going up
                return v0 * local_t - 0.5 * g * local_t * local_t;
            } else {
                // Coming down
                let dt = local_t - half;
                return h - 0.5 * g * dt * dt;
            }
        }
        elapsed += t_bounce;
        h *= restitution * restitution; // energy = 0.5*m*v^2, v scales with restitution
        if h < 1e-6 {
            return 0.0;
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: twist — progressive rotation along chain
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: drill bit, tornado, rope coiling, wringing cloth,
//            DNA helix rotation, corkscrew, barber pole
//
// Config:
//   speed (1.0) — rotations per second
//   maxAngle (6.28) — total twist across full chain (radians)
//   axis ("y") — twist axis
//   progressive (true) — if true, twist accumulates along chain;
//                         if false, all bones twist equally
//   oscillate (false) — if true, twist oscillates back and forth
//   amplitudeCurve ("uniform")

fn generate_twist(ctx: &GenContext) -> Vec<Value> {
    let speed = ctx.f("speed", 1.0);
    let max_angle = ctx.f("maxAngle", PI * 2.0);
    let axis = ctx.s("axis", "y");
    let progressive = ctx
        .config
        .get("progressive")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let oscillate = ctx
        .config
        .get("oscillate")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let bone_t = ctx.bone_t(bone_idx);
        let bone_amp = ctx.amplitude_envelope(bone_idx, 1.0);

        let per_bone_angle = if progressive {
            max_angle * bone_t * bone_amp
        } else {
            max_angle * bone_amp / ctx.bone_count.max(1) as f64
        };

        let rotations: Vec<[f64; 4]> = times
            .iter()
            .map(|&t| {
                let phase = t * speed * PI * 2.0;
                let angle = if oscillate {
                    per_bone_angle * phase.sin()
                } else {
                    per_bone_angle + phase
                };
                quat_axis_angle(axis, angle)
            })
            .collect();
        channels.push(rotation_channel(bone_idx, &times, rotations));
    }

    append_locomotion(&mut channels, ctx.config, ctx.duration, ctx.fps);
    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: pendulum — swinging with damping
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: chain links, rope, chandelier, grandfather clock,
//            wrecking ball, hanging sign, earring
//
// Config:
//   initialAngle (0.5) — starting angle in radians
//   damping (0.3) — exponential decay rate (0=no damping)
//   frequency (1.5) — natural frequency (Hz)
//   axis ("z") — swing axis
//   cascade (0.0) — phase delay between bones (chain effect)
//   amplitudeCurve ("uniform")

fn generate_pendulum(ctx: &GenContext) -> Vec<Value> {
    let initial_angle = ctx.f("initialAngle", 0.5);
    let damping = ctx.f("damping", 0.3);
    let frequency = ctx.f("frequency", 1.5);
    let axis = ctx.s("axis", "z");
    let cascade = ctx.f("cascade", 0.0);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let bone_amp = ctx.amplitude_envelope(bone_idx, initial_angle);
        let phase = bone_idx as f64 * cascade;

        let rotations: Vec<[f64; 4]> = times
            .iter()
            .map(|&t| {
                let envelope = (-damping * t).exp();
                let angle = bone_amp * envelope * (PI * 2.0 * frequency * t - phase).cos();
                quat_axis_angle(axis, angle)
            })
            .collect();
        channels.push(rotation_channel(bone_idx, &times, rotations));
    }

    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: spring — damped harmonic oscillation
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: jiggle physics (hair, breasts, belly), wobble after impact,
//            car suspension, springboard, antenna boing
//
// Config:
//   stiffness (10.0) — spring constant (higher = faster oscillation)
//   damping (2.0) — damping coefficient (higher = settles faster)
//   initialDisplacement (0.3) — starting offset
//   property ("rotation") — "rotation" or "position"
//   axis ("x") — motion axis
//   amplitudeCurve ("tailHeavy")

fn generate_spring(ctx: &GenContext) -> Vec<Value> {
    let stiffness = ctx.f("stiffness", 10.0);
    let damping_coeff = ctx.f("damping", 2.0);
    let initial = ctx.f("initialDisplacement", 0.3);
    let property = ctx.s("property", "rotation");
    let axis = ctx.s("axis", "x");
    let times = ctx.times();
    let mut channels = Vec::new();

    // Damped harmonic: x(t) = A * e^(-ζωt) * cos(ωd*t)
    // where ω = sqrt(k), ζ = c/(2*sqrt(k)), ωd = ω*sqrt(1-ζ²)
    let omega = stiffness.sqrt();
    let zeta = (damping_coeff / (2.0 * omega)).clamp(0.0, 0.999);
    let omega_d = omega * (1.0 - zeta * zeta).sqrt();

    for bone_idx in 0..ctx.bone_count {
        let bone_amp = ctx.amplitude_envelope(bone_idx, initial);
        // Stagger: later bones start oscillating later (wave propagation)
        let delay = bone_idx as f64 * 0.05;

        if property == "position" {
            let positions: Vec<[f64; 3]> = times
                .iter()
                .map(|&t| {
                    let t_local = (t - delay).max(0.0);
                    let val =
                        bone_amp * (-zeta * omega * t_local).exp() * (omega_d * t_local).cos();
                    match axis {
                        "x" | "X" => [val, 0.0, 0.0],
                        "z" | "Z" => [0.0, 0.0, val],
                        _ => [0.0, val, 0.0],
                    }
                })
                .collect();
            channels.push(position_channel(bone_idx, &times, positions));
        } else {
            let rotations: Vec<[f64; 4]> = times
                .iter()
                .map(|&t| {
                    let t_local = (t - delay).max(0.0);
                    let angle =
                        bone_amp * (-zeta * omega * t_local).exp() * (omega_d * t_local).cos();
                    quat_axis_angle(axis, angle)
                })
                .collect();
            channels.push(rotation_channel(bone_idx, &times, rotations));
        }
    }

    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: breathe — rhythmic scale pulsing
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: character idle breathing, heart pulse, jellyfish,
//            pulsating glow source, expanding/contracting
//
// Config:
//   rate (0.25) — breaths per second
//   scaleAmount (0.05) — max scale deviation from 1.0
//   axis ("y") — primary scale axis; "all" for uniform
//   asymmetry (0.6) — inhale/exhale ratio (0.5=symmetric, >0.5=quick inhale)
//   amplitudeCurve ("uniform")

fn generate_breathe(ctx: &GenContext) -> Vec<Value> {
    let rate = ctx.f("rate", 0.25);
    let scale_amount = ctx.f("scaleAmount", 0.05);
    let axis = ctx.s("axis", "y");
    let asymmetry = ctx.f("asymmetry", 0.6).clamp(0.01, 0.99);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let bone_amp = ctx.amplitude_envelope(bone_idx, scale_amount);

        let scales: Vec<[f64; 3]> = times
            .iter()
            .map(|&t| {
                // Asymmetric breathing curve using smoothstep
                let phase = (t * rate).fract();
                let breath = if phase < asymmetry {
                    // Inhale (quick rise)
                    let p = phase / asymmetry;
                    p * p * (3.0 - 2.0 * p) // smoothstep
                } else {
                    // Exhale (slow fall)
                    let p = (phase - asymmetry) / (1.0 - asymmetry);
                    1.0 - p * p * (3.0 - 2.0 * p)
                };

                let s = 1.0 + bone_amp * breath;
                let c = 1.0 - bone_amp * 0.3 * breath; // counter-scale for volume preservation
                match axis {
                    "x" | "X" => [s, c, c],
                    "z" | "Z" => [c, c, s],
                    "all" => [s, s, s],
                    _ => [c, s, c], // y default
                }
            })
            .collect();
        channels.push(scale_channel(bone_idx, &times, scales));
    }

    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: sway — wind-driven multi-frequency motion
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: tree branches, grass, hair, cloth, seaweed,
//            lantern, hanging cables, tall buildings
//
// Config:
//   windStrength (0.2) — base rotation amplitude
//   windFrequency (0.8) — primary wind oscillation Hz
//   gustFrequency (0.15) — secondary gust frequency
//   gustStrength (0.4) — gust amplitude relative to wind
//   turbulence (0.3) — high-frequency noise amount
//   axis ("z") — primary sway axis
//   secondaryAxis ("x") — secondary sway axis (crosswind)
//   secondaryStrength (0.3) — crosswind relative strength
//   amplitudeCurve ("tailHeavy") — tips sway more

fn generate_sway(ctx: &GenContext) -> Vec<Value> {
    let wind_str = ctx.f("windStrength", 0.2);
    let wind_freq = ctx.f("windFrequency", 0.8);
    let gust_freq = ctx.f("gustFrequency", 0.15);
    let gust_str = ctx.f("gustStrength", 0.4);
    let turbulence = ctx.f("turbulence", 0.3);
    let axis = ctx.s("axis", "z");
    let sec_axis = ctx.s("secondaryAxis", "x");
    let sec_str = ctx.f("secondaryStrength", 0.3);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let bone_amp = ctx.amplitude_envelope(bone_idx, wind_str);
        let seed = bone_idx as u64 * 7919 + 42; // deterministic per bone

        let rotations: Vec<[f64; 4]> = times
            .iter()
            .map(|&t| {
                // Primary wind
                let primary = (t * wind_freq * PI * 2.0 + bone_idx as f64 * 0.7).sin();
                // Gust overlay
                let gust = (t * gust_freq * PI * 2.0 + bone_idx as f64 * 1.3).sin() * gust_str;
                // Turbulence (noise)
                let turb = noise_1d(seed, t * 3.0) * turbulence;

                let angle_primary = bone_amp * (primary + gust + turb);
                let angle_secondary = bone_amp
                    * sec_str
                    * ((t * wind_freq * 0.7 * PI * 2.0 + bone_idx as f64 * 1.1).sin()
                        + noise_1d(seed + 1000, t * 2.5) * turbulence * 0.5);

                let q1 = quat_axis_angle(axis, angle_primary);
                let q2 = quat_axis_angle(sec_axis, angle_secondary);
                quat_mul(q1, q2)
            })
            .collect();
        channels.push(rotation_channel(bone_idx, &times, rotations));
    }

    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: orbit — circular or elliptical motion
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: planet orbiting star, carousel horses, spinning top,
//            electron cloud, rotating platform, clock hands
//
// Config:
//   radius (1.0) — orbit radius
//   radiusY (0.0) — if >0, makes elliptical (height of ellipse)
//   speed (1.0) — orbits per second
//   axis ("y") — orbit axis (rotation plane normal)
//   tilt (0.0) — orbit plane tilt in radians
//   phaseOffset (0.0) — per-bone phase stagger (for solar systems)
//   spinSpeed (0.0) — self-rotation speed (RPM around own axis)

fn generate_orbit(ctx: &GenContext) -> Vec<Value> {
    let radius = ctx.f("radius", 1.0);
    let radius_y = ctx.f("radiusY", 0.0);
    let speed = ctx.f("speed", 1.0);
    let tilt = ctx.f("tilt", 0.0);
    let phase_offset = ctx.f("phaseOffset", 0.0);
    let spin_speed = ctx.f("spinSpeed", 0.0);
    let axis = ctx.s("axis", "y");
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let phase = bone_idx as f64 * phase_offset;
        let r = radius * ctx.amplitude_envelope(bone_idx, 1.0);
        let ry = if radius_y > 0.0 {
            radius_y * ctx.amplitude_envelope(bone_idx, 1.0)
        } else {
            r
        };

        let positions: Vec<[f64; 3]> = times
            .iter()
            .map(|&t| {
                let angle = t * speed * PI * 2.0 + phase;
                let cx = r * angle.cos();
                let cy = ry * angle.sin();

                // Apply tilt
                let (tilted_a, tilted_b) = if tilt.abs() > 1e-6 {
                    (cx, cy * tilt.cos())
                } else {
                    (cx, cy)
                };
                let tilted_c = if tilt.abs() > 1e-6 {
                    cy * tilt.sin()
                } else {
                    0.0
                };

                match axis {
                    "x" | "X" => [tilted_c, tilted_a, tilted_b],
                    "z" | "Z" => [tilted_a, tilted_b, tilted_c],
                    _ => [tilted_a, tilted_c, tilted_b], // y-up default
                }
            })
            .collect();
        channels.push(position_channel(bone_idx, &times, positions));

        // Optional self-spin
        if spin_speed.abs() > 1e-6 {
            let rotations: Vec<[f64; 4]> = times
                .iter()
                .map(|&t| {
                    let angle = t * spin_speed * PI * 2.0;
                    quat_axis_angle(axis, angle)
                })
                .collect();
            channels.push(rotation_channel(bone_idx, &times, rotations));
        }
    }

    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: spiral — helical rotation + translation
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: DNA helix, rising smoke, screw/bolt, tornado funnel,
//            spiral staircase animation, spring coil
//
// Config:
//   radius (0.5) — spiral radius
//   rise (1.0) — vertical distance per revolution
//   speed (1.0) — revolutions per second
//   axis ("y") — spiral vertical axis
//   shrink (0.0) — radius decrease per revolution (funnel effect)
//   amplitudeCurve ("uniform")

fn generate_spiral(ctx: &GenContext) -> Vec<Value> {
    let radius = ctx.f("radius", 0.5);
    let rise = ctx.f("rise", 1.0);
    let speed = ctx.f("speed", 1.0);
    let axis = ctx.s("axis", "y");
    let shrink = ctx.f("shrink", 0.0);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let bone_amp = ctx.amplitude_envelope(bone_idx, 1.0);
        let bone_phase = bone_idx as f64 * PI * 2.0 / ctx.bone_count.max(1) as f64;

        let positions: Vec<[f64; 3]> = times
            .iter()
            .map(|&t| {
                let angle = t * speed * PI * 2.0 + bone_phase;
                let revolutions = t * speed;
                let r = (radius - shrink * revolutions).max(0.0) * bone_amp;
                let h = rise * revolutions * bone_amp;
                let cx = r * angle.cos();
                let cy = r * angle.sin();

                match axis {
                    "x" | "X" => [h, cx, cy],
                    "z" | "Z" => [cx, cy, h],
                    _ => [cx, h, cy],
                }
            })
            .collect();
        channels.push(position_channel(bone_idx, &times, positions));

        // Bones also rotate to face tangent direction
        let rotations: Vec<[f64; 4]> = times
            .iter()
            .map(|&t| {
                let angle = t * speed * PI * 2.0 + bone_phase;
                quat_axis_angle(axis, angle)
            })
            .collect();
        channels.push(rotation_channel(bone_idx, &times, rotations));
    }

    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: figure8 — Lissajous / figure-eight path
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: figure skating path, juggling ball trajectory, infinity symbol,
//            butterfly wing tip, fish schooling pattern
//
// Config:
//   scaleX (1.0) — horizontal extent
//   scaleY (0.5) — vertical extent
//   speed (0.5) — cycles per second
//   ratioX (1) — Lissajous X frequency ratio
//   ratioY (2) — Lissajous Y frequency ratio (2=figure-8, 3=trefoil)
//   plane ("xz") — motion plane: "xy", "xz", or "yz"
//   phaseOffset (0.0) — per-bone phase stagger

fn generate_figure8(ctx: &GenContext) -> Vec<Value> {
    let scale_x = ctx.f("scaleX", 1.0);
    let scale_y = ctx.f("scaleY", 0.5);
    let speed = ctx.f("speed", 0.5);
    let ratio_x = ctx.f("ratioX", 1.0);
    let ratio_y = ctx.f("ratioY", 2.0);
    let plane = ctx.s("plane", "xz");
    let phase_offset = ctx.f("phaseOffset", 0.0);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let phase = bone_idx as f64 * phase_offset;
        let amp = ctx.amplitude_envelope(bone_idx, 1.0);

        let positions: Vec<[f64; 3]> = times
            .iter()
            .map(|&t| {
                let angle = t * speed * PI * 2.0 + phase;
                let lx = scale_x * amp * (ratio_x * angle).sin();
                let ly = scale_y * amp * (ratio_y * angle).sin();

                match plane {
                    "xy" => [lx, ly, 0.0],
                    "yz" => [0.0, lx, ly],
                    _ => [lx, 0.0, ly], // xz
                }
            })
            .collect();
        channels.push(position_channel(bone_idx, &times, positions));
    }

    append_locomotion(&mut channels, ctx.config, ctx.duration, ctx.fps);
    channels
}

// ═══════════════════════════════════════════════════════════════════════════
// Generator: tremble — high-frequency low-amplitude noise shake
// ═══════════════════════════════════════════════════════════════════════════
//
// Use cases: fear/cold shivering, engine vibration, earthquake,
//            nervous character, electrical buzzing, camera shake
//
// Config:
//   intensity (0.03) — max rotation amplitude (radians)
//   frequency (15.0) — base frequency (Hz)
//   decay (0.0) — exponential decay rate (0=sustained, >0=settling)
//   axes ("xyz") — which axes to shake; any combo of x, y, z
//   positionIntensity (0.0) — also shake position (world units)
//   amplitudeCurve ("uniform")

fn generate_tremble(ctx: &GenContext) -> Vec<Value> {
    let intensity = ctx.f("intensity", 0.03);
    let frequency = ctx.f("frequency", 15.0);
    let decay = ctx.f("decay", 0.0);
    let axes = ctx.s("axes", "xyz");
    let pos_intensity = ctx.f("positionIntensity", 0.0);
    let times = ctx.times();
    let mut channels = Vec::new();

    for bone_idx in 0..ctx.bone_count {
        let bone_amp = ctx.amplitude_envelope(bone_idx, intensity);
        let seed_base = bone_idx as u64 * 31337;

        // Rotation channels
        let rotations: Vec<[f64; 4]> = times
            .iter()
            .map(|&t| {
                let envelope = if decay > 0.0 { (-decay * t).exp() } else { 1.0 };
                let amp = bone_amp * envelope;

                // Composite noise at different frequencies for each axis
                let ax = if axes.contains('x') {
                    amp * ((t * frequency * PI * 2.0).sin() * 0.7
                        + noise_1d(seed_base, t * frequency * 0.5) * 0.3)
                } else {
                    0.0
                };
                let ay = if axes.contains('y') {
                    amp * ((t * frequency * PI * 2.0 * 1.1 + 1.7).sin() * 0.7
                        + noise_1d(seed_base + 100, t * frequency * 0.6) * 0.3)
                } else {
                    0.0
                };
                let az = if axes.contains('z') {
                    amp * ((t * frequency * PI * 2.0 * 0.9 + 2.3).sin() * 0.7
                        + noise_1d(seed_base + 200, t * frequency * 0.4) * 0.3)
                } else {
                    0.0
                };

                let qx = quat_axis_angle("x", ax);
                let qy = quat_axis_angle("y", ay);
                let qz = quat_axis_angle("z", az);
                quat_mul(quat_mul(qx, qy), qz)
            })
            .collect();
        channels.push(rotation_channel(bone_idx, &times, rotations));

        // Optional position shake
        if pos_intensity > 0.0 {
            let bone_pos_amp = ctx.amplitude_envelope(bone_idx, pos_intensity);
            let positions: Vec<[f64; 3]> = times
                .iter()
                .map(|&t| {
                    let envelope = if decay > 0.0 { (-decay * t).exp() } else { 1.0 };
                    let amp = bone_pos_amp * envelope;
                    [
                        amp * noise_1d(seed_base + 300, t * frequency * 0.8),
                        amp * noise_1d(seed_base + 400, t * frequency * 0.9),
                        amp * noise_1d(seed_base + 500, t * frequency * 0.7),
                    ]
                })
                .collect();
            channels.push(position_channel(bone_idx, &times, positions));
        }
    }

    channels
}
