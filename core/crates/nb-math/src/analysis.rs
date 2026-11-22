//! Numerical analysis of functions of one variable (E2.5): roots, extrema, inflection
//! points, intersections, asymptotes, tangents and integrals, on a window `[a, b]`.
//!
//! Everything is numeric and works on any [`Expr`]: candidates are found on a fine grid
//! (sign changes of f, f′ or f″), then refined by bisection or golden-section search.
//! Poles are told apart from roots by checking that |f| stays small at the refined point.
//! Exact values (e.g. √2) are the CAS's job; here results are shown to 3 decimals.

use crate::Expr;

/// Grid size for candidate search.
const GRID: usize = 2000;

fn finite(v: f64) -> bool {
    v.is_finite() && v.abs() < 1e12
}

/// f′(x) by a central difference.
pub fn derivative(f: &Expr, x: f64) -> f64 {
    let h = 1e-5 * (1.0 + x.abs());
    (f.eval(x + h) - f.eval(x - h)) / (2.0 * h)
}

/// f″(x) by a central difference.
pub fn second_derivative(f: &Expr, x: f64) -> f64 {
    let h = 1e-4 * (1.0 + x.abs());
    (f.eval(x + h) - 2.0 * f.eval(x) + f.eval(x - h)) / (h * h)
}

fn grid(a: f64, b: f64) -> impl Iterator<Item = f64> {
    (0..=GRID).map(move |i| a + (b - a) * i as f64 / GRID as f64)
}

