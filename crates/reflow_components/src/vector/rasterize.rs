//! Vector rasterize actor — renders Path2D to RGBA pixels via tiny-skia.
//!
//! ## Inports
//! - `path` — SVG path string (from Shape2DActor or any path source)
//! - `fill` — fill config (optional, overrides config)
//! - `stroke` — stroke config (optional, overrides config)
//!
//! ## Config
//! ```json
//! {
//!   "width": 512,
//!   "height": 512,
//!   "background": [0, 0, 0, 0],
//!   "fill": { "color": "#ff5500" },
//!   "stroke": { "width": 2, "color": "#000000" },
//!   "antiAlias": true
//! }
//! ```
//!
//! ## Outports
//! - `image` — RGBA bytes (width * height * 4)

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;
use tiny_skia as tsk;

#[actor(
    VectorRasterizeActor,
    inports::<10>(path, fill, stroke, transform, tick, x, y, scale, rotation),
    outports::<1>(image, metadata),
    state(MemoryState)
)]
pub async fn vector_rasterize_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    let width = config.get("width").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let height = config.get("height").and_then(|v| v.as_u64()).unwrap_or(512) as u32;
    let anti_alias = config
        .get("antiAlias")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Cache path in pool — it arrives once, transform arrives every tick
    if let Some(Message::String(s)) = payload.get("path") {
        ctx.pool_upsert("_raster", "path", json!(s.to_string()));
    }

    // Only render when transform or tick arrives — not on path-only input.
    // Path is cached for reuse; the per-tick trigger is transform/tick.
    let has_trigger = payload.contains_key("transform")
        || payload.contains_key("tick")
        || payload.contains_key("scale")
        || payload.contains_key("rotation")
        || payload.contains_key("x")
        || payload.contains_key("y");

    if !has_trigger {
        return Ok(HashMap::new()); // Path cached, waiting for per-tick trigger
    }

    // Get cached path
    let svg_d = ctx
        .get_pool("_raster")
        .into_iter()
        .find(|(k, _)| k == "path")
        .and_then(|(_, v)| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            payload.get("path").and_then(|m| match m {
                Message::String(s) => Some(s.to_string()),
                _ => None,
            })
        })
        .unwrap_or_default();

    if svg_d.is_empty() {
        return Ok(HashMap::new());
    }

    // Parse fill config (inport overrides config)
    let fill_cfg = match payload.get("fill") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            Some(v)
        }
        _ => config.get("fill").cloned(),
    };

    // Parse stroke config (inport overrides config)
    let stroke_cfg = match payload.get("stroke") {
        Some(Message::Object(obj)) => {
            let v: Value = obj.as_ref().clone().into();
            Some(v)
        }
        _ => config.get("stroke").cloned(),
    };

    // Background color
    let bg = config
        .get("background")
        .and_then(|v| v.as_array())
        .map(|a| {
            [
                a.first().and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                a.get(1).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                a.get(2).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
                a.get(3).and_then(|v| v.as_u64()).unwrap_or(0) as u8,
            ]
        })
        .unwrap_or([0, 0, 0, 0]);

    // Create pixmap
    let mut pixmap = tsk::Pixmap::new(width, height)
        .ok_or_else(|| anyhow::anyhow!("Failed to create pixmap {}x{}", width, height))?;

    // Fill background
    if bg[3] > 0 {
        pixmap.fill(tsk::Color::from_rgba8(bg[0], bg[1], bg[2], bg[3]));
    }

    // Parse SVG path via reflow_vector, then convert to tiny-skia path
    let vec_path = reflow_vector::Path2D::from_svg(&svg_d);
    let ts_path = path2d_to_tiny_skia(&vec_path)
        .ok_or_else(|| anyhow::anyhow!("Failed to build tiny-skia path"))?;

    // Cache individual transform fields from inports
    for (port, key) in [
        ("x", "x"),
        ("y", "y"),
        ("scale", "scale"),
        ("rotation", "rotation"),
    ] {
        if let Some(msg) = payload.get(port) {
            let val = match msg {
                Message::Float(f) => *f,
                Message::Integer(i) => *i as f64,
                Message::Object(obj) => {
                    let v: Value = obj.as_ref().clone().into();
                    v.as_f64().unwrap_or(0.0)
                }
                _ => continue,
            };
            ctx.pool_upsert("_tf", key, json!(val));
        }
    }

    // Accept full transform object inport (timeline `values` output or direct)
    if let Some(Message::Object(obj)) = payload.get("transform") {
        let v: Value = obj.as_ref().clone().into();
        let tf_map = config.get("transformMap");

        if let Value::Object(map) = v {
            for (k, val) in &map {
                if let Some(f) = val.as_f64() {
                    let target_key = tf_map
                        .and_then(|m| m.get(k))
                        .and_then(|v| v.as_str())
                        .unwrap_or(k);
                    ctx.pool_upsert("_tf", target_key, json!(f));
                }
            }
        }
    }

    // Build transform: pool (from inports) > config defaults
    let tf_config = config.get("transform").cloned().unwrap_or(json!({}));
    let tf_pool: HashMap<String, Value> = ctx.get_pool("_tf").into_iter().collect();

    let _get_tf = |key: &str, default: f64| -> f32 {
        tf_pool
            .get(key)
            .and_then(|v| v.as_f64())
            .or_else(|| tf_config.get(key).and_then(|v| v.as_f64()))
            .unwrap_or(default) as f32
    };

    // Re-read pool AFTER transform inport processing
    let tf_pool: HashMap<String, Value> = ctx.get_pool("_tf").into_iter().collect();
    let get_tf = |key: &str, default: f64| -> f32 {
        tf_pool
            .get(key)
            .and_then(|v| v.as_f64())
            .or_else(|| tf_config.get(key).and_then(|v| v.as_f64()))
            .unwrap_or(default) as f32
    };

    let tx = get_tf("x", 0.0);
    let ty = get_tf("y", 0.0);
    let rot = get_tf("rotation", 0.0);
    let scale = get_tf("scale", 1.0);

    let transform = tsk::Transform::from_translate(tx, ty)
        .post_rotate(rot)
        .post_scale(scale, scale);

    let fill_rule = tsk::FillRule::Winding;

    // Fill
    if let Some(ref fill) = fill_cfg {
        let color = parse_color(fill.get("color"));
        let mut paint = tsk::Paint::default();
        paint.set_color(color);
        paint.anti_alias = anti_alias;

        pixmap.fill_path(&ts_path, &paint, fill_rule, transform, None);
    }

    // Stroke
    if let Some(ref stroke) = stroke_cfg {
        let color = parse_color(stroke.get("color"));
        let stroke_width = stroke.get("width").and_then(|v| v.as_f64()).unwrap_or(1.0) as f32;

        let mut paint = tsk::Paint::default();
        paint.set_color(color);
        paint.anti_alias = anti_alias;

        let mut ts_stroke = tsk::Stroke::default();
        ts_stroke.width = stroke_width;

        if let Some(cap) = stroke.get("cap").and_then(|v| v.as_str()) {
            ts_stroke.line_cap = match cap {
                "round" => tsk::LineCap::Round,
                "square" => tsk::LineCap::Square,
                _ => tsk::LineCap::Butt,
            };
        }
        if let Some(join) = stroke.get("join").and_then(|v| v.as_str()) {
            ts_stroke.line_join = match join {
                "round" => tsk::LineJoin::Round,
                "bevel" => tsk::LineJoin::Bevel,
                _ => tsk::LineJoin::Miter,
            };
        }
        if let Some(dash) = stroke.get("dashArray").and_then(|v| v.as_array()) {
            let intervals: Vec<f32> = dash
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();
            if !intervals.is_empty() {
                let offset = stroke
                    .get("dashOffset")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as f32;
                ts_stroke.dash = tsk::StrokeDash::new(intervals, offset);
            }
        }

        pixmap.stroke_path(&ts_path, &paint, &ts_stroke, transform, None);
    }

    // Output RGBA bytes
    let rgba = pixmap.data().to_vec();

    let mut out = HashMap::new();
    out.insert("image".to_string(), Message::bytes(rgba));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "width": width,
            "height": height,
            "format": "rgba",
            "hasFill": fill_cfg.is_some(),
            "hasStroke": stroke_cfg.is_some(),
        }))),
    );
    Ok(out)
}

