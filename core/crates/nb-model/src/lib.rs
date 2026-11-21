//! Document model: pages, items and reversible operations with undo/redo.
//! Port of `app/web/src/model.js`. [`crdt::CrdtPage`] stores a page in Loro (ADR-0001, D6);
//! [`Page`] and [`Op`] remain the plain in-memory view and command vocabulary.

pub mod crdt;
pub mod inkcodec;
pub mod library;
pub mod notebook;
pub mod select;

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
/// 0.2.0 (P1-06…P1-19): pencil/brush tools, pen tilt, rich text spans, attachments, groups,
/// infinite pages. All additions are optional fields or new values, so 0.1 files still read.
pub const DOCUMENT_VERSION: &str = "0.2.0";

use nb_ink::{Nib, Point};
use nb_recog::Shape;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Id = String;

/// Ink tool of a stroke (P1-10). Readers that do not know a tool draw it as a pen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tool {
    Pen,
    Highlighter,
    Pencil,
    Brush,
}

impl Tool {
    /// Pressure/tilt response of the tool.
    pub fn nib(self) -> Nib {
        match self {
            Tool::Pen => Nib::Pen,
            Tool::Highlighter => Nib::Highlighter,
            Tool::Pencil => Nib::Pencil,
            Tool::Brush => Nib::Brush,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Highlighter => "highlighter",
            Tool::Pencil => "pencil",
            Tool::Brush => "brush",
        }
    }

    /// Tool from its stored name; unknown names are pens.
    pub fn from_name(s: &str) -> Tool {
        match s {
            "highlighter" => Tool::Highlighter,
            "pencil" => Tool::Pencil,
            "brush" => Tool::Brush,
            _ => Tool::Pen,
        }
    }
}

/// 2 × 2 linear map `[a, b, c, d]`: (x, y) → (a·x + c·y, b·x + d·y).
pub type Linear = [f64; 4];

/// The identity [`Linear`] map.
pub const IDENTITY: Linear = [1.0, 0.0, 0.0, 1.0];

fn identity() -> Linear {
    IDENTITY
}

/// `m ∘ n` (apply `n` first).
pub fn compose(m: Linear, n: Linear) -> Linear {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
    ]
}

/// Scale and rotation about a centre, then a translation (P1-15): the transforms the
/// selection handles produce. Applied to raw ink it only changes the stroke's frame, never
/// its stored points (ADR-0003).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Similarity {
    pub cx: f64,
    pub cy: f64,
    pub scale: f64,
    /// Radians, clockwise on screen (y points down).
    pub angle: f64,
    pub dx: f64,
    pub dy: f64,
}

impl Similarity {
    pub fn translation(dx: f64, dy: f64) -> Self {
        Self {
            cx: 0.0,
            cy: 0.0,
            scale: 1.0,
            angle: 0.0,
            dx,
            dy,
        }
    }

    pub fn linear(&self) -> Linear {
        let (s, c) = self.angle.sin_cos();
        [self.scale * c, self.scale * s, -self.scale * s, self.scale * c]
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let m = self.linear();
        let (u, v) = (x - self.cx, y - self.cy);
        (
            self.cx + m[0] * u + m[2] * v + self.dx,
            self.cy + m[1] * u + m[3] * v + self.dy,
        )
    }

    fn point(&self, p: &Point) -> Point {
        let (x, y) = self.apply(p.x, p.y);
        Point { x, y, ..*p }
    }

    /// Rectangle with its centre moved and its size scaled (blocks stay upright).
    fn rect(&self, r: &Rect) -> Rect {
        let (cx, cy) = self.apply(r.x + r.w / 2.0, r.y + r.h / 2.0);
        let (w, h) = (r.w * self.scale, r.h * self.scale);
        Rect {
            x: cx - w / 2.0,
            y: cy - h / 2.0,
            w,
            h,
        }
    }

