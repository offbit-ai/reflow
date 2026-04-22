//! Vector and matrix math actors for geometry operations.
//!
//! All vectors are represented as Message::Array of floats.
//! Matrices are column-major arrays (4 columns × 4 rows = 16 floats).

use crate::{Actor, ActorBehavior, Message, Port};
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use reflow_actor_macro::actor;
use serde_json::json;
use std::collections::HashMap;

// ─── Helpers ─────────────────────────────────────────────────────

fn get_vec3(payload: &HashMap<String, Message>, port: &str) -> [f64; 3] {
    match payload.get(port) {
        Some(Message::Object(v)) => {
            let val: serde_json::Value = v.as_ref().clone().into();
            [
                val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                val.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0),
            ]
        }
        Some(Message::Float(v)) => [*v, *v, *v],
        _ => [0.0; 3],
    }
}

fn vec3_msg(v: [f64; 3]) -> Message {
    Message::object(EncodableValue::from(
        json!({ "x": v[0], "y": v[1], "z": v[2] }),
    ))
}

fn get_vec4(payload: &HashMap<String, Message>, port: &str) -> [f64; 4] {
    match payload.get(port) {
        Some(Message::Object(v)) => {
            let val: serde_json::Value = v.as_ref().clone().into();
            [
                val.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                val.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                val.get("z").and_then(|v| v.as_f64()).unwrap_or(0.0),
                val.get("w").and_then(|v| v.as_f64()).unwrap_or(0.0),
            ]
        }
        _ => [0.0; 4],
    }
}

fn vec4_msg(v: [f64; 4]) -> Message {
    Message::object(EncodableValue::from(
        json!({ "x": v[0], "y": v[1], "z": v[2], "w": v[3] }),
    ))
}

fn mat4_msg(m: &[f64; 16]) -> Message {
    Message::object(EncodableValue::from(json!(m.to_vec())))
}

fn get_mat4(payload: &HashMap<String, Message>, port: &str) -> [f64; 16] {
    match payload.get(port) {
        Some(Message::Object(v)) => {
            let val: serde_json::Value = v.as_ref().clone().into();
            if let Some(arr) = val.as_array() {
                let mut m = [0.0f64; 16];
                for (i, v) in arr.iter().enumerate().take(16) {
                    m[i] = v.as_f64().unwrap_or(0.0);
                }
                return m;
            }
            MAT4_IDENTITY
        }
        _ => MAT4_IDENTITY,
    }
}

const MAT4_IDENTITY: [f64; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

// ─── Vector3 Operations ─────────────────────────────────────────

#[actor(Vec3AddActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_add_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec3(p, "a");
    let b = get_vec3(p, "b");
    Ok([(
        "result".to_string(),
        vec3_msg([a[0] + b[0], a[1] + b[1], a[2] + b[2]]),
    )]
    .into())
}

#[actor(Vec3SubtractActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_subtract_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec3(p, "a");
    let b = get_vec3(p, "b");
    Ok([(
        "result".to_string(),
        vec3_msg([a[0] - b[0], a[1] - b[1], a[2] - b[2]]),
    )]
    .into())
}

#[actor(Vec3ScaleActor, inports::<10>(vector, scalar), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_scale_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let v = get_vec3(p, "vector");
    let s = match p.get("scalar") {
        Some(Message::Float(f)) => *f,
        Some(Message::Integer(i)) => *i as f64,
        _ => 1.0,
    };
    Ok([(
        "result".to_string(),
        vec3_msg([v[0] * s, v[1] * s, v[2] * s]),
    )]
    .into())
}

#[actor(Vec3DotActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_dot_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec3(p, "a");
    let b = get_vec3(p, "b");
    Ok([(
        "result".to_string(),
        Message::Float(a[0] * b[0] + a[1] * b[1] + a[2] * b[2]),
    )]
    .into())
}

#[actor(Vec3CrossActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_cross_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec3(p, "a");
    let b = get_vec3(p, "b");
    Ok([(
        "result".to_string(),
        vec3_msg([
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]),
    )]
    .into())
}

