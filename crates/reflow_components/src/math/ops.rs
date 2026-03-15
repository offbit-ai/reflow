//! All math operation actors.

use crate::{Actor, ActorBehavior, Message, Port};
use actor_macro::actor;
use anyhow::{Error, Result};
use reflow_actor::ActorContext;
use std::collections::HashMap;

fn get_float(msg: Option<&Message>) -> f64 {
    match msg {
        Some(Message::Float(v)) => *v,
        Some(Message::Integer(v)) => *v as f64,
        Some(Message::String(s)) => s.parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn get_float_vec(msg: Option<&Message>) -> Vec<f64> {
    match msg {
        Some(Message::Array(arr)) => arr
            .iter()
            .map(|v| match v.decode::<f64>() {
                Some(f) => f,
                None => 0.0,
            })
            .collect(),
        _ => vec![],
    }
}

fn precision(config: &HashMap<String, serde_json::Value>) -> u32 {
    config
        .get("precision")
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as u32
}

fn round_to(value: f64, decimals: u32) -> f64 {
    let factor = 10f64.powi(decimals as i32);
    (value * factor).round() / factor
}

fn result_output(value: f64, prec: u32) -> HashMap<String, Message> {
    let mut out = HashMap::new();
    out.insert("result".to_string(), Message::Float(round_to(value, prec)));
    out
}

// ── Add ─────────────────────────────────────────────────────────

#[actor(MathAddActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn math_add_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let a = get_float(payload.get("a"));
    let b = get_float(payload.get("b"));
    Ok(result_output(a + b, precision(&config)))
}

// ── Subtract ────────────────────────────────────────────────────

#[actor(MathSubtractActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn math_subtract_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let a = get_float(payload.get("a"));
    let b = get_float(payload.get("b"));
    Ok(result_output(a - b, precision(&config)))
}

// ── Multiply ────────────────────────────────────────────────────

#[actor(MathMultiplyActor, inports::<10>(a, b), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn math_multiply_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let a = get_float(payload.get("a"));
    let b = get_float(payload.get("b"));
    Ok(result_output(a * b, precision(&config)))
}

// ── Divide ──────────────────────────────────────────────────────

#[actor(MathDivideActor, inports::<10>(a, b), outports::<1>(result, error), state(MemoryState), await_all_inports)]
pub async fn math_divide_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let a = get_float(payload.get("a"));
    let b = get_float(payload.get("b"));
    if b == 0.0 {
        let mut out = HashMap::new();
        out.insert(
            "error".to_string(),
            Message::Error("Division by zero".to_string().into()),
        );
        return Ok(out);
    }
    Ok(result_output(a / b, precision(&config)))
}

// ── Modulo ──────────────────────────────────────────────────────

#[actor(MathModuloActor, inports::<10>(a, b), outports::<1>(result, error), state(MemoryState), await_all_inports)]
pub async fn math_modulo_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let a = get_float(payload.get("a"));
    let b = get_float(payload.get("b"));
    if b == 0.0 {
        let mut out = HashMap::new();
        out.insert(
            "error".to_string(),
            Message::Error("Modulo by zero".to_string().into()),
        );
        return Ok(out);
    }
    Ok(result_output(a % b, precision(&config)))
}

// ── Power ───────────────────────────────────────────────────────

#[actor(MathPowerActor, inports::<10>(base, exponent), outports::<1>(result), state(MemoryState), await_all_inports)]
pub async fn math_power_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let base = get_float(payload.get("base"));
    let exp = get_float(payload.get("exponent"));
    Ok(result_output(base.powf(exp), precision(&config)))
}

// ── Sqrt ────────────────────────────────────────────────────────

#[actor(MathSqrtActor, inports::<10>(input), outports::<1>(result, error), state(MemoryState))]
pub async fn math_sqrt_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let v = get_float(payload.get("input"));
    if v < 0.0 {
        let mut out = HashMap::new();
        out.insert(
            "error".to_string(),
            Message::Error("Square root of negative number".to_string().into()),
        );
        return Ok(out);
    }
    Ok(result_output(v.sqrt(), precision(&config)))
}

// ── Absolute ────────────────────────────────────────────────────

