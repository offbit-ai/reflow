//! SDF Path actor — builds a tubular shape from a spine curve + radius profile.
//!
//! Takes an SVG-like path DSL defining a 2D spine curve, plus a radius
//! profile array. Places spheres along the sampled curve, smooth-unions
//! them into a continuous tubular shape, and outputs the SDF tree.
//!
//! DSL syntax (subset of SVG path):
//!   M x,y        — move to (start point)
//!   L x,y        — line to
//!   C cx1,cy1 cx2,cy2 x,y  — cubic bezier
//!   Q cx,cy x,y  — quadratic bezier
//!
//! Config:
//!   path: string — SVG-like path string
//!   profile: [f32] — radius at evenly spaced points (interpolated to segments)
//!   segments: u32 — number of spheres along the path (default 16)
//!   smoothness: f32 — smooth union k factor (default 0.1)
//!   plane: "xz" | "xy" | "yz" — which plane the 2D path maps to (default "xz")

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext, MemoryState};
use reflow_sdf::ir::SdfNode;
use std::collections::HashMap;

fn sdf_output(node: &SdfNode) -> HashMap<String, Message> {
    let json = serde_json::to_value(node).unwrap_or_default();
    let mut out = HashMap::new();
    out.insert(
        "sdf".to_string(),
        Message::object(EncodableValue::from(json)),
    );
    out
}

#[actor(
    SdfPathActor,
    inports::<1>(),
    outports::<1>(sdf, metadata, error),
    state(MemoryState)
)]
pub async fn sdf_path_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = ctx.get_config_hashmap();

    let path_str = config
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or("M 0,0 L 5,0");
    let profile: Vec<f32> = config
        .get("profile")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_f64().map(|f| f as f32)).collect())
        .unwrap_or_else(|| vec![0.1]);
    let segments = config
        .get("segments")
        .and_then(|v| v.as_u64())
        .unwrap_or(16) as usize;
    let smoothness = config
        .get("smoothness")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.1) as f32;
    let plane = config
        .get("plane")
        .and_then(|v| v.as_str())
        .unwrap_or("xz");

    // Parse path into 2D points
    let commands = parse_path(path_str);
    if commands.is_empty() {
        return Ok(error_out("Empty or invalid path"));
    }

    // Sample `segments` evenly-spaced points along the path
    let points = sample_path(&commands, segments);
    if points.len() < 2 {
        return Ok(error_out("Path produced fewer than 2 points"));
    }

    // Interpolate radius profile to match point count
    let radii = interpolate_profile(&profile, points.len());

    // Build SDF as a single TubePath primitive — evaluates min distance
    // across all segments in one WGSL function call. No union artifacts.
    let mut pts3d: Vec<[f32; 3]> = Vec::with_capacity(points.len());
    for (px, py) in &points {
        let p = match plane {
            "xy" => [*px, *py, 0.0],
            "yz" => [0.0, *px, *py],
            _ => [*px, 0.0, *py],
        };
        pts3d.push(p);
    }

    let node = SdfNode::tube_path(pts3d, radii, smoothness);

    let mut out = sdf_output(&node);
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(serde_json::json!({
            "segments": points.len(),
            "pathLength": path_length(&points),
        }))),
    );
    Ok(out)
}

// ─── Path parsing ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum PathCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    CubicBezier(f32, f32, f32, f32, f32, f32), // cx1,cy1, cx2,cy2, x,y
    QuadBezier(f32, f32, f32, f32),             // cx,cy, x,y
}

pub fn parse_path(s: &str) -> Vec<PathCmd> {
    let mut cmds = Vec::new();
    let mut chars = s.chars().peekable();
    let mut current_cmd = ' ';

    while chars.peek().is_some() {
        // Skip whitespace
        while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            chars.next();
        }

        // Check for command letter
        if let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                current_cmd = c;
                chars.next();
            }
        }

        // Skip whitespace after command
        while chars.peek().map(|c| c.is_whitespace()).unwrap_or(false) {
            chars.next();
        }

        match current_cmd.to_ascii_uppercase() {
            'M' | 'L' => {
                if let Some((x, y)) = parse_pair(&mut chars) {
                    let cmd = if current_cmd.to_ascii_uppercase() == 'M' {
                        PathCmd::MoveTo(x, y)
                    } else {
                        PathCmd::LineTo(x, y)
                    };
                    cmds.push(cmd);
                }
            }
            'C' => {
                if let (Some((cx1, cy1)), Some((cx2, cy2)), Some((x, y))) = (
                    parse_pair(&mut chars),
                    parse_pair(&mut chars),
                    parse_pair(&mut chars),
                ) {
                    cmds.push(PathCmd::CubicBezier(cx1, cy1, cx2, cy2, x, y));
                }
            }
            'Q' => {
                if let (Some((cx, cy)), Some((x, y))) =
                    (parse_pair(&mut chars), parse_pair(&mut chars))
                {
                    cmds.push(PathCmd::QuadBezier(cx, cy, x, y));
                }
            }
            _ => {
                chars.next(); // skip unknown
            }
        }
    }

    cmds
}