    fn shape(&self, s: &mut Shape) {
        match s {
            Shape::Line { a, b } => {
                *a = self.point(a);
                *b = self.point(b);
            }
            Shape::Polygon { points, .. } => points.iter_mut().for_each(|p| *p = self.point(p)),
            Shape::Ellipse {
                cx, cy, rx, ry, angle, ..
            } => {
                (*cx, *cy) = self.apply(*cx, *cy);
                *rx *= self.scale;
                *ry *= self.scale;
                *angle += self.angle;
            }
        }
    }
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
    /// Points as displayed (stored ink with the stroke's transform applied).
    pub points: Vec<Point>,
    pub interp: Option<Interp>,
    /// Linear part of the transform from the stored (immutable) ink to `points`; the
    /// translation follows from the points. Identity for new ink. Not part of the exchange
    /// format, where `points` are already transformed.
    #[serde(skip, default = "identity")]
    pub frame: Linear,
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

/// Formatting of a run of text in a rich text block (P1-17). Offsets count Unicode scalar
/// values (`char`s). `heading` (1–3) and `list` (1 bullet, 2 numbered) apply to whole
/// paragraphs; the editor always sets them on full lines.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSpan {
    pub start: u32,
    pub end: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub underline: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub heading: u8,
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub list: u8,
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_u8(v: &u8) -> bool {
    *v == 0
}

impl TextSpan {
    /// Same formatting (ignoring the range).
    pub fn same_style(&self, o: &TextSpan) -> bool {
        (
            self.bold,
            self.italic,
            self.underline,
            self.color,
            self.heading,
            self.list,
        ) == (o.bold, o.italic, o.underline, o.color, o.heading, o.list)
    }

