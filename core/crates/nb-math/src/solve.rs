//! Geometric constraint solver (E2.7).
//!
//! Unknowns are the coordinates of the free points; each constraint is a residual that is 0
//! when satisfied. The solver minimises Σ residual² with damped Gauss–Newton
//! (Levenberg–Marquardt) starting from the drawn positions: damped steps are small
//! corrections, so the figure stays where the teacher drew it, and the damping keeps the
//! steps defined when constraints leave freedom (rigid motions, free sizes).
//!
//! After solving it reports the degrees of freedom left (2·points − rank of the constraint
//! Jacobian; 3 means "rigid": only translations and rotation are free) and, if constraints
//! cannot all hold, which ones conflict.

/// A constraint between named points.
#[derive(Clone, Debug, PartialEq)]
pub enum Constraint {
    Distance(String, String, f64),
    /// Angle ABC in degrees (at B, 0..180).
    Angle(String, String, String, f64),
    Horizontal(String, String),
    Vertical(String, String),
    /// Lines AB and CD.
    Parallel(String, String, String, String),
    Perpendicular(String, String, String, String),
    /// |AB| = |CD|.
    EqualLength(String, String, String, String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Solution {
    /// Positions of the points, same order as given.
    pub points: Vec<(f64, f64)>,
    /// Degrees of freedom left (3: rigid; more: under-constrained).
    pub dof: usize,
    /// Indices of constraints that are not satisfied (conflicts), empty if all hold.
    pub conflicts: Vec<usize>,
    pub iterations: usize,
}

type Pt = (f64, f64);

/// A constraint with its points resolved to indices.
struct Resolved<'a> {
    c: &'a Constraint,
    pts: Vec<usize>,
}

fn residual(c: &Constraint, p: &[Pt]) -> f64 {
    let sub = |a: Pt, b: Pt| (a.0 - b.0, a.1 - b.1);
    let len = |v: Pt| (v.0 * v.0 + v.1 * v.1).sqrt();
    let unit_cross = |u: Pt, v: Pt| (u.0 * v.1 - u.1 * v.0) / (len(u) * len(v)).max(1e-12);
    let unit_dot = |u: Pt, v: Pt| (u.0 * v.0 + u.1 * v.1) / (len(u) * len(v)).max(1e-12);
    match c {
        Constraint::Distance(_, _, d) => len(sub(p[1], p[0])) - d,
        Constraint::Angle(_, _, _, deg) => {
            let (u, w) = (sub(p[0], p[1]), sub(p[2], p[1]));
            let ang = (u.0 * w.1 - u.1 * w.0).atan2(u.0 * w.0 + u.1 * w.1).abs();
            ang - deg.to_radians()
        }
        Constraint::Horizontal(..) => p[1].1 - p[0].1,
        Constraint::Vertical(..) => p[1].0 - p[0].0,
        Constraint::Parallel(..) => unit_cross(sub(p[1], p[0]), sub(p[3], p[2])),
        Constraint::Perpendicular(..) => unit_dot(sub(p[1], p[0]), sub(p[3], p[2])),
        Constraint::EqualLength(..) => len(sub(p[1], p[0])) - len(sub(p[3], p[2])),
    }
}

/// Points a constraint refers to.
pub fn names(c: &Constraint) -> Vec<&str> {
    match c {
        Constraint::Distance(a, b, _) | Constraint::Horizontal(a, b) | Constraint::Vertical(a, b) => vec![a, b],
        Constraint::Angle(a, b, c, _) => vec![a, b, c],
        Constraint::Parallel(a, b, c, d)
        | Constraint::Perpendicular(a, b, c, d)
        | Constraint::EqualLength(a, b, c, d) => {
            vec![a, b, c, d]
        }
    }
}

impl Resolved<'_> {
    fn value(&self, x: &[f64]) -> f64 {
        let p: Vec<Pt> = self.pts.iter().map(|&i| (x[2 * i], x[2 * i + 1])).collect();
        residual(self.c, &p)
    }
    /// Unknowns this constraint depends on (deduplicated) and ∂r/∂x for each.
    fn gradient(&self, x: &mut [f64]) -> Vec<(usize, f64)> {
        let mut vars: Vec<usize> = self.pts.iter().flat_map(|&i| [2 * i, 2 * i + 1]).collect();
        vars.sort_unstable();
        vars.dedup();
        vars.into_iter()
            .map(|k| {
                let old = x[k];
                let h = 1e-7 * (1.0 + old.abs());
                x[k] = old + h;
                let rp = self.value(x);
                x[k] = old - h;
                let rm = self.value(x);
                x[k] = old;
                (k, (rp - rm) / (2.0 * h))
            })
            .collect()
    }
}