#[actor(Vec3NormalizeActor, inports::<10>(input), outports::<1>(result, length), state(MemoryState))]
pub async fn vec3_normalize_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let v = get_vec3(p, "input");
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    let mut out = HashMap::new();
    if len > 1e-10 {
        out.insert(
            "result".to_string(),
            vec3_msg([v[0] / len, v[1] / len, v[2] / len]),
        );
    } else {
        out.insert("result".to_string(), vec3_msg([0.0, 0.0, 0.0]));
    }
    out.insert("length".to_string(), Message::Float(len));
    Ok(out)
}

#[actor(Vec3LengthActor, inports::<10>(input), outports::<1>(result), state(MemoryState))]
pub async fn vec3_length_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let v = get_vec3(p, "input");
    Ok([(
        "result".to_string(),
        Message::Float((v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()),
    )]
    .into())
}

#[actor(Vec3DistanceActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_distance_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec3(p, "a");
    let b = get_vec3(p, "b");
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    Ok([(
        "result".to_string(),
        Message::Float((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()),
    )]
    .into())
}

#[actor(Vec3LerpActor, inports::<10>(a, b, t), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_lerp_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec3(p, "a");
    let b = get_vec3(p, "b");
    let t = match p.get("t") {
        Some(Message::Float(f)) => *f,
        _ => 0.5,
    };
    Ok([(
        "result".to_string(),
        vec3_msg([
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
        ]),
    )]
    .into())
}

#[actor(Vec3ReflectActor, inports::<10>(incident, normal), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn vec3_reflect_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let i = get_vec3(p, "incident");
    let n = get_vec3(p, "normal");
    let d = 2.0 * (i[0] * n[0] + i[1] * n[1] + i[2] * n[2]);
    Ok([(
        "result".to_string(),
        vec3_msg([i[0] - d * n[0], i[1] - d * n[1], i[2] - d * n[2]]),
    )]
    .into())
}

// ─── Vec3 Constructor ───────────────────────────────────────────

/// Constructs a vec3 from x/y/z inputs or config.
/// Inports override config when connected.
#[actor(Vec3Actor, inports::<10>(x, y, z), outports::<1>(result), state(MemoryState))]
pub async fn vec3_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let c = ctx.get_config_hashmap();
    let x = get_float_or_config(p.get("x"), c.get("x"), 0.0);
    let y = get_float_or_config(p.get("y"), c.get("y"), 0.0);
    let z = get_float_or_config(p.get("z"), c.get("z"), 0.0);
    Ok([("result".to_string(), vec3_msg([x, y, z]))].into())
}

/// Read a float from inport message first, fall back to config value.
fn get_float_or_config(
    msg: Option<&Message>,
    cfg: Option<&serde_json::Value>,
    default: f64,
) -> f64 {
    match msg {
        Some(Message::Float(v)) => return *v,
        Some(Message::Integer(v)) => return *v as f64,
        _ => {}
    }
    cfg.and_then(|v| v.as_f64()).unwrap_or(default)
}

// ─── Matrix4 Operations ─────────────────────────────────────────

fn mat4_mul(a: &[f64; 16], b: &[f64; 16]) -> [f64; 16] {
    let mut r = [0.0f64; 16];
    for col in 0..4 {
        for row in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[k * 4 + row] * b[col * 4 + k];
            }
            r[col * 4 + row] = sum;
        }
    }
    r
}

fn mat4_transform_vec3(m: &[f64; 16], v: [f64; 3]) -> [f64; 3] {
    let w = m[3] * v[0] + m[7] * v[1] + m[11] * v[2] + m[15];
    let w = if w.abs() > 1e-10 { w } else { 1.0 };
    [
        (m[0] * v[0] + m[4] * v[1] + m[8] * v[2] + m[12]) / w,
        (m[1] * v[0] + m[5] * v[1] + m[9] * v[2] + m[13]) / w,
        (m[2] * v[0] + m[6] * v[1] + m[10] * v[2] + m[14]) / w,
    ]
}

#[actor(Mat4MultiplyActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn mat4_multiply_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_mat4(p, "a");
    let b = get_mat4(p, "b");
    Ok([("result".to_string(), mat4_msg(&mat4_mul(&a, &b)))].into())
}

#[actor(Mat4TransformActor, inports::<10>(matrix, vector), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn mat4_transform_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let m = get_mat4(p, "matrix");
    let v = get_vec3(p, "vector");
    Ok([("result".to_string(), vec3_msg(mat4_transform_vec3(&m, v)))].into())
}