/// Bisection for a sign change of `g` on [lo, hi] (g(lo)·g(hi) < 0).
fn bisect(g: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    let mut glo = g(lo);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if mid <= lo || mid >= hi {
            break;
        }
        let gm = g(mid);
        if gm == 0.0 {
            return mid;
        }
        if (gm < 0.0) == (glo < 0.0) {
            lo = mid;
            glo = gm;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Golden-section search for a minimum of `g` on [lo, hi].
fn golden_min(g: impl Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    let r = 0.5 * (5f64.sqrt() - 1.0);
    let (mut c, mut d) = (hi - r * (hi - lo), lo + r * (hi - lo));
    for _ in 0..120 {
        if g(c) < g(d) {
            hi = d;
        } else {
            lo = c;
        }
        c = hi - r * (hi - lo);
        d = lo + r * (hi - lo);
    }
    0.5 * (lo + hi)
}

/// Typical size of the function's values on the window (for tolerances).
fn scale(f: &Expr, a: f64, b: f64) -> f64 {
    let mut vals: Vec<f64> = grid(a, b)
        .step_by(20)
        .map(|x| f.eval(x).abs())
        .filter(|v| finite(*v))
        .collect();
    if vals.is_empty() {
        return 1.0;
    }
    vals.sort_by(f64::total_cmp);
    vals[vals.len() / 2].max(1.0)
}

/// Whether f is continuous around x (no pole): values near x stay bounded.
fn continuous_at(f: &Expr, x: f64, scale: f64) -> bool {
    let h = 1e-6 * (1.0 + x.abs());
    [x - h, x, x + h]
        .iter()
        .all(|&t| finite(f.eval(t)) && f.eval(t).abs() < 1e4 * scale)
}

fn push_unique(out: &mut Vec<f64>, x: f64, tol: f64) {
    if !out.iter().any(|v| (v - x).abs() < tol) {
        out.push(x);
    }
}

/// Zeros of `g` on [a, b] (sign changes and touching zeros), excluding poles.
fn zeros(g: &dyn Fn(f64) -> f64, a: f64, b: f64, tol_y: f64) -> Vec<f64> {
    let xs: Vec<f64> = grid(a, b).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| g(x)).collect();
    let tol_x = (b - a) * 1e-6;
    let mut out = vec![];
    for i in 0..xs.len() {
        let y = ys[i];
        if !finite(y) {
            continue;
        }
        if y == 0.0 {
            push_unique(&mut out, xs[i], tol_x);
            continue;
        }
        // Sign change to the next sample.
        if i + 1 < xs.len() && finite(ys[i + 1]) && ys[i + 1] != 0.0 && (y < 0.0) != (ys[i + 1] < 0.0) {
            let x = bisect(g, xs[i], xs[i + 1]);
            if g(x).abs() < tol_y {
                push_unique(&mut out, x, tol_x);
            }
        }
        // Touching zero: local minimum of |g| close to 0 (e.g. x² at 0).
        if i > 0 && i + 1 < xs.len() && finite(ys[i - 1]) && finite(ys[i + 1]) {
            let (l, r) = (ys[i - 1].abs(), ys[i + 1].abs());
            if y.abs() <= l && y.abs() <= r && (ys[i - 1] < 0.0) == (ys[i + 1] < 0.0) {
                let x = golden_min(|t| g(t).abs(), xs[i - 1], xs[i + 1]);
                if g(x).abs() < tol_y * 1e-3 {
                    push_unique(&mut out, x, tol_x);
                }
            }
        }
    }
    out.sort_by(f64::total_cmp);
    out
}

/// Real roots of f on [a, b].
pub fn roots(f: &Expr, a: f64, b: f64) -> Vec<f64> {
    let s = scale(f, a, b);
    zeros(&|x| f.eval(x), a, b, 1e-7 * s)
        .into_iter()
        .filter(|&x| continuous_at(f, x, s))
        .collect()
}

/// x-coordinates where f and g meet on [a, b].
pub fn intersections(f: &Expr, g: &Expr, a: f64, b: f64) -> Vec<f64> {
    let s = scale(f, a, b).max(scale(g, a, b));
    zeros(&|x| f.eval(x) - g.eval(x), a, b, 1e-7 * s)
        .into_iter()
        .filter(|&x| continuous_at(f, x, s) && continuous_at(g, x, s))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extremum {
    Min,
    Max,
}

/// Local extrema of f on the open interval (a, b): sign changes of f′.
pub fn extrema(f: &Expr, a: f64, b: f64) -> Vec<(f64, Extremum)> {
    let s = scale(f, a, b);
    let xs: Vec<f64> = grid(a, b).collect();
    let d: Vec<f64> = xs.iter().map(|&x| derivative(f, x)).collect();
    let tol_x = (b - a) * 1e-6;
    let mut out: Vec<(f64, Extremum)> = vec![];
    for i in 0..xs.len() - 1 {
        let (d0, d1) = (d[i], d[i + 1]);
        if !(finite(d0) && finite(d1)) || d0 == d1 {
            continue;
        }
        let change = (d0 > 0.0 && d1 <= 0.0) || (d0 < 0.0 && d1 >= 0.0);
        if !change || (d1 == 0.0 && i + 2 < xs.len() && (d[i + 2] > 0.0) == (d0 > 0.0)) {
            continue;
        }
        let x = if d1 == 0.0 {
            xs[i + 1]
        } else {
            bisect(|t| derivative(f, t), xs[i], xs[i + 1])
        };
        // A pole is not an extremum (1/x² has f′ changing sign at 0).
        if !continuous_at(f, x, s) || x <= a || x >= b {
            continue;
        }
        let kind = if d0 > 0.0 { Extremum::Max } else { Extremum::Min };
        if !out.iter().any(|(v, _)| (v - x).abs() < tol_x) {
            out.push((x, kind));
        }
    }
    out
}

/// Inflection points of f on (a, b): sign changes of f″ (values close to zero, e.g. for
/// straight lines, count as zero and never as a change).
pub fn inflections(f: &Expr, a: f64, b: f64) -> Vec<f64> {
    let s = scale(f, a, b);
    let span = b - a;
    let thr = 1e-5 * s / (span * span).max(1e-12);
    let xs: Vec<f64> = grid(a, b).collect();
    let sign = |x: f64| {
        let v = second_derivative(f, x);
        if !finite(v) {
            None
        } else if v.abs() < thr {
            Some(0)
        } else {
            Some(if v > 0.0 { 1 } else { -1 })
        }
    };
    let mut out = vec![];
    let mut last: Option<(f64, i32)> = None;
    for &x in &xs {
        match sign(x) {
            None => last = None,
            Some(0) => {}
            Some(sg) => {
                if let Some((lx, ls)) = last {
                    if ls != sg {
                        let p = bisect(|t| second_derivative(f, t), lx, x);
                        if continuous_at(f, p, s) && p > a && p < b {
                            push_unique(&mut out, p, span * 1e-6);
                        }
                    }
                }
                last = Some((x, sg));
            }
        }
    }
    out
}

/// Vertical asymptotes in [a, b]: points where |f| grows without bound.
pub fn vertical_asymptotes(f: &Expr, a: f64, b: f64) -> Vec<f64> {
    let s = scale(f, a, b);
    let xs: Vec<f64> = grid(a, b).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| f.eval(x)).collect();
    let mut cands = vec![];
    for i in 0..xs.len() - 1 {
        let (y0, y1) = (ys[i], ys[i + 1]);
        if finite(y0) != finite(y1) {
            // Edge of the domain (e.g. ln x at 0): find the edge.
            let edge = bisect(|t| if f.eval(t).is_finite() { 1.0 } else { -1.0 }, xs[i], xs[i + 1]);
            cands.push(edge);
        } else if finite(y0) && finite(y1) && (y0 < 0.0) != (y1 < 0.0) {
            cands.push(bisect(|t| f.eval(t), xs[i], xs[i + 1]));
        } else if i > 0 && finite(y0) && finite(y1) && finite(ys[i - 1]) {
            // Local peak of |f| (1/x²).
            if y0.abs() >= ys[i - 1].abs() && y0.abs() >= y1.abs() && y0.abs() > 10.0 * s {
                // Minimise 1/|f| (smooth near a pole, 0 at it) rather than −|f| (flat at ∞).
                cands.push(golden_min(
                    |t| {
                        let v = f.eval(t).abs();
                        if finite(v) {
                            1.0 / v
                        } else {
                            0.0
                        }
                    },
                    xs[i - 1],
                    xs[i + 1],
                ));
            }
        }
    }
    let mut out = vec![];
    for x in cands {
        // Blow-up test: |f| keeps growing as we approach x from a side where it is defined.
        let grows = [-1.0, 1.0].iter().any(|side| {
            // |f| at x ± 10⁻², …, 10⁻¹²: must keep increasing (also slowly, as ln x does)
            // and end far above the function's typical size.
            let vals: Vec<f64> = (2..=12)
                .map(|k| f.eval(x + side * 10f64.powi(-k) * (1.0 + x.abs())).abs())
                .collect();
            vals.iter().all(|v| v.is_finite())
                && vals.windows(2).all(|p| p[1] > p[0])
                && vals[10] > 10.0 * s
                && vals[10] - vals[0] > 5.0 * s
        });
        if grows {
            push_unique(&mut out, x, (b - a) * 1e-4);
        }
    }
    out.sort_by(f64::total_cmp);
    out
}

/// Asymptote at −∞ or +∞: `y = slope·x + intercept` (slope 0: horizontal).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub slope: f64,
    pub intercept: f64,
}

