//! Ink geometry: smoothing, simplification, variable-width outlines, hit testing.
//! Port of `app/web/src/ink.js`; points are in page coordinates.

use serde::{Deserialize, Serialize};

/// One sampled stylus point. `p` is pressure in 0..=1 (0.5 when unknown); `t` is the pen
/// tilt in radians, 0 (perpendicular to the screen, or unknown) to π/2 (flat), as Android
/// reports it in `MotionEvent.AXIS_TILT`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub p: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub t: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Point {
    pub const fn new(x: f64, y: f64, p: f64) -> Self {
        Self { x, y, p, t: 0.0 }
    }
    pub const fn with_tilt(x: f64, y: f64, p: f64, t: f64) -> Self {
        Self { x, y, p, t }
    }
    pub const fn xy(x: f64, y: f64) -> Self {
        Self { x, y, p: 0.5, t: 0.0 }
    }
    /// Linear interpolation of every channel (position, pressure, tilt).
    pub fn lerp(self, o: Point, k: f64) -> Point {
        Point {
            x: self.x + (o.x - self.x) * k,
            y: self.y + (o.y - self.y) * k,
            p: self.p + (o.p - self.p) * k,
            t: self.t + (o.t - self.t) * k,
        }
    }
}

/// How a tool turns pressure and tilt into stroke width (P1-10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nib {
    /// Firm, mostly pressure-driven; tilt ignored.
    Pen,
    /// Thin at light pressure, widens when tilted (shading with the side of the lead).
    Pencil,
    /// Very pressure-sensitive (thin to thick), widens with tilt.
    Brush,
    /// Constant width.
    Highlighter,
}

impl Nib {
    /// Opacity the tool is drawn with.
    pub fn alpha(self) -> f32 {
        match self {
            Nib::Pen | Nib::Brush => 1.0,
            Nib::Pencil => 0.8,
            Nib::Highlighter => 0.35,
        }
    }
}

