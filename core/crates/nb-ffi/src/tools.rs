//! Drawing aids: shape tidying modes (E2.8), snapping (E2.9) and virtual instruments (E2.10).
//!
//! Geometry lives in `nb_recog::{snap, instruments}`; this module keeps the page state (the
//! instrument lying on the page) and turns pen strokes into tidied, snapped or guided ink.
//! Raw ink is always kept (ADR-0003): a tidied shape is an interpretation shown in place of
//! the ink, and can be switched back to the original at any time.

use crate::{new_id, Current, PageDoc, State};
use nb_ink::Point;
use nb_model::{Content, Interp, Item, Op, Stroke, IDENTITY};
use nb_recog::instruments::Instrument;
use nb_recog::snap::{grid_step, key_points, Snapper};
use nb_recog::{recognize, Shape};

/// How hand-drawn shapes are tidied when the pen is lifted.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TidyMode {
    /// Ink stays as drawn (holding the pen still at the end still tidies that stroke).
    Free,
    /// A recognised shape is kept as a suggestion; `set_shapes_shown` accepts it.
    Suggest,
    /// Recognised shapes replace the ink at once.
    Auto,
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstrumentKind {
    Ruler,
    Compass,
    Protractor,
}

/// An instrument on the page. `x`, `y`: ruler zero / compass or protractor centre; `angle`
/// in radians (page coordinates); `size`: ruler length, compass opening or protractor radius.
#[derive(uniffi::Record, Clone, Copy, Debug, PartialEq)]
pub struct InstrumentSpec {
    pub kind: InstrumentKind,
    pub x: f64,
    pub y: f64,
    pub angle: f64,
    pub size: f64,
    /// How close (page units) a stroke must start to the instrument to follow it.
    pub capture: f64,
}

impl InstrumentSpec {
    pub(crate) fn instrument(&self) -> Instrument {
        let size = self.size.max(1.0);
        match self.kind {
            InstrumentKind::Ruler => Instrument::Ruler {
                x: self.x,
                y: self.y,
                angle: self.angle,
                length: size,
            },
            InstrumentKind::Compass => Instrument::Compass {
                cx: self.x,
                cy: self.y,
                r: size,
            },
            InstrumentKind::Protractor => Instrument::Protractor {
                cx: self.x,
                cy: self.y,
                angle: self.angle,
                r: size,
            },
        }
    }
}

/// Key points of the tidied shapes on the page, for snapping.
fn page_key_points(st: &State) -> Vec<Point> {
    st.doc
        .page()
        .items
        .iter()
        .filter_map(|it| match &it.content {
            Content::Stroke(Stroke {
                interp: Some(Interp { shape, show: true }),
                ..
            }) => Some(key_points(shape)),
            _ => None,
        })
        .flatten()
        .collect()
}

impl State {
    /// Guide for a stroke starting at `p`, if it starts on the instrument.
    pub(crate) fn guide_for(&self, p: Point) -> Option<nb_recog::instruments::Guide> {
        let spec = self.instrument?;
        let inst = spec.instrument();
        inst.captures(p, spec.capture.max(1.0)).then(|| inst.guide(p))
    }
}

#[uniffi::export]
impl PageDoc {
    /// Put an instrument on the page (replacing any other), or take it away with `None`.
    /// Not saved: instruments belong to this view.
    pub fn set_instrument(&self, spec: Option<InstrumentSpec>) {
        self.state.lock().unwrap().instrument = spec;
    }

    pub fn instrument(&self) -> Option<InstrumentSpec> {
        self.state.lock().unwrap().instrument
    }

    /// The instrument's reading for the pen at (x, y): cm along the ruler, compass angle or
    /// protractor angle in degrees. 0 without an instrument.
    pub fn instrument_reading(&self, x: f64, y: f64) -> f64 {
        self.state
            .lock()
            .unwrap()
            .instrument
            .map(|s| s.instrument().reading(Point::xy(x, y)))
            .unwrap_or(0.0)
    }

    /// Whether the stroke being drawn follows the instrument.
    pub fn stroke_guided(&self) -> bool {
        self.state
            .lock()
            .unwrap()
            .current
            .as_ref()
            .is_some_and(|c| c.guide.is_some())
    }

    /// First point of the stroke being drawn (x, y), after any instrument moved it.
    pub fn stroke_start(&self) -> Vec<f32> {
        let st = self.state.lock().unwrap();
        st.current
            .as_ref()
            .and_then(|c| c.points.first())
            .map(|p| vec![p.x as f32, p.y as f32])
            .unwrap_or_default()
    }