/// Horizontal or oblique asymptote towards `dir` (−1: −∞, +1: +∞), if f has one.
pub fn asymptote_at_infinity(f: &Expr, dir: f64) -> Option<Line> {
    let (x1, x2) = (dir * 1e6, dir * 2e6);
    let (y1, y2) = (f.eval(x1), f.eval(x2));
    if !(finite(y1) && finite(y2)) {
        return None;
    }
    let slope = (y2 - y1) / (x2 - x1);
    let intercept = y1 - slope * x1;
    // Check the line also predicts f further out.
    let x3 = dir * 4e6;
    let y3 = f.eval(x3);
    let pred = slope * x3 + intercept;
    let ok = finite(y3) && (y3 - pred).abs() < 1e-3 * (1.0 + intercept.abs() + slope.abs());
    if !ok {
        return None;
    }
    let tidy = |v: f64| if v.abs() < 1e-6 { 0.0 } else { v };
    Some(Line {
        slope: tidy(slope),
        intercept: tidy(intercept),
    })
}

/// Tangent to f at x0.
pub fn tangent(f: &Expr, x0: f64) -> Option<Line> {
    let (y, m) = (f.eval(x0), derivative(f, x0));
    (finite(y) && finite(m)).then_some(Line {
        slope: m,
        intercept: y - m * x0,
    })
}

