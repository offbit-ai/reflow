//! Easing functions — data-driven cubic bezier + presets.
//!
//! All easing is a function `f(t) → t` where t is normalized [0..1].
//! Stored as JSON, evaluated at runtime. No actor — pure math.
//!
//! ## Format
//!
//! Easing is a string in component data:
//!
//! - Presets: `"linear"`, `"easeIn"`, `"easeOut"`, `"easeInOut"`
//! - Cubic bezier: `"cubicBezier(0.4, 0.0, 0.2, 1.0)"`
//! - Spring: `"spring(300, 20, 1)"` — stiffness, damping, mass
//! - Steps: `"steps(5)"` — staircase
//! - Named: `"bounceOut"`, `"elasticOut"`, `"backInOut"`

/// Evaluate an easing function string at progress t ∈ [0, 1].
pub fn eval(easing: &str, t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);

    if let Some(params) = easing.strip_prefix("cubicBezier(").and_then(|s| s.strip_suffix(')')) {
        let p: Vec<f64> = params.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        if p.len() == 4 {
            return cubic_bezier(p[0], p[1], p[2], p[3], t);
        }
    }

    if let Some(params) = easing.strip_prefix("spring(").and_then(|s| s.strip_suffix(')')) {
        let p: Vec<f64> = params.split(',').filter_map(|s| s.trim().parse().ok()).collect();
        let stiffness = p.first().copied().unwrap_or(300.0);
        let damping = p.get(1).copied().unwrap_or(20.0);
        let mass = p.get(2).copied().unwrap_or(1.0);
        return spring(stiffness, damping, mass, t);
    }

    if let Some(params) = easing.strip_prefix("steps(").and_then(|s| s.strip_suffix(')')) {
        let n: usize = params.trim().parse().unwrap_or(1);
        return steps(n, t);
    }

    match easing {
        "linear" => t,

        // Quad
        "easeInQuad" => t * t,
        "easeOutQuad" => t * (2.0 - t),
        "easeInOutQuad" => if t < 0.5 { 2.0 * t * t } else { -1.0 + (4.0 - 2.0 * t) * t },

        // Cubic
        "easeIn" | "easeInCubic" => t * t * t,
        "easeOut" | "easeOutCubic" => { let u = t - 1.0; u * u * u + 1.0 },
        "easeInOut" | "easeInOutCubic" => {
            if t < 0.5 { 4.0 * t * t * t } else { let u = 2.0 * t - 2.0; 0.5 * u * u * u + 1.0 }
        },

        // Quart
        "easeInQuart" => t * t * t * t,
        "easeOutQuart" => { let u = t - 1.0; 1.0 - u * u * u * u },
        "easeInOutQuart" => {
            if t < 0.5 { 8.0 * t * t * t * t } else { let u = t - 1.0; 1.0 - 8.0 * u * u * u * u }
        },

        // Expo
        "easeInExpo" => if t == 0.0 { 0.0 } else { (2.0f64).powf(10.0 * (t - 1.0)) },
        "easeOutExpo" => if t == 1.0 { 1.0 } else { 1.0 - (2.0f64).powf(-10.0 * t) },
        "easeInOutExpo" => {
            if t == 0.0 { 0.0 }
            else if t == 1.0 { 1.0 }
            else if t < 0.5 { (2.0f64).powf(20.0 * t - 10.0) / 2.0 }
            else { (2.0 - (2.0f64).powf(-20.0 * t + 10.0)) / 2.0 }
        },

        // Back (overshoot)
        "backIn" | "easeInBack" => {
            let s = 1.70158;
            t * t * ((s + 1.0) * t - s)
        },
        "backOut" | "easeOutBack" => {
            let s = 1.70158;
            let u = t - 1.0;
            u * u * ((s + 1.0) * u + s) + 1.0
        },
        "backInOut" | "easeInOutBack" => {
            let s = 1.70158 * 1.525;
            if t < 0.5 {
                let u = 2.0 * t;
                (u * u * ((s + 1.0) * u - s)) / 2.0
            } else {
                let u = 2.0 * t - 2.0;
                (u * u * ((s + 1.0) * u + s) + 2.0) / 2.0
            }
        },

        // Bounce
        "bounceOut" | "easeOutBounce" => bounce_out(t),
        "bounceIn" | "easeInBounce" => 1.0 - bounce_out(1.0 - t),
        "bounceInOut" | "easeInOutBounce" => {
            if t < 0.5 { (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0 }
            else { (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0 }
        },

        // Elastic
        "elasticOut" | "easeOutElastic" => {
            if t == 0.0 || t == 1.0 { t }
            else { (2.0f64).powf(-10.0 * t) * ((t * 10.0 - 0.75) * std::f64::consts::TAU / 3.0).sin() + 1.0 }
        },
        "elasticIn" | "easeInElastic" => {
            if t == 0.0 || t == 1.0 { t }
            else { -(2.0f64).powf(10.0 * t - 10.0) * ((t * 10.0 - 10.75) * std::f64::consts::TAU / 3.0).sin() }
        },
        "elasticInOut" | "easeInOutElastic" => {
            if t == 0.0 || t == 1.0 { t }
            else if t < 0.5 {
                -((2.0f64).powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * std::f64::consts::TAU / 4.5).sin()) / 2.0
            } else {
                ((2.0f64).powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * std::f64::consts::TAU / 4.5).sin()) / 2.0 + 1.0
            }
        },

        // Sine
        "easeInSine" => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
        "easeOutSine" => (t * std::f64::consts::FRAC_PI_2).sin(),
        "easeInOutSine" => -(((std::f64::consts::PI * t).cos() - 1.0) / 2.0),

        // Circ
        "easeInCirc" => 1.0 - (1.0 - t * t).sqrt(),
        "easeOutCirc" => { let u = t - 1.0; (1.0 - u * u).sqrt() },
        "easeInOutCirc" => {
            if t < 0.5 { (1.0 - (1.0 - (2.0 * t).powi(2)).sqrt()) / 2.0 }
            else { ((1.0 - (-2.0 * t + 2.0).powi(2)).sqrt() + 1.0) / 2.0 }
        },

        // CSS named easings
        "ease" => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
        "ease-in" => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
        "ease-out" => cubic_bezier(0.0, 0.0, 0.58, 1.0, t),
        "ease-in-out" => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),

        // Material Design
        "materialStandard" => cubic_bezier(0.2, 0.0, 0.0, 1.0, t),
        "materialDecelerate" => cubic_bezier(0.0, 0.0, 0.0, 1.0, t),
        "materialAccelerate" => cubic_bezier(0.3, 0.0, 1.0, 1.0, t),

        _ => t, // fallback to linear
    }
}