#[actor(MathAbsoluteActor, inports::<10>(input), outports::<1>(result), state(MemoryState))]
pub async fn math_absolute_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let v = get_float(payload.get("input"));
    Ok(result_output(v.abs(), precision(&config)))
}

// ── Clamp ───────────────────────────────────────────────────────

#[actor(MathClampActor, inports::<10>(input), outports::<1>(result), state(MemoryState))]
pub async fn math_clamp_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let v = get_float(payload.get("input"));
    let min = config.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max = config.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0);
    Ok(result_output(v.clamp(min, max), precision(&config)))
}

// ── Min/Max ─────────────────────────────────────────────────────

#[actor(MathMinMaxActor, inports::<10>(a, b), outports::<1>(min, max), state(MemoryState), await_all_inports)]
pub async fn math_min_max_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let a = get_float(payload.get("a"));
    let b = get_float(payload.get("b"));
    let prec = precision(&config);
    let mut out = HashMap::new();
    out.insert("min".to_string(), Message::Float(round_to(a.min(b), prec)));
    out.insert("max".to_string(), Message::Float(round_to(a.max(b), prec)));
    Ok(out)
}

// ── Round ───────────────────────────────────────────────────────

#[actor(MathRoundActor, inports::<10>(input), outports::<1>(result), state(MemoryState))]
pub async fn math_round_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let v = get_float(payload.get("input"));
    let mode = config
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("round");
    let prec = precision(&config);
    let result = match mode {
        "floor" => v.floor(),
        "ceil" => v.ceil(),
        "trunc" => v.trunc(),
        _ => round_to(v, prec),
    };
    Ok(result_output(result, prec))
}

// ── Random ──────────────────────────────────────────────────────

#[actor(MathRandomActor, inports::<1>(trigger), outports::<1>(result), state(MemoryState))]
pub async fn math_random_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let min = config.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max = config.get("max").and_then(|v| v.as_f64()).unwrap_or(1.0);
    // Simple PRNG (not cryptographic)
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let random = (seed as f64 / u32::MAX as f64) * (max - min) + min;
    Ok(result_output(random, precision(&config)))
}

// ── Average ─────────────────────────────────────────────────────

#[actor(MathAverageActor, inports::<10>(values), outports::<1>(result), state(MemoryState))]
pub async fn math_average_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let values = get_float_vec(payload.get("values"));
    let avg = if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    };
    Ok(result_output(avg, precision(&config)))
}

// ── Sum ─────────────────────────────────────────────────────────

#[actor(MathSumActor, inports::<10>(values), outports::<1>(result), state(MemoryState))]
pub async fn math_sum_actor(context: ActorContext) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let values = get_float_vec(payload.get("values"));
    Ok(result_output(values.iter().sum(), precision(&config)))
}

// ── Statistics ──────────────────────────────────────────────────

#[actor(MathStatisticsActor, inports::<10>(values), outports::<1>(mean, median, stddev, min, max, count), state(MemoryState))]
pub async fn math_statistics_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let payload = context.get_payload();
    let config = context.get_config_hashmap();
    let mut values = get_float_vec(payload.get("values"));
    let prec = precision(&config);
    let count = values.len();

    if count == 0 {
        let mut out = HashMap::new();
        out.insert("count".to_string(), Message::Integer(0));
        return Ok(out);
    }

    let mean = values.iter().sum::<f64>() / count as f64;

    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if count % 2 == 0 {
        (values[count / 2 - 1] + values[count / 2]) / 2.0
    } else {
        values[count / 2]
    };

    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count as f64;
    let stddev = variance.sqrt();

    let mut out = HashMap::new();
    out.insert("mean".to_string(), Message::Float(round_to(mean, prec)));
    out.insert("median".to_string(), Message::Float(round_to(median, prec)));
    out.insert("stddev".to_string(), Message::Float(round_to(stddev, prec)));
    out.insert("min".to_string(), Message::Float(round_to(values[0], prec)));
    out.insert(
        "max".to_string(),
        Message::Float(round_to(values[count - 1], prec)),
    );
    out.insert("count".to_string(), Message::Integer(count as i64));
    Ok(out)
}

