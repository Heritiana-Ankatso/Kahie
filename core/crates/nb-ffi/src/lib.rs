//! UniFFI bindings for the Android (Kotlin) app. One `PageDoc` holds a page
//! (a Loro CRDT document with per-user undo, [`CrdtPage`]), the stroke being drawn and the eraser gesture in progress.
//! The UI sends input events and asks for render geometry; all logic stays here.

use nb_ink::{finish_points, outline, segment_hits_polyline, Point};
use nb_model::crdt::CrdtPage;
use nb_model::{Content, Interp, Item, Op, Page, Stroke, Tool};
use nb_recog::{recognize, Shape};
use nb_store::Store;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

uniffi::setup_scaffolding!();

mod kahie_file;
mod notebook;
mod sync;
pub use kahie_file::{export_kahie, import_kahie};
pub use notebook::{NotebookDoc, OutlineEntry, OutlineKind};
pub use sync::{SyncListener, SyncSession, SyncStatus};

#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    Io { detail: String },
    Format { detail: String },
}

impl std::error::Error for CoreError {}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::Io { detail } => write!(f, "I/O error: {detail}"),
            CoreError::Format { detail } => write!(f, "format error: {detail}"),
        }
    }
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum InkTool {
    Pen,
    Highlighter,
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryKind {
    /// Filled outline polygon (pressure-sensitive ink).
    Fill,
    /// Polyline stroked with `width` (a tidied shape).
    Line,
}

/// A typed block on the page (text for now; maths, plots and images come later).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct TextBlock {
    pub key: String,
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub text: String,
    pub size: f32,
    pub color: u32,
}

/// Geometry the UI can turn into a path. `coords` is x0, y0, x1, y1, …
#[derive(uniffi::Record, Clone, Debug)]
pub struct RenderItem {
    pub key: String,
    pub kind: GeometryKind,
    pub closed: bool,
    pub color: u32,
    pub alpha: f32,
    pub width: f32,
    pub coords: Vec<f32>,
}

struct Current {
    tool: InkTool,
    color: u32,
    width: f64,
    points: Vec<Point>,
    snapped: Option<Shape>,
}

pub(crate) struct State {
    pub(crate) doc: CrdtPage,
    /// Durable storage; `None` for in-memory pages.
    pub(crate) store: Option<Store>,
    /// First persistence error since the last successful `save`, reported by `save`.
    store_error: Option<String>,
    current: Option<Current>,
    erasing: Option<HashSet<String>>,
    seq: u64,
}

#[derive(uniffi::Object)]
pub struct PageDoc {
    pub(crate) state: Mutex<State>,
}

/// CRDT peer id for this session: must differ between devices and app runs.
pub(crate) fn peer_id() -> u64 {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut h = DefaultHasher::new();
    (t, std::process::id()).hash(&mut h);
    // Loro reserves u64::MAX; keep within range.
    h.finish() >> 1
}

fn new_id(seq: u64) -> String {
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{t:x}-{seq:x}")
}

fn stroke_shape_points(s: &Stroke) -> Option<Vec<Point>> {
    match &s.interp {
        Some(Interp { shape, show: true }) => Some(shape.points()),
        _ => None,
    }
}

fn item_key(item: &Item) -> String {
    let mut h = DefaultHasher::new();
    if let Content::Stroke(s) = &item.content {
        s.points.len().hash(&mut h);
        for p in s.points.iter().step_by(7) {
            p.x.to_bits().hash(&mut h);
            p.y.to_bits().hash(&mut h);
        }
        s.color.hash(&mut h);
        s.width.to_bits().hash(&mut h);
        if let Some(i) = &s.interp {
            i.show.hash(&mut h);
        }
    } else {
        // Blocks: any change to content or position gives a new key.
        serde_json::to_string(&item.content).unwrap_or_default().hash(&mut h);
    }
    format!("{}:{:x}", item.id, h.finish())
}

fn flat(points: &[Point]) -> Vec<f32> {
    points.iter().flat_map(|p| [p.x as f32, p.y as f32]).collect()
}

