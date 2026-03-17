//! Path2D — 2D bezier path with SVG `d` attribute parsing.

use serde::{Deserialize, Serialize};

/// A 2D point.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn lerp(self, other: Self, t: f64) -> Self {
        Self {
            x: self.x + (other.x - self.x) * t,
            y: self.y + (other.y - self.y) * t,
        }
    }

    pub fn distance(self, other: Self) -> f64 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        (dx * dx + dy * dy).sqrt()
    }
}

/// Path segment commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PathCmd {
    MoveTo(Point),
    LineTo(Point),
    QuadTo(Point, Point),         // control, end
    CubicTo(Point, Point, Point), // ctrl1, ctrl2, end
    ArcTo {
        rx: f64,
        ry: f64,
        rotation: f64,
        large_arc: bool,
        sweep: bool,
        end: Point,
    },
    Close,
}

/// 2D bezier path.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Path2D {
    pub commands: Vec<PathCmd>,
}

impl Path2D {
    pub fn new() -> Self {
        Self { commands: Vec::new() }
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.commands.push(PathCmd::MoveTo(Point::new(x, y)));
        self
    }

    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Self {
        self.commands.push(PathCmd::LineTo(Point::new(x, y)));
        self
    }

    pub fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> &mut Self {
        self.commands.push(PathCmd::QuadTo(Point::new(cx, cy), Point::new(x, y)));
        self
    }