// ── Expression ──────────────────────────────────────────────────

/// Evaluates a simple math expression string.
/// Supports: +, -, *, /, ^, (, ), and variable substitution from payload.
#[actor(MathExpressionActor, inports::<10>(input), outports::<1>(result, error), state(MemoryState))]
pub async fn math_expression_actor(
    context: ActorContext,
) -> Result<HashMap<String, Message>, Error> {
    let config = context.get_config_hashmap();
    let payload = context.get_payload();

    let expr = config
        .get("expression")
        .and_then(|v| v.as_str())
        .unwrap_or("0");

    // Simple substitution: replace $input with the input value
    let input_val = get_float(payload.get("input"));
    let resolved = expr
        .replace("$input", &input_val.to_string())
        .replace("$x", &input_val.to_string());

    // Evaluate using a minimal recursive descent parser
    match eval_expr(&resolved) {
        Ok(v) => Ok(result_output(v, precision(&config))),
        Err(e) => {
            let mut out = HashMap::new();
            out.insert("error".to_string(), Message::Error(e.into()));
            Ok(out)
        }
    }
}

// Minimal expression evaluator (no external deps)
fn eval_expr(s: &str) -> Result<f64, String> {
    let tokens: Vec<char> = s.chars().filter(|c| !c.is_whitespace()).collect();
    let mut pos = 0;
    let result = parse_add_sub(&tokens, &mut pos)?;
    if pos < tokens.len() {
        Err(format!("Unexpected character at position {}", pos))
    } else {
        Ok(result)
    }
}

fn parse_add_sub(tokens: &[char], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_mul_div(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            '+' => {
                *pos += 1;
                left += parse_mul_div(tokens, pos)?;
            }
            '-' => {
                *pos += 1;
                left -= parse_mul_div(tokens, pos)?;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_mul_div(tokens: &[char], pos: &mut usize) -> Result<f64, String> {
    let mut left = parse_power(tokens, pos)?;
    while *pos < tokens.len() {
        match tokens[*pos] {
            '*' => {
                *pos += 1;
                left *= parse_power(tokens, pos)?;
            }
            '/' => {
                *pos += 1;
                let right = parse_power(tokens, pos)?;
                if right == 0.0 {
                    return Err("Division by zero".to_string());
                }
                left /= right;
            }
            '%' => {
                *pos += 1;
                let right = parse_power(tokens, pos)?;
                if right == 0.0 {
                    return Err("Modulo by zero".to_string());
                }
                left %= right;
            }
            _ => break,
        }
    }
    Ok(left)
}

fn parse_power(tokens: &[char], pos: &mut usize) -> Result<f64, String> {
    let base = parse_unary(tokens, pos)?;
    if *pos < tokens.len() && tokens[*pos] == '^' {
        *pos += 1;
        let exp = parse_power(tokens, pos)?; // right-associative
        Ok(base.powf(exp))
    } else {
        Ok(base)
    }
}

fn parse_unary(tokens: &[char], pos: &mut usize) -> Result<f64, String> {
    if *pos < tokens.len() && tokens[*pos] == '-' {
        *pos += 1;
        Ok(-parse_atom(tokens, pos)?)
    } else {
        parse_atom(tokens, pos)
    }
}

fn parse_atom(tokens: &[char], pos: &mut usize) -> Result<f64, String> {
    if *pos >= tokens.len() {
        return Err("Unexpected end of expression".to_string());
    }
    if tokens[*pos] == '(' {
        *pos += 1;
        let result = parse_add_sub(tokens, pos)?;
        if *pos < tokens.len() && tokens[*pos] == ')' {
            *pos += 1;
        }
        return Ok(result);
    }
    // Parse number
    let start = *pos;
    while *pos < tokens.len() && (tokens[*pos].is_ascii_digit() || tokens[*pos] == '.') {
        *pos += 1;
    }
    if start == *pos {
        return Err(format!("Expected number at position {}", *pos));
    }
    let num_str: String = tokens[start..*pos].iter().collect();
    num_str
        .parse::<f64>()
        .map_err(|_| format!("Invalid number: {}", num_str))
}