/// Response curve of a nib: stroke radius for base `width`, pressure `p` (0 = unknown) and
/// tilt `t` (radians from perpendicular).
pub fn nib_radius(nib: Nib, width: f64, p: f64, t: f64) -> f64 {
    let pr = if p <= 0.0 { 0.5 } else { p.min(1.0) };
    let tilt = t.clamp(0.0, std::f64::consts::FRAC_PI_2).sin();
    let half = width / 2.0;
    match nib {
        Nib::Pen => half * (0.45 + 0.9 * pr),
        Nib::Pencil => half * (0.55 + 0.45 * pr) * (1.0 + 1.2 * tilt),
        Nib::Brush => half * (0.12 + 1.6 * pr.powf(1.6)) * (1.0 + 0.8 * tilt),
        Nib::Highlighter => half,
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
    let t = if len2 > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
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
        out.push(Point::with_tilt(
            q.x + (p.x - q.x) * k + (n.x - p.x) * k * 0.25,
            q.y + (p.y - q.y) * k + (n.y - p.y) * k * 0.25,
            p.p,
            p.t,
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
        .map(|p| Point::with_tilt(round2(p.x), round2(p.y), round2(p.p), round2(p.t)))
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
    outline_with(pts, |p| radius_at(width, p.p, pressure_sensitive))
}

/// Outline of a stroke drawn with `nib` (pressure and tilt response, P1-10).
pub fn outline_nib(pts: &[Point], width: f64, nib: Nib) -> Vec<Point> {
    outline_with(pts, |p| nib_radius(nib, width, p.p, p.t))
}

/// Outline with the radius at each point given by `radius`.
pub fn outline_with(pts: &[Point], radius: impl Fn(&Point) -> f64) -> Vec<Point> {
    let n = pts.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 || path_length(pts) < 0.5 {
        let r = radius(&pts[0]).max(0.5);
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
        let r = radius(&pts[i]);
        left.push(Point::xy(pts[i].x - dy * r, pts[i].y + dx * r));
        right.push(Point::xy(pts[i].x + dy * r, pts[i].y - dx * r));
    }
    let cap_end = cap(pts[n - 1], pts[n - 2], radius(&pts[n - 1]));
    let cap_start = cap(pts[0], pts[1], radius(&pts[0]));
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

/// Partial eraser (P1-11): the pieces of polyline `pts` that lie outside every eraser
/// capsule (segment `a`–`b`, radius `r`), in drawing order, or `None` when no capsule
/// touches it. The input is not modified (ADR-0003): pieces are new point lists made of the
/// original points plus interpolated cut points. Pieces shorter than `min_len` are dropped.
pub fn erase_pieces(pts: &[Point], capsules: &[(Point, Point)], r: f64, min_len: f64) -> Option<Vec<Vec<Point>>> {
    if pts.is_empty() {
        return None;
    }
    // Only capsules near the stroke matter.
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in pts {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let near: Vec<(Point, Point)> = capsules
        .iter()
        .filter(|(a, b)| {
            a.x.min(b.x) - r <= x1 && a.x.max(b.x) + r >= x0 && a.y.min(b.y) - r <= y1 && a.y.max(b.y) + r >= y0
        })
        .copied()
        .collect();
    if near.is_empty() {
        return None;
    }
    let inside = |p: Point| near.iter().any(|(a, b)| dist_to_segment(p, *a, *b) <= r);
    if pts.len() == 1 {
        return inside(pts[0]).then(Vec::new);
    }
    let mut touched = false;
    let mut pieces: Vec<Vec<Point>> = vec![];
    let mut cur: Vec<Point> = vec![];
    let mut prev_in = inside(pts[0]);
    if prev_in {
        touched = true;
    } else {
        cur.push(pts[0]);
    }
    let step = (r / 4.0).max(0.25);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let steps = (dist(a, b) / step).ceil().max(1.0) as usize;
        let mut t_prev = 0.0;
        for k in 1..=steps {
            let t = k as f64 / steps as f64;
            let now_in = inside(a.lerp(b, t));
            if now_in != prev_in {
                touched = true;
                // Bisect for the boundary between the two samples.
                let (mut lo, mut hi) = (t_prev, t);
                for _ in 0..16 {
                    let mid = (lo + hi) / 2.0;
                    if inside(a.lerp(b, mid)) == prev_in {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                // Leaving the eraser starts a new piece; entering it ends the current one.
                cur.push(a.lerp(b, if prev_in { hi } else { lo }));
                if !prev_in {
                    pieces.push(std::mem::take(&mut cur));
                }
            }
            prev_in = now_in;
            t_prev = t;
        }
        if !prev_in {
            cur.push(b);
        }
    }
    if !touched {
        return None;
    }
    if !cur.is_empty() {
        pieces.push(cur);
    }
    pieces.retain(|p| p.len() >= 2 && path_length(p) >= min_len);
    Some(pieces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wave() -> Vec<Point> {
        (0..50)
            .map(|i| Point::xy(i as f64 * 2.0, (i as f64 / 5.0).sin() * 10.0))
            .collect()
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
        assert!(segment_hits_polyline(
            &pts,
            Point::xy(20.0, -30.0),
            Point::xy(20.0, 30.0),
            1.0
        ));
        assert!(!segment_hits_polyline(
            &pts,
            Point::xy(-30.0, -30.0),
            Point::xy(-20.0, 30.0),
            1.0
        ));
    }

    #[test]
    fn nib_curves() {
        // Pen matches the historical curve; tilt only matters for pencil and brush.
        assert_eq!(nib_radius(Nib::Pen, 4.0, 0.7, 1.0), radius_at(4.0, 0.7, true));
        assert!(nib_radius(Nib::Pencil, 4.0, 0.5, 1.2) > nib_radius(Nib::Pencil, 4.0, 0.5, 0.0) * 1.8);
        assert!(nib_radius(Nib::Brush, 4.0, 1.0, 0.0) > nib_radius(Nib::Brush, 4.0, 0.1, 0.0) * 5.0);
        assert_eq!(nib_radius(Nib::Highlighter, 20.0, 0.1, 1.0), 10.0);
        assert_eq!(
            nib_radius(Nib::Brush, 4.0, 0.0, 0.0),
            nib_radius(Nib::Brush, 4.0, 0.5, 0.0)
        );
        let pts: Vec<Point> = (0..20).map(|i| Point::with_tilt(i as f64, 0.0, 0.5, 1.0)).collect();
        assert!(outline_nib(&pts, 4.0, Nib::Pencil).len() > 20);
    }

    #[test]
    fn partial_erase_splits_and_keeps_original_points() {
        let pts: Vec<Point> = (0..=100).map(|i| Point::new(i as f64, 0.0, 0.6)).collect();
        let cut = [(Point::xy(50.0, -20.0), Point::xy(50.0, 20.0))];
        let pieces = erase_pieces(&pts, &cut, 5.0, 0.5).unwrap();
        assert_eq!(pieces.len(), 2);
        let (l, r) = (&pieces[0], &pieces[1]);
        assert_eq!(l[0], pts[0]);
        assert!((l.last().unwrap().x - 45.0).abs() < 0.01, "{:?}", l.last());
        assert!((r[0].x - 55.0).abs() < 0.01);
        assert_eq!(*r.last().unwrap(), pts[100]);
        assert!(
            l[..l.len() - 1].iter().all(|p| pts.contains(p)),
            "original points reused as they were"
        );
        assert_eq!(
            erase_pieces(&pts, &[(Point::xy(0.0, 50.0), Point::xy(100.0, 50.0))], 5.0, 0.5),
            None
        );
        let all = [(Point::xy(-10.0, 0.0), Point::xy(110.0, 0.0))];
        assert_eq!(erase_pieces(&pts, &all, 5.0, 0.5), Some(vec![]));
        // Erasing the start only leaves one piece.
        let start = [(Point::xy(0.0, 0.0), Point::xy(0.0, 0.0))];
        let p = erase_pieces(&pts, &start, 10.0, 0.5).unwrap();
        assert_eq!(p.len(), 1);
        assert!((p[0][0].x - 10.0).abs() < 0.01);
    }

    #[test]
    fn pressure_changes_radius() {
        assert!(radius_at(4.0, 1.0, true) > radius_at(4.0, 0.1, true));
        assert_eq!(radius_at(4.0, 1.0, false), 2.0);
        assert_eq!(radius_at(4.0, 0.0, true), radius_at(4.0, 0.5, true));
    }
}