    /// Commit the current stroke, tidied according to `mode`. With `snap`, a tidied shape's
    /// corners, ends and centre snap to other shapes' key points and to the page grid within
    /// `snap_radius`, and lines turn to multiples of 15° when close. Strokes drawn along an
    /// instrument are never reinterpreted. Returns the stroke's id when it holds a suggestion
    /// (mode `Suggest`), for the UI to offer "Tidy".
    pub fn end_stroke_tidy(&self, view_scale: f64, mode: TidyMode, snap: bool, snap_radius: f64) -> Option<String> {
        let mut st = self.state.lock().unwrap();
        let cur: Current = st.current.take()?;
        let guided = cur.guide.is_some();
        let held = cur.snapped.clone();
        let shape: Option<Shape> = if guided {
            None
        } else {
            held.clone().or_else(|| match mode {
                TidyMode::Free => None,
                _ if cur.points.len() >= 4 => recognize(&cur.points),
                _ => None,
            })
        };
        let shape = shape.map(|s| {
            if snap {
                let grid = grid_step(st.doc.template());
                Snapper::new(grid, page_key_points(&st), snap_radius.max(1.0)).shape(&s)
            } else {
                s
            }
        });
        // A shape chosen with the hold gesture is shown at once in every mode.
        let show = held.is_some() || mode == TidyMode::Auto;
        st.seq += 1;
        let id = new_id(st.seq);
        let suggestion = shape.is_some() && !show;
        let points = if guided {
            cur.points.clone()
        } else {
            crate::finish_points(&cur.points, view_scale.max(0.05))
        };
        let item = Item {
            id: id.clone(),
            content: Content::Stroke(Stroke {
                tool: cur.tool.model(),
                color: cur.color,
                width: cur.width,
                points,
                interp: shape.map(|shape| Interp { shape, show }),
                frame: IDENTITY,
            }),
        };
        let op = Op::add(st.doc.page(), vec![item]);
        (st.commit(op) && suggestion).then_some(id)
    }

    /// Show the tidied shape (`true`) or the original ink (`false`) of these strokes, as one
    /// undo step. Strokes without a recognised shape are left alone; returns whether any
    /// changed.
    pub fn set_shapes_shown(&self, ids: Vec<String>, show: bool) -> bool {
        let mut st = self.state.lock().unwrap();
        let changed: Vec<Item> = st
            .doc
            .page()
            .items
            .iter()
            .filter(|it| ids.contains(&it.id))
            .filter_map(|it| match &it.content {
                Content::Stroke(s) if s.interp.as_ref().is_some_and(|i| i.show != show) => {
                    let mut s = s.clone();
                    if let Some(i) = s.interp.as_mut() {
                        i.show = show;
                    }
                    Some(Item {
                        id: it.id.clone(),
                        content: Content::Stroke(s),
                    })
                }
                _ => None,
            })
            .collect();
        if changed.is_empty() {
            return false;
        }
        let op = Op::update(st.doc.page(), changed);
        st.commit(op)
    }