fn path2d_to_tiny_skia(path: &reflow_vector::Path2D) -> Option<tsk::Path> {
    use reflow_vector::path::PathCmd;
    let mut pb = tsk::PathBuilder::new();

    for cmd in &path.commands {
        match cmd {
            PathCmd::MoveTo(p) => pb.move_to(p.x as f32, p.y as f32),
            PathCmd::LineTo(p) => pb.line_to(p.x as f32, p.y as f32),
            PathCmd::QuadTo(c, p) => pb.quad_to(c.x as f32, c.y as f32, p.x as f32, p.y as f32),
            PathCmd::CubicTo(c1, c2, p) => pb.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                p.x as f32,
                p.y as f32,
            ),
            PathCmd::ArcTo { end, .. } => {
                // Simplified: line to endpoint (proper arc conversion is complex)
                pb.line_to(end.x as f32, end.y as f32);
            }
            PathCmd::Close => pb.close(),
        }
    }

    pb.finish()
}

fn parse_color(val: Option<&Value>) -> tsk::Color {
    match val {
        Some(Value::String(s)) => {
            let c = reflow_vector::Color::hex(s);
            let [r, g, b, a] = c.to_u8();
            tsk::Color::from_rgba8(r, g, b, a)
        }
        Some(Value::Array(arr)) => {
            let r = arr.first().and_then(|v| v.as_f64()).unwrap_or(0.0);
            let g = arr.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b = arr.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let a = arr.get(3).and_then(|v| v.as_f64()).unwrap_or(1.0);
            tsk::Color::from_rgba(r as f32, g as f32, b as f32, a as f32)
                .unwrap_or(tsk::Color::BLACK)
        }
        _ => tsk::Color::BLACK,
    }
}