fn render_stroke(key: String, s: &Stroke) -> RenderItem {
    let hl = s.tool == Tool::Highlighter;
    let alpha = if hl { 0.35 } else { 1.0 };
    match &s.interp {
        Some(Interp { shape, show: true }) => RenderItem {
            key,
            kind: GeometryKind::Line,
            closed: !matches!(shape, Shape::Line { .. } | Shape::Polygon { closed: false, .. }),
            color: s.color,
            alpha,
            width: (s.width * 0.9) as f32,
            coords: flat(&shape.points()),
        },
        _ => RenderItem {
            key,
            kind: GeometryKind::Fill,
            closed: true,
            color: s.color,
            alpha,
            width: 0.0,
            coords: flat(&outline(&s.points, s.width, !hl)),
        },
    }
}

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// Compact the update log into a snapshot after this many updates.
const COMPACT_AFTER: i64 = 500;

impl State {
    /// Apply a command and write it to storage.
    fn commit(&mut self, op: Op) -> bool {
        let changed = self.doc.apply(&op);
        self.persist();
        changed
    }

    /// Append unsaved CRDT changes to the store (durable on return).
    pub(crate) fn persist(&mut self) {
        let Some(store) = self.store.as_mut() else {
            self.doc.take_unsaved();
            return;
        };
        let Some(update) = self.doc.take_unsaved() else { return };
        let result = store.append_update(&update).and_then(|_| {
            if store.pending_updates()? >= COMPACT_AFTER {
                store.compact(&self.doc.snapshot())?;
            }
            Ok(())
        });
        if let Err(e) = result {
            self.store_error.get_or_insert(e.to_string());
        }
    }
}

#[uniffi::export]
impl PageDoc {
    /// Open the page stored at `path` (a SQLite database, created if missing). Every edit is
    /// written to it as it happens. Older single-file pages (JSON, or a bare CRDT snapshot)
    /// are migrated in place; the original file is kept next to it with a `.bak` suffix.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<std::sync::Arc<Self>, CoreError> {
        let p = PathBuf::from(&path);
        let io = |e: std::io::Error| CoreError::Io { detail: e.to_string() };
        let fmt = |e: &dyn std::fmt::Display| CoreError::Format { detail: e.to_string() };
        let legacy = match std::fs::read(&p) {
            Ok(bytes) if bytes.starts_with(SQLITE_MAGIC) => None,
            Ok(bytes) if bytes.first() == Some(&b'{') => {
                let page: Page = serde_json::from_slice(&bytes).map_err(|e| fmt(&e))?;
                Some(CrdtPage::from_page(&page, peer_id()))
            }
            Ok(bytes) => Some(CrdtPage::from_snapshot(&bytes, peer_id()).map_err(|e| fmt(&e))?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(io(e)),
        };
        if let Some(mut doc) = legacy {
            let bak = PathBuf::from(format!("{path}.bak"));
            std::fs::rename(&p, &bak).map_err(io)?;
            let mut store = Store::open(&p).map_err(|e| fmt(&e))?;
            store.compact(&doc.snapshot()).map_err(|e| fmt(&e))?;
            doc.take_unsaved();
            return Ok(std::sync::Arc::new(Self::with_doc(doc, Some(store))));
        }
        let store = Store::open(&p).map_err(|e| fmt(&e))?;
        let loaded = store.load().map_err(|e| fmt(&e))?;
        let mut chunks: Vec<&[u8]> = loaded.snapshot.iter().map(Vec::as_slice).collect();
        chunks.extend(loaded.updates.iter().map(Vec::as_slice));
        let doc = if chunks.is_empty() {
            CrdtPage::new(p.file_stem().and_then(|s| s.to_str()).unwrap_or("page"), peer_id())
        } else {
            CrdtPage::load(&chunks, peer_id()).map_err(|e| fmt(&e))?
        };
        let d = Self::with_doc(doc, Some(store));
        d.state.lock().unwrap().persist();
        Ok(std::sync::Arc::new(d))
    }