    pub fn cubic_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) -> &mut Self {
        self.commands.push(PathCmd::CubicTo(
            Point::new(c1x, c1y),
            Point::new(c2x, c2y),
            Point::new(x, y),
        ));
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.commands.push(PathCmd::Close);
        self
    }

    /// Parse an SVG path `d` attribute string.
    pub fn from_svg(d: &str) -> Self {
        let mut path = Self::new();
        let mut chars = d.chars().peekable();
        let mut current = Point::new(0.0, 0.0);
        let mut start = Point::new(0.0, 0.0);
        let mut last_ctrl = None;

        fn skip_ws_comma(chars: &mut std::iter::Peekable<std::str::Chars>) {
            while let Some(&c) = chars.peek() {
                if c == ' ' || c == ',' || c == '\t' || c == '\n' || c == '\r' {
                    chars.next();
                } else {
                    break;
                }
            }
        }

        fn parse_num(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f64> {
            skip_ws_comma(chars);
            let mut s = String::new();
            if let Some(&c) = chars.peek() {
                if c == '-' || c == '+' {
                    s.push(c);
                    chars.next();
                }
            }
            let mut has_dot = false;
            let mut has_e = false;
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    chars.next();
                } else if c == '.' && !has_dot {
                    has_dot = true;
                    s.push(c);
                    chars.next();
                } else if (c == 'e' || c == 'E') && !has_e {
                    has_e = true;
                    s.push(c);
                    chars.next();
                    if let Some(&c2) = chars.peek() {
                        if c2 == '-' || c2 == '+' {
                            s.push(c2);
                            chars.next();
                        }
                    }
                } else {
                    break;
                }
            }
            s.parse().ok()
        }

        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == ',' {
                chars.next();
                continue;
            }

            if c.is_alphabetic() {
                chars.next();
                let is_relative = c.is_lowercase();

                match c.to_uppercase().next().unwrap() {
                    'M' => {
                        while let Some(x) = parse_num(&mut chars) {
                            let y = parse_num(&mut chars).unwrap_or(0.0);
                            let p = if is_relative {
                                Point::new(current.x + x, current.y + y)
                            } else {
                                Point::new(x, y)
                            };
                            if path.commands.is_empty() || matches!(path.commands.last(), Some(PathCmd::Close)) {
                                path.move_to(p.x, p.y);
                                start = p;
                            } else {
                                path.line_to(p.x, p.y);
                            }
                            current = p;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'L' => {
                        while let Some(x) = parse_num(&mut chars) {
                            let y = parse_num(&mut chars).unwrap_or(0.0);
                            let p = if is_relative {
                                Point::new(current.x + x, current.y + y)
                            } else {
                                Point::new(x, y)
                            };
                            path.line_to(p.x, p.y);
                            current = p;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'H' => {
                        while let Some(x) = parse_num(&mut chars) {
                            let px = if is_relative { current.x + x } else { x };
                            path.line_to(px, current.y);
                            current.x = px;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'V' => {
                        while let Some(y) = parse_num(&mut chars) {
                            let py = if is_relative { current.y + y } else { y };
                            path.line_to(current.x, py);
                            current.y = py;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'C' => {
                        while let Some(c1x) = parse_num(&mut chars) {
                            let c1y = parse_num(&mut chars).unwrap_or(0.0);
                            let c2x = parse_num(&mut chars).unwrap_or(0.0);
                            let c2y = parse_num(&mut chars).unwrap_or(0.0);
                            let x = parse_num(&mut chars).unwrap_or(0.0);
                            let y = parse_num(&mut chars).unwrap_or(0.0);
                            let (p1, p2, p3) = if is_relative {
                                (
                                    Point::new(current.x + c1x, current.y + c1y),
                                    Point::new(current.x + c2x, current.y + c2y),
                                    Point::new(current.x + x, current.y + y),
                                )
                            } else {
                                (Point::new(c1x, c1y), Point::new(c2x, c2y), Point::new(x, y))
                            };
                            path.cubic_to(p1.x, p1.y, p2.x, p2.y, p3.x, p3.y);
                            last_ctrl = Some(p2);
                            current = p3;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'S' => {
                        while let Some(c2x) = parse_num(&mut chars) {
                            let c2y = parse_num(&mut chars).unwrap_or(0.0);
                            let x = parse_num(&mut chars).unwrap_or(0.0);
                            let y = parse_num(&mut chars).unwrap_or(0.0);
                            // Reflected control point
                            let c1 = if let Some(lc) = last_ctrl {
                                Point::new(2.0 * current.x - lc.x, 2.0 * current.y - lc.y)
                            } else {
                                current
                            };
                            let (p2, p3) = if is_relative {
                                (
                                    Point::new(current.x + c2x, current.y + c2y),
                                    Point::new(current.x + x, current.y + y),
                                )
                            } else {
                                (Point::new(c2x, c2y), Point::new(x, y))
                            };
                            path.cubic_to(c1.x, c1.y, p2.x, p2.y, p3.x, p3.y);
                            last_ctrl = Some(p2);
                            current = p3;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'Q' => {
                        while let Some(cx) = parse_num(&mut chars) {
                            let cy = parse_num(&mut chars).unwrap_or(0.0);
                            let x = parse_num(&mut chars).unwrap_or(0.0);
                            let y = parse_num(&mut chars).unwrap_or(0.0);
                            let (ctrl, end) = if is_relative {
                                (
                                    Point::new(current.x + cx, current.y + cy),
                                    Point::new(current.x + x, current.y + y),
                                )
                            } else {
                                (Point::new(cx, cy), Point::new(x, y))
                            };
                            path.quad_to(ctrl.x, ctrl.y, end.x, end.y);
                            last_ctrl = Some(ctrl);
                            current = end;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'T' => {
                        while let Some(x) = parse_num(&mut chars) {
                            let y = parse_num(&mut chars).unwrap_or(0.0);
                            let ctrl = if let Some(lc) = last_ctrl {
                                Point::new(2.0 * current.x - lc.x, 2.0 * current.y - lc.y)
                            } else {
                                current
                            };
                            let end = if is_relative {
                                Point::new(current.x + x, current.y + y)
                            } else {
                                Point::new(x, y)
                            };
                            path.quad_to(ctrl.x, ctrl.y, end.x, end.y);
                            last_ctrl = Some(ctrl);
                            current = end;
                            skip_ws_comma(&mut chars);
                        }
                    }
                    'Z' => {
                        path.close();
                        current = start;
                        last_ctrl = None;
                    }
                    _ => {}
                }

                if !matches!(c.to_uppercase().next().unwrap(), 'C' | 'S' | 'Q' | 'T') {
                    last_ctrl = None;
                }
            } else {
                chars.next();
            }
        }

        path
    }

    /// Serialize to SVG path `d` attribute string.
    pub fn to_svg(&self) -> String {
        let mut s = String::new();
        for cmd in &self.commands {
            if !s.is_empty() { s.push(' '); }
            match cmd {
                PathCmd::MoveTo(p) => s.push_str(&format!("M{},{}", p.x, p.y)),
                PathCmd::LineTo(p) => s.push_str(&format!("L{},{}", p.x, p.y)),
                PathCmd::QuadTo(c, p) => s.push_str(&format!("Q{},{} {},{}", c.x, c.y, p.x, p.y)),
                PathCmd::CubicTo(c1, c2, p) => s.push_str(&format!("C{},{} {},{} {},{}", c1.x, c1.y, c2.x, c2.y, p.x, p.y)),
                PathCmd::ArcTo { rx, ry, rotation, large_arc, sweep, end } => {
                    s.push_str(&format!("A{},{} {} {} {} {},{}",
                        rx, ry, rotation,
                        if *large_arc { 1 } else { 0 },
                        if *sweep { 1 } else { 0 },
                        end.x, end.y,
                    ));
                }
                PathCmd::Close => s.push('Z'),
            }
        }
        s
    }

    /// Compute axis-aligned bounding box.
    pub fn bounds(&self) -> (Point, Point) {
        let mut min = Point::new(f64::MAX, f64::MAX);
        let mut max = Point::new(f64::MIN, f64::MIN);
        let mut current = Point::new(0.0, 0.0);

        for cmd in &self.commands {
            let pts: Vec<Point> = match cmd {
                PathCmd::MoveTo(p) | PathCmd::LineTo(p) => { current = *p; vec![*p] }
                PathCmd::QuadTo(c, p) => { current = *p; vec![*c, *p] }
                PathCmd::CubicTo(c1, c2, p) => { current = *p; vec![*c1, *c2, *p] }
                PathCmd::ArcTo { end, .. } => { current = *end; vec![*end] }
                PathCmd::Close => vec![current],
            };
            for p in pts {
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
            }
        }
        (min, max)
    }

    /// Flatten all curves to line segments with given tolerance.
    pub fn flatten(&self, tolerance: f64) -> Vec<Point> {
        let mut points = Vec::new();
        let mut current = Point::new(0.0, 0.0);

        for cmd in &self.commands {
            match cmd {
                PathCmd::MoveTo(p) => {
                    current = *p;
                    points.push(*p);
                }
                PathCmd::LineTo(p) => {
                    current = *p;
                    points.push(*p);
                }
                PathCmd::QuadTo(ctrl, end) => {
                    let steps = adaptive_steps(current.distance(*end), tolerance);
                    for i in 1..=steps {
                        let t = i as f64 / steps as f64;
                        let p = quad_bezier(current, *ctrl, *end, t);
                        points.push(p);
                    }
                    current = *end;
                }
                PathCmd::CubicTo(c1, c2, end) => {
                    let steps = adaptive_steps(current.distance(*end), tolerance);
                    for i in 1..=steps {
                        let t = i as f64 / steps as f64;
                        let p = cubic_bezier(current, *c1, *c2, *end, t);
                        points.push(p);
                    }
                    current = *end;
                }
                PathCmd::ArcTo { end, .. } => {
                    // Simplified: treat as line (proper arc conversion TODO)
                    current = *end;
                    points.push(*end);
                }
                PathCmd::Close => {
                    if let Some(&first) = points.first() {
                        points.push(first);
                        current = first;
                    }
                }
            }
        }
        points
    }

    /// Sample a point at parameter t ∈ [0, 1] along the path's total arc length.
    pub fn sample(&self, t: f64) -> (Point, f64) {
        let flat = self.flatten(0.5);
        if flat.len() < 2 {
            return (Point::new(0.0, 0.0), 0.0);
        }

        // Compute cumulative arc lengths
        let mut lengths = vec![0.0];
        for i in 1..flat.len() {
            lengths.push(lengths[i - 1] + flat[i - 1].distance(flat[i]));
        }
        let total = *lengths.last().unwrap();
        if total < 1e-10 {
            return (flat[0], 0.0);
        }

        let target = t.clamp(0.0, 1.0) * total;

        // Binary search for segment
        let idx = lengths.partition_point(|&l| l < target).min(flat.len() - 1).max(1);
        let seg_start = lengths[idx - 1];
        let seg_len = lengths[idx] - seg_start;
        let local_t = if seg_len > 0.0 { (target - seg_start) / seg_len } else { 0.0 };

        let p = flat[idx - 1].lerp(flat[idx], local_t);

        // Tangent angle
        let dx = flat[idx].x - flat[idx - 1].x;
        let dy = flat[idx].y - flat[idx - 1].y;
        let angle = dy.atan2(dx);

        (p, angle)
    }

    /// Total arc length of the path.
    pub fn length(&self) -> f64 {
        let flat = self.flatten(0.5);
        let mut total = 0.0;
        for i in 1..flat.len() {
            total += flat[i - 1].distance(flat[i]);
        }
        total
    }

    /// Number of segments in the path.
    pub fn segment_count(&self) -> usize {
        self.commands.len()
    }

    /// Is the path closed?
    pub fn is_closed(&self) -> bool {
        matches!(self.commands.last(), Some(PathCmd::Close))
    }
}

fn quad_bezier(p0: Point, p1: Point, p2: Point, t: f64) -> Point {
    let u = 1.0 - t;
    Point {
        x: u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
        y: u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
    }
}

fn cubic_bezier(p0: Point, p1: Point, p2: Point, p3: Point, t: f64) -> Point {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    Point {
        x: uu * u * p0.x + 3.0 * uu * t * p1.x + 3.0 * u * tt * p2.x + tt * t * p3.x,
        y: uu * u * p0.y + 3.0 * uu * t * p1.y + 3.0 * u * tt * p2.y + tt * t * p3.y,
    }
}

fn adaptive_steps(chord_len: f64, tolerance: f64) -> usize {
    ((chord_len / tolerance).ceil() as usize).clamp(4, 128)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_svg_line() {
        let p = Path2D::from_svg("M0,0 L10,10 L20,0 Z");
        assert_eq!(p.commands.len(), 4);
        assert!(p.is_closed());
    }

    #[test]
    fn parse_svg_cubic() {
        let p = Path2D::from_svg("M0,0 C5,10 15,10 20,0");
        assert_eq!(p.commands.len(), 2);
    }

    #[test]
    fn roundtrip_svg() {
        let original = "M0,0 L10,10 L20,0 Z";
        let p = Path2D::from_svg(original);
        let svg = p.to_svg();
        assert!(svg.contains("M0,0"));
        assert!(svg.contains("Z"));
    }

    #[test]
    fn bounds() {
        let p = Path2D::from_svg("M0,0 L10,10 L20,0 Z");
        let (min, max) = p.bounds();
        assert_eq!(min.x, 0.0);
        assert_eq!(max.x, 20.0);
    }

    #[test]
    fn sample_midpoint() {
        let p = Path2D::from_svg("M0,0 L10,0");
        let (pt, _angle) = p.sample(0.5);
        assert!((pt.x - 5.0).abs() < 0.5);
        assert!(pt.y.abs() < 0.1);
    }
}
