//! Snapping and alignment (E2.9): grid, key points of other shapes and angles.
//!
//! Applied to recognised shapes when they are tidied: a vertex or line end close to a key
//! point (line ends and midpoints, polygon corners, ellipse centres) moves onto it, else onto
//! the nearest grid node; a line whose ends did not snap to points is turned to the nearest
//! multiple of the angle step when it is close to one. Raw ink is never changed (ADR-0003):
//! only the shape interpretation moves.

use crate::Shape;
use nb_ink::{dist, Point};
use std::f64::consts::PI;

const DEG: f64 = PI / 180.0;

/// Grid spacing (page units) of a paper template, if it has a grid to snap to.
pub fn grid_step(template: &str) -> Option<f64> {
    match template {
        "grid" | "dots" => Some(24.0),
        // Millimetre paper: snap to half centimetres.
        "graph" => Some(794.0 / 210.0 * 5.0),
        _ => None,
    }
}

/// What a point snapped to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    None,
    Point,
    Grid,
}

/// Snapping settings and the key points of the page.
#[derive(Clone, Debug)]
pub struct Snapper {
    /// Grid spacing, if the page has a grid.
    pub grid: Option<f64>,
    /// Key points of the other shapes on the page.
    pub points: Vec<Point>,
    /// Capture radius in page units (larger when zoomed out).
    pub radius: f64,
    /// Angle step for lines, in degrees (0 = no angle snapping).
    pub angle_step: f64,
    /// How close (degrees) a line must be to a multiple of the step to turn.
    pub angle_tolerance: f64,
}

impl Snapper {
    pub fn new(grid: Option<f64>, points: Vec<Point>, radius: f64) -> Self {
        Self {
            grid,
            points,
            radius,
            angle_step: 15.0,
            angle_tolerance: 6.0,
        }
    }

    /// Snap one point: the nearest key point within the radius, else the nearest grid node
    /// within the radius.
    pub fn point(&self, p: Point) -> (Point, SnapKind) {
        let best = self
            .points
            .iter()
            .map(|k| (dist(*k, p), *k))
            .filter(|(d, _)| *d <= self.radius)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, k)) = best {
            return (Point::xy(k.x, k.y), SnapKind::Point);
        }
        if let Some(g) = self.grid.filter(|g| *g > 0.0) {
            let q = Point::xy((p.x / g).round() * g, (p.y / g).round() * g);
            if dist(q, p) <= self.radius {
                return (q, SnapKind::Grid);
            }
        }
        (p, SnapKind::None)
    }

    /// Turn `b` around `a` onto the nearest multiple of the angle step, if close to one.
    pub fn angle(&self, a: Point, b: Point) -> Point {
        snap_angle(a, b, self.angle_step, self.angle_tolerance)
    }

    /// The shape with its vertices, ends or centre snapped.
    pub fn shape(&self, shape: &Shape) -> Shape {
        match shape {
            Shape::Line { a, b } => {
                let (a2, ka) = self.point(*a);
                let (b2, kb) = self.point(*b);
                // Two key points: the line joins them exactly.
                if ka == SnapKind::Point && kb == SnapKind::Point {
                    return Shape::Line { a: a2, b: b2 };
                }
                // Otherwise keep the anchored end and turn the other to a round angle,
                // unless it snapped to a point itself.
                match (ka, kb) {
                    (_, SnapKind::Point) => Shape::Line {
                        a: self.angle(b2, a2),
                        b: b2,
                    },
                    _ => {
                        let turned = self.angle(a2, *b);
                        let b3 = if turned == *b { b2 } else { turned };
                        Shape::Line { a: a2, b: b3 }
                    }
                }
            }
            Shape::Polygon { points, closed, label } => Shape::Polygon {
                points: points.iter().map(|p| self.point(*p).0).collect(),
                closed: *closed,
                label: *label,
            },
            Shape::Ellipse {
                cx,
                cy,
                rx,
                ry,
                angle,
                label,
            } => {
                let (c, _) = self.point(Point::xy(*cx, *cy));
                Shape::Ellipse {
                    cx: c.x,
                    cy: c.y,
                    rx: *rx,
                    ry: *ry,
                    angle: *angle,
                    label: *label,
                }
            }
        }
    }
}

/// Turn `b` around `a` onto the nearest multiple of `step` degrees if within `tolerance`.
pub fn snap_angle(a: Point, b: Point, step: f64, tolerance: f64) -> Point {
    if step <= 0.0 {
        return b;
    }
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len < 1e-9 {
        return b;
    }
    let ang = dy.atan2(dx) / DEG;
    let target = (ang / step).round() * step;
    if (ang - target).abs() > tolerance {
        return b;
    }
    let t = target * DEG;
    Point::xy(a.x + len * t.cos(), a.y + len * t.sin())
}