    /// In-memory page (tests, previews).
    #[uniffi::constructor]
    pub fn new_in_memory() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::with_doc(CrdtPage::new("memory", peer_id()), None))
    }

    /// Edits are already durable when they happen; `save` compacts the log into a snapshot
    /// (faster next open) and reports any earlier write failure.
    pub fn save(&self) -> Result<(), CoreError> {
        let mut st = self.state.lock().unwrap();
        st.persist();
        if let Some(e) = st.store_error.take() {
            return Err(CoreError::Io { detail: e });
        }
        let snapshot = st.doc.snapshot();
        let Some(store) = st.store.as_mut() else { return Ok(()) };
        store
            .compact(&snapshot)
            .map_err(|e| CoreError::Io { detail: e.to_string() })
    }

    pub fn template(&self) -> String {
        self.state.lock().unwrap().doc.template().to_string()
    }

    pub fn set_template(&self, template: String) {
        let mut st = self.state.lock().unwrap();
        st.doc.set_template(&template);
        st.persist();
    }

    pub fn item_count(&self) -> u32 {
        self.state.lock().unwrap().doc.page().items.len() as u32
    }

    // ----- ink -----

    pub fn begin_stroke(&self, tool: InkTool, color: u32, width: f64, x: f64, y: f64, pressure: f64) {
        let mut st = self.state.lock().unwrap();
        st.current = Some(Current {
            tool,
            color,
            width,
            points: vec![Point::new(x, y, pressure)],
            snapped: None,
        });
    }

    /// Adds a point; returns false if it was dropped as a duplicate.
    pub fn add_point(&self, x: f64, y: f64, pressure: f64) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(cur) = st.current.as_mut() else { return false };
        let last = *cur.points.last().unwrap();
        if (x - last.x).hypot(y - last.y) < 0.2 {
            return false;
        }
        // Light pressure smoothing, as in the web app.
        cur.points.push(Point::new(x, y, last.p * 0.6 + pressure * 0.4));
        true
    }

    /// Batched [`PageDoc::add_point`]: one call per input event (`xyp` = x0, y0, p0, x1, …).
    /// Returns the accepted points as x, y, smoothed pressure triples, for wet-ink drawing.
    pub fn add_points(&self, xyp: Vec<f64>) -> Vec<f32> {
        let mut st = self.state.lock().unwrap();
        let Some(cur) = st.current.as_mut() else { return vec![] };
        let mut accepted = Vec::with_capacity(xyp.len());
        for c in xyp.chunks_exact(3) {
            let (x, y, pressure) = (c[0], c[1], c[2]);
            let last = *cur.points.last().unwrap();
            if (x - last.x).hypot(y - last.y) < 0.2 {
                continue;
            }
            let p = Point::new(x, y, last.p * 0.6 + pressure * 0.4);
            cur.points.push(p);
            accepted.extend([p.x as f32, p.y as f32, p.p as f32]);
        }
        accepted
    }

    /// Try to snap the current stroke into a shape (pen held still).
    /// Returns the preview geometry when a shape was recognised.
    pub fn snap_current(&self) -> Option<RenderItem> {
        let mut st = self.state.lock().unwrap();
        let cur = st.current.as_mut()?;
        if cur.points.len() < 4 {
            return None;
        }
        let shape = recognize(&cur.points)?;
        cur.snapped = Some(shape.clone());
        let preview = Stroke {
            tool: if cur.tool == InkTool::Highlighter {
                Tool::Highlighter
            } else {
                Tool::Pen
            },
            color: cur.color,
            width: cur.width,
            points: vec![],
            interp: Some(Interp { shape, show: true }),
        };
        Some(render_stroke("preview".into(), &preview))
    }

    pub fn clear_snap(&self) {
        if let Some(cur) = self.state.lock().unwrap().current.as_mut() {
            cur.snapped = None;
        }
    }

    /// Commit the current stroke. `auto_shapes` recognises shapes without the hold gesture.
    pub fn end_stroke(&self, view_scale: f64, auto_shapes: bool) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(cur) = st.current.take() else { return false };
        let shape = cur.snapped.or_else(|| {
            if auto_shapes && cur.points.len() >= 4 {
                recognize(&cur.points)
            } else {
                None
            }
        });
        st.seq += 1;
        let item = Item {
            id: new_id(st.seq),
            content: Content::Stroke(Stroke {
                tool: if cur.tool == InkTool::Highlighter {
                    Tool::Highlighter
                } else {
                    Tool::Pen
                },
                color: cur.color,
                width: cur.width,
                points: finish_points(&cur.points, view_scale.max(0.05)),
                interp: shape.map(|shape| Interp { shape, show: true }),
            }),
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op)
    }

    pub fn cancel_stroke(&self) {
        self.state.lock().unwrap().current = None;
    }

    // ----- eraser -----

    pub fn begin_erase(&self) {
        self.state.lock().unwrap().erasing = Some(HashSet::new());
    }

    /// Mark items touched by the segment; returns true when something new was hit.
    pub fn erase_segment(&self, ax: f64, ay: f64, bx: f64, by: f64, radius: f64) -> bool {
        let mut st = self.state.lock().unwrap();
        let State { doc, erasing, .. } = &mut *st;
        let page = doc.page();
        let Some(set) = erasing.as_mut() else { return false };
        let (a, b) = (Point::xy(ax, ay), Point::xy(bx, by));
        let mut hit = false;
        for it in &page.items {
            if set.contains(&it.id) {
                continue;
            }
            let touched = match &it.content {
                Content::Stroke(s) => {
                    let pts = stroke_shape_points(s).unwrap_or_else(|| s.points.clone());
                    segment_hits_polyline(&pts, a, b, radius + s.width / 2.0)
                }
                _ => it.bounds().contains(bx, by),
            };
            if touched {
                set.insert(it.id.clone());
                hit = true;
            }
        }
        hit
    }

    /// Keys of items currently being erased (hidden by the UI until the gesture ends).
    pub fn erasing_keys(&self) -> Vec<String> {
        let st = self.state.lock().unwrap();
        let Some(set) = &st.erasing else { return vec![] };
        st.doc
            .page()
            .items
            .iter()
            .filter(|it| set.contains(&it.id))
            .map(item_key)
            .collect()
    }

    /// Abandon the eraser gesture without changing the page.
    pub fn cancel_erase(&self) {
        self.state.lock().unwrap().erasing = None;
    }

    pub fn end_erase(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(set) = st.erasing.take() else { return false };
        let ids: Vec<&str> = set.iter().map(String::as_str).collect();
        let op = Op::remove(st.doc.page(), &ids);
        st.commit(op)
    }

    // ----- history -----

    pub fn undo(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let done = st.doc.undo();
        st.persist();
        done
    }

    pub fn redo(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let done = st.doc.redo();
        st.persist();
        done
    }

    pub fn can_undo(&self) -> bool {
        self.state.lock().unwrap().doc.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.state.lock().unwrap().doc.can_redo()
    }

    pub fn clear(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let ids: Vec<String> = st.doc.page().items.iter().map(|i| i.id.clone()).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let op = Op::remove(st.doc.page(), &refs);
        st.commit(op)
    }

    // ----- rendering -----

    /// Item keys in drawing order. A key changes whenever the item's geometry does,
    /// so the UI can cache one path per key.
    pub fn layout(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .doc
            .page()
            .items
            .iter()
            .map(item_key)
            .collect()
    }

    // ----- text blocks -----

    /// Text blocks in drawing order.
    pub fn text_blocks(&self) -> Vec<TextBlock> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .filter_map(|it| match &it.content {
                Content::Text {
                    rect,
                    text,
                    size,
                    color,
                } => Some(TextBlock {
                    key: item_key(it),
                    id: it.id.clone(),
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.w as f32,
                    height: rect.h as f32,
                    text: text.clone(),
                    size: *size as f32,
                    color: *color,
                }),
                _ => None,
            })
            .collect()
    }

    /// Add a text block with its top-left corner at (x, y); returns its id.
    /// `height` is the laid-out height measured by the UI (used for hit tests and erasing).
    #[allow(clippy::too_many_arguments)] // flat FFI signature
    pub fn add_text(&self, x: f64, y: f64, width: f64, height: f64, text: String, size: f64, color: u32) -> String {
        let mut st = self.state.lock().unwrap();
        st.seq += 1;
        let id = new_id(st.seq);
        let item = Item {
            id: id.clone(),
            content: Content::Text {
                rect: nb_model::Rect {
                    x,
                    y,
                    w: width,
                    h: height,
                },
                text,
                size,
                color,
            },
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op);
        id
    }

    /// Replace the text of a block (and its measured height). Empty text deletes the block.
    pub fn set_text(&self, id: String, text: String, height: f64) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(old) = st.doc.page().items.iter().find(|i| i.id == id).cloned() else {
            return false;
        };
        if text.trim().is_empty() {
            let op = Op::remove(st.doc.page(), &[id.as_str()]);
            return st.commit(op);
        }
        let Content::Text { rect, size, color, .. } = &old.content else {
            return false;
        };
        let item = Item {
            id,
            content: Content::Text {
                rect: nb_model::Rect {
                    h: height,
                    ..rect.clone()
                },
                text,
                size: *size,
                color: *color,
            },
        };
        let op = Op::update(st.doc.page(), vec![item]);
        st.commit(op)
    }

    /// Topmost text block containing (x, y), if any.
    pub fn text_at(&self, x: f64, y: f64) -> Option<String> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .rev()
            .find(|it| matches!(it.content, Content::Text { .. }) && it.bounds().contains(x, y))
            .map(|it| it.id.clone())
    }

    pub fn geometry(&self, key: String) -> Option<RenderItem> {
        let st = self.state.lock().unwrap();
        let id = key.split(':').next()?;
        let item = st.doc.page().items.iter().find(|i| i.id == id)?;
        match &item.content {
            Content::Stroke(s) => Some(render_stroke(item_key(item), s)),
            _ => None,
        }
    }
}

