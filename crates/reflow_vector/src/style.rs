//! Fill and stroke style definitions.

use serde::{Deserialize, Serialize};

/// RGBA color.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Color {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Color {
    pub fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r, g, b, a }
    }

    pub fn rgb(r: f64, g: f64, b: f64) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    pub fn hex(s: &str) -> Self {
        let s = s.trim_start_matches('#');
        let (r, g, b, a) = match s.len() {
            3 => {
                let r = u8::from_str_radix(&s[0..1], 16).unwrap_or(0);
                let g = u8::from_str_radix(&s[1..2], 16).unwrap_or(0);
                let b = u8::from_str_radix(&s[2..3], 16).unwrap_or(0);
                (r * 17, g * 17, b * 17, 255u8)
            }
            6 => {
                let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
                let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
                (r, g, b, 255u8)
            }
            8 => {
                let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0);
                let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0);
                let a = u8::from_str_radix(&s[6..8], 16).unwrap_or(255);
                (r, g, b, a)
            }
            _ => (0, 0, 0, 255),
        };
        Self {
            r: r as f64 / 255.0,
            g: g as f64 / 255.0,
            b: b as f64 / 255.0,
            a: a as f64 / 255.0,
        }
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
            a: self.a + (other.a - self.a) * t,
        }
    }

    pub fn to_u8(self) -> [u8; 4] {
        [
            (self.r.clamp(0.0, 1.0) * 255.0) as u8,
            (self.g.clamp(0.0, 1.0) * 255.0) as u8,
            (self.b.clamp(0.0, 1.0) * 255.0) as u8,
            (self.a.clamp(0.0, 1.0) * 255.0) as u8,
        ]
    }
}

/// Gradient color stop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GradientStop {
    pub offset: f64, // 0..1
    pub color: Color,
}

/// Fill style.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Fill {
    #[serde(rename = "solid")]
    Solid { color: Color },
    #[serde(rename = "linear")]
    LinearGradient {
        stops: Vec<GradientStop>,
        angle: f64, // degrees
    },
    #[serde(rename = "radial")]
    RadialGradient {
        stops: Vec<GradientStop>,
        center: [f64; 2],
        radius: f64,
    },
    #[serde(rename = "conic")]
    ConicGradient {
        stops: Vec<GradientStop>,
        center: [f64; 2],
        start_angle: f64,
    },
}

impl Fill {
    pub fn solid(color: Color) -> Self {
        Self::Solid { color }
    }

    pub fn sample_linear(stops: &[GradientStop], t: f64) -> Color {
        if stops.is_empty() {
            return Color::rgba(0.0, 0.0, 0.0, 1.0);
        }
        if stops.len() == 1 || t <= stops[0].offset {
            return stops[0].color;
        }
        if t >= stops.last().unwrap().offset {
            return stops.last().unwrap().color;
        }
        for i in 1..stops.len() {
            if t <= stops[i].offset {
                let seg = stops[i].offset - stops[i - 1].offset;
                let local_t = if seg > 0.0 { (t - stops[i - 1].offset) / seg } else { 0.0 };
                return stops[i - 1].color.lerp(stops[i].color, local_t);
            }
        }
        stops.last().unwrap().color
    }
}

/// Stroke line cap.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineCap {
    Butt,
    Round,
    Square,
}

/// Stroke line join.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}

/// Stroke style.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stroke {
    pub width: f64,
    pub color: Color,
    pub cap: LineCap,
    pub join: LineJoin,
    pub miter_limit: f64,
    #[serde(default)]
    pub dash_array: Vec<f64>,
    pub dash_offset: f64,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 1.0,
            color: Color::rgba(0.0, 0.0, 0.0, 1.0),
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            miter_limit: 4.0,
            dash_array: Vec::new(),
            dash_offset: 0.0,
        }
    }
}