#[actor(Mat4IdentityActor, inports::<1>(), outports::<1>(result), state(MemoryState))]
pub async fn mat4_identity_actor(_ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    Ok([("result".to_string(), mat4_msg(&MAT4_IDENTITY))].into())
}

#[actor(Mat4TranslateActor, inports::<10>(x, y, z), outports::<1>(result), state(MemoryState))]
pub async fn mat4_translate_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let c = ctx.get_config_hashmap();
    let x = get_float_or_config(p.get("x"), c.get("x"), 0.0);
    let y = get_float_or_config(p.get("y"), c.get("y"), 0.0);
    let z = get_float_or_config(p.get("z"), c.get("z"), 0.0);
    let m = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
    ];
    Ok([("result".to_string(), mat4_msg(&m))].into())
}

#[actor(Mat4ScaleActor, inports::<10>(x, y, z), outports::<1>(result), state(MemoryState))]
pub async fn mat4_scale_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let c = ctx.get_config_hashmap();
    let x = get_float_or_config(p.get("x"), c.get("x"), 1.0);
    let y = get_float_or_config(p.get("y"), c.get("y"), 1.0);
    let z = get_float_or_config(p.get("z"), c.get("z"), 1.0);
    let m = [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    Ok([("result".to_string(), mat4_msg(&m))].into())
}

#[actor(Mat4RotateXActor, inports::<10>(angle), outports::<1>(result), state(MemoryState))]
pub async fn mat4_rotate_x_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let c = ctx.get_config_hashmap();
    let deg = get_float_or_config(p.get("angle"), c.get("angle"), 0.0);
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    let m = [
        1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    Ok([("result".to_string(), mat4_msg(&m))].into())
}

#[actor(Mat4RotateYActor, inports::<10>(angle), outports::<1>(result), state(MemoryState))]
pub async fn mat4_rotate_y_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let cfg = ctx.get_config_hashmap();
    let deg = get_float_or_config(p.get("angle"), cfg.get("angle"), 0.0);
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    let m = [
        c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    Ok([("result".to_string(), mat4_msg(&m))].into())
}

#[actor(Mat4RotateZActor, inports::<10>(angle), outports::<1>(result), state(MemoryState))]
pub async fn mat4_rotate_z_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let cfg = ctx.get_config_hashmap();
    let deg = get_float_or_config(p.get("angle"), cfg.get("angle"), 0.0);
    let r = deg.to_radians();
    let (s, c) = (r.sin(), r.cos());
    let m = [
        c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    Ok([("result".to_string(), mat4_msg(&m))].into())
}

#[actor(Mat4LookAtActor, inports::<10>(eye, target, up), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn mat4_look_at_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let eye = get_vec3(p, "eye");
    let target = get_vec3(p, "target");
    let up = get_vec3(p, "up");

    let fwd = {
        let d = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if l > 1e-10 {
            [d[0] / l, d[1] / l, d[2] / l]
        } else {
            [0.0, 0.0, -1.0]
        }
    };
    let right = {
        let c = [
            fwd[1] * up[2] - fwd[2] * up[1],
            fwd[2] * up[0] - fwd[0] * up[2],
            fwd[0] * up[1] - fwd[1] * up[0],
        ];
        let l = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
        if l > 1e-10 {
            [c[0] / l, c[1] / l, c[2] / l]
        } else {
            [1.0, 0.0, 0.0]
        }
    };
    let new_up = [
        right[1] * fwd[2] - right[2] * fwd[1],
        right[2] * fwd[0] - right[0] * fwd[2],
        right[0] * fwd[1] - right[1] * fwd[0],
    ];

    let m = [
        right[0],
        new_up[0],
        -fwd[0],
        0.0,
        right[1],
        new_up[1],
        -fwd[1],
        0.0,
        right[2],
        new_up[2],
        -fwd[2],
        0.0,
        -(right[0] * eye[0] + right[1] * eye[1] + right[2] * eye[2]),
        -(new_up[0] * eye[0] + new_up[1] * eye[1] + new_up[2] * eye[2]),
        fwd[0] * eye[0] + fwd[1] * eye[1] + fwd[2] * eye[2],
        1.0,
    ];
    Ok([("result".to_string(), mat4_msg(&m))].into())
}

