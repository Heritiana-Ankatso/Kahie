//! Shape recognition: turns a hand-drawn stroke into a clean geometric
//! interpretation. Raw ink is never modified (ADR-0003). Port of `app/web/src/shapes.js`.

use nb_ink::{dist, dist_to_segment, path_length, simplify, Point};
use std::f64::consts::PI;

const DEG: f64 = PI / 180.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeLabel {
    Circle,
    Rectangle,
    Triangle,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Line { a: Point, b: Point },
    Polygon { points: Vec<Point>, closed: bool, label: Option<ShapeLabel> },
    Ellipse { cx: f64, cy: f64, rx: f64, ry: f64, angle: f64, label: Option<ShapeLabel> },
}

impl Shape {
    pub fn label(&self) -> Option<ShapeLabel> {
        match self {
            Shape::Line { .. } => None,
            Shape::Polygon { label, .. } | Shape::Ellipse { label, .. } => *label,
        }
    }

    /// Points describing the shape (closed polygons repeat their first point).
    pub fn points(&self) -> Vec<Point> {
        match self {
            Shape::Line { a, b } => vec![*a, *b],
            Shape::Polygon { points, closed, .. } => {
                let mut v = points.clone();
                if *closed {
                    if let Some(f) = points.first() {
                        v.push(*f);
                    }
                }
                v
            }
            Shape::Ellipse { cx, cy, rx, ry, angle, .. } => (0..=64)
                .map(|i| {
                    let t = (i as f64 / 64.0) * 2.0 * PI;
                    let (c, s) = (angle.cos(), angle.sin());
                    let (ex, ey) = (rx * t.cos(), ry * t.sin());
                    Point::xy(cx + ex * c - ey * s, cy + ex * s + ey * c)
                })
                .collect(),
        }
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        let m = |p: &mut Point| {
            p.x += dx;
            p.y += dy;
        };
        match self {
            Shape::Line { a, b } => {
                m(a);
                m(b);
            }
            Shape::Polygon { points, .. } => points.iter_mut().for_each(m),
            Shape::Ellipse { cx, cy, .. } => {
                *cx += dx;
                *cy += dy;
            }
        }
    }
}

pub fn recognize(points: &[Point]) -> Option<Shape> {
    let pts: Vec<Point> = points.iter().map(|p| Point::xy(p.x, p.y)).collect();
    if pts.len() < 2 {
        return None;
    }
    let l = path_length(&pts);
    if l < 12.0 {
        return None;
    }
    let (first, last) = (pts[0], pts[pts.len() - 1]);
    let d = dist(first, last);

    // Straight line: endpoints far apart and every point close to the chord.
    if d / l > 0.85 {
        let dev = pts.iter().map(|p| dist_to_segment(*p, first, last)).fold(0.0, f64::max);
        if dev < (0.06 * l).max(4.0) {
            return Some(snap_line(first, last));
        }
    }

    let closed = d < (0.2 * l).max(18.0);
    if !closed {
        let poly = corners(&pts, l);
        if (3..=4).contains(&poly.len()) && fits_polyline(&pts, &poly, l) {
            return Some(Shape::Polygon { points: poly, closed: false, label: None });
        }
        return None;
    }

    let mut ring = pts.clone();
    *ring.last_mut().unwrap() = ring[0];
    let mut poly = corners(&ring, l);
    poly.pop();
    let mut loop_poly = poly.clone();
    if let Some(f) = poly.first() {
        loop_poly.push(*f);
    }
    let poly_ok = (3..=6).contains(&poly.len()) && fits_polyline(&ring, &loop_poly, l);
    let ellipse = fit_ellipse(&pts);
    // Few sharp corners beat a mediocre ellipse; a smooth 5–6 "corner" loop is an ellipse.
    match ellipse {
        Some((_, err)) if poly_ok && poly.len() <= 4 && err > 0.05 => Some(tidy_polygon(poly)),
        None if poly_ok => Some(tidy_polygon(poly)),
        Some((e, _)) => Some(e),
        None => None,
    }
}

