//! Virtual instruments (E2.10): ruler, compass and protractor.
//!
//! An instrument lies on the page; a stroke that starts close to its drawing edge is guided:
//! each point is moved onto the edge (ruler), the circle (compass) or a ray at a whole number
//! of degrees from the centre (protractor). Readings (length in cm, angle in degrees) are
//! computed here so that every client shows the same values.
//!
//! Page units: A4 is 794 units wide for 210 mm, so 1 cm = 794 / 21 ≈ 37.8 units.

use nb_ink::Point;
use std::f64::consts::PI;

/// Page units per centimetre.
pub const UNITS_PER_CM: f64 = 794.0 / 21.0;
const DEG: f64 = PI / 180.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instrument {
    /// A straight edge from (x, y), `length` long, turned by `angle` (radians); the drawing
    /// edge is the segment itself.
    Ruler { x: f64, y: f64, angle: f64, length: f64 },
    /// A compass: centre and opening.
    Compass { cx: f64, cy: f64, r: f64 },
    /// A half-disc protractor: centre of its baseline, baseline direction (radians), radius.
    Protractor { cx: f64, cy: f64, angle: f64, r: f64 },
}

impl Instrument {
    /// Whether a stroke starting at `p` should follow the instrument (`tol` in page units).
    pub fn captures(&self, p: Point, tol: f64) -> bool {
        match *self {
            Instrument::Ruler { .. } => {
                let (q, t) = self.ruler_project(p);
                let len = self.ruler_len();
                (-tol..=len + tol).contains(&t) && (q.x - p.x).hypot(q.y - p.y) <= tol
            }
            Instrument::Compass { cx, cy, r } => ((p.x - cx).hypot(p.y - cy) - r).abs() <= tol,
            Instrument::Protractor { cx, cy, .. } => (p.x - cx).hypot(p.y - cy) <= tol,
        }
    }

    fn ruler_len(&self) -> f64 {
        match *self {
            Instrument::Ruler { length, .. } => length,
            _ => 0.0,
        }
    }

    /// Projection on the ruler's edge and its position along it (unclamped).
    fn ruler_project(&self, p: Point) -> (Point, f64) {
        let Instrument::Ruler { x, y, angle, .. } = *self else {
            return (p, 0.0);
        };
        let (c, s) = (angle.cos(), angle.sin());
        let t = (p.x - x) * c + (p.y - y) * s;
        (Point::xy(x + t * c, y + t * s), t)
    }

    /// A guide for one stroke that started at `start`.
    pub fn guide(&self, start: Point) -> Guide {
        Guide {
            inst: *self,
            start,
            ray: None,
        }
    }

    /// Reading for the pen at `p`: distance from the ruler's zero in cm, the compass angle in
    /// degrees (0 to the right, counter-clockwise on screen), or the protractor angle in
    /// whole degrees from its baseline (0–180).
    pub fn reading(&self, p: Point) -> f64 {
        match *self {
            Instrument::Ruler { .. } => self.ruler_project(p).1 / UNITS_PER_CM,
            Instrument::Compass { cx, cy, .. } => {
                let a = -(p.y - cy).atan2(p.x - cx) / DEG;
                a.rem_euclid(360.0)
            }
            Instrument::Protractor { cx, cy, angle, .. } => protractor_degrees(cx, cy, angle, p),
        }
    }

    /// Length (cm) of the segment a–b, as a ruler shows it.
    pub fn length_cm(a: Point, b: Point) -> f64 {
        (b.x - a.x).hypot(b.y - a.y) / UNITS_PER_CM
    }
}

/// Angle of `p` seen from the centre, measured from the baseline (screen y points down, so
/// "up" from the baseline is positive), rounded to a whole degree and kept in 0–180.
fn protractor_degrees(cx: f64, cy: f64, base: f64, p: Point) -> f64 {
    let a = (-(p.y - cy)).atan2(p.x - cx) / DEG + base / DEG;
    let a = a.rem_euclid(360.0);
    let a = if a > 180.0 {
        if a > 270.0 {
            0.0
        } else {
            180.0
        }
    } else {
        a
    };
    a.round()
}

/// One guided stroke: moves the pen's points onto the instrument.
#[derive(Clone, Copy, Debug)]
pub struct Guide {
    inst: Instrument,
    start: Point,
    /// Protractor: the ray's screen-up angle (radians) once chosen.
    ray: Option<f64>,
}