/// ∫ₐᵇ g by adaptive Simpson (`None` if g is not finite on the interval).
pub fn integrate(g: &dyn Fn(f64) -> f64, a: f64, b: f64) -> Option<f64> {
    fn simpson(g: &dyn Fn(f64) -> f64, a: f64, fa: f64, b: f64, fb: f64) -> (f64, f64, f64) {
        let m = 0.5 * (a + b);
        let fm = g(m);
        (m, fm, (b - a) / 6.0 * (fa + 4.0 * fm + fb))
    }
    #[allow(clippy::too_many_arguments)]
    fn rec(
        g: &dyn Fn(f64) -> f64,
        a: f64,
        fa: f64,
        b: f64,
        fb: f64,
        m: f64,
        fm: f64,
        whole: f64,
        eps: f64,
        depth: u32,
    ) -> f64 {
        let (lm, flm, left) = simpson(g, a, fa, m, fm);
        let (rm, frm, right) = simpson(g, m, fm, b, fb);
        let delta = left + right - whole;
        if depth == 0 || delta.abs() <= 15.0 * eps {
            return left + right + delta / 15.0;
        }
        rec(g, a, fa, m, fm, lm, flm, left, eps / 2.0, depth - 1)
            + rec(g, m, fm, b, fb, rm, frm, right, eps / 2.0, depth - 1)
    }
    if a == b {
        return Some(0.0);
    }
    // Split into 64 panels first so narrow features are not missed.
    let n = 64;
    let mut total = 0.0;
    for i in 0..n {
        let (x0, x1) = (
            a + (b - a) * i as f64 / n as f64,
            a + (b - a) * (i + 1) as f64 / n as f64,
        );
        let (f0, f1) = (g(x0), g(x1));
        let (m, fm, whole) = simpson(g, x0, f0, x1, f1);
        let v = rec(g, x0, f0, x1, f1, m, fm, whole, 1e-10, 30);
        if !v.is_finite() {
            return None;
        }
        total += v;
    }
    (total.is_finite() && total.abs() < 1e12).then_some(total)
}

/// A number for a label: integers as integers, otherwise up to 3 decimals.
pub fn fmt_num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;
    let r = if r == 0.0 { 0.0 } else { r }; // no "-0"
    if (r - r.round()).abs() < 1e-9 {
        format!("{}", r.round() as i64)
    } else {
        let s = format!("{r:.3}");
        s.trim_end_matches('0').to_string()
    }
}