/// Solve `constraints` for the points `point_names` starting from `start`. Constraints that
/// name an unknown point are reported as conflicts.
pub fn solve(point_names: &[String], start: &[(f64, f64)], constraints: &[Constraint]) -> Solution {
    let n = point_names.len() * 2;
    let index = |name: &str| point_names.iter().position(|p| p == name);
    let mut unknown = vec![];
    let mut rs: Vec<(usize, Resolved)> = vec![];
    for (ci, c) in constraints.iter().enumerate() {
        match names(c).iter().map(|s| index(s)).collect::<Option<Vec<usize>>>() {
            Some(pts) => rs.push((ci, Resolved { c, pts })),
            None => unknown.push(ci),
        }
    }
    let m = rs.len();
    let scale = start.iter().map(|p| p.0.abs().max(p.1.abs())).fold(1.0, f64::max);
    let eval = |x: &[f64]| -> Vec<f64> { rs.iter().map(|(_, r)| r.value(x)).collect() };
    let cost = |x: &[f64]| -> f64 { eval(x).iter().map(|v| v * v).sum() };
    let mut x: Vec<f64> = start.iter().flat_map(|p| [p.0, p.1]).collect();
    let mut mu = 1e-6;
    let mut iterations = 0;
    for it in 0..200 {
        iterations = it + 1;
        let r = eval(&x);
        if r.iter().all(|v| v.abs() < 1e-11 * scale) {
            break;
        }
        // Sparse rows of J → JᵀJ and Jᵀr.
        let mut a = vec![0.0; n * n];
        let mut g = vec![0.0; n];
        let mut xs = x.clone();
        for (i, (_, c)) in rs.iter().enumerate() {
            let row = c.gradient(&mut xs);
            for &(p, jp) in &row {
                g[p] += jp * r[i];
                for &(q, jq) in &row {
                    a[p * n + q] += jp * jq;
                }
            }
        }
        let c0 = r.iter().map(|v| v * v).sum::<f64>();
        let mut improved = false;
        for _ in 0..16 {
            let mut aa = a.clone();
            for p in 0..n {
                // Marquardt scaling, with a floor for unknowns no constraint touches.
                aa[p * n + p] += mu * (a[p * n + p] + 1e-9);
            }
            let Some(delta) = cholesky_solve(&mut aa, &g, n) else {
                mu *= 10.0;
                continue;
            };
            let xn: Vec<f64> = x.iter().zip(&delta).map(|(a, d)| a - d).collect();
            if cost(&xn) < c0 {
                x = xn;
                mu = (mu * 0.2).max(1e-12);
                improved = true;
                break;
            }
            mu *= 10.0;
        }
        if !improved {
            break;
        }
    }
    let r = eval(&x);
    let tol = 1e-6 * scale;
    let mut conflicts: Vec<usize> = r
        .iter()
        .enumerate()
        .filter(|(_, v)| v.abs() > tol)
        .map(|(i, _)| rs[i].0)
        .collect();
    conflicts.extend(unknown);
    conflicts.sort_unstable();
    let mut xs = x.clone();
    let mut jac = vec![vec![0.0; n]; m];
    for (i, (_, c)) in rs.iter().enumerate() {
        for (k, v) in c.gradient(&mut xs) {
            jac[i][k] = v;
        }
    }
    Solution {
        points: x.chunks(2).map(|c| (c[0], c[1])).collect(),
        dof: n.saturating_sub(rank(&jac, 1e-7)),
        conflicts,
        iterations,
    }
}