#[actor(Mat4PerspectiveActor, inports::<10>(fov, aspect, near, far), outports::<1>(result), state(MemoryState))]
pub async fn mat4_perspective_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let c = ctx.get_config_hashmap();
    let fov = get_float_or_config(p.get("fov"), c.get("fov"), 45.0).to_radians();
    let aspect = get_float_or_config(p.get("aspect"), c.get("aspect"), 1.0);
    let near = get_float_or_config(p.get("near"), c.get("near"), 0.1);
    let far = get_float_or_config(p.get("far"), c.get("far"), 100.0);

    let f = 1.0 / (fov / 2.0).tan();
    let nf = 1.0 / (near - far);

    let m = [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        (far + near) * nf,
        -1.0,
        0.0,
        0.0,
        2.0 * far * near * nf,
        0.0,
    ];
    Ok([("result".to_string(), mat4_msg(&m))].into())
}

// ─── Quaternion Operations ──────────────────────────────────────

#[actor(QuatFromEulerActor, inports::<10>(x, y, z), outports::<1>(result), state(MemoryState))]
pub async fn quat_from_euler_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let c = ctx.get_config_hashmap();
    let x = get_float_or_config(p.get("x"), c.get("x"), 0.0).to_radians() * 0.5;
    let y = get_float_or_config(p.get("y"), c.get("y"), 0.0).to_radians() * 0.5;
    let z = get_float_or_config(p.get("z"), c.get("z"), 0.0).to_radians() * 0.5;

    let (sx, cx) = (x.sin(), x.cos());
    let (sy, cy) = (y.sin(), y.cos());
    let (sz, cz) = (z.sin(), z.cos());

    Ok([(
        "result".to_string(),
        vec4_msg([
            sx * cy * cz - cx * sy * sz,
            cx * sy * cz + sx * cy * sz,
            cx * cy * sz - sx * sy * cz,
            cx * cy * cz + sx * sy * sz,
        ]),
    )]
    .into())
}

#[actor(QuatMultiplyActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn quat_multiply_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec4(p, "a");
    let b = get_vec4(p, "b");
    Ok([(
        "result".to_string(),
        vec4_msg([
            a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
            a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
            a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
            a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
        ]),
    )]
    .into())
}

#[actor(QuatSlerpActor, inports::<10>(a, b, t), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn quat_slerp_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let a = get_vec4(p, "a");
    let b = get_vec4(p, "b");
    let t = match p.get("t") {
        Some(Message::Float(f)) => *f,
        _ => 0.5,
    };

    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let mut b = b;
    if dot < 0.0 {
        b = [-b[0], -b[1], -b[2], -b[3]];
        dot = -dot;
    }

    if dot > 0.9995 {
        // Linear interpolation for very close quaternions
        let r = [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ];
        let l = (r[0] * r[0] + r[1] * r[1] + r[2] * r[2] + r[3] * r[3]).sqrt();
        return Ok([(
            "result".to_string(),
            vec4_msg([r[0] / l, r[1] / l, r[2] / l, r[3] / l]),
        )]
        .into());
    }

    let theta = dot.clamp(-1.0, 1.0).acos();
    let sin_theta = theta.sin();
    let sa = ((1.0 - t) * theta).sin() / sin_theta;
    let sb = (t * theta).sin() / sin_theta;

    Ok([(
        "result".to_string(),
        vec4_msg([
            sa * a[0] + sb * b[0],
            sa * a[1] + sb * b[1],
            sa * a[2] + sb * b[2],
            sa * a[3] + sb * b[3],
        ]),
    )]
    .into())
}

#[actor(QuatRotateVec3Actor, inports::<10>(quaternion, vector), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn quat_rotate_vec3_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let p = ctx.get_payload();
    let q = get_vec4(p, "quaternion");
    let v = get_vec3(p, "vector");

    // q * v * q^-1 (quaternion rotation)
    let t = [
        2.0 * (q[1] * v[2] - q[2] * v[1]),
        2.0 * (q[2] * v[0] - q[0] * v[2]),
        2.0 * (q[0] * v[1] - q[1] * v[0]),
    ];

    Ok([(
        "result".to_string(),
        vec3_msg([
            v[0] + q[3] * t[0] + q[1] * t[2] - q[2] * t[1],
            v[1] + q[3] * t[1] + q[2] * t[0] - q[0] * t[2],
            v[2] + q[3] * t[2] + q[0] * t[1] - q[1] * t[0],
        ]),
    )]
    .into())
}
