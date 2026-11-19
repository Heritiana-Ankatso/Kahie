//! Ink geometry: smoothing, simplification, variable-width outlines, hit testing.
//! Port of `app/web/src/ink.js`; points are in page coordinates.

/// One sampled stylus point. `p` is pressure in 0..=1 (0.5 when unknown).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub p: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64, p: f64) -> Self {
        Self { x, y, p }
    }
    pub const fn xy(x: f64, y: f64) -> Self {
        Self { x, y, p: 0.5 }
    }
}

pub fn dist(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

pub fn path_length(pts: &[Point]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}

pub fn dist_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

/// Ramer–Douglas–Peucker simplification; keeps pressure.
pub fn simplify(pts: &[Point], eps: f64) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0, pts.len() - 1)];
    while let Some((i0, i1)) = stack.pop() {
        let (mut dmax, mut idx) = (0.0, 0);
        for i in i0 + 1..i1 {
            let d = dist_to_segment(pts[i], pts[i0], pts[i1]);
            if d > dmax {
                dmax = d;
                idx = i;
            }
        }
        if dmax > eps && idx > 0 {
            keep[idx] = true;
            stack.push((i0, idx));
            stack.push((idx, i1));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// Light exponential smoothing of raw input (reduces digitiser jitter).
pub fn smooth(pts: &[Point], k: f64) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(pts.len());
    out.push(pts[0]);
    for i in 1..pts.len() - 1 {
        let (p, q, n) = (pts[i], out[i - 1], pts[i + 1]);
        out.push(Point::new(
            q.x + (p.x - q.x) * k + (n.x - p.x) * k * 0.25,
            q.y + (p.y - q.y) * k + (n.y - p.y) * k * 0.25,
            p.p,
        ));
    }
    out.push(pts[pts.len() - 1]);
    out
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Prepare a finished stroke for storage: smooth, simplify, round to 0.01 px.
pub fn finish_points(raw: &[Point], scale: f64) -> Vec<Point> {
    simplify(&smooth(raw, 0.6), 0.35 / scale)
        .into_iter()
        .map(|p| Point::new(round2(p.x), round2(p.y), round2(p.p)))
        .collect()
}

/// Stroke radius at a point given base width and pressure.
pub fn radius_at(width: f64, p: f64, pressure_sensitive: bool) -> f64 {
    if !pressure_sensitive {
        return width / 2.0;
    }
    let pr = if p == 0.0 { 0.5 } else { p };
    (width / 2.0) * (0.45 + 0.9 * pr)
}

/// Variable-width outline polygon for a stroke (closed list of points, pressure unused).
pub fn outline(pts: &[Point], width: f64, pressure_sensitive: bool) -> Vec<Point> {
    let n = pts.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 || path_length(pts) < 0.5 {
        let r = radius_at(width, pts[0].p, pressure_sensitive).max(0.5);
        return circle_poly(pts[0], r, 12);
    }
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
        let (mut dx, mut dy) = (b.x - a.x, b.y - a.y);
        let len = dx.hypot(dy);
        let len = if len > 0.0 { len } else { 1.0 };
        dx /= len;
        dy /= len;
        let r = radius_at(width, pts[i].p, pressure_sensitive);
        left.push(Point::xy(pts[i].x - dy * r, pts[i].y + dx * r));
        right.push(Point::xy(pts[i].x + dy * r, pts[i].y - dx * r));
    }
    let cap_end = cap(pts[n - 1], pts[n - 2], radius_at(width, pts[n - 1].p, pressure_sensitive));
    let cap_start = cap(pts[0], pts[1], radius_at(width, pts[0].p, pressure_sensitive));
    right.reverse();
    let mut out = left;
    out.extend(cap_end);
    out.extend(right);
    out.extend(cap_start);
    out
}

fn cap(p: Point, prev: Point, r: f64) -> Vec<Point> {
    let ang = (p.y - prev.y).atan2(p.x - prev.x);
    (1..8)
        .map(|i| {
            let t = ang + std::f64::consts::FRAC_PI_2 - (i as f64 / 8.0) * std::f64::consts::PI;
            Point::xy(p.x + t.cos() * r, p.y + t.sin() * r)
        })
        .collect()
}

fn circle_poly(c: Point, r: f64, n: usize) -> Vec<Point> {
    (0..n)
        .map(|i| {
            let t = (i as f64 / n as f64) * std::f64::consts::TAU;
            Point::xy(c.x + t.cos() * r, c.y + t.sin() * r)
        })
        .collect()
}

/// True when `p` is within `r` of the polyline.
pub fn hit_polyline(pts: &[Point], p: Point, r: f64) -> bool {
    match pts.len() {
        0 => false,
        1 => dist(pts[0], p) <= r,
        _ => pts.windows(2).any(|w| dist_to_segment(p, w[0], w[1]) <= r),
    }
}

/// True when segment a–b passes within `r` of the polyline (used by the eraser).
pub fn segment_hits_polyline(pts: &[Point], a: Point, b: Point, r: f64) -> bool {
    let steps = (dist(a, b) / r.max(1.0)).ceil().max(1.0) as usize;
    (0..=steps).any(|i| {
        let t = i as f64 / steps as f64;
        hit_polyline(pts, Point::xy(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), r)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave() -> Vec<Point> {
        (0..50).map(|i| Point::xy(i as f64 * 2.0, (i as f64 / 5.0).sin() * 10.0)).collect()
    }

    #[test]
    fn outline_surrounds_stroke() {
        let pts = wave();
        assert!(outline(&pts, 4.0, true).len() > pts.len() * 2);
        assert_eq!(outline(&pts[..1], 4.0, true).len(), 12);
    }

    #[test]
    fn simplify_keeps_endpoints_and_reduces() {
        let pts = wave();
        let s = simplify(&pts, 0.5);
        assert_eq!(s[0], pts[0]);
        assert_eq!(*s.last().unwrap(), *pts.last().unwrap());
        assert!(s.len() < pts.len());
        assert!(finish_points(&pts, 1.0).len() <= pts.len());
    }

    #[test]
    fn hit_tests() {
        let pts = wave();
        assert!(hit_polyline(&pts, Point::xy(20.0, 2f64.sin() * 10.0 + 1.0), 2.0));
        assert!(!hit_polyline(&pts, Point::xy(20.0, 50.0), 2.0));
        assert!(segment_hits_polyline(&pts, Point::xy(20.0, -30.0), Point::xy(20.0, 30.0), 1.0));
        assert!(!segment_hits_polyline(&pts, Point::xy(-30.0, -30.0), Point::xy(-20.0, 30.0), 1.0));
    }

    #[test]
    fn pressure_changes_radius() {
        assert!(radius_at(4.0, 1.0, true) > radius_at(4.0, 0.1, true));
        assert_eq!(radius_at(4.0, 1.0, false), 2.0);
        assert_eq!(radius_at(4.0, 0.0, true), radius_at(4.0, 0.5, true));
    }
}
