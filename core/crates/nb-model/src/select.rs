//! Selection hit tests (P1-14): rectangle and lasso over mixed objects.
//!
//! A lasso is a closed polygon (the finger or pen path). An object is selected when most of
//! it lies inside: for ink, at least half of (up to [`SAMPLES`]) points spread along the
//! stroke; for blocks, the centre of their box. Objects whose bounding box misses the
//! lasso's are rejected first, and point-in-polygon tests only visit the polygon edges of
//! one horizontal band, so a 10 000-object page stays well under a frame (see the test).

use crate::{Content, Interp, Item, Rect};

/// Points sampled per stroke for the lasso test.
pub const SAMPLES: usize = 16;
/// Horizontal bands of the edge index.
const BANDS: usize = 64;

/// A closed polygon prepared for fast point-in-polygon tests.
pub struct Lasso {
    pts: Vec<(f64, f64)>,
    bounds: Rect,
    y0: f64,
    band_h: f64,
    /// Edge indices (edge i joins pts[i] and pts[i+1 mod n]) crossing each band.
    bands: Vec<Vec<u32>>,
}

impl Lasso {
    /// `None` when the path has fewer than 3 points or no area.
    pub fn new(pts: Vec<(f64, f64)>) -> Option<Self> {
        if pts.len() < 3 || pts.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return None;
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for &(x, y) in &pts {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let band_h = (y1 - y0) / BANDS as f64;
        let mut bands = vec![vec![]; BANDS];
        let n = pts.len();
        let band = |y: f64| (((y - y0) / band_h).floor().max(0.0) as usize).min(BANDS - 1);
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let (lo, hi) = (band(a.1.min(b.1)), band(a.1.max(b.1)));
            for list in &mut bands[lo..=hi] {
                list.push(i as u32);
            }
        }
        Some(Self {
            pts,
            bounds: Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            },
            y0,
            band_h,
            bands,
        })
    }

    /// Even-odd rule.
    pub fn contains(&self, x: f64, y: f64) -> bool {
        if !self.bounds.contains(x, y) {
            return false;
        }
        let b = (((y - self.y0) / self.band_h).floor().max(0.0) as usize).min(BANDS - 1);
        let n = self.pts.len();
        let mut inside = false;
        for &i in &self.bands[b] {
            let (a, c) = (self.pts[i as usize], self.pts[(i as usize + 1) % n]);
            if (a.1 > y) != (c.1 > y) {
                let xi = a.0 + (y - a.1) * (c.0 - a.0) / (c.1 - a.1);
                if x < xi {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Whether the lasso selects `item`.
    pub fn selects(&self, item: &Item) -> bool {
        let r = item.bounds();
        if !r.intersects(&self.bounds) {
            return false;
        }
        match &item.content {
            Content::Stroke(s) => {
                let shape;
                let pts = match &s.interp {
                    Some(Interp { shape: sh, show: true }) => {
                        shape = sh.points();
                        &shape
                    }
                    _ => &s.points,
                };
                if pts.is_empty() {
                    return false;
                }
                let step = pts.len().div_ceil(SAMPLES).max(1);
                let (mut total, mut inside) = (0, 0);
                for p in pts.iter().step_by(step) {
                    total += 1;
                    if self.contains(p.x, p.y) {
                        inside += 1;
                    }
                }
                inside * 2 >= total
            }
            _ => self.contains(r.x + r.w / 2.0, r.y + r.h / 2.0),
        }
    }
}

/// Ids of the items the lasso `pts` selects, in drawing order.
pub fn lasso(items: &[Item], pts: Vec<(f64, f64)>) -> Vec<String> {
    let Some(l) = Lasso::new(pts) else { return vec![] };
    items
        .iter()
        .filter(|it| l.selects(it))
        .map(|it| it.id.clone())
        .collect()
}

/// Ids of the items whose bounds intersect `r`, in drawing order.
pub fn rect(items: &[Item], r: &Rect) -> Vec<String> {
    items
        .iter()
        .filter(|it| it.bounds().intersects(r))
        .map(|it| it.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Stroke, Tool, IDENTITY};
    use nb_ink::Point;

    fn stroke(id: String, x: f64, y: f64) -> Item {
        Item {
            id,
            content: Content::Stroke(Stroke {
                tool: Tool::Pen,
                color: 0,
                width: 2.0,
                points: (0..40)
                    .map(|k| Point::new(x + k as f64 * 0.5, y + (k as f64 * 0.3).sin() * 4.0, 0.5))
                    .collect(),
                interp: None,
                frame: IDENTITY,
            }),
        }
    }

    fn circle(cx: f64, cy: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
        (0..n)
            .map(|i| {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    #[test]
    fn lasso_selects_mixed_objects_mostly_inside() {
        let text = Item {
            id: "t".into(),
            content: Content::Text {
                rect: Rect {
                    x: 90.0,
                    y: 90.0,
                    w: 20.0,
                    h: 20.0,
                },
                text: "x".into(),
                size: 12.0,
                color: 0,
                spans: vec![],
            },
        };
        let far = Item {
            id: "far".into(),
            content: Content::Image {
                rect: Rect {
                    x: 500.0,
                    y: 500.0,
                    w: 20.0,
                    h: 20.0,
                },
                asset: "h".into(),
            },
        };
        let items = vec![
            stroke("in".into(), 80.0, 100.0),
            stroke("edge".into(), 146.0, 100.0), // mostly outside
            text,
            far,
        ];
        assert_eq!(lasso(&items, circle(100.0, 100.0, 50.0, 100)), ["in", "t"]);
        assert!(lasso(&items, vec![(0.0, 0.0), (1.0, 1.0)]).is_empty(), "not a polygon");
        // Concave lasso (a U): the notch is outside.
        let u = vec![
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 80.0),
            (70.0, 80.0),
            (70.0, 0.0),
            (100.0, 0.0),
            (100.0, 100.0),
            (0.0, 100.0),
        ];
        let l = Lasso::new(u).unwrap();
        assert!(l.contains(10.0, 50.0) && !l.contains(50.0, 40.0) && l.contains(50.0, 90.0));
    }

    /// P1-14 budget: a lasso hit test on a page with 10 000 objects takes ≤ 16 ms. Measured
    /// in release builds (`cargo test --release -p nb-model lasso_budget`); debug builds
    /// only check a looser bound.
    #[test]
    fn lasso_budget_10000_objects() {
        let items: Vec<Item> = (0..10_000)
            .map(|i| stroke(format!("s{i}"), (i % 100) as f64 * 25.0, (i / 100) as f64 * 25.0))
            .collect();
        // A wobbly lasso around the top-left quarter, 400 points (a long hand-drawn path).
        let lasso_pts: Vec<(f64, f64)> = circle(625.0, 625.0, 600.0, 400)
            .into_iter()
            .enumerate()
            .map(|(i, (x, y))| (x + (i as f64).sin() * 6.0, y + (i as f64 * 0.7).cos() * 6.0))
            .collect();
        let start = std::time::Instant::now();
        let picked = lasso(&items, lasso_pts.clone());
        let ms = start.elapsed().as_secs_f64() * 1e3;
        assert!(picked.len() > 1000 && picked.len() < 5000, "{}", picked.len());
        let budget = if cfg!(debug_assertions) { 400.0 } else { 16.0 };
        eprintln!("lasso over 10 000 objects: {ms:.2} ms ({} selected)", picked.len());
        assert!(ms <= budget, "{ms:.2} ms > {budget} ms");
        let start = std::time::Instant::now();
        let r = rect(
            &items,
            &Rect {
                x: 0.0,
                y: 0.0,
                w: 1250.0,
                h: 1250.0,
            },
        );
        let ms = start.elapsed().as_secs_f64() * 1e3;
        eprintln!("rectangle over 10 000 objects: {ms:.2} ms ({} selected)", r.len());
        assert!(ms <= budget);
    }
}