/// Key points of a shape: line ends and midpoint, polygon corners (and side midpoints),
/// ellipse centre.
pub fn key_points(shape: &Shape) -> Vec<Point> {
    let mid = |a: Point, b: Point| Point::xy((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
    match shape {
        Shape::Line { a, b } => vec![*a, *b, mid(*a, *b)],
        Shape::Polygon { points, closed, .. } => {
            let mut v = points.clone();
            let n = points.len();
            let sides = if *closed { n } else { n.saturating_sub(1) };
            for i in 0..sides {
                v.push(mid(points[i], points[(i + 1) % n]));
            }
            v
        }
        Shape::Ellipse { cx, cy, .. } => vec![Point::xy(*cx, *cy)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: Point, x: f64, y: f64) -> bool {
        (a.x - x).abs() < 1e-6 && (a.y - y).abs() < 1e-6
    }

    #[test]
    fn points_beat_grid_and_grid_is_used_within_radius() {
        let s = Snapper::new(Some(24.0), vec![Point::xy(50.0, 50.0)], 8.0);
        assert_eq!(s.point(Point::xy(46.0, 53.0)).1, SnapKind::Point);
        assert!(near(s.point(Point::xy(46.0, 53.0)).0, 50.0, 50.0));
        let (g, k) = s.point(Point::xy(97.0, 118.0));
        assert_eq!(k, SnapKind::Grid);
        assert!(near(g, 96.0, 120.0));
        assert_eq!(s.point(Point::xy(108.0, 108.0)).1, SnapKind::None, "between nodes");
        assert_eq!(
            Snapper::new(None, vec![], 8.0).point(Point::xy(97.0, 118.0)).1,
            SnapKind::None
        );
    }

    #[test]
    fn lines_turn_to_round_angles_or_join_key_points() {
        let s = Snapper::new(None, vec![], 8.0);
        let Shape::Line { a, b } = s.shape(&Shape::Line {
            a: Point::xy(0.0, 0.0),
            b: Point::xy(100.0, 4.0),
        }) else {
            unreachable!()
        };
        assert!(near(a, 0.0, 0.0));
        assert!(
            (b.y).abs() < 1e-9 && (b.x - 100.08).abs() < 0.01,
            "horizontal, same length"
        );
        // 30° is a multiple of 15°; 37° is not close enough to 30 or 45.
        let p = snap_angle(
            Point::xy(0.0, 0.0),
            Point::xy(100.0 * (32.0 * DEG).cos(), 100.0 * (32.0 * DEG).sin()),
            15.0,
            6.0,
        );
        assert!((p.y.atan2(p.x) / DEG - 30.0).abs() < 1e-9);
        let q = Point::xy(100.0 * (37.5 * DEG).cos(), 100.0 * (37.5 * DEG).sin());
        assert_eq!(snap_angle(Point::xy(0.0, 0.0), q, 15.0, 6.0), q);

        let keys = vec![Point::xy(0.0, 0.0), Point::xy(200.0, 37.0)];
        let s = Snapper::new(None, keys, 8.0);
        let Shape::Line { a, b } = s.shape(&Shape::Line {
            a: Point::xy(3.0, -2.0),
            b: Point::xy(196.0, 40.0),
        }) else {
            unreachable!()
        };
        assert!(near(a, 0.0, 0.0) && near(b, 200.0, 37.0), "joins both points exactly");
    }

    #[test]
    fn polygons_and_circles_snap_to_corners_and_centres() {
        let tri = Shape::Polygon {
            points: vec![Point::xy(0.0, 0.0), Point::xy(100.0, 0.0), Point::xy(50.0, 80.0)],
            closed: true,
            label: None,
        };
        let keys = key_points(&tri);
        assert_eq!(keys.len(), 6, "corners and side midpoints");
        let s = Snapper::new(None, keys, 10.0);
        let Shape::Polygon { points, .. } = s.shape(&Shape::Polygon {
            points: vec![Point::xy(103.0, 4.0), Point::xy(150.0, 2.0), Point::xy(52.0, 77.0)],
            closed: true,
            label: None,
        }) else {
            unreachable!()
        };
        assert!(near(points[0], 100.0, 0.0) && near(points[2], 50.0, 80.0));
        assert!(near(points[1], 150.0, 2.0), "nothing near");
        let Shape::Ellipse { cx, cy, rx, .. } = s.shape(&Shape::Ellipse {
            cx: 48.0,
            cy: 3.0,
            rx: 30.0,
            ry: 30.0,
            angle: 0.0,
            label: None,
        }) else {
            unreachable!()
        };
        assert_eq!((cx, cy, rx), (50.0, 0.0, 30.0), "centre on the side midpoint");
        assert_eq!(grid_step("graph").map(|g| g.round()), Some(19.0));
    }
}
