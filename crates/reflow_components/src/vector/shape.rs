//! Shape2D actor — generates 2D vector paths from config.
//!
//! ## Config
//!
//! ```json
//! { "shape": "rect", "width": 200, "height": 100, "cornerRadius": 10 }
//! { "shape": "ellipse", "rx": 50, "ry": 30 }
//! { "shape": "circle", "radius": 40 }
//! { "shape": "polygon", "radius": 50, "sides": 6 }
//! { "shape": "star", "outerRadius": 50, "innerRadius": 25, "points": 5 }
//! { "shape": "line", "x1": 0, "y1": 0, "x2": 100, "y2": 100 }
//! { "shape": "path", "d": "M0,0 C50,0 50,100 100,100" }
//! ```
//!
//! ## Inports
//! - `params` — override config dynamically (partial merge)
//!
//! ## Outports
//! - `path` — SVG path string (d attribute)
//! - `metadata` — bounds, length, segment count, plus any rendering
//!   properties from config (`color`, `shadow`, `border`, `index`,
//!   `cornerRadius`) for downstream GPU renderers

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::{json, Value};
use std::collections::HashMap;

#[actor(
    Shape2DActor,
    inports::<10>(params),
    outports::<1>(path, metadata),
    state(MemoryState)
)]
pub async fn shape_2d_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = ctx.get_payload();
    let config = ctx.get_config_hashmap();

    // Merge inport params over config
    let mut params = config.clone();
    if let Some(Message::Object(obj)) = payload.get("params") {
        let v: Value = obj.as_ref().clone().into();
        if let Value::Object(map) = v {
            for (k, v) in map {
                params.insert(k, v);
            }
        }
    }

    let shape = params
        .get("shape")
        .and_then(|v| v.as_str())
        .unwrap_or("rect");
    let cx = params.get("cx").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let cy = params.get("cy").and_then(|v| v.as_f64()).unwrap_or(0.0);

    let path = match shape {
        "rect" => {
            let w = params
                .get("width")
                .and_then(|v| v.as_f64())
                .unwrap_or(100.0);
            let h = params
                .get("height")
                .and_then(|v| v.as_f64())
                .unwrap_or(100.0);
            let r = params
                .get("cornerRadius")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let x = cx - w / 2.0;
            let y = cy - h / 2.0;
            reflow_vector::shapes::rect(x, y, w, h, r)
        }
        "ellipse" => {
            let rx = params.get("rx").and_then(|v| v.as_f64()).unwrap_or(50.0);
            let ry = params.get("ry").and_then(|v| v.as_f64()).unwrap_or(50.0);
            reflow_vector::shapes::ellipse(cx, cy, rx, ry)
        }
        "circle" => {
            let r = params
                .get("radius")
                .and_then(|v| v.as_f64())
                .unwrap_or(50.0);
            reflow_vector::shapes::circle(cx, cy, r)
        }
        "polygon" => {
            let r = params
                .get("radius")
                .and_then(|v| v.as_f64())
                .unwrap_or(50.0);
            let sides = params.get("sides").and_then(|v| v.as_u64()).unwrap_or(6) as usize;
            reflow_vector::shapes::polygon(cx, cy, r, sides)
        }
        "star" => {
            let outer = params
                .get("outerRadius")
                .and_then(|v| v.as_f64())
                .unwrap_or(50.0);
            let inner = params
                .get("innerRadius")
                .and_then(|v| v.as_f64())
                .unwrap_or(25.0);
            let points = params.get("points").and_then(|v| v.as_u64()).unwrap_or(5) as usize;
            reflow_vector::shapes::star(cx, cy, outer, inner, points)
        }
        "line" => {
            let x1 = params.get("x1").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y1 = params.get("y1").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let x2 = params.get("x2").and_then(|v| v.as_f64()).unwrap_or(100.0);
            let y2 = params.get("y2").and_then(|v| v.as_f64()).unwrap_or(100.0);
            reflow_vector::shapes::line(x1, y1, x2, y2)
        }
        "path" => {
            let d = params.get("d").and_then(|v| v.as_str()).unwrap_or("M0,0");
            reflow_vector::Path2D::from_svg(d)
        }
        _ => reflow_vector::shapes::rect(0.0, 0.0, 100.0, 100.0, 0.0),
    };

    let svg = path.to_svg();
    let (min, max) = path.bounds();
    let length = path.length();

    // Build metadata with geometry + passthrough rendering properties
    // Bounds = [x, y, w, h]. cx/cy define the initial center position for
    // GPU renderer fallback (before animation provides sN_x/sN_y).
    let w = max.x - min.x;
    let h = max.y - min.y;
    let bx = cx - w / 2.0;
    let by = cy - h / 2.0;
    let mut meta = json!({
        "type": shape,
        "bounds": [bx, by, w, h],
        "width": w,
        "height": h,
        "length": length,
        "segments": path.segment_count(),
        "closed": path.is_closed(),
    });

    // Pass through rendering properties for GPU consumers
    let render_keys = ["color", "shadow", "border", "index", "cornerRadius"];
    if let Some(obj) = meta.as_object_mut() {
        for key in render_keys {
            if let Some(val) = params.get(key) {
                obj.insert(key.to_string(), val.clone());
            }
        }
    }

    let mut out = HashMap::new();
    out.insert("path".to_string(), Message::String(svg.into()));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(meta)),
    );
    Ok(out)
}
