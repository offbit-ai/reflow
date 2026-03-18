//! Expression evaluator — general-purpose math expression engine.
//!
//! Used by: ECS expression system, data transforms, rules engine,
//! animation drivers, design token computation, procedural generation.
//!
//! ## Syntax
//!
//! - Arithmetic: `a + b`, `a * b`, `a / b`, `a % b`, `a - b`
//! - Grouping: `(a + b) * c`
//! - Functions: `sin(x)`, `cos(x)`, `abs(x)`, `min(a, b)`, `clamp(x, lo, hi)`
//! - Constants: `pi`, `tau`
//! - Variables: any identifier, resolved from a provided HashMap
//!
//! ## Functions
//!
//! `sin cos tan asin acos atan abs floor ceil round sqrt pow
//!  min max clamp lerp mod sign step smoothstep fract log log2`

use std::collections::HashMap;

/// Evaluate a math expression with the given variable bindings.
///
/// ```ignore
/// let mut vars = HashMap::new();
/// vars.insert("time".to_string(), 1.5);
/// vars.insert("speed".to_string(), 2.0);
/// let result = eval("sin(time * speed * pi)", &vars);
/// ```
pub fn eval(expr: &str, vars: &HashMap<String, f64>) -> Option<f64> {
    let tokens = tokenize(expr);
    let mut pos = 0;
    parse_additive(&tokens, &mut pos, vars)
}

/// Evaluate with no variables (constants + literals only).
pub fn eval_const(expr: &str) -> Option<f64> {
    let vars = HashMap::new();
    eval(expr, &vars)
}

// ─── Tokenizer ───

#[derive(Debug, Clone)]
enum Token {
    Num(f64),
    Ident(String),
    Op(char),
    LParen,
    RParen,
    Comma,
}

fn tokenize(s: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => i += 1,
            '+' | '*' | '/' | '%' | '^' => {
                tokens.push(Token::Op(chars[i]));
                i += 1;
            }
            '-' => {
                // Disambiguate unary minus vs subtraction
                let is_unary = tokens.is_empty()
                    || matches!(
                        tokens.last(),
                        Some(Token::Op(_)) | Some(Token::LParen) | Some(Token::Comma)
                    );
                if is_unary {
                    // Look ahead for a number
                    let start = i;
                    i += 1;
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                        i += 1;
                    }
                    if i > start + 1 {
                        let s: String = chars[start..i].iter().collect();
                        tokens.push(Token::Num(s.parse().unwrap_or(0.0)));
                    } else {
                        tokens.push(Token::Op('-'));
                    }
                } else {
                    tokens.push(Token::Op('-'));
                    i += 1;
                }
            }
            '(' => {
                tokens.push(Token::LParen);
                i += 1;
            }
            ')' => {
                tokens.push(Token::RParen);
                i += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                i += 1;
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                // Scientific notation: 1e5, 1.5e-3
                if i < chars.len() && (chars[i] == 'e' || chars[i] == 'E') {
                    i += 1;
                    if i < chars.len() && (chars[i] == '+' || chars[i] == '-') {
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Token::Num(s.parse().unwrap_or(0.0)));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                tokens.push(Token::Ident(s));
            }
            _ => i += 1,
        }
    }
    tokens
}

// ─── Recursive descent parser ───

