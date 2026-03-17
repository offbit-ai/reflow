//! 2D affine transforms for paths.

use crate::path::{Path2D, PathCmd, Point};

/// 2D affine transform matrix [a, b, c, d, tx, ty].
#[derive(Debug, Clone, Copy)]
pub struct Transform2D {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Transform2D {
    pub fn identity() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: 0.0, ty: 0.0 }
    }

    pub fn translate(x: f64, y: f64) -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, tx: x, ty: y }
    }

    pub fn scale(sx: f64, sy: f64) -> Self {
        Self { a: sx, b: 0.0, c: 0.0, d: sy, tx: 0.0, ty: 0.0 }
    }

    pub fn rotate(angle_rad: f64) -> Self {
        let (s, c) = (angle_rad.sin(), angle_rad.cos());
        Self { a: c, b: s, c: -s, d: c, tx: 0.0, ty: 0.0 }
    }

    pub fn skew(ax: f64, ay: f64) -> Self {
        Self { a: 1.0, b: ay.tan(), c: ax.tan(), d: 1.0, tx: 0.0, ty: 0.0 }
    }

    pub fn apply(&self, p: Point) -> Point {
        Point {
            x: self.a * p.x + self.c * p.y + self.tx,
            y: self.b * p.x + self.d * p.y + self.ty,
        }
    }

    pub fn then(&self, other: &Transform2D) -> Transform2D {
        Transform2D {
            a: other.a * self.a + other.b * self.c,
            b: other.a * self.b + other.b * self.d,
            c: other.c * self.a + other.d * self.c,
            d: other.c * self.b + other.d * self.d,
            tx: other.a * self.tx + other.c * self.ty + other.tx,
            ty: other.b * self.tx + other.d * self.ty + other.ty,
        }
    }
}

/// Apply a transform to an entire path.
pub fn transform_path(path: &Path2D, t: &Transform2D) -> Path2D {
    let mut result = Path2D::new();
    for cmd in &path.commands {
        result.commands.push(match cmd {
            PathCmd::MoveTo(p) => PathCmd::MoveTo(t.apply(*p)),
            PathCmd::LineTo(p) => PathCmd::LineTo(t.apply(*p)),
            PathCmd::QuadTo(c, p) => PathCmd::QuadTo(t.apply(*c), t.apply(*p)),
            PathCmd::CubicTo(c1, c2, p) => PathCmd::CubicTo(t.apply(*c1), t.apply(*c2), t.apply(*p)),
            PathCmd::ArcTo { rx, ry, rotation, large_arc, sweep, end } => {
                // Simplified: transform endpoint, scale radii
                let scale = ((t.a * t.a + t.b * t.b).sqrt() + (t.c * t.c + t.d * t.d).sqrt()) / 2.0;
                PathCmd::ArcTo {
                    rx: rx * scale,
                    ry: ry * scale,
                    rotation: *rotation,
                    large_arc: *large_arc,
                    sweep: *sweep,
                    end: t.apply(*end),
                }
            }
            PathCmd::Close => PathCmd::Close,
        });
    }
    result
}

/// Reverse the direction of a path.
pub fn reverse_path(path: &Path2D) -> Path2D {
    let points = path.flatten(0.5);
    let mut result = Path2D::new();
    if points.is_empty() {
        return result;
    }
    result.move_to(points.last().unwrap().x, points.last().unwrap().y);
    for p in points.iter().rev().skip(1) {
        result.line_to(p.x, p.y);
    }
    if path.is_closed() {
        result.close();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_point() {
        let t = Transform2D::translate(10.0, 20.0);
        let p = t.apply(Point::new(5.0, 5.0));
        assert_eq!(p.x, 15.0);
        assert_eq!(p.y, 25.0);
    }

    #[test]
    fn scale_path() {
        let path = crate::shapes::rect(0.0, 0.0, 10.0, 10.0, 0.0);
        let scaled = transform_path(&path, &Transform2D::scale(2.0, 2.0));
        let (_, max) = scaled.bounds();
        assert!((max.x - 20.0).abs() < 0.01);
    }
}