/// Cubic bezier easing — same algorithm as CSS transitions.
/// Control points: (0,0), (x1,y1), (x2,y2), (1,1).
fn cubic_bezier(x1: f64, y1: f64, x2: f64, y2: f64, t: f64) -> f64 {
    // Find t_bezier such that bezier_x(t_bezier) = t using Newton's method
    let mut guess = t;
    for _ in 0..8 {
        let x = bezier_component(x1, x2, guess) - t;
        let dx = bezier_derivative(x1, x2, guess);
        if dx.abs() < 1e-12 { break; }
        guess -= x / dx;
        guess = guess.clamp(0.0, 1.0);
    }
    bezier_component(y1, y2, guess)
}

fn bezier_component(p1: f64, p2: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
}

fn bezier_derivative(p1: f64, p2: f64, t: f64) -> f64 {
    let u = 1.0 - t;
    3.0 * u * u * p1 + 6.0 * u * t * (p2 - p1) + 3.0 * t * t * (1.0 - p2)
}

/// Spring physics easing — critically/over/under-damped.
fn spring(stiffness: f64, damping: f64, mass: f64, t: f64) -> f64 {
    let omega = (stiffness / mass).sqrt();
    let zeta = damping / (2.0 * (stiffness * mass).sqrt());

    if zeta < 1.0 {
        // Underdamped
        let omega_d = omega * (1.0 - zeta * zeta).sqrt();
        1.0 - (-zeta * omega * t).exp() * ((omega_d * t).cos() + (zeta * omega / omega_d) * (omega_d * t).sin())
    } else if (zeta - 1.0).abs() < 1e-6 {
        // Critically damped
        1.0 - (1.0 + omega * t) * (-omega * t).exp()
    } else {
        // Overdamped
        let s1 = -omega * (zeta - (zeta * zeta - 1.0).sqrt());
        let s2 = -omega * (zeta + (zeta * zeta - 1.0).sqrt());
        let a = s1 / (s1 - s2);
        let b = -s2 / (s1 - s2);
        1.0 - a * (s2 * t).exp() - b * (s1 * t).exp()
    }
}

/// Steps easing — staircase function.
fn steps(n: usize, t: f64) -> f64 {
    if n == 0 { return t; }
    ((t * n as f64).floor() / n as f64).clamp(0.0, 1.0)
}

fn bounce_out(t: f64) -> f64 {
    if t < 1.0 / 2.75 {
        7.5625 * t * t
    } else if t < 2.0 / 2.75 {
        let t = t - 1.5 / 2.75;
        7.5625 * t * t + 0.75
    } else if t < 2.5 / 2.75 {
        let t = t - 2.25 / 2.75;
        7.5625 * t * t + 0.9375
    } else {
        let t = t - 2.625 / 2.75;
        7.5625 * t * t + 0.984375
    }
}

/// Interpolate a scalar value with easing.
pub fn lerp_eased(from: f64, to: f64, t: f64, easing: &str) -> f64 {
    let e = eval(easing, t);
    from + (to - from) * e
}

/// Interpolate a vec3 with easing.
pub fn lerp_vec3_eased(from: [f64; 3], to: [f64; 3], t: f64, easing: &str) -> [f64; 3] {
    let e = eval(easing, t);
    [
        from[0] + (to[0] - from[0]) * e,
        from[1] + (to[1] - from[1]) * e,
        from[2] + (to[2] - from[2]) * e,
    ]
}

/// Slerp a quaternion [x,y,z,w] with easing.
pub fn slerp_eased(from: [f64; 4], to: [f64; 4], t: f64, easing: &str) -> [f64; 4] {
    let e = eval(easing, t);

    let mut dot = from[0]*to[0] + from[1]*to[1] + from[2]*to[2] + from[3]*to[3];
    let mut b = to;
    if dot < 0.0 {
        dot = -dot;
        b = [-to[0], -to[1], -to[2], -to[3]];
    }

    if dot > 0.9995 {
        // Near-parallel: linear interpolation
        let r = [
            from[0] + (b[0] - from[0]) * e,
            from[1] + (b[1] - from[1]) * e,
            from[2] + (b[2] - from[2]) * e,
            from[3] + (b[3] - from[3]) * e,
        ];
        let len = (r[0]*r[0] + r[1]*r[1] + r[2]*r[2] + r[3]*r[3]).sqrt();
        return [r[0]/len, r[1]/len, r[2]/len, r[3]/len];
    }

    let theta = dot.acos();
    let sin_theta = theta.sin();
    let wa = ((1.0 - e) * theta).sin() / sin_theta;
    let wb = (e * theta).sin() / sin_theta;

    [
        from[0] * wa + b[0] * wb,
        from[1] * wa + b[1] * wb,
        from[2] * wa + b[2] * wb,
        from[3] * wa + b[3] * wb,
    ]
}