fn parse_additive(tokens: &[Token], pos: &mut usize, vars: &HashMap<String, f64>) -> Option<f64> {
    let mut left = parse_multiplicative(tokens, pos, vars)?;
    while *pos < tokens.len() {
        match tokens.get(*pos) {
            Some(Token::Op('+')) => {
                *pos += 1;
                left += parse_multiplicative(tokens, pos, vars)?;
            }
            Some(Token::Op('-')) => {
                *pos += 1;
                left -= parse_multiplicative(tokens, pos, vars)?;
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_multiplicative(
    tokens: &[Token],
    pos: &mut usize,
    vars: &HashMap<String, f64>,
) -> Option<f64> {
    let mut left = parse_power(tokens, pos, vars)?;
    while *pos < tokens.len() {
        match tokens.get(*pos) {
            Some(Token::Op('*')) => {
                *pos += 1;
                left *= parse_power(tokens, pos, vars)?;
            }
            Some(Token::Op('/')) => {
                *pos += 1;
                let r = parse_power(tokens, pos, vars)?;
                left /= if r == 0.0 { 1.0 } else { r };
            }
            Some(Token::Op('%')) => {
                *pos += 1;
                let r = parse_power(tokens, pos, vars)?;
                left %= if r == 0.0 { 1.0 } else { r };
            }
            _ => break,
        }
    }
    Some(left)
}

fn parse_power(tokens: &[Token], pos: &mut usize, vars: &HashMap<String, f64>) -> Option<f64> {
    let base = parse_unary(tokens, pos, vars)?;
    if matches!(tokens.get(*pos), Some(Token::Op('^'))) {
        *pos += 1;
        let exp = parse_unary(tokens, pos, vars)?;
        Some(base.powf(exp))
    } else {
        Some(base)
    }
}

fn parse_unary(tokens: &[Token], pos: &mut usize, vars: &HashMap<String, f64>) -> Option<f64> {
    if let Some(Token::Op('-')) = tokens.get(*pos) {
        *pos += 1;
        return Some(-parse_primary(tokens, pos, vars)?);
    }
    parse_primary(tokens, pos, vars)
}

fn parse_primary(tokens: &[Token], pos: &mut usize, vars: &HashMap<String, f64>) -> Option<f64> {
    match tokens.get(*pos)? {
        Token::Num(n) => {
            let v = *n;
            *pos += 1;
            Some(v)
        }
        Token::LParen => {
            *pos += 1;
            let v = parse_additive(tokens, pos, vars)?;
            if matches!(tokens.get(*pos), Some(Token::RParen)) {
                *pos += 1;
            }
            Some(v)
        }
        Token::Ident(name) => {
            let name = name.clone();
            *pos += 1;

            // Function call
            if matches!(tokens.get(*pos), Some(Token::LParen)) {
                *pos += 1;
                let mut args = Vec::new();
                while !matches!(tokens.get(*pos), Some(Token::RParen) | None) {
                    if let Some(v) = parse_additive(tokens, pos, vars) {
                        args.push(v);
                    }
                    if matches!(tokens.get(*pos), Some(Token::Comma)) {
                        *pos += 1;
                    }
                }
                if matches!(tokens.get(*pos), Some(Token::RParen)) {
                    *pos += 1;
                }
                return eval_function(&name, &args);
            }

            // Built-in constants
            match name.as_str() {
                "pi" => Some(std::f64::consts::PI),
                "tau" => Some(std::f64::consts::TAU),
                "e" => Some(std::f64::consts::E),
                "inf" => Some(f64::INFINITY),
                "true" => Some(1.0),
                "false" => Some(0.0),
                _ => vars.get(&name).copied().or(Some(0.0)),
            }
        }
        _ => {
            *pos += 1;
            Some(0.0)
        }
    }
}

fn eval_function(name: &str, args: &[f64]) -> Option<f64> {
    let a = args.first().copied().unwrap_or(0.0);
    let b = args.get(1).copied().unwrap_or(0.0);
    let c = args.get(2).copied().unwrap_or(0.0);

    Some(match name {
        "sin" => a.sin(),
        "cos" => a.cos(),
        "tan" => a.tan(),
        "asin" => a.asin(),
        "acos" => a.acos(),
        "atan" => {
            if args.len() >= 2 {
                a.atan2(b)
            } else {
                a.atan()
            }
        }
        "abs" => a.abs(),
        "floor" => a.floor(),
        "ceil" => a.ceil(),
        "round" => a.round(),
        "sqrt" => a.sqrt(),
        "pow" => a.powf(b),
        "min" => a.min(b),
        "max" => a.max(b),
        "clamp" => a.clamp(b, c),
        "lerp" => a + (b - a) * c,
        "mod" => a % if b == 0.0 { 1.0 } else { b },
        "sign" => {
            if a > 0.0 {
                1.0
            } else if a < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        "step" => {
            if b >= a {
                1.0
            } else {
                0.0
            }
        }
        "smoothstep" => {
            let t = ((c - a) / (b - a)).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        }
        "fract" => a.fract(),
        "log" => a.ln(),
        "log2" => a.log2(),
        "exp" => a.exp(),
        "deg" => a.to_degrees(),
        "rad" => a.to_radians(),
        "mix" => a * (1.0 - c) + b * c, // GLSL-style mix(a, b, t)
        _ => 0.0,
    })
}