/// Solve A·δ = b for symmetric positive definite A (n × n, row-major) by Cholesky.
fn cholesky_solve(a: &mut [f64], b: &[f64], n: usize) -> Option<Vec<f64>> {
    for j in 0..n {
        let mut d = a[j * n + j];
        for k in 0..j {
            d -= a[j * n + k] * a[j * n + k];
        }
        if d <= 0.0 || !d.is_finite() {
            return None;
        }
        let d = d.sqrt();
        a[j * n + j] = d;
        for i in j + 1..n {
            let mut s = a[i * n + j];
            for k in 0..j {
                s -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = s / d;
        }
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let s: f64 = (0..i).map(|k| a[i * n + k] * y[k]).sum();
        y[i] = (b[i] - s) / a[i * n + i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let s: f64 = (i + 1..n).map(|k| a[k * n + i] * x[k]).sum();
        x[i] = (y[i] - s) / a[i * n + i];
    }
    Some(x)
}

/// Numerical rank of a matrix (rows × cols) by elimination with a relative tolerance.
fn rank(m: &[Vec<f64>], tol: f64) -> usize {
    let mut a: Vec<Vec<f64>> = m.to_vec();
    let rows = a.len();
    let cols = a.first().map_or(0, Vec::len);
    let big = a.iter().flatten().fold(0.0f64, |s, v| s.max(v.abs())).max(1e-300);
    let mut r = 0;
    for col in 0..cols {
        if r == rows {
            break;
        }
        let piv = (r..rows)
            .max_by(|&i, &j| a[i][col].abs().total_cmp(&a[j][col].abs()))
            .unwrap();
        if a[piv][col].abs() <= tol * big {
            continue;
        }
        a.swap(r, piv);
        let pivot_row = a[r].clone();
        for row in a.iter_mut().skip(r + 1) {
            let f = row[col] / pivot_row[col];
            for (v, p) in row.iter_mut().zip(&pivot_row).skip(col) {
                *v -= f * p;
            }
        }
        r += 1;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    fn d(p: (f64, f64), q: (f64, f64)) -> f64 {
        ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()
    }

    #[test]
    fn triangle_with_two_sides_and_an_angle_is_rigid() {
        let names = s(&["A", "B", "C"]);
        let start = [(0.0, 0.0), (4.0, 0.3), (1.0, 2.5)];
        let cs = vec![
            Constraint::Distance("A".into(), "B".into(), 5.0),
            Constraint::Distance("A".into(), "C".into(), 3.0),
            Constraint::Angle("B".into(), "A".into(), "C".into(), 60.0),
        ];
        let sol = solve(&names, &start, &cs);
        let [a, b, c] = [sol.points[0], sol.points[1], sol.points[2]];
        assert!((d(a, b) - 5.0).abs() < 1e-8 && (d(a, c) - 3.0).abs() < 1e-8);
        // Law of cosines: BC² = 25 + 9 − 2·5·3·cos 60° = 19.
        assert!((d(b, c) - 19f64.sqrt()).abs() < 1e-7);
        assert_eq!((sol.dof, sol.conflicts.len()), (3, 0), "rigid, no conflict");
        // It stays close to the drawing (A barely moves).
        assert!(d(a, start[0]) < 1.0);
    }

    #[test]
    fn under_constrained_and_conflicting() {
        let names = s(&["A", "B", "C", "D"]);
        let start = [(0.0, 0.0), (3.0, 0.2), (3.2, 2.0), (0.1, 2.1)];
        // A rectangle: AB horizontal, AD vertical, BC ⟂ AB, CD ∥ AB — sizes free.
        let mut cs = vec![
            Constraint::Horizontal("A".into(), "B".into()),
            Constraint::Vertical("A".into(), "D".into()),
            Constraint::Perpendicular("A".into(), "B".into(), "B".into(), "C".into()),
            Constraint::Parallel("A".into(), "B".into(), "D".into(), "C".into()),
        ];
        let sol = solve(&names, &start, &cs);
        assert!(sol.conflicts.is_empty());
        // 8 unknowns, 4 independent constraints: translation (2) + width + height = 4 free.
        assert_eq!(sol.dof, 4);
        let [a, b, c, dd] = [sol.points[0], sol.points[1], sol.points[2], sol.points[3]];
        assert!(
            (a.1 - b.1).abs() < 1e-8
                && (a.0 - dd.0).abs() < 1e-8
                && (c.1 - dd.1).abs() < 1e-7
                && (b.0 - c.0).abs() < 1e-7
        );
        // A square of side 2 whose diagonal is also 2: impossible.
        cs.push(Constraint::Distance("A".into(), "B".into(), 2.0));
        cs.push(Constraint::EqualLength("A".into(), "B".into(), "A".into(), "D".into()));
        cs.push(Constraint::Distance("A".into(), "C".into(), 2.0));
        let bad = solve(&names, &start, &cs);
        assert!(!bad.conflicts.is_empty(), "conflict detected");
    }

    #[test]
    fn hundred_constraints_within_budget() {
        // A chain of 51 points: 50 distances + 50 angles = 100 constraints.
        let n = 51;
        let names: Vec<String> = (0..n).map(|i| format!("P{i}")).collect();
        let start: Vec<(f64, f64)> = (0..n).map(|i| (i as f64 * 1.1, ((i as f64) * 0.7).sin())).collect();
        let mut cs = vec![];
        for i in 0..n - 1 {
            cs.push(Constraint::Distance(names[i].clone(), names[i + 1].clone(), 1.0));
        }
        for i in 1..n - 1 {
            cs.push(Constraint::Angle(
                names[i - 1].clone(),
                names[i].clone(),
                names[i + 1].clone(),
                150.0,
            ));
        }
        cs.push(Constraint::Horizontal(names[0].clone(), names[1].clone()));
        assert_eq!(cs.len(), 100);
        let t = std::time::Instant::now();
        let sol = solve(&names, &start, &cs);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        assert!(sol.conflicts.is_empty(), "{:?}", sol.conflicts);
        // Budget: 16 ms in release builds; debug builds are ~20× slower.
        let budget = if cfg!(debug_assertions) { 2000.0 } else { 16.0 };
        assert!(ms < budget, "{ms:.1} ms for 100 constraints");
        eprintln!("100 constraints: {ms:.2} ms, {} iterations", sol.iterations);
    }
}
