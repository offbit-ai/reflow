//! L-System (Lindenmayer System) string rewriting actor.
//!
//! Takes axiom + production rules, iterates N generations,
//! outputs the resulting string and optionally interprets it
//! as turtle graphics coordinates.

use crate::{Actor, ActorBehavior, Message, Port};
use reflow_actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::{message::EncodableValue, ActorContext};
use serde_json::json;
use std::collections::HashMap;

#[actor(
    LSystemActor,
    inports::<10>(axiom),
    outports::<1>(output, points, metadata),
    state(MemoryState)
)]
pub async fn lsystem_actor(ctx: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let c = ctx.get_config_hashmap();
    let p = ctx.get_payload();

    let axiom = match p.get("axiom") {
        Some(Message::String(s)) => s.to_string(),
        _ => c
            .get("axiom")
            .and_then(|v| v.as_str())
            .unwrap_or("F")
            .to_string(),
    };

    let iterations = c.get("iterations").and_then(|v| v.as_u64()).unwrap_or(4) as usize;
    let angle = c.get("angle").and_then(|v| v.as_f64()).unwrap_or(25.0);
    let step_length = c.get("stepLength").and_then(|v| v.as_f64()).unwrap_or(1.0);

    // Parse rules from config: "F=FF+[+F-F-F]-[-F+F+F]" format
    let rules_str = c
        .get("rules")
        .and_then(|v| v.as_str())
        .unwrap_or("F=F+F-F-FF+F+F-F");
    let mut rules: HashMap<char, String> = HashMap::new();
    for rule in rules_str.split(';') {
        let parts: Vec<&str> = rule.trim().splitn(2, '=').collect();
        if parts.len() == 2 {
            if let Some(ch) = parts[0].trim().chars().next() {
                rules.insert(ch, parts[1].trim().to_string());
            }
        }
    }

    // Iterate
    let mut current = axiom;
    for _ in 0..iterations {
        let mut next = String::with_capacity(current.len() * 2);
        for ch in current.chars() {
            if let Some(replacement) = rules.get(&ch) {
                next.push_str(replacement);
            } else {
                next.push(ch);
            }
        }
        current = next;
    }

    // Turtle graphics interpretation
    let rad = angle.to_radians();
    let mut x = 0.0f64;
    let mut y = 0.0f64;
    let mut dir = std::f64::consts::FRAC_PI_2; // start facing up
    let mut stack: Vec<(f64, f64, f64)> = Vec::new();
    let mut points: Vec<[f64; 2]> = vec![[x, y]];

    for ch in current.chars() {
        match ch {
            'F' | 'G' => {
                x += dir.cos() * step_length;
                y += dir.sin() * step_length;
                points.push([x, y]);
            }
            'f' | 'g' => {
                x += dir.cos() * step_length;
                y += dir.sin() * step_length;
            }
            '+' => dir += rad,
            '-' => dir -= rad,
            '[' => stack.push((x, y, dir)),
            ']' => {
                if let Some((sx, sy, sd)) = stack.pop() {
                    x = sx;
                    y = sy;
                    dir = sd;
                    points.push([x, y]);
                }
            }
            _ => {}
        }
    }

    // Encode points as f64 LE bytes
    let point_bytes: Vec<u8> = points
        .iter()
        .flat_map(|p| [p[0].to_le_bytes(), p[1].to_le_bytes()].concat())
        .collect();

    let string_length = current.len();
    let mut out = HashMap::new();
    out.insert("output".to_string(), Message::String(current.into()));
    out.insert("points".to_string(), Message::bytes(point_bytes));
    out.insert(
        "metadata".to_string(),
        Message::object(EncodableValue::from(json!({
            "stringLength": string_length,
            "pointCount": points.len(),
            "iterations": iterations,
            "angle": angle,
        }))),
    );
    Ok(out)
}
