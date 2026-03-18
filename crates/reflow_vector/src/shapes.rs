//! Shape primitives — generate Path2D from high-level parameters.

use crate::path::Path2D;
use std::f64::consts::PI;

/// Rectangle with optional corner radius.
pub fn rect(x: f64, y: f64, w: f64, h: f64, corner_radius: f64) -> Path2D {
    let mut p = Path2D::new();
    let r = corner_radius.min(w / 2.0).min(h / 2.0);

    if r <= 0.0 {
        p.move_to(x, y);
        p.line_to(x + w, y);
        p.line_to(x + w, y + h);
        p.line_to(x, y + h);
        p.close();
    } else {
        // Rounded corners with cubic bezier approximation
        let k = 0.5522847498; // (4/3) * (sqrt(2) - 1)
        let kr = k * r;

        p.move_to(x + r, y);
        p.line_to(x + w - r, y);
        p.cubic_to(x + w - r + kr, y, x + w, y + r - kr, x + w, y + r);
        p.line_to(x + w, y + h - r);
        p.cubic_to(
            x + w,
            y + h - r + kr,
            x + w - r + kr,
            y + h,
            x + w - r,
            y + h,
        );
        p.line_to(x + r, y + h);
        p.cubic_to(x + r - kr, y + h, x, y + h - r + kr, x, y + h - r);
        p.line_to(x, y + r);
        p.cubic_to(x, y + r - kr, x + r - kr, y, x + r, y);
        p.close();
    }
    p
}

/// Ellipse centered at (cx, cy).
pub fn ellipse(cx: f64, cy: f64, rx: f64, ry: f64) -> Path2D {
    let mut p = Path2D::new();
    let k = 0.5522847498;
    let kx = k * rx;
    let ky = k * ry;

    p.move_to(cx + rx, cy);
    p.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
    p.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
    p.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
    p.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
    p.close();
    p
}

/// Circle centered at (cx, cy).
pub fn circle(cx: f64, cy: f64, r: f64) -> Path2D {
    ellipse(cx, cy, r, r)
}

/// Regular polygon with N sides.
pub fn polygon(cx: f64, cy: f64, radius: f64, sides: usize) -> Path2D {
    let mut p = Path2D::new();
    if sides < 3 {
        return p;
    }

    for i in 0..sides {
        let angle = (i as f64 / sides as f64) * PI * 2.0 - PI / 2.0;
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();
        if i == 0 {
            p.move_to(x, y);
        } else {
            p.line_to(x, y);
        }
    }
    p.close();
    p
}

/// Star with outer and inner radius.
pub fn star(cx: f64, cy: f64, outer_r: f64, inner_r: f64, points: usize) -> Path2D {
    let mut p = Path2D::new();
    if points < 2 {
        return p;
    }

    let total = points * 2;
    for i in 0..total {
        let angle = (i as f64 / total as f64) * PI * 2.0 - PI / 2.0;
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let x = cx + r * angle.cos();
        let y = cy + r * angle.sin();
        if i == 0 {
            p.move_to(x, y);
        } else {
            p.line_to(x, y);
        }
    }
    p.close();
    p
}

/// Line segment.
pub fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> Path2D {
    let mut p = Path2D::new();
    p.move_to(x1, y1);
    p.line_to(x2, y2);
    p
}

/// Arrow (line with triangular head).
pub fn arrow(x1: f64, y1: f64, x2: f64, y2: f64, head_size: f64) -> Path2D {
    let mut p = Path2D::new();
    let dx = x2 - x1;
    let dy = y2 - y1;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return p;
    }

    let ux = dx / len;
    let uy = dy / len;
    let px = -uy;
    let py = ux;

    // Shaft
    p.move_to(x1, y1);
    p.line_to(x2 - ux * head_size, y2 - uy * head_size);

    // Head
    let base_x = x2 - ux * head_size;
    let base_y = y2 - uy * head_size;
    p.move_to(x2, y2);
    p.line_to(base_x + px * head_size * 0.5, base_y + py * head_size * 0.5);
    p.line_to(base_x - px * head_size * 0.5, base_y - py * head_size * 0.5);
    p.close();

    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_has_5_commands() {
        let r = rect(0.0, 0.0, 100.0, 50.0, 0.0);
        assert_eq!(r.commands.len(), 5); // M L L L Z
    }

    #[test]
    fn rounded_rect_has_curves() {
        let r = rect(0.0, 0.0, 100.0, 50.0, 10.0);
        assert!(r.commands.len() > 5);
    }

    #[test]
    fn circle_is_closed() {
        let c = circle(50.0, 50.0, 25.0);
        assert!(c.is_closed());
    }

    #[test]
    fn polygon_sides() {
        let hex = polygon(0.0, 0.0, 50.0, 6);
        // M + 5 L + Z = 7
        assert_eq!(hex.commands.len(), 7);
    }

    #[test]
    fn star_points() {
        let s = star(0.0, 0.0, 50.0, 25.0, 5);
        // M + 9 L + Z = 11
        assert_eq!(s.commands.len(), 11);
    }
}