    /// For the selection menu: `Some(true)` if some of these strokes show a tidied shape,
    /// `Some(false)` if some have one that is hidden (and none shown), `None` if none has one.
    pub fn shapes_shown(&self, ids: Vec<String>) -> Option<bool> {
        let st = self.state.lock().unwrap();
        let mut any = None;
        for it in st.doc.page().items.iter().filter(|it| ids.contains(&it.id)) {
            if let Content::Stroke(Stroke { interp: Some(i), .. }) = &it.content {
                if i.show {
                    return Some(true);
                }
                any = Some(false);
            }
        }
        any
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InkTool;

    fn draw(doc: &PageDoc, pts: &[(f64, f64)]) {
        doc.begin_stroke_tilt(InkTool::Pen, 0, 2.0, pts[0].0, pts[0].1, 0.5, 0.0);
        let xypt: Vec<f64> = pts[1..].iter().flat_map(|(x, y)| [*x, *y, 0.5, 0.0]).collect();
        doc.add_points_tilt(xypt);
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<(f64, f64)> {
        (0..=40)
            .map(|k| {
                let t = k as f64 / 40.0;
                (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t + (t * 9.0).sin() * 0.8)
            })
            .collect()
    }

    fn shape_of(doc: &PageDoc, id: &str) -> Option<Interp> {
        let st = doc.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .find(|i| i.id == id)
            .and_then(|i| match &i.content {
                Content::Stroke(s) => s.interp.clone(),
                _ => None,
            })
    }

    fn last_id(doc: &PageDoc) -> String {
        doc.state.lock().unwrap().doc.page().items.last().unwrap().id.clone()
    }

    #[test]
    fn tidy_modes_keep_the_ink_and_suggestions_can_be_accepted_or_undone() {
        let doc = PageDoc::new_in_memory();
        draw(&doc, &line(10.0, 10.0, 300.0, 14.0));
        assert_eq!(doc.end_stroke_tidy(1.0, TidyMode::Free, false, 8.0), None);
        assert!(shape_of(&doc, &last_id(&doc)).is_none(), "free: plain ink");

        draw(&doc, &line(10.0, 100.0, 300.0, 104.0));
        let id = doc
            .end_stroke_tidy(1.0, TidyMode::Suggest, false, 8.0)
            .expect("a suggestion");
        assert!(!shape_of(&doc, &id).unwrap().show, "suggested, not shown");
        assert_eq!(doc.shapes_shown(vec![id.clone()]), Some(false));
        assert!(doc.set_shapes_shown(vec![id.clone()], true), "accept");
        assert!(shape_of(&doc, &id).unwrap().show);
        assert!(
            doc.set_shapes_shown(vec![id.clone()], false),
            "back to the original ink"
        );
        assert!(doc.undo());
        assert!(shape_of(&doc, &id).unwrap().show, "one undo step each");

        draw(&doc, &line(10.0, 200.0, 300.0, 204.0));
        assert_eq!(doc.end_stroke_tidy(1.0, TidyMode::Auto, false, 8.0), None);
        let i = shape_of(&doc, &last_id(&doc)).unwrap();
        assert!(i.show);
        let st = doc.state.lock().unwrap();
        let Content::Stroke(s) = &st.doc.page().items.last().unwrap().content else {
            unreachable!()
        };
        assert!(s.points.len() > 2, "raw ink kept under the shape");
    }

    #[test]
    fn snapping_joins_shapes_and_uses_the_grid() {
        let doc = PageDoc::new_in_memory();
        doc.set_template("grid".into());
        // A line ending near (300, 100) …
        draw(&doc, &line(97.0, 101.0, 299.0, 97.0));
        doc.end_stroke_tidy(1.0, TidyMode::Auto, true, 10.0);
        let Some(Interp {
            shape: Shape::Line { a, b },
            ..
        }) = shape_of(&doc, &last_id(&doc))
        else {
            panic!("a line")
        };
        assert_eq!((a.x, a.y), (96.0, 96.0), "start on a grid node");
        assert!((b.y - 96.0).abs() < 1e-9, "turned horizontal");
        // … and a second line starting close to its end starts exactly there.
        draw(&doc, &line(b.x + 4.0, b.y - 3.0, 340.0, 300.0));
        doc.end_stroke_tidy(1.0, TidyMode::Auto, true, 10.0);
        let Some(Interp {
            shape: Shape::Line { a: a2, .. },
            ..
        }) = shape_of(&doc, &last_id(&doc))
        else {
            panic!("a line")
        };
        assert_eq!((a2.x, a2.y), (b.x, b.y), "joined to the first line's end");
    }

    #[test]
    fn instruments_guide_strokes_that_start_on_them() {
        let doc = PageDoc::new_in_memory();
        doc.set_instrument(Some(InstrumentSpec {
            kind: InstrumentKind::Ruler,
            x: 100.0,
            y: 100.0,
            angle: 0.0,
            size: 500.0,
            capture: 12.0,
        }));
        draw(&doc, &[(150.0, 106.0), (200.0, 120.0), (260.0, 90.0), (320.0, 111.0)]);
        assert!(doc.stroke_guided());
        assert_eq!(doc.stroke_start(), vec![150.0, 100.0], "start moved onto the edge");
        doc.end_stroke_tidy(1.0, TidyMode::Auto, true, 10.0);
        {
            let st = doc.state.lock().unwrap();
            let Content::Stroke(s) = &st.doc.page().items[0].content else {
                unreachable!()
            };
            assert!(s.points.iter().all(|p| p.y == 100.0), "on the ruler's edge");
            assert!(s.interp.is_none(), "guided ink is not reinterpreted");
        }
        assert!((doc.instrument_reading(100.0 + 3.0 * nb_recog::instruments::UNITS_PER_CM, 140.0) - 3.0).abs() < 1e-9);

        // A stroke far from the ruler is free.
        draw(&doc, &[(150.0, 300.0), (200.0, 320.0)]);
        assert!(!doc.stroke_guided());
        doc.cancel_stroke();

        doc.set_instrument(Some(InstrumentSpec {
            kind: InstrumentKind::Compass,
            x: 400.0,
            y: 400.0,
            angle: 0.0,
            size: 100.0,
            capture: 12.0,
        }));
        draw(&doc, &[(503.0, 400.0), (480.0, 330.0), (400.0, 290.0), (330.0, 340.0)]);
        doc.end_stroke_tidy(1.0, TidyMode::Free, false, 10.0);
        let st = doc.state.lock().unwrap();
        let Content::Stroke(s) = &st.doc.page().items.last().unwrap().content else {
            unreachable!()
        };
        assert!(
            s.points
                .iter()
                .all(|p| ((p.x - 400.0).hypot(p.y - 400.0) - 100.0).abs() < 0.1),
            "on the circle (ink is stored to 1/16 unit): {:?}",
            s.points
        );
        drop(st);
        doc.set_instrument(None);
        assert_eq!(doc.instrument_reading(1.0, 1.0), 0.0);
    }
}
