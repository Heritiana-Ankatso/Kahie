//! UniFFI bindings for the Android (Kotlin) app. One `PageDoc` holds a page,
//! its undo history, the stroke being drawn and the eraser gesture in progress.
//! The UI sends input events and asks for render geometry; all logic stays here.

use nb_ink::{finish_points, outline, segment_hits_polyline, Point};
use nb_model::{Content, History, Interp, Item, Op, Page, Stroke, Tool};
use nb_recog::{recognize, Shape};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

uniffi::setup_scaffolding!();

#[derive(Debug, uniffi::Error)]
pub enum CoreError {
    Io { message: String },
    Format { message: String },
}

impl std::error::Error for CoreError {}

impl std::fmt::Display for CoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoreError::Io { message } => write!(f, "I/O error: {message}"),
            CoreError::Format { message } => write!(f, "format error: {message}"),
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

struct State {
    page: Page,
    history: History,
    path: Option<PathBuf>,
    current: Option<Current>,
    erasing: Option<HashSet<String>>,
    seq: u64,
}

#[derive(uniffi::Object)]
pub struct PageDoc {
    state: Mutex<State>,
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

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

#[uniffi::export]
impl PageDoc {
    /// Open a page stored at `path` (created on first save), or an empty page.
    #[uniffi::constructor]
    pub fn open(path: String) -> Result<std::sync::Arc<Self>, CoreError> {
        let p = PathBuf::from(&path);
        let page = match std::fs::read(&p) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| CoreError::Format { message: e.to_string() })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Page::new(p.file_stem().and_then(|s| s.to_str()).unwrap_or("page"))
            }
            Err(e) => return Err(CoreError::Io { message: e.to_string() }),
        };
        Ok(std::sync::Arc::new(Self::with_page(page, Some(p))))
    }

    /// In-memory page (tests, previews).
    #[uniffi::constructor]
    pub fn new_in_memory() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::with_page(Page::new("memory"), None))
    }

    pub fn save(&self) -> Result<(), CoreError> {
        let st = self.state.lock().unwrap();
        let Some(path) = &st.path else { return Ok(()) };
        let json = serde_json::to_vec(&st.page).map_err(|e| CoreError::Format { message: e.to_string() })?;
        write_atomic(path, &json).map_err(|e| CoreError::Io { message: e.to_string() })
    }

    pub fn template(&self) -> String {
        self.state.lock().unwrap().page.template.clone()
    }

    pub fn set_template(&self, template: String) {
        self.state.lock().unwrap().page.template = template;
    }

    pub fn item_count(&self) -> u32 {
        self.state.lock().unwrap().page.items.len() as u32
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
        let State { page, history, .. } = &mut *st;
        let op = Op::add(page, vec![item]);
        history.apply(page, op)
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
        let State { page, erasing, .. } = &mut *st;
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
        st.page
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
        let op = Op::remove(&st.page, &ids);
        let State { page, history, .. } = &mut *st;
        history.apply(page, op)
    }

    // ----- history -----

    pub fn undo(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let State { page, history, .. } = &mut *st;
        history.undo(page)
    }

    pub fn redo(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let State { page, history, .. } = &mut *st;
        history.redo(page)
    }

    pub fn can_undo(&self) -> bool {
        self.state.lock().unwrap().history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.state.lock().unwrap().history.can_redo()
    }

    pub fn clear(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let ids: Vec<String> = st.page.items.iter().map(|i| i.id.clone()).collect();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let op = Op::remove(&st.page, &refs);
        let State { page, history, .. } = &mut *st;
        history.apply(page, op)
    }

    // ----- rendering -----

    /// Item keys in drawing order. A key changes whenever the item's geometry does,
    /// so the UI can cache one path per key.
    pub fn layout(&self) -> Vec<String> {
        self.state.lock().unwrap().page.items.iter().map(item_key).collect()
    }

    pub fn geometry(&self, key: String) -> Option<RenderItem> {
        let st = self.state.lock().unwrap();
        let id = key.split(':').next()?;
        let item = st.page.items.iter().find(|i| i.id == id)?;
        match &item.content {
            Content::Stroke(s) => Some(render_stroke(item_key(item), s)),
            _ => None,
        }
    }
}

impl PageDoc {
    fn with_page(page: Page, path: Option<PathBuf>) -> Self {
        Self {
            state: Mutex::new(State {
                page,
                history: History::new(500),
                path,
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
        std::fs::remove_dir_all(dir).unwrap();
    }
}
