//! Document model: pages, items and reversible operations with undo/redo.
//! Port of `app/web/src/model.js`. [`crdt::CrdtPage`] stores a page in Loro (ADR-0001, D6);
//! [`Page`] and [`Op`] remain the plain in-memory view and command vocabulary.

pub mod crdt;
pub mod inkcodec;

/// `document.json` of a `.kahie` file (docs/format/kahie-v0.md): readable without the app.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentJson {
    /// Always `"kahie-document"`.
    pub format: String,
    /// Semantic version of the format, e.g. `"0.1.0"`.
    pub version: String,
    pub notebook: NotebookInfo,
    pub pages: Vec<Page>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotebookInfo {
    pub id: Id,
    pub title: String,
}

pub const DOCUMENT_FORMAT: &str = "kahie-document";
pub const DOCUMENT_VERSION: &str = "0.1.0";

use nb_ink::Point;
use nb_recog::Shape;
use serde::{Deserialize, Serialize};

pub type Id = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Pen,
    Highlighter,
}

/// A recognised shape attached to a stroke; `show` toggles it against the raw ink (ADR-0003).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Interp {
    pub shape: Shape,
    pub show: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub tool: Tool,
    pub color: u32,
    pub width: f64,
    pub points: Vec<Point>,
    pub interp: Option<Interp>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x <= o.x + o.w && o.x <= self.x + self.w && self.y <= o.y + o.h && o.y <= self.y + self.h
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Content {
    Stroke(Stroke),
    Text {
        rect: Rect,
        text: String,
        size: f64,
        color: u32,
    },
    Math {
        rect: Rect,
        latex: String,
        size: f64,
        color: u32,
    },
    Plot {
        rect: Rect,
        exprs: Vec<String>,
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
    },
    Image {
        rect: Rect,
        asset: Id,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: Id,
    pub content: Content,
}

impl Item {
    pub fn bounds(&self) -> Rect {
        match &self.content {
            Content::Stroke(s) => {
                let pts = match &s.interp {
                    Some(Interp { shape, show: true }) => shape.points(),
                    _ => s.points.clone(),
                };
                if pts.is_empty() {
                    return Rect {
                        x: 0.0,
                        y: 0.0,
                        w: 0.0,
                        h: 0.0,
                    };
                }
                let (mut x0, mut y0, mut x1, mut y1) =
                    (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
                for p in &pts {
                    x0 = x0.min(p.x);
                    x1 = x1.max(p.x);
                    y0 = y0.min(p.y);
                    y1 = y1.max(p.y);
                }
                let r = s.width / 2.0 + 1.0;
                Rect {
                    x: x0 - r,
                    y: y0 - r,
                    w: x1 - x0 + 2.0 * r,
                    h: y1 - y0 + 2.0 * r,
                }
            }
            Content::Text { rect, .. }
            | Content::Math { rect, .. }
            | Content::Plot { rect, .. }
            | Content::Image { rect, .. } => rect.clone(),
        }
    }

    /// Copy moved by (dx, dy); raw ink and its interpretation move together.
    pub fn translated(&self, dx: f64, dy: f64) -> Item {
        let mut it = self.clone();
        match &mut it.content {
            Content::Stroke(s) => {
                for p in &mut s.points {
                    p.x += dx;
                    p.y += dy;
                }
                if let Some(i) = &mut s.interp {
                    i.shape.translate(dx, dy);
                }
            }
            Content::Text { rect, .. }
            | Content::Math { rect, .. }
            | Content::Plot { rect, .. }
            | Content::Image { rect, .. } => {
                rect.x += dx;
                rect.y += dy;
            }
        }
        it
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: Id,
    #[serde(default = "default_template")]
    pub template: String,
    pub items: Vec<Item>,
}

fn default_template() -> String {
    "lined".into()
}

impl Page {
    pub fn new(id: impl Into<Id>) -> Self {
        Self {
            id: id.into(),
            template: default_template(),
            items: vec![],
        }
    }
}

/// Reversible operations; every op has an exact inverse.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Add(Vec<(usize, Item)>),
    Remove(Vec<(usize, Item)>),
    Update { before: Vec<Item>, after: Vec<Item> },
}

impl Op {
    pub fn add(page: &Page, items: Vec<Item>) -> Op {
        let n = page.items.len();
        Op::Add(items.into_iter().enumerate().map(|(i, it)| (n + i, it)).collect())
    }
    pub fn remove(page: &Page, ids: &[&str]) -> Op {
        Op::Remove(
            page.items
                .iter()
                .enumerate()
                .filter(|(_, it)| ids.contains(&it.id.as_str()))
                .map(|(i, it)| (i, it.clone()))
                .collect(),
        )
    }
    pub fn update(page: &Page, after: Vec<Item>) -> Op {
        let (mut b, mut a) = (vec![], vec![]);
        for it in after {
            if let Some(old) = page.items.iter().find(|o| o.id == it.id) {
                b.push(old.clone());
                a.push(it);
            }
        }
        Op::Update { before: b, after: a }
    }
    pub fn inverse(&self) -> Op {
        match self {
            Op::Add(e) => Op::Remove(e.clone()),
            Op::Remove(e) => Op::Add(e.clone()),
            Op::Update { before, after } => Op::Update {
                before: after.clone(),
                after: before.clone(),
            },
        }
    }
    pub fn is_empty(&self) -> bool {
        match self {
            Op::Add(e) | Op::Remove(e) => e.is_empty(),
            Op::Update { after, .. } => after.is_empty(),
        }
    }
    pub fn apply(&self, page: &mut Page) {
        match self {
            Op::Add(entries) => {
                let mut sorted = entries.clone();
                sorted.sort_by_key(|(i, _)| *i);
                for (i, it) in sorted {
                    let at = i.min(page.items.len());
                    page.items.insert(at, it);
                }
            }
            Op::Remove(entries) => page.items.retain(|it| !entries.iter().any(|(_, e)| e.id == it.id)),
            Op::Update { after, .. } => {
                for it in &mut page.items {
                    if let Some(a) = after.iter().find(|a| a.id == it.id) {
                        *it = a.clone();
                    }
                }
            }
        }
    }
}

/// Linear undo/redo history for one page.
#[derive(Default)]
pub struct History {
    undo: Vec<Op>,
    redo: Vec<Op>,
    limit: usize,
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: vec![],
            redo: vec![],
            limit,
        }
    }
    pub fn apply(&mut self, page: &mut Page, op: Op) -> bool {
        if op.is_empty() {
            return false;
        }
        op.apply(page);
        self.undo.push(op);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
        true
    }
    pub fn undo(&mut self, page: &mut Page) -> bool {
        let Some(op) = self.undo.pop() else { return false };
        op.inverse().apply(page);
        self.redo.push(op);
        true
    }
    pub fn redo(&mut self, page: &mut Page) -> bool {
        let Some(op) = self.redo.pop() else { return false };
        op.apply(page);
        self.undo.push(op);
        true
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(id: &str, pts: &[(f64, f64)]) -> Item {
        Item {
            id: id.into(),
            content: Content::Stroke(Stroke {
                tool: Tool::Pen,
                color: 0x1d1d1f,
                width: 3.0,
                points: pts.iter().map(|&(x, y)| Point::xy(x, y)).collect(),
                interp: None,
            }),
        }
    }
    fn ids(p: &Page) -> Vec<&str> {
        p.items.iter().map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn add_remove_update_round_trip() {
        let mut page = Page::new("p");
        let mut h = History::new(500);
        let op = Op::add(
            &page,
            vec![stroke("a", &[(0.0, 0.0), (10.0, 0.0)]), stroke("b", &[(0.0, 5.0)])],
        );
        h.apply(&mut page, op);
        assert_eq!(ids(&page), ["a", "b"]);
        let op = Op::remove(&page, &["a"]);
        h.apply(&mut page, op);
        assert_eq!(ids(&page), ["b"]);
        h.undo(&mut page);
        assert_eq!(ids(&page), ["a", "b"], "restored at its index");
        let moved = page.items[1].translated(5.0, 5.0);
        let op = Op::update(&page, vec![moved]);
        h.apply(&mut page, op);
        assert_eq!(page.items[1].bounds().x, 5.0 - 2.5);
        h.undo(&mut page);
        assert_eq!(page.items[1].bounds().x, -2.5);
        h.redo(&mut page);
        assert!(!h.can_redo());
        while h.undo(&mut page) {}
        assert!(page.items.is_empty());
    }

    #[test]
    fn empty_ops_ignored_and_new_op_clears_redo() {
        let mut page = Page::new("p");
        let mut h = History::new(10);
        assert!(!h.apply(&mut page, Op::remove(&Page::new("x"), &["missing"])));
        let op = Op::add(&page, vec![stroke("a", &[(0.0, 0.0)])]);
        h.apply(&mut page, op);
        h.undo(&mut page);
        assert!(h.can_redo());
        let op = Op::add(&page, vec![stroke("b", &[(0.0, 0.0)])]);
        h.apply(&mut page, op);
        assert!(!h.can_redo());
    }

    #[test]
    fn page_round_trips_through_json() {
        let mut page = Page::new("p");
        let mut s = stroke("a", &[(0.0, 0.0), (100.0, 1.0)]);
        if let Content::Stroke(st) = &mut s.content {
            st.interp = nb_recog::recognize(&st.points).map(|shape| Interp { shape, show: true });
        }
        page.items.push(s);
        let json = serde_json::to_string(&page).unwrap();
        assert!(json.contains("\"type\":\"stroke\"") && json.contains("\"kind\":\"line\""));
        assert_eq!(serde_json::from_str::<Page>(&json).unwrap(), page);
        let old: Page = serde_json::from_str(r#"{"id":"q","items":[]}"#).unwrap();
        assert_eq!(old.template, "lined", "missing fields get defaults");
    }

    #[test]
    fn interpretation_moves_with_raw_ink() {
        let mut s = stroke("a", &[(0.0, 0.0), (100.0, 0.0)]);
        if let Content::Stroke(st) = &mut s.content {
            st.interp = Some(Interp {
                shape: Shape::Line {
                    a: Point::xy(0.0, 0.0),
                    b: Point::xy(100.0, 0.0),
                },
                show: true,
            });
        }
        let t = s.translated(10.0, 20.0);
        let Content::Stroke(st) = &t.content else {
            unreachable!()
        };
        assert_eq!((st.points[1].x, st.points[1].y), (110.0, 20.0));
        assert_eq!(st.interp.as_ref().unwrap().shape.points()[1], Point::xy(110.0, 20.0));
        let Content::Stroke(orig) = &s.content else {
            unreachable!()
        };
        assert_eq!(orig.points[1].x, 100.0, "original untouched");
    }

    /// The committed example in docs/format/examples must match what the code writes.
    /// Regenerate with `UPDATE_GOLDEN=1 cargo test -p nb-model`.
    #[test]
    fn document_json_example_is_current() {
        let mut page = Page::new("page-1");
        page.template = "grid".into();
        let mut s = stroke("s1", &[(10.0, 10.0), (110.0, 10.5)]);
        if let Content::Stroke(st) = &mut s.content {
            st.interp = nb_recog::recognize(&st.points).map(|shape| Interp { shape, show: true });
        }
        page.items.push(s);
        page.items.push(Item {
            id: "t1".into(),
            content: Content::Text {
                rect: Rect {
                    x: 10.0,
                    y: 40.0,
                    w: 200.0,
                    h: 24.0,
                },
                text: "Théorème de Pythagore".into(),
                size: 16.0,
                color: 0x1d1d1f,
            },
        });
        page.items.push(Item {
            id: "m1".into(),
            content: Content::Math {
                rect: Rect {
                    x: 10.0,
                    y: 80.0,
                    w: 120.0,
                    h: 30.0,
                },
                latex: "a^2 + b^2 = c^2".into(),
                size: 18.0,
                color: 0x1d1d1f,
            },
        });
        page.items.push(Item {
            id: "g1".into(),
            content: Content::Plot {
                rect: Rect {
                    x: 10.0,
                    y: 120.0,
                    w: 300.0,
                    h: 200.0,
                },
                exprs: vec!["x^2 - 2".into()],
                x0: -5.0,
                x1: 5.0,
                y0: -3.0,
                y1: 10.0,
            },
        });
        page.items.push(Item {
            id: "i1".into(),
            content: Content::Image {
                rect: Rect {
                    x: 320.0,
                    y: 120.0,
                    w: 100.0,
                    h: 80.0,
                },
                asset: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".into(),
            },
        });
        let doc = DocumentJson {
            format: DOCUMENT_FORMAT.into(),
            version: DOCUMENT_VERSION.into(),
            notebook: NotebookInfo {
                id: "nb-1".into(),
                title: "Géométrie".into(),
            },
            pages: vec![page, Page::new("page-2")],
        };
        let json = serde_json::to_string_pretty(&doc).unwrap() + "\n";
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../docs/format/examples/document.json"
        );
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(path, &json).unwrap();
        }
        let committed = std::fs::read_to_string(path).expect("run with UPDATE_GOLDEN=1 once");
        assert_eq!(committed, json, "docs/format/examples/document.json is stale");
        assert_eq!(serde_json::from_str::<DocumentJson>(&committed).unwrap(), doc);
    }
}