impl PageDoc {
    fn with_doc(doc: CrdtPage, store: Option<Store>) -> Self {
        Self {
            state: Mutex::new(State {
                doc,
                store,
                store_error: None,
                current: None,
                erasing: None,
                seq: 0,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(doc: &PageDoc, pts: &[(f64, f64)], hold: bool) {
        doc.begin_stroke(InkTool::Pen, 0x1d1d1f, 3.0, pts[0].0, pts[0].1, 0.5);
        for &(x, y) in &pts[1..] {
            doc.add_point(x, y, 0.6);
        }
        if hold {
            doc.snap_current();
        }
        doc.end_stroke(1.0, false);
    }

    fn circle() -> Vec<(f64, f64)> {
        (0..=60)
            .map(|i| {
                let a = i as f64 / 60.0 * std::f64::consts::TAU;
                (100.0 + 50.0 * a.cos(), 100.0 + 50.0 * a.sin())
            })
            .collect()
    }

    #[test]
    fn draw_snap_erase_undo() {
        let doc = PageDoc::new_in_memory();
        draw(&doc, &[(0.0, 0.0), (5.0, 3.0), (9.0, 1.0), (14.0, 6.0)], false);
        draw(&doc, &circle(), true);
        assert_eq!(doc.item_count(), 2);
        let keys = doc.layout();
        let g = doc.geometry(keys[1].clone()).unwrap();
        assert_eq!(g.kind, GeometryKind::Line, "held circle becomes a shape");
        assert_eq!(doc.geometry(keys[0].clone()).unwrap().kind, GeometryKind::Fill);

        doc.begin_erase();
        assert!(doc.erase_segment(100.0, 40.0, 100.0, 60.0, 4.0));
        assert_eq!(doc.erasing_keys().len(), 1);
        assert!(doc.end_erase());
        assert_eq!(doc.item_count(), 1);
        assert!(doc.undo());
        assert_eq!(doc.item_count(), 2);
        assert!(doc.redo());
        assert_eq!(doc.item_count(), 1);

        doc.begin_erase();
        doc.erase_segment(0.0, 0.0, 14.0, 6.0, 4.0);
        doc.cancel_erase();
        assert_eq!(doc.item_count(), 1, "cancelled erase changes nothing");
        assert!(!doc.end_erase());
    }

    #[test]
    fn batched_points_match_single_calls() {
        let a = PageDoc::new_in_memory();
        let b = PageDoc::new_in_memory();
        let pts = circle();
        draw(&a, &pts, false);
        b.begin_stroke(InkTool::Pen, 0x1d1d1f, 3.0, pts[0].0, pts[0].1, 0.5);
        let flat: Vec<f64> = pts[1..].iter().flat_map(|&(x, y)| [x, y, 0.6]).collect();
        let accepted = b.add_points(flat);
        assert_eq!(accepted.len() % 3, 0);
        b.end_stroke(1.0, false);
        assert_eq!(a.layout().len(), 1);
        assert_eq!(
            a.geometry(a.layout()[0].clone()).unwrap().coords,
            b.geometry(b.layout()[0].clone()).unwrap().coords
        );
    }

    #[test]
    fn text_blocks_add_edit_find_delete_undo() {
        let doc = PageDoc::new_in_memory();
        let id = doc.add_text(100.0, 50.0, 300.0, 24.0, "Bonjour".into(), 18.0, 0x1d1d1f);
        let k1 = doc.text_blocks()[0].key.clone();
        assert_eq!(doc.text_at(110.0, 60.0), Some(id.clone()));
        assert_eq!(doc.text_at(10.0, 10.0), None);
        assert!(doc.set_text(id.clone(), "Bonjour à tous\nLigne 2".into(), 48.0));
        let b = &doc.text_blocks()[0];
        assert_eq!((b.text.as_str(), b.height), ("Bonjour à tous\nLigne 2", 48.0));
        assert_ne!(b.key, k1, "edited block gets a new key");
        assert!(doc.layout().contains(&b.key));
        assert!(doc.set_text(id.clone(), "  ".into(), 0.0), "empty text deletes");
        assert!(doc.text_blocks().is_empty());
        assert!(doc.undo());
        assert_eq!(doc.text_blocks().len(), 1);
        doc.begin_erase();
        assert!(
            doc.erase_segment(90.0, 60.0, 120.0, 60.0, 4.0),
            "the eraser removes text blocks"
        );
        assert!(doc.end_erase());
        assert!(doc.text_blocks().is_empty());
    }

    #[test]
    fn save_and_reopen() {
        let dir = std::env::temp_dir().join(format!("kahie-ffi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("page-1.json").to_string_lossy().to_string();
        let doc = PageDoc::open(path.clone()).unwrap();
        assert_eq!(doc.template(), "lined");
        draw(&doc, &circle(), true);
        doc.set_template("grid".into());
        doc.save().unwrap();
        let again = PageDoc::open(path).unwrap();
        assert_eq!(again.item_count(), 1);
        assert_eq!(again.template(), "grid");
        assert_eq!(again.layout(), doc.layout());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn corrupt_file_is_an_error_not_a_crash() {
        let dir = std::env::temp_dir().join(format!("kahie-ffi-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert!(matches!(
            PageDoc::open(path.to_string_lossy().into()),
            Err(CoreError::Format { .. })
        ));
        std::fs::write(&path, b"\x00garbage").unwrap();
        assert!(matches!(
            PageDoc::open(path.to_string_lossy().into()),
            Err(CoreError::Format { .. })
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_json_page_is_migrated_on_open() {
        let dir = std::env::temp_dir().join(format!("kahie-ffi-legacy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.json");
        let json = r#"{"id":"p","template":"grid","items":[{"id":"a","content":{"type":"stroke",
            "tool":"pen","color":0,"width":3.0,"points":[{"x":1.0,"y":2.0,"p":0.5},{"x":30.0,"y":2.0,"p":0.5}],
            "interp":null}}]}"#;
        std::fs::write(&path, json).unwrap();
        let doc = PageDoc::open(path.to_string_lossy().into()).unwrap();
        assert_eq!((doc.template(), doc.item_count()), ("grid".to_string(), 1));
        assert!(!doc.can_undo(), "migration is not an undo step");
        doc.save().unwrap();
        assert_ne!(std::fs::read(&path).unwrap()[0], b'{', "saved in the CRDT format");
        let again = PageDoc::open(path.to_string_lossy().into()).unwrap();
        assert_eq!(again.layout(), doc.layout());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn edits_are_durable_without_save() {
        let dir = std::env::temp_dir().join(format!("kahie-ffi-durable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.kahiepage").to_string_lossy().to_string();
        {
            let doc = PageDoc::open(path.clone()).unwrap();
            draw(&doc, &circle(), false);
            draw(&doc, &[(0.0, 0.0), (50.0, 50.0)], false);
            assert!(doc.undo());
            // No save(): simulates the app being killed.
        }
        let again = PageDoc::open(path).unwrap();
        assert_eq!(again.item_count(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// P0-12 budget: load a 2 000-stroke page in ≤ 400 ms (release build, desktop).
    /// Run with `cargo test --release -p nb-ffi -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn load_2000_stroke_page_budget() {
        let dir = std::env::temp_dir().join(format!("kahie-ffi-perf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.kahiepage").to_string_lossy().to_string();
        {
            let doc = PageDoc::open(path.clone()).unwrap();
            for i in 0..2_000 {
                let y = (i % 40) as f64 * 25.0;
                let x0 = (i / 40) as f64 * 15.0;
                let pts: Vec<(f64, f64)> = (0..150)
                    .map(|k| (x0 + k as f64 * 0.1, y + (k as f64 * 0.3).sin() * 5.0))
                    .collect();
                draw(&doc, &pts, false);
            }
            doc.save().unwrap();
        }
        let t = std::time::Instant::now();
        let doc = PageDoc::open(path).unwrap();
        let keys = doc.layout();
        let geoms = keys.iter().filter(|k| doc.geometry((*k).clone()).is_some()).count();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        eprintln!("open + geometry of {geoms} strokes: {ms:.0} ms");
        assert_eq!(geoms, 2_000);
        assert!(ms <= 400.0, "{ms:.0} ms");
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// P0-04 budget: committing a 500-point stroke ≤ 1 ms (core side, release build).
    /// Prints in-memory and on-disk (durable, fsync) timings.
    /// Run with `cargo test --release -p nb-ffi -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn commit_500_point_stroke_budget() {
        let pts: Vec<(f64, f64)> = (0..500)
            .map(|k| (k as f64 * 0.5, (k as f64 * 0.05).sin() * 40.0 + 100.0))
            .collect();
        let dir = std::env::temp_dir().join(format!("kahie-ffi-p004-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (label, doc) in [
            ("in memory", PageDoc::new_in_memory()),
            (
                "durable (SQLite, fsync)",
                PageDoc::open(dir.join("p.db").to_string_lossy().into()).unwrap(),
            ),
        ] {
            // 200 strokes already on the page, as in a typical note.
            for i in 0..200 {
                let off: Vec<(f64, f64)> = pts.iter().map(|&(x, y)| (x, y + i as f64)).collect();
                draw(&doc, &off, false);
            }
            let mut add = vec![];
            let mut commit = vec![];
            for _ in 0..50 {
                let t = std::time::Instant::now();
                doc.begin_stroke(InkTool::Pen, 0, 3.0, pts[0].0, pts[0].1, 0.5);
                // ~4 input events per frame-sized batch, as on a 240 Hz stylus at 60 fps.
                for chunk in pts[1..].chunks(4) {
                    doc.add_points(chunk.iter().flat_map(|&(x, y)| [x, y, 0.6]).collect());
                }
                add.push(t.elapsed().as_secs_f64() * 1000.0);
                let t = std::time::Instant::now();
                doc.end_stroke(1.0, false);
                commit.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            add.sort_by(f64::total_cmp);
            commit.sort_by(f64::total_cmp);
            eprintln!(
                "{label}: 499 points in 4-point add_points batches p50 {:.3} ms; end_stroke p50 {:.3} ms, p95 {:.3} ms",
                add[25], commit[25], commit[47]
            );
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