fn parse_pair(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<(f32, f32)> {
    let x = parse_number(chars)?;
    // Skip comma or whitespace
    while chars
        .peek()
        .map(|c| *c == ',' || c.is_whitespace())
        .unwrap_or(false)
    {
        chars.next();
    }
    let y = parse_number(chars)?;
    // Skip trailing comma/whitespace
    while chars
        .peek()
        .map(|c| *c == ',' || c.is_whitespace())
        .unwrap_or(false)
    {
        chars.next();
    }
    Some((x, y))
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f32> {
    let mut s = String::new();
    // Allow leading minus
    if chars.peek() == Some(&'-') {
        s.push('-');
        chars.next();
    }
    while chars
        .peek()
        .map(|c| c.is_ascii_digit() || *c == '.')
        .unwrap_or(false)
    {
        s.push(chars.next().unwrap());
    }
    s.parse().ok()
}

// ─── Path sampling ───────────────────────────────────────────────

/// Sample N evenly-spaced points along the parsed path commands.
pub fn sample_path(cmds: &[PathCmd], n: usize) -> Vec<(f32, f32)> {
    // First, expand commands into fine-grained line segments
    let mut polyline: Vec<(f32, f32)> = Vec::new();
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;

    for cmd in cmds {
        match cmd {
            PathCmd::MoveTo(x, y) => {
                cx = *x;
                cy = *y;
                polyline.push((cx, cy));
            }
            PathCmd::LineTo(x, y) => {
                polyline.push((*x, *y));
                cx = *x;
                cy = *y;
            }
            PathCmd::CubicBezier(cx1, cy1, cx2, cy2, x, y) => {
                let steps = 20;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let (px, py) =
                        cubic_bezier(cx, cy, *cx1, *cy1, *cx2, *cy2, *x, *y, t);
                    polyline.push((px, py));
                }
                cx = *x;
                cy = *y;
            }
            PathCmd::QuadBezier(qcx, qcy, x, y) => {
                let steps = 15;
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let (px, py) = quad_bezier(cx, cy, *qcx, *qcy, *x, *y, t);
                    polyline.push((px, py));
                }
                cx = *x;
                cy = *y;
            }
        }
    }

    if polyline.len() < 2 {
        return polyline;
    }

    // Compute cumulative arc lengths
    let mut lengths = vec![0.0f32];
    for i in 1..polyline.len() {
        let dx = polyline[i].0 - polyline[i - 1].0;
        let dy = polyline[i].1 - polyline[i - 1].1;
        lengths.push(lengths[i - 1] + (dx * dx + dy * dy).sqrt());
    }
    let total = *lengths.last().unwrap();

    // Sample N evenly spaced points by arc length
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let target = total * (i as f32 / (n - 1).max(1) as f32);
        // Find segment containing target
        let seg = lengths
            .windows(2)
            .position(|w| w[0] <= target && target <= w[1])
            .unwrap_or(polyline.len() - 2);
        let seg_len = lengths[seg + 1] - lengths[seg];
        let t = if seg_len > 1e-6 {
            (target - lengths[seg]) / seg_len
        } else {
            0.0
        };
        let px = polyline[seg].0 + t * (polyline[seg + 1].0 - polyline[seg].0);
        let py = polyline[seg].1 + t * (polyline[seg + 1].1 - polyline[seg].1);
        result.push((px, py));
    }

    result
}

fn cubic_bezier(
    x0: f32, y0: f32, cx1: f32, cy1: f32, cx2: f32, cy2: f32, x1: f32, y1: f32, t: f32,
) -> (f32, f32) {
    let u = 1.0 - t;
    let u2 = u * u;
    let u3 = u2 * u;
    let t2 = t * t;
    let t3 = t2 * t;
    (
        u3 * x0 + 3.0 * u2 * t * cx1 + 3.0 * u * t2 * cx2 + t3 * x1,
        u3 * y0 + 3.0 * u2 * t * cy1 + 3.0 * u * t2 * cy2 + t3 * y1,
    )
}

fn quad_bezier(x0: f32, y0: f32, cx: f32, cy: f32, x1: f32, y1: f32, t: f32) -> (f32, f32) {
    let u = 1.0 - t;
    (
        u * u * x0 + 2.0 * u * t * cx + t * t * x1,
        u * u * y0 + 2.0 * u * t * cy + t * t * y1,
    )
}

fn path_length(points: &[(f32, f32)]) -> f32 {
    let mut len = 0.0;
    for i in 1..points.len() {
        let dx = points[i].0 - points[i - 1].0;
        let dy = points[i].1 - points[i - 1].1;
        len += (dx * dx + dy * dy).sqrt();
    }
    len
}

pub fn interpolate_profile(profile: &[f32], n: usize) -> Vec<f32> {
    if profile.is_empty() {
        return vec![0.1; n];
    }
    if profile.len() == 1 {
        return vec![profile[0]; n];
    }
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / (n - 1).max(1) as f32;
        let pos = t * (profile.len() - 1) as f32;
        let idx = (pos as usize).min(profile.len() - 2);
        let frac = pos - idx as f32;
        result.push(profile[idx] * (1.0 - frac) + profile[idx + 1] * frac);
    }
    result
}

fn error_out(msg: &str) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert(
        "error".to_string(),
        Message::Error(msg.to_string().into()),
    );
    out
}