    pub fn is_plain(&self) -> bool {
        self.same_style(&TextSpan::default())
    }
}

/// Canonical form of `spans` for a text of `len` chars: clipped, sorted, non-overlapping
/// (later spans win where they overlap), adjacent equal runs merged, plain runs dropped.
pub fn normalize_spans(spans: &[TextSpan], len: u32) -> Vec<TextSpan> {
    let mut cuts: Vec<u32> = vec![0, len];
    for s in spans {
        cuts.push(s.start.min(len));
        cuts.push(s.end.min(len));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<TextSpan> = vec![];
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a >= b {
            continue;
        }
        let Some(style) = spans.iter().rev().find(|s| s.start <= a && s.end >= b) else {
            continue;
        };
        if style.is_plain() {
            continue;
        }
        match out.last_mut() {
            Some(last) if last.end == a && last.same_style(style) => last.end = b,
            _ => out.push(TextSpan {
                start: a,
                end: b,
                ..style.clone()
            }),
        }
    }
    out
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
        /// Rich text formatting (P1-17); empty for plain text.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        spans: Vec<TextSpan>,
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
    /// Any other file (P1-19), kept as an asset and opened with another app.
    Attachment {
        rect: Rect,
        asset: Id,
        name: String,
        mime: String,
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
                let shape;
                let pts = match &s.interp {
                    Some(Interp { shape: sh, show: true }) => {
                        shape = sh.points();
                        &shape
                    }
                    _ => &s.points,
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
                for p in pts {
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
            | Content::Image { rect, .. }
            | Content::Attachment { rect, .. } => rect.clone(),
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
            | Content::Image { rect, .. }
            | Content::Attachment { rect, .. } => {
                rect.x += dx;
                rect.y += dy;
            }
        }
        it
    }

    /// Copy scaled and rotated (P1-15). Ink is transformed through its frame, so the stored
    /// points never change; its width and recognised shape follow. Blocks stay upright:
    /// their centre moves and their size (and font size) scales.
    pub fn transformed(&self, t: &Similarity) -> Item {
        let mut it = self.clone();
        match &mut it.content {
            Content::Stroke(s) => {
                for p in &mut s.points {
                    *p = t.point(p);
                }
                s.frame = compose(t.linear(), s.frame);
                s.width *= t.scale;
                if let Some(i) = &mut s.interp {
                    t.shape(&mut i.shape);
                }
            }
            Content::Text { rect, size, .. } | Content::Math { rect, size, .. } => {
                *rect = t.rect(rect);
                *size *= t.scale;
            }
            Content::Plot { rect, .. } | Content::Image { rect, .. } | Content::Attachment { rect, .. } => {
                *rect = t.rect(rect)
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
    /// Paper size (P1-05): one of [`PAGE_SIZES`] or a custom `<w>x<h>mm` ([`custom_size`]);
    /// A4 portrait by default.
    #[serde(default = "default_size", skip_serializing_if = "is_default_size")]
    pub size: String,
    pub items: Vec<Item>,
    /// Groups (P1-16): item id → group id. Items of a group are selected together.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<Id, Id>,
}

fn default_template() -> String {
    "lined".into()
}

fn default_size() -> String {
    "a4".into()
}

fn is_default_size(s: &String) -> bool {
    s == "a4"
}

/// Supported paper sizes and their dimensions in page units (1 unit = 1 px at 96 dpi).
pub const PAGE_SIZES: &[(&str, f64, f64)] = &[
    ("a3", 1123.0, 1587.0),
    ("a3-landscape", 1587.0, 1123.0),
    ("a4", 794.0, 1123.0),
    ("a4-landscape", 1123.0, 794.0),
    ("a5", 559.0, 794.0),
    ("a5-landscape", 794.0, 559.0),
    ("letter", 816.0, 1056.0),
    ("letter-landscape", 1056.0, 816.0),
];

/// Size name of an infinite canvas page (P1-06): no paper edges, pan and zoom anywhere.
pub const INFINITE: &str = "infinite";

/// Limits of a custom paper size, in millimetres (each side).
pub const CUSTOM_SIZE_MM: (f64, f64) = (50.0, 1000.0);

/// Page units per millimetre (96 per inch).
const UNITS_PER_MM: f64 = 96.0 / 25.4;

/// Canonical name of a custom paper size `<w>x<h>mm` (millimetres, at most one decimal),
/// or `None` if a side is outside [`CUSTOM_SIZE_MM`] or not a number.
pub fn custom_size(width_mm: f64, height_mm: f64) -> Option<String> {
    let (lo, hi) = CUSTOM_SIZE_MM;
    let ok = |v: f64| v.is_finite() && (lo..=hi).contains(&v);
    if !(ok(width_mm) && ok(height_mm)) {
        return None;
    }
    let fmt = |v: f64| {
        let t = (v * 10.0).round() / 10.0;
        if t.fract() == 0.0 {
            format!("{t:.0}")
        } else {
            format!("{t:.1}")
        }
    };
    Some(format!("{}x{}mm", fmt(width_mm), fmt(height_mm)))
}

/// Width and height in millimetres of a custom size name `<w>x<h>mm` (canonical form only).
pub fn parse_custom_size(size: &str) -> Option<(f64, f64)> {
    let (w, h) = size.strip_suffix("mm")?.split_once('x')?;
    let (w, h) = (w.parse().ok()?, h.parse().ok()?);
    (custom_size(w, h)? == size).then_some((w, h))
}

/// Whether `size` is a supported paper size (named, custom or [`INFINITE`]).
pub fn valid_size(size: &str) -> bool {
    size == INFINITE || PAGE_SIZES.iter().any(|(n, _, _)| *n == size) || parse_custom_size(size).is_some()
}

/// Width and height of a paper size in page units; unknown names are A4 portrait. An
/// infinite canvas has no size: it gets A4 here (see [`Page::extent`] for its content).
pub fn page_dims(size: &str) -> (f64, f64) {
    if let Some((w, h)) = parse_custom_size(size) {
        return ((w * UNITS_PER_MM).round(), (h * UNITS_PER_MM).round());
    }
    PAGE_SIZES
        .iter()
        .find(|(n, _, _)| *n == size)
        .map_or((794.0, 1123.0), |&(_, w, h)| (w, h))
}

impl Page {
    pub fn new(id: impl Into<Id>) -> Self {
        Self {
            id: id.into(),
            template: default_template(),
            size: default_size(),
            items: vec![],
            groups: BTreeMap::new(),
        }
    }

    /// Bounding box of all items (`None` for an empty page).
    pub fn content_bounds(&self) -> Option<Rect> {
        let mut b: Option<(f64, f64, f64, f64)> = None;
        for it in &self.items {
            let r = it.bounds();
            b = Some(match b {
                None => (r.x, r.y, r.x + r.w, r.y + r.h),
                Some((a, c, d, e)) => (a.min(r.x), c.min(r.y), d.max(r.x + r.w), e.max(r.y + r.h)),
            });
        }
        b.map(|(x0, y0, x1, y1)| Rect {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        })
    }

    /// The area to show or export: the paper, or for an infinite canvas the content with a
    /// margin (at least an A4 sheet from the origin when empty).
    pub fn extent(&self) -> Rect {
        let (w, h) = page_dims(&self.size);
        if self.size != INFINITE {
            return Rect { x: 0.0, y: 0.0, w, h };
        }
        match self.content_bounds() {
            None => Rect { x: 0.0, y: 0.0, w, h },
            Some(r) => Rect {
                x: r.x - 40.0,
                y: r.y - 40.0,
                w: r.w + 80.0,
                h: r.h + 80.0,
            },
        }
    }

    /// Ids of `ids` plus every item grouped with one of them, in drawing order.
    pub fn with_groups(&self, ids: &[String]) -> Vec<String> {
        let groups: std::collections::HashSet<&Id> = ids.iter().filter_map(|id| self.groups.get(id)).collect();
        if groups.is_empty() {
            return ids.to_vec();
        }
        self.items
            .iter()
            .filter(|it| ids.contains(&it.id) || self.groups.get(&it.id).is_some_and(|g| groups.contains(g)))
            .map(|it| it.id.clone())
            .collect()
    }
}

/// Reversible operations; every op has an exact inverse.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    Add(Vec<(usize, Item)>),
    Remove(Vec<(usize, Item)>),
    Update {
        before: Vec<Item>,
        after: Vec<Item>,
    },
    /// Several ops applied in order as one command (one undo step), e.g. the partial
    /// eraser: remove a stroke, add the pieces left of it at its place.
    Batch(Vec<Op>),
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
            Op::Batch(ops) => Op::Batch(ops.iter().rev().map(Op::inverse).collect()),
        }
    }
    pub fn is_empty(&self) -> bool {
        match self {
            Op::Add(e) | Op::Remove(e) => e.is_empty(),
            Op::Update { after, .. } => after.is_empty(),
            Op::Batch(ops) => ops.iter().all(Op::is_empty),
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
            Op::Batch(ops) => ops.iter().for_each(|o| o.apply(page)),
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
                frame: IDENTITY,
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
    fn spans_are_normalised() {
        let b = |s, e| TextSpan {
            start: s,
            end: e,
            bold: true,
            ..Default::default()
        };
        let i = |s, e| TextSpan {
            start: s,
            end: e,
            italic: true,
            ..Default::default()
        };
        assert_eq!(normalize_spans(&[b(0, 3), b(3, 6)], 10), vec![b(0, 6)]);
        assert_eq!(normalize_spans(&[b(4, 20)], 10), vec![b(4, 10)]);
        assert_eq!(
            normalize_spans(&[b(0, 6), i(2, 4)], 10),
            vec![b(0, 2), i(2, 4), b(4, 6)]
        );
        assert_eq!(
            normalize_spans(
                &[TextSpan {
                    start: 0,
                    end: 5,
                    ..Default::default()
                }],
                10
            ),
            vec![]
        );
        // Old documents without spans still read.
        let old = r#"{"type":"text","rect":{"x":0,"y":0,"w":10,"h":10},"text":"hi","size":12,"color":0}"#;
        assert!(
            matches!(serde_json::from_str::<Content>(old).unwrap(), Content::Text { spans, .. } if spans.is_empty())
        );
    }

    #[test]
    fn similarity_scales_and_rotates_ink_through_its_frame() {
        let s = stroke("a", &[(10.0, 0.0), (20.0, 0.0)]);
        let t = Similarity {
            cx: 0.0,
            cy: 0.0,
            scale: 2.0,
            angle: std::f64::consts::FRAC_PI_2,
            dx: 1.0,
            dy: 0.0,
        };
        let r = s.transformed(&t);
        let Content::Stroke(st) = &r.content else {
            unreachable!()
        };
        assert!((st.points[0].x - 1.0).abs() < 1e-9 && (st.points[0].y - 20.0).abs() < 1e-9);
        assert!((st.points[1].y - 40.0).abs() < 1e-9);
        assert_eq!(st.width, 6.0);
        assert!((st.frame[1] - 2.0).abs() < 1e-12 && st.frame[0].abs() < 1e-12);
        let mut page = Page::new("p");
        page.items.push(s);
        page.items.push(stroke("b", &[(0.0, 0.0)]));
        page.groups.insert("a".into(), "g".into());
        page.groups.insert("b".into(), "g".into());
        assert_eq!(page.with_groups(&["b".into()]), ["a", "b"]);
    }

    #[test]
    fn infinite_canvas_extent_follows_content() {
        let mut page = Page::new("p");
        page.size = INFINITE.into();
        assert!(valid_size(INFINITE));
        assert_eq!(page.extent().w, 794.0);
        page.items.push(stroke("a", &[(-1000.0, -50.0), (3000.0, 20.0)]));
        let e = page.extent();
        assert!(e.x < -1000.0 && e.x + e.w > 3000.0);
    }

    #[test]
    fn batch_is_one_reversible_command() {
        let mut page = Page::new("p");
        let mut h = History::new(10);
        let op = Op::add(&page, vec![stroke("a", &[(0.0, 0.0)]), stroke("b", &[(1.0, 1.0)])]);
        h.apply(&mut page, op);
        let rm = Op::remove(&page, &["a"]);
        let add = Op::Add(vec![(0, stroke("a1", &[(0.0, 0.0)])), (1, stroke("a2", &[(0.5, 0.0)]))]);
        h.apply(&mut page, Op::Batch(vec![rm, add]));
        assert_eq!(ids(&page), ["a1", "a2", "b"]);
        h.undo(&mut page);
        assert_eq!(ids(&page), ["a", "b"]);
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
                spans: vec![
                    TextSpan {
                        start: 0,
                        end: 9,
                        bold: true,
                        ..Default::default()
                    },
                    TextSpan {
                        start: 13,
                        end: 22,
                        italic: true,
                        color: Some(0x2b59c3),
                        ..Default::default()
                    },
                ],
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
        page.items.push(Item {
            id: "f1".into(),
            content: Content::Attachment {
                rect: Rect {
                    x: 440.0,
                    y: 120.0,
                    w: 180.0,
                    h: 48.0,
                },
                asset: "60303ae22b998861bce3b28f33eec1be758a213c86c93c076dbe9f558c11c752".into(),
                name: "worksheet.odt".into(),
                mime: "application/vnd.oasis.opendocument.text".into(),
            },
        });
        page.groups.insert("i1".into(), "grp-1".into());
        page.groups.insert("f1".into(), "grp-1".into());
        let mut canvas = Page::new("page-2");
        canvas.size = INFINITE.into();
        canvas.template = "blank".into();
        let mut ink = stroke("s2", &[(-500.0, -200.0), (-450.0, -180.0)]);
        if let Content::Stroke(st) = &mut ink.content {
            st.tool = Tool::Pencil;
            st.points[1].t = 0.6;
        }
        canvas.items.push(ink);
        let doc = DocumentJson {
            format: DOCUMENT_FORMAT.into(),
            version: DOCUMENT_VERSION.into(),
            notebook: NotebookInfo {
                id: "nb-1".into(),
                title: "Géométrie".into(),
            },
            pages: vec![page, canvas],
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