fn corners(pts: &[Point], l: f64) -> Vec<Point> {
    let simp = simplify(pts, (0.045 * l).max(4.0));
    let mut out = vec![simp[0]];
    for p in &simp[1..] {
        if dist(*p, *out.last().unwrap()) > 0.08 * l {
            out.push(*p);
        }
    }
    let end = *simp.last().unwrap();
    if out.len() > 1 && dist(*out.last().unwrap(), end) > 1e-9 {
        *out.last_mut().unwrap() = end;
    }
    out
}

fn fits_polyline(pts: &[Point], poly: &[Point], l: f64) -> bool {
    let total: f64 = pts
        .iter()
        .map(|p| poly.windows(2).map(|w| dist_to_segment(*p, w[0], w[1])).fold(f64::INFINITY, f64::min))
        .sum();
    total / (pts.len() as f64) < (0.025 * l).max(3.0)
}

fn snap_line(a: Point, b: Point) -> Shape {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ang = dy.atan2(dx);
    let len = dx.hypot(dy);
    for k in -4..=4 {
        let target = k as f64 * 45.0 * DEG;
        if (ang - target).abs() < 5.0 * DEG {
            let (mx, my) = ((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
            let (hx, hy) = (target.cos() * len / 2.0, target.sin() * len / 2.0);
            return Shape::Line { a: Point::xy(mx - hx, my - hy), b: Point::xy(mx + hx, my + hy) };
        }
    }
    Shape::Line { a, b }
}

fn angle_at(a: Point, b: Point, c: Point) -> f64 {
    let (v1, v2) = ((a.x - b.x, a.y - b.y), (c.x - b.x, c.y - b.y));
    let n = v1.0.hypot(v1.1) * v2.0.hypot(v2.1);
    let cos = (v1.0 * v2.0 + v1.1 * v2.1) / if n > 0.0 { n } else { 1.0 };
    cos.clamp(-1.0, 1.0).acos() / DEG
}

fn tidy_polygon(poly: Vec<Point>) -> Shape {
    if poly.len() == 4 && (0..4).all(|i| (angle_at(poly[(i + 3) % 4], poly[i], poly[(i + 1) % 4]) - 90.0).abs() < 18.0) {
        return Shape::Polygon { points: best_rect(&poly), closed: true, label: Some(ShapeLabel::Rectangle) };
    }
    let label = (poly.len() == 3).then_some(ShapeLabel::Triangle);
    Shape::Polygon { points: poly, closed: true, label }
}

/// Rectangle through the corners' centroid, oriented by the first edge,
/// snapped axis-aligned within 6°.
fn best_rect(poly: &[Point]) -> Vec<Point> {
    let cx = poly.iter().map(|p| p.x).sum::<f64>() / 4.0;
    let cy = poly.iter().map(|p| p.y).sum::<f64>() / 4.0;
    let mut ang = (poly[1].y - poly[0].y).atan2(poly[1].x - poly[0].x);
    let q = (ang / (90.0 * DEG)).round() * 90.0 * DEG;
    if (ang - q).abs() < 6.0 * DEG {
        ang = q;
    }
    let (ux, uy) = ((ang.cos(), ang.sin()), (-ang.sin(), ang.cos()));
    let (mut hw, mut hh) = (0.0, 0.0);
    for p in poly {
        let (dx, dy) = (p.x - cx, p.y - cy);
        hw += (dx * ux.0 + dy * ux.1).abs();
        hh += (dx * uy.0 + dy * uy.1).abs();
    }
    hw /= 4.0;
    hh /= 4.0;
    let pt = |sx: f64, sy: f64| Point::xy(cx + ux.0 * hw * sx + uy.0 * hh * sy, cy + ux.1 * hw * sx + uy.1 * hh * sy);
    vec![pt(-1.0, -1.0), pt(1.0, -1.0), pt(1.0, 1.0), pt(-1.0, 1.0)]
}

/// Ellipse from second moments (circle when axes are within 15 %); returns the fit error too.
fn fit_ellipse(pts: &[Point]) -> Option<(Shape, f64)> {
    let n = pts.len() as f64;
    let cx = pts.iter().map(|p| p.x).sum::<f64>() / n;
    let cy = pts.iter().map(|p| p.y).sum::<f64>() / n;
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for p in pts {
        let (dx, dy) = (p.x - cx, p.y - cy);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    sxx /= n;
    syy /= n;
    sxy /= n;
    let (tr, det) = (sxx + syy, sxx * syy - sxy * sxy);
    let disc = (tr * tr / 4.0 - det).max(0.0).sqrt();
    let (l1, l2) = (tr / 2.0 + disc, tr / 2.0 - disc);
    let (rx, ry) = ((2.0 * l1).sqrt(), (2.0 * l2.max(0.0)).sqrt());
    let mut angle = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    if ry < 3.0 {
        return None;
    }
    let (c, s) = ((-angle).cos(), (-angle).sin());
    let err = pts
        .iter()
        .map(|p| {
            let (dx, dy) = (p.x - cx, p.y - cy);
            let (u, v) = (dx * c - dy * s, dx * s + dy * c);
            ((u / rx).hypot(v / ry) - 1.0).abs()
        })
        .sum::<f64>()
        / n;
    if err > 0.16 {
        return None;
    }
    if rx / ry < 1.15 {
        let r = (rx + ry) / 2.0;
        return Some((Shape::Ellipse { cx, cy, rx: r, ry: r, angle: 0.0, label: Some(ShapeLabel::Circle) }, err));
    }
    let q = (angle / (90.0 * DEG)).round() * 90.0 * DEG;
    if (angle - q).abs() < 8.0 * DEG {
        angle = q;
    }
    Some((Shape::Ellipse { cx, cy, rx, ry, angle, label: None }, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jitter(points: &[(f64, f64)], amp: f64) -> Vec<Point> {
        let mut s: u64 = 1;
        let mut rnd = || {
            s = (s * 16807) % 2147483647;
            (s as f64 / 2147483647.0 - 0.5) * 2.0 * amp
        };
        points.iter().map(|&(x, y)| Point::xy(x + rnd(), y + rnd())).collect()
    }

    fn along(path: &[(f64, f64)]) -> Vec<(f64, f64)> {
        let mut out = vec![];
        for w in path.windows(2) {
            for k in 0..20 {
                let t = k as f64 / 20.0;
                out.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
            }
        }
        out.push(*path.last().unwrap());
        out
    }

    fn ring(rx: f64, ry: f64) -> Vec<(f64, f64)> {
        (0..80).map(|i| {
            let a = i as f64 / 79.0 * 2.0 * PI;
            (100.0 + rx * a.cos(), 100.0 + ry * a.sin())
        }).collect()
    }

    #[test]
    fn line_snaps_horizontal() {
        match recognize(&jitter(&along(&[(0.0, 0.0), (200.0, 4.0)]), 1.5)) {
            Some(Shape::Line { a, b }) => assert!((a.y - b.y).abs() < 1e-9),
            other => panic!("expected line, got {other:?}"),
        }
    }

    #[test]
    fn circle_and_ellipse() {
        let c = recognize(&jitter(&ring(60.0, 60.0), 1.5)).unwrap();
        assert_eq!(c.label(), Some(ShapeLabel::Circle));
        match recognize(&jitter(&ring(100.0, 40.0), 1.5)).unwrap() {
            Shape::Ellipse { rx, ry, label: None, .. } => assert!((rx - 100.0).abs() < 12.0 && (ry - 40.0).abs() < 8.0),
            other => panic!("expected ellipse, got {other:?}"),
        }
    }

    #[test]
    fn rectangle_and_triangle() {
        let r = recognize(&jitter(&along(&[(0.0, 0.0), (200.0, 2.0), (198.0, 120.0), (-2.0, 118.0), (0.0, 3.0)]), 1.5)).unwrap();
        assert_eq!(r.label(), Some(ShapeLabel::Rectangle));
        if let Shape::Polygon { points, .. } = &r {
            assert!((points[0].y - points[1].y).abs() < 1e-6);
        }
        assert_eq!(r.points().len(), 5, "closed polygon repeats first point");
        let t = recognize(&jitter(&along(&[(0.0, 0.0), (150.0, 0.0), (75.0, 120.0), (1.0, 2.0)]), 1.5)).unwrap();
        assert_eq!(t.label(), Some(ShapeLabel::Triangle));
    }

    #[test]
    fn scribble_is_not_a_shape() {
        let pts: Vec<Point> = (0..120).map(|i| Point::xy(i as f64 * 3.0, 40.0 * (i as f64 / 3.0).sin() + (i % 7) as f64 * 5.0)).collect();
        assert_eq!(recognize(&pts), None);
    }
}
