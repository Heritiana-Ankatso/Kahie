//! Rendering pipeline core (P0-05, architecture overview §"display list").
//!
//! The core turns page content into a **display list** per screen tile; each platform
//! only executes it (Android `Canvas`/Skia today; the SVG writer here for export and
//! golden tests). Strokes are indexed in a uniform grid so a tile query touches only
//! nearby strokes, and outlines are computed per **level of detail** (simplification
//! tolerance doubles each level) and cached, so zoomed-out views stay cheap.

use nb_ink::{outline, simplify, Point};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

/// Axis-aligned rectangle in page units, `x0 <= x1`, `y0 <= y1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    fn of_points(pts: &[Point], pad: f64) -> Rect {
        let mut r = Rect::new(f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in pts {
            r.x0 = r.x0.min(p.x);
            r.y0 = r.y0.min(p.y);
            r.x1 = r.x1.max(p.x);
            r.y1 = r.y1.max(p.y);
        }
        Rect::new(r.x0 - pad, r.y0 - pad, r.x1 + pad, r.y1 + pad)
    }
}

/// One drawing instruction. Coordinates are page units, `x0, y0, x1, y1, …`.
#[derive(Clone, Debug, PartialEq)]
pub enum DrawOp {
    /// Filled closed outline (pressure-sensitive ink).
    Fill { color: u32, alpha: f32, coords: Vec<f32> },
    /// Polyline stroked with `width` (shapes; ink seen from very far away).
    Stroke {
        color: u32,
        alpha: f32,
        width: f32,
        closed: bool,
        coords: Vec<f32>,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub ops: Vec<DrawOp>,
}

impl DisplayList {
    /// Number of vertices, a proxy for GPU/raster cost.
    pub fn vertices(&self) -> usize {
        self.ops
            .iter()
            .map(|o| match o {
                DrawOp::Fill { coords, .. } | DrawOp::Stroke { coords, .. } => coords.len() / 2,
            })
            .sum()
    }