impl Guide {
    /// Where the point `p` (pressure and tilt kept) goes.
    pub fn apply(&mut self, p: Point) -> Point {
        let at = |x: f64, y: f64| Point { x, y, ..p };
        match self.inst {
            Instrument::Ruler { x, y, angle, length } => {
                let t = self.inst.ruler_project(p).1.clamp(0.0, length);
                at(x + t * angle.cos(), y + t * angle.sin())
            }
            Instrument::Compass { cx, cy, r } => {
                let a = (p.y - cy).atan2(p.x - cx);
                at(cx + r * a.cos(), cy + r * a.sin())
            }
            Instrument::Protractor { cx, cy, angle, .. } => {
                let d = (p.x - cx).hypot(p.y - cy);
                // The direction is chosen once the pen is clearly away from the centre, at a
                // whole number of degrees; before that the pen stays at the start.
                if self.ray.is_none() && d >= 24.0 {
                    // Screen-up angle of the ray: the reading minus the baseline's turn.
                    self.ray = Some(protractor_degrees(cx, cy, angle, p) * DEG - angle);
                }
                match self.ray {
                    Some(phi) => {
                        // Distance along the ray (never behind the centre).
                        let t = ((p.x - cx) * phi.cos() - (p.y - cy) * phi.sin()).max(0.0);
                        at(cx + t * phi.cos(), cy - t * phi.sin())
                    }
                    None => at(self.start.x, self.start.y),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn ruler_keeps_strokes_on_its_edge_and_measures_cm() {
        let r = Instrument::Ruler {
            x: 100.0,
            y: 100.0,
            angle: 30.0 * DEG,
            length: 600.0,
        };
        let on = Point::xy(100.0 + 200.0 * (30.0 * DEG).cos(), 100.0 + 200.0 * (30.0 * DEG).sin());
        assert!(r.captures(Point::xy(on.x + 5.0, on.y - 6.0), 12.0));
        assert!(!r.captures(Point::xy(on.x + 30.0, on.y - 40.0), 12.0));
        let mut g = r.guide(on);
        let q = g.apply(Point::new(on.x + 3.0, on.y - 5.0, 0.7));
        // On the line through (100,100) at 30°.
        assert!(close(
            (q.y - 100.0) * (30.0 * DEG).cos() - (q.x - 100.0) * (30.0 * DEG).sin(),
            0.0
        ));
        assert_eq!(q.p, 0.7, "pressure kept");
        let end = g.apply(Point::xy(2000.0, 900.0));
        assert!(close(end.x, 100.0 + 600.0 * (30.0 * DEG).cos()), "clamped to the ruler");
        assert!(close(r.reading(on), 200.0 / UNITS_PER_CM));
        assert!(close(
            Instrument::length_cm(Point::xy(0.0, 0.0), Point::xy(794.0, 0.0)),
            21.0
        ));
    }

    #[test]
    fn compass_draws_on_its_circle() {
        let c = Instrument::Compass {
            cx: 300.0,
            cy: 300.0,
            r: 100.0,
        };
        assert!(c.captures(Point::xy(405.0, 300.0), 10.0));
        assert!(!c.captures(Point::xy(350.0, 300.0), 10.0));
        let mut g = c.guide(Point::xy(400.0, 300.0));
        for (x, y) in [(380.0, 250.0), (300.0, 180.0), (210.0, 320.0)] {
            let q = g.apply(Point::xy(x, y));
            assert!(close((q.x - 300.0).hypot(q.y - 300.0), 100.0));
        }
        assert!(close(c.reading(Point::xy(300.0, 200.0)), 90.0), "up is 90°");
    }

    #[test]
    fn protractor_measures_and_draws_whole_degree_rays() {
        let p = Instrument::Protractor {
            cx: 200.0,
            cy: 400.0,
            angle: 0.0,
            r: 150.0,
        };
        let at = |deg: f64, d: f64| Point::xy(200.0 + d * (deg * DEG).cos(), 400.0 - d * (deg * DEG).sin());
        assert_eq!(p.reading(at(37.3, 100.0)), 37.0);
        assert_eq!(p.reading(at(120.0, 50.0)), 120.0);
        assert_eq!(p.reading(at(-5.0, 50.0)), 0.0, "below the baseline");
        assert!(p.captures(Point::xy(205.0, 398.0), 12.0));
        let mut g = p.guide(Point::xy(201.0, 399.0));
        let first = g.apply(at(52.6, 10.0));
        assert_eq!((first.x, first.y), (201.0, 399.0), "waits for a direction");
        let q = g.apply(at(52.6, 120.0));
        let deg = (-(q.y - 400.0)).atan2(q.x - 200.0) / DEG;
        assert!(close(deg, 53.0), "ray at a whole degree: {deg}");
        let q2 = g.apply(at(58.0, 160.0));
        let deg2 = (-(q2.y - 400.0)).atan2(q2.x - 200.0) / DEG;
        assert!(close(deg2, 53.0), "direction kept for the whole stroke");
        // Turned protractor: baseline at 90° (pointing down on screen) shifts readings.
        let turned = Instrument::Protractor {
            cx: 200.0,
            cy: 400.0,
            angle: 90.0 * DEG,
            r: 150.0,
        };
        assert_eq!(turned.reading(at(0.0, 80.0)), 90.0);
    }
}