/// `y = mx + p` for a label.
pub fn fmt_line(l: Line) -> String {
    let m = fmt_num(l.slope);
    let p = fmt_num(l.intercept);
    let mx = match m.as_str() {
        "0" => String::new(),
        "1" => "x".into(),
        "-1" => "−x".into(),
        _ => format!("{}x", m.replace('-', "−")),
    };
    match (mx.is_empty(), p.as_str()) {
        (true, _) => format!("y = {}", p.replace('-', "−")),
        (false, "0") => format!("y = {mx}"),
        (false, p) if p.starts_with('-') => format!("y = {mx} − {}", &p[1..]),
        (false, p) => format!("y = {mx} + {p}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn f(s: &str) -> Expr {
        parse(s).unwrap()
    }
    fn close(a: &[f64], b: &[f64]) {
        assert_eq!(a.len(), b.len(), "{a:?} vs {b:?}");
        for (x, y) in a.iter().zip(b) {
            assert!((x - y).abs() < 1e-6, "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn roots_including_touching_and_not_poles() {
        close(&roots(&f("x^2 - 2"), -5.0, 5.0), &[-2f64.sqrt(), 2f64.sqrt()]);
        close(&roots(&f("(x-1)^2"), -5.0, 5.0), &[1.0]);
        close(&roots(&f("1/x"), -5.0, 5.0), &[]);
        close(&roots(&f("tan(x)"), -2.0, 2.0), &[0.0]);
        let pi = std::f64::consts::PI;
        close(&roots(&f("sin(x)"), -7.0, 7.0), &[-2.0 * pi, -pi, 0.0, pi, 2.0 * pi]);
        close(&roots(&f("ln(x)"), -1.0, 5.0), &[1.0]);
        close(&roots(&f("x^3 - x"), -2.0, 2.0), &[-1.0, 0.0, 1.0]);
    }

    #[test]
    fn extrema_inflections_intersections() {
        let e = extrema(&f("x^3 - 3x"), -3.0, 3.0);
        assert_eq!(e.len(), 2);
        assert!((e[0].0 + 1.0).abs() < 1e-6 && e[0].1 == Extremum::Max);
        assert!((e[1].0 - 1.0).abs() < 1e-6 && e[1].1 == Extremum::Min);
        assert!(extrema(&f("1/x^2"), -3.0, 3.0).is_empty(), "a pole is not an extremum");
        let abs = extrema(&f("abs(x - 0.5)"), -3.0, 3.0);
        assert_eq!(abs.len(), 1);
        assert!((abs[0].0 - 0.5).abs() < 1e-5 && abs[0].1 == Extremum::Min);
        close(&inflections(&f("x^3 - 3x"), -3.0, 3.0), &[0.0]);
        assert!(inflections(&f("2x + 1"), -3.0, 3.0).is_empty(), "lines have none");
        assert!(inflections(&f("x^4"), -3.0, 3.0).is_empty());
        assert!(inflections(&f("1/x"), -3.0, 3.0).is_empty(), "the pole is not one");
        close(&intersections(&f("x^2"), &f("x + 2"), -5.0, 5.0), &[-1.0, 2.0]);
    }

    #[test]
    fn asymptotes_tangents_integrals() {
        close(&vertical_asymptotes(&f("1/(x-1)"), -5.0, 5.0), &[1.0]);
        close(&vertical_asymptotes(&f("1/x^2"), -3.0, 3.0), &[0.0]);
        close(&vertical_asymptotes(&f("ln(x)"), -1.0, 5.0), &[0.0]);
        assert!(vertical_asymptotes(&f("x^2"), -3.0, 3.0).is_empty());
        assert!(
            vertical_asymptotes(&f("sqrt(x)"), -3.0, 3.0).is_empty(),
            "domain edge, bounded"
        );
        let h = asymptote_at_infinity(&f("(2x+1)/(x-3)"), 1.0).unwrap();
        assert!(h.slope == 0.0 && (h.intercept - 2.0).abs() < 1e-4);
        let o = asymptote_at_infinity(&f("x + 1/x"), -1.0).unwrap();
        assert!((o.slope - 1.0).abs() < 1e-6 && o.intercept.abs() < 1e-4);
        assert!(asymptote_at_infinity(&f("x^2"), 1.0).is_none());
        assert!(asymptote_at_infinity(&f("sin(x)"), 1.0).is_none());
        let t = tangent(&f("x^2"), 1.0).unwrap();
        assert!((t.slope - 2.0).abs() < 1e-6 && (t.intercept + 1.0).abs() < 1e-6);
        assert_eq!(fmt_line(t), "y = 2x − 1");
        let sq = f("x^2");
        assert!((integrate(&|x| sq.eval(x), 0.0, 3.0).unwrap() - 9.0).abs() < 1e-9);
        let s = f("sin(x)");
        assert!((integrate(&|x| s.eval(x), 0.0, std::f64::consts::PI).unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(
            (fmt_num(1.4142857), fmt_num(-0.0001), fmt_num(3.0)),
            ("1.414".into(), "0".into(), "3".into())
        );
    }
}