    /// SVG document for `view` (page units). Used for export and golden tests.
    pub fn to_svg(&self, view: Rect) -> String {
        let mut s = String::new();
        let _ = write!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}">"#,
            fmt(view.x0),
            fmt(view.y0),
            fmt(view.x1 - view.x0),
            fmt(view.y1 - view.y0)
        );
        for op in &self.ops {
            match op {
                DrawOp::Fill { color, alpha, coords } => {
                    let _ = write!(s, r##"<path fill="#{color:06x}""##);
                    if *alpha < 1.0 {
                        let _ = write!(s, r#" fill-opacity="{}""#, fmt(f64::from(*alpha)));
                    }
                    let _ = write!(s, r#" d="{}Z"/>"#, path_data(coords));
                }
                DrawOp::Stroke {
                    color,
                    alpha,
                    width,
                    closed,
                    coords,
                } => {
                    let _ = write!(
                        s,
                        r##"<path fill="none" stroke="#{color:06x}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round""##,
                        fmt(f64::from(*width))
                    );
                    if *alpha < 1.0 {
                        let _ = write!(s, r#" stroke-opacity="{}""#, fmt(f64::from(*alpha)));
                    }
                    let _ = write!(s, r#" d="{}{}"/>"#, path_data(coords), if *closed { "Z" } else { "" });
                }
            }
        }
        s.push_str("</svg>");
        s
    }
}

fn fmt(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if r == r.trunc() {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

fn path_data(c: &[f32]) -> String {
    let mut d = String::with_capacity(c.len() * 4);
    for (i, xy) in c.chunks_exact(2).enumerate() {
        d.push(if i == 0 { 'M' } else { 'L' });
        d.push_str(&fmt(f64::from(xy[0])));
        d.push(' ');
        d.push_str(&fmt(f64::from(xy[1])));
    }
    d
}

/// Number of detail levels; level `l` simplifies with tolerance `BASE_TOL * 2^l`.
pub const LEVELS: usize = 10;
/// Tolerance at level 0, in screen pixels.
const BASE_TOL_PX: f64 = 0.25;

/// Level of detail for a zoom `scale` (screen px per page unit): the coarsest level whose
/// tolerance stays ≤ 0.25 px on screen.
pub fn level_for_scale(scale: f64) -> usize {
    if scale >= 1.0 {
        return 0;
    }
    ((1.0 / scale).log2().floor().max(0.0) as usize).min(LEVELS - 1)
}

struct Item {
    bounds: Rect,
    color: u32,
    alpha: f32,
    width: f64,
    pressure: bool,
    points: Vec<Point>,
    lods: [OnceLock<Vec<f32>>; LEVELS],
    /// Simplified centre lines per level, for strokes that are small on screen.
    lines: [OnceLock<Vec<f32>>; LEVELS],
}

impl Item {
    fn coords(&self, level: usize) -> &[f32] {
        self.lods[level].get_or_init(|| {
            let tol = BASE_TOL_PX * f64::from(1u32 << level);
            let pts = simplify(&self.points, tol);
            outline(&pts, self.width, self.pressure)
                .iter()
                .flat_map(|p| [p.x as f32, p.y as f32])
                .collect()
        })
    }

    fn centre_line(&self, level: usize) -> &[f32] {
        self.lines[level].get_or_init(|| {
            let tol = BASE_TOL_PX * f64::from(1u32 << level);
            simplify(&self.points, tol)
                .iter()
                .flat_map(|p| [p.x as f32, p.y as f32])
                .collect()
        })
    }
}

/// Strokes smaller than this on screen (px) are drawn as centre lines, not outlines.
const SMALL_PX: f64 = 8.0;

/// Spatially indexed strokes of one page or infinite canvas.
pub struct Scene {
    cell: f64,
    items: Vec<Item>,
    grid: HashMap<(i32, i32), Vec<u32>>,
}

impl Scene {
    /// `cell` is the grid cell size in page units (≈ a typical tile, e.g. 256).
    pub fn new(cell: f64) -> Self {
        Self {
            cell,
            items: vec![],
            grid: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn cells(&self, r: &Rect) -> impl Iterator<Item = (i32, i32)> {
        let (c, f) = (self.cell, |v: f64, c: f64| (v / c).floor() as i32);
        let (cx0, cy0, cx1, cy1) = (f(r.x0, c), f(r.y0, c), f(r.x1, c), f(r.y1, c));
        (cx0..=cx1).flat_map(move |x| (cy0..=cy1).map(move |y| (x, y)))
    }

    /// Add an ink stroke; returns its index (z-order = insertion order).
    pub fn add_stroke(&mut self, points: Vec<Point>, width: f64, color: u32, alpha: f32, pressure: bool) -> u32 {
        let bounds = Rect::of_points(&points, width);
        let id = self.items.len() as u32;
        let cells: Vec<_> = self.cells(&bounds).collect();
        for c in cells {
            self.grid.entry(c).or_default().push(id);
        }
        self.items.push(Item {
            bounds,
            color,
            alpha,
            width,
            pressure,
            points,
            lods: Default::default(),
            lines: Default::default(),
        });
        id
    }

    /// Display list for the page-space rectangle `view` drawn at `scale` px per unit.
    pub fn tile(&self, view: Rect, scale: f64) -> DisplayList {
        let level = level_for_scale(scale);
        let mut ids: Vec<u32> = self
            .cells(&view)
            .filter_map(|c| self.grid.get(&c))
            .flatten()
            .copied()
            .filter(|&i| self.items[i as usize].bounds.intersects(&view))
            .collect();
        ids.sort_unstable();
        ids.dedup();
        let tiny = 0.75 / scale; // below ~1 px on screen, a dot is enough
        let ops = ids
            .into_iter()
            .map(|i| {
                let it = &self.items[i as usize];
                let (w, h) = (it.bounds.x1 - it.bounds.x0, it.bounds.y1 - it.bounds.y0);
                if w.max(h) < tiny {
                    let (cx, cy) = (
                        ((it.bounds.x0 + it.bounds.x1) / 2.0) as f32,
                        ((it.bounds.y0 + it.bounds.y1) / 2.0) as f32,
                    );
                    DrawOp::Stroke {
                        color: it.color,
                        alpha: it.alpha,
                        width: (1.0 / scale) as f32,
                        closed: false,
                        coords: vec![cx, cy, cx, cy],
                    }
                } else if w.max(h) * scale < SMALL_PX {
                    DrawOp::Stroke {
                        color: it.color,
                        alpha: it.alpha,
                        width: it.width as f32,
                        closed: false,
                        coords: it.centre_line(level).to_vec(),
                    }
                } else {
                    DrawOp::Fill {
                        color: it.color,
                        alpha: it.alpha,
                        coords: it.coords(level).to_vec(),
                    }
                }
            })
            .collect();
        DisplayList { ops }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(x: f64, y: f64, n: usize) -> Vec<Point> {
        (0..n)
            .map(|k| Point::new(x + k as f64, y + (k as f64 * 0.2).sin() * 3.0, 0.5))
            .collect()
    }

    #[test]
    fn tile_returns_only_intersecting_strokes_in_z_order() {
        let mut s = Scene::new(100.0);
        s.add_stroke(line(0.0, 0.0, 50), 2.0, 0x111111, 1.0, true);
        s.add_stroke(line(1000.0, 1000.0, 50), 2.0, 0x222222, 1.0, true);
        s.add_stroke(line(10.0, 5.0, 300), 2.0, 0x333333, 1.0, true); // spans many cells
        let dl = s.tile(Rect::new(0.0, 0.0, 60.0, 60.0), 1.0);
        let colors: Vec<u32> = dl
            .ops
            .iter()
            .map(|o| match o {
                DrawOp::Fill { color, .. } | DrawOp::Stroke { color, .. } => *color,
            })
            .collect();
        assert_eq!(
            colors,
            [0x111111, 0x333333],
            "no duplicates across cells, insertion order kept"
        );
        assert!(s.tile(Rect::new(5000.0, 5000.0, 5100.0, 5100.0), 1.0).ops.is_empty());
    }

    #[test]
    fn lod_reduces_vertices_when_zoomed_out() {
        let mut s = Scene::new(256.0);
        for k in 0..20 {
            s.add_stroke(line(0.0, k as f64 * 10.0, 400), 3.0, 0, 1.0, true);
        }
        let view = Rect::new(-10.0, -10.0, 500.0, 300.0);
        let near = s.tile(view, 2.0).vertices();
        let far = s.tile(view, 0.1).vertices();
        assert!(far * 2 < near, "near {near}, far {far}");
        let very_far = s.tile(view, 0.01).vertices();
        assert!(very_far * 10 < near, "near {near}, very far {very_far}");
        assert_eq!(level_for_scale(4.0), 0);
        assert_eq!(level_for_scale(0.5), 1);
        assert_eq!(level_for_scale(0.001), LEVELS - 1);
    }

    /// P1-06 budget: an infinite canvas with 50 000 strokes stays within an 18 ms frame
    /// (p95) while panning at every zoom from 5 % to 3 200 %. Each frame is a full
    /// 1600 × 2560 px screen; the first frame at a zoom builds the LOD geometry (cold), the
    /// following ones pan and reuse it, as the UI's tile cache does. The budget is checked in
    /// release builds (`cargo test --release -p nb-render infinite_canvas`); debug builds
    /// only check a loose bound.
    #[test]
    fn infinite_canvas_50k_strokes_pan_zoom_budget() {
        let mut seed = 7u64;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64
        };
        // Handwriting-sized strokes spread over a 40 000 × 40 000 unit canvas, including
        // negative coordinates (an infinite canvas has no origin corner).
        let mut s = Scene::new(256.0);
        for _ in 0..50_000 {
            let (mut x, mut y, mut a) = (rnd() * 40_000.0 - 20_000.0, rnd() * 40_000.0 - 20_000.0, rnd() * 6.3);
            let n = 20 + (rnd() * 60.0) as usize;
            let pts: Vec<Point> = (0..n)
                .map(|_| {
                    a += (rnd() - 0.5) * 0.6;
                    x += a.cos() * 1.5;
                    y += a.sin() * 1.5;
                    Point::new(x, y, 0.3 + rnd() * 0.5)
                })
                .collect();
            s.add_stroke(pts, 2.5, 0x1d1d1f, 1.0, true);
        }
        let (sw, sh) = (1600.0, 2560.0);
        let mut worst_p95 = 0.0f64;
        for zoom in [0.05, 0.25, 1.0, 4.0, 32.0] {
            let (w, h) = (sw / zoom, sh / zoom);
            let mut times = vec![];
            for k in 0..24 {
                // Pan by a tenth of the screen per frame.
                let x = -w / 2.0 + k as f64 * w / 10.0;
                let y = -h / 2.0 + k as f64 * h / 20.0;
                let t = std::time::Instant::now();
                let dl = s.tile(Rect::new(x, y, x + w, y + h), zoom);
                let ms = t.elapsed().as_secs_f64() * 1e3;
                if k > 0 {
                    times.push(ms);
                }
                std::hint::black_box(dl);
            }
            times.sort_by(f64::total_cmp);
            let p95 = times[times.len() * 95 / 100];
            eprintln!(
                "zoom {:>5.0} %: p95 {p95:.2} ms, max {:.2} ms",
                zoom * 100.0,
                times[times.len() - 1]
            );
            worst_p95 = worst_p95.max(p95);
        }
        let budget = if cfg!(debug_assertions) { 400.0 } else { 18.0 };
        assert!(worst_p95 <= budget, "p95 {worst_p95:.2} ms > {budget} ms");
    }

    #[test]
    fn tiny_strokes_become_dots_far_away() {
        let mut s = Scene::new(256.0);
        s.add_stroke(line(0.0, 0.0, 3), 1.0, 0, 1.0, true);
        let dl = s.tile(Rect::new(-100.0, -100.0, 100.0, 100.0), 0.01);
        assert!(matches!(dl.ops[0], DrawOp::Stroke { ref coords, .. } if coords.len() == 4));
    }

    #[test]
    fn svg_output_is_stable() {
        let dl = DisplayList {
            ops: vec![
                DrawOp::Fill {
                    color: 0x1d1d1f,
                    alpha: 1.0,
                    coords: vec![0.0, 0.0, 10.0, 0.0, 10.0, 5.5],
                },
                DrawOp::Stroke {
                    color: 0xffe066,
                    alpha: 0.35,
                    width: 2.0,
                    closed: false,
                    coords: vec![1.0, 1.0, 2.0, 3.0],
                },
            ],
        };
        assert_eq!(
            dl.to_svg(Rect::new(0.0, 0.0, 100.0, 50.0)),
            concat!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50">"##,
                r##"<path fill="#1d1d1f" d="M0 0L10 0L10 5.5Z"/>"##,
                r##"<path fill="none" stroke="#ffe066" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" stroke-opacity="0.35" d="M1 1L2 3"/>"##,
                "</svg>"
            )
        );
    }
}
