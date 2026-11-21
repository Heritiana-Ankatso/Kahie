//! UniFFI bindings for the Android (Kotlin) app. One `PageDoc` holds a page
//! (a Loro CRDT document with per-user undo, [`CrdtPage`]), the stroke being drawn and the eraser gesture in progress.
//! The UI sends input events and asks for render geometry; all logic stays here.

use nb_ink::{finish_points, outline_nib, segment_hits_polyline, Point};
use nb_model::crdt::CrdtPage;
use nb_model::{Content, Interp, Item, Op, Page, Stroke, Tool, IDENTITY};
use nb_recog::{recognize, Shape};
use nb_store::Store;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

uniffi::setup_scaffolding!();

mod cas;
mod docexport;
mod editing;
mod kahie_file;
mod library;
mod notebook;
mod pdf;
mod search;
mod sync;
pub use cas::{cas_available, cas_command, cas_eval, CasAnswer, CasOp};
pub use editing::{ink_radius, AttachmentBlock, RichSpan};
pub use kahie_file::{export_kahie, import_kahie};
pub use library::{LibraryDoc, LibraryEntry, LibraryKind};
pub use notebook::{NotebookDoc, OutlineEntry, OutlineKind};
pub use pdf::{pdf_extract_text, pdf_file, pdf_import, PdfPageRef, PdfPageSize, PdfWord};
pub use search::{parse_page_range, HitKind, SearchHit, SearchIndex};
pub use sync::{sync_page_once, SyncListener, SyncSession, SyncStatus};

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
    Pencil,
    Brush,
}

impl InkTool {
    pub(crate) fn model(self) -> Tool {
        match self {
            InkTool::Pen => Tool::Pen,
            InkTool::Highlighter => Tool::Highlighter,
            InkTool::Pencil => Tool::Pencil,
            InkTool::Brush => Tool::Brush,
        }
    }
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
    /// Rich text formatting (P1-17), offsets in UTF-16 code units of `text`.
    pub spans: Vec<RichSpan>,
}

/// A maths block: LaTeX source rendered by the UI.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct MathBlock {
    pub key: String,
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub latex: String,
    pub size: f32,
    pub color: u32,
}

/// A point on a plot, in page coordinates, with its label (e.g. `max (1, -2)`).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct PlotMarker {
    pub x: f32,
    pub y: f32,
    /// `root`, `max`, `min`, `inflection`, `intersection`, `point`, `free` (a construction's
    /// free point: draggable, see `PageDoc::move_plot_point`) or `label` (text only).
    pub kind: String,
    pub label: String,
}

/// A shaded area of a plot: polygon in page coordinates (x0, y0, x1, y1, …) and its value.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct PlotShade {
    pub points: Vec<f32>,
    pub label: String,
    pub label_x: f32,
    pub label_y: f32,
}

/// A plot: functions of x, parametric and polar curves, points and analysis lines (roots,
/// extrema, tangents, areas…; syntax in `nb_math::plot`) drawn in a window, inside a box.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct PlotBlock {
    pub key: String,
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub exprs: Vec<String>,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
    /// One entry per expression: polylines in page coordinates (x0, y0, x1, y1, …), split at
    /// poles and undefined points. Empty when the expression does not parse.
    pub curves: Vec<Vec<Vec<f32>>>,
    /// Per line: draw dashed (tangents, normals, asymptotes, `[dashed]`).
    pub dashed: Vec<bool>,
    /// Per line: ARGB colour from a style suffix (0: default palette) and width (0: default).
    pub colors: Vec<u32>,
    pub widths: Vec<f32>,
    /// Points found by analysis lines (roots, extrema, …) and point lines, inside the window.
    pub markers: Vec<PlotMarker>,
    /// Shaded areas (`area f from a to b`, polygons).
    pub shades: Vec<PlotShade>,
    /// Draw axes and grid (false after an `axes off` line).
    pub axes: bool,
    /// Lines that are invalid, with the reason (same order as `exprs`, empty if fine).
    pub errors: Vec<String>,
    /// Tick spacing on each axis, in window units.
    pub x_step: f64,
    pub y_step: f64,
}

fn plot_block(it: &Item) -> Option<PlotBlock> {
    let Content::Plot {
        rect,
        exprs,
        x0,
        x1,
        y0,
        y1,
    } = &it.content
    else {
        return None;
    };
    // Window → box: x right, y up.
    let sx = rect.w / (x1 - x0);
    let sy = rect.h / (y1 - y0);
    let to_page = |(x, y): (f64, f64)| [(rect.x + (x - x0) * sx) as f32, (rect.y + (y1 - y) * sy) as f32];
    let window = nb_math::plot::Window {
        x0: *x0,
        x1: *x1,
        y0: *y0,
        y1: *y1,
    };
    let r = nb_math::plot::render(exprs, &window);
    let curves = r
        .curves
        .into_iter()
        .map(|segs| {
            segs.into_iter()
                .map(|seg| seg.into_iter().flat_map(to_page).collect())
                .collect()
        })
        .collect();
    let markers = r
        .markers
        .into_iter()
        .map(|m| {
            let [x, y] = to_page((m.x, m.y));
            PlotMarker {
                x,
                y,
                kind: format!("{:?}", m.kind).to_lowercase(),
                label: m.label,
            }
        })
        .collect();
    let shades = r
        .shades
        .into_iter()
        .map(|sh| {
            let [lx, ly] = to_page(sh.at);
            PlotShade {
                points: sh.polygon.into_iter().flat_map(to_page).collect(),
                label: sh.label,
                label_x: lx,
                label_y: ly,
            }
        })
        .collect();
    let (dashed, errors, axes, colors, widths) = (r.dashed, r.errors, r.axes, r.colors, r.widths);
    Some(PlotBlock {
        key: item_key(it),
        id: it.id.clone(),
        x: rect.x as f32,
        y: rect.y as f32,
        width: rect.w as f32,
        height: rect.h as f32,
        exprs: exprs.clone(),
        x0: *x0,
        x1: *x1,
        y0: *y0,
        y1: *y1,
        curves,
        dashed,
        colors,
        widths,
        markers,
        shades,
        axes,
        errors,
        x_step: nb_math::tick_step(x1 - x0, 8.0),
        y_step: nb_math::tick_step(y1 - y0, 8.0),
    })
}

/// An image on the page; its bytes come from `PageDoc.image_bytes(hash)`.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct ImageBlock {
    pub key: String,
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// SHA-256 of the image bytes (hex).
    pub hash: String,
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
    /// Partial eraser gesture (P1-11): the eraser path so far and its largest radius.
    partial: Option<(Vec<(Point, Point)>, f64)>,
    seq: u64,
}

/// Minimum time between two local history checkpoints of a page, and how many are kept.
const CHECKPOINT_EVERY_S: i64 = 10 * 60;
const CHECKPOINTS_KEPT: usize = 30;

/// One local history checkpoint: Unix time and encoded Loro frontiers (hex).
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub(crate) struct Checkpoint {
    t: i64,
    f: String,
}

impl Checkpoint {
    fn frontiers(&self) -> Option<Vec<u8>> {
        (0..self.f.len())
            .step_by(2)
            .map(|i| self.f.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
            .collect()
    }
}

/// An earlier version of a page, for "Restore this page as it was".
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct PageVersion {
    /// Pass to `restore_version`.
    pub index: u32,
    /// Unix time (seconds) when this device recorded it.
    pub time_secs: i64,
    /// Number of objects on the page then.
    pub items: u32,
}

/// The app clipboard: items, the image bytes they need and their groups (process-wide).
#[derive(Clone)]
struct Clip {
    items: Vec<Item>,
    assets: Vec<(String, Vec<u8>)>,
    groups: std::collections::BTreeMap<String, String>,
}

static CLIPBOARD: Mutex<Option<Clip>> = Mutex::new(None);

/// Number of items on the app clipboard (0: nothing to paste).
#[uniffi::export]
pub fn clipboard_count() -> u32 {
    CLIPBOARD.lock().unwrap().as_ref().map_or(0, |c| c.items.len() as u32)
}

/// Name of a custom paper size of `width_mm` × `height_mm` (each side 50–1000 mm, rounded
/// to 0.1 mm), for `set_page_size`; `None` if out of range.
#[uniffi::export]
pub fn custom_page_size(width_mm: f64, height_mm: f64) -> Option<String> {
    nb_model::custom_size(width_mm, height_mm)
}

/// Width and height in millimetres of a custom size name (`None` for named sizes).
#[uniffi::export]
pub fn custom_page_size_mm(size: String) -> Option<Vec<f64>> {
    nb_model::parse_custom_size(&size).map(|(w, h)| vec![w, h])
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

fn check_window(x0: f64, x1: f64, y0: f64, y1: f64) -> Result<(), CoreError> {
    let ok = [x0, x1, y0, y1].iter().all(|v| v.is_finite()) && x1 > x0 && y1 > y0;
    if ok {
        Ok(())
    } else {
        Err(CoreError::Format {
            detail: "the window needs x min < x max and y min < y max".into(),
        })
    }
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
    let alpha = s.tool.nib().alpha();
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
            coords: flat(&outline_nib(&s.points, s.width, s.tool.nib())),
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

    /// Record the current version in the page's local history (P1-28): at most one
    /// checkpoint per [`CHECKPOINT_EVERY_S`], the newest [`CHECKPOINTS_KEPT`] kept. Local
    /// only (not synced): the history of what this device saw.
    pub(crate) fn checkpoint(&mut self, force: bool) {
        let Some(store) = self.store.as_ref() else { return };
        let mut list: Vec<Checkpoint> = store
            .meta("versions")
            .ok()
            .flatten()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let f: String = self.doc.frontiers().iter().map(|b| format!("{b:02x}")).collect();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if let Some(last) = list.last() {
            if last.f == f || (!force && now - last.t < CHECKPOINT_EVERY_S) {
                return;
            }
        }
        list.push(Checkpoint { t: now, f });
        let excess = list.len().saturating_sub(CHECKPOINTS_KEPT);
        list.drain(..excess);
        if let Ok(json) = serde_json::to_vec(&list) {
            let _ = store.set_meta("versions", &json);
        }
    }

    pub(crate) fn checkpoints(&self) -> Vec<Checkpoint> {
        self.store
            .as_ref()
            .and_then(|s| s.meta("versions").ok().flatten())
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
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
        {
            let mut st = d.state.lock().unwrap();
            st.persist();
            // The page as it was when opened, before this session's edits.
            st.checkpoint(true);
        }
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
        st.checkpoint(false);
        let snapshot = st.doc.snapshot();
        let Some(store) = st.store.as_mut() else { return Ok(()) };
        store
            .compact(&snapshot)
            .map_err(|e| CoreError::Io { detail: e.to_string() })
    }

    /// Earlier versions recorded on this device, newest first (excluding the current state).
    pub fn versions(&self) -> Vec<PageVersion> {
        let st = self.state.lock().unwrap();
        let now = st.doc.frontiers();
        st.checkpoints()
            .iter()
            .enumerate()
            .rev()
            .filter_map(|(i, c)| {
                let f = c.frontiers()?;
                if f == now {
                    return None;
                }
                let page = st.doc.page_at(&f).ok()?;
                Some(PageVersion {
                    index: i as u32,
                    time_secs: c.t,
                    items: page.items.len() as u32,
                })
            })
            .collect()
    }

    /// Make the page look as it did in version `index` (one undoable change, synced).
    pub fn restore_version(&self, index: u32) -> Result<bool, CoreError> {
        let mut st = self.state.lock().unwrap();
        let fmt = |e: &dyn std::fmt::Display| CoreError::Format { detail: e.to_string() };
        let f = st
            .checkpoints()
            .get(index as usize)
            .and_then(Checkpoint::frontiers)
            .ok_or_else(|| CoreError::Format {
                detail: "no such version".into(),
            })?;
        // Keep the state being replaced, so the restore itself can be undone later too.
        st.checkpoint(true);
        let changed = st.doc.revert_to(&f).map_err(|e| fmt(&e))?;
        st.persist();
        Ok(changed)
    }

    /// Paper size name (`a3`, `a3-landscape`, `a4`, `a4-landscape`, `a5`, `a5-landscape`, `letter`, `letter-landscape`,
    /// or a custom `<w>x<h>mm` from [`custom_page_size`]).
    pub fn page_size(&self) -> String {
        self.state.lock().unwrap().doc.size().to_string()
    }

    /// Unknown or invalid names are ignored.
    pub fn set_page_size(&self, size: String) {
        let mut st = self.state.lock().unwrap();
        st.doc.set_size(&size);
        st.persist();
    }

    /// Page width and height in page units (A4 portrait: 794 × 1123).
    pub fn page_dims(&self) -> Vec<f32> {
        let (w, h) = nb_model::page_dims(self.state.lock().unwrap().doc.size());
        vec![w as f32, h as f32]
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
            tool: cur.tool.model(),
            color: cur.color,
            width: cur.width,
            points: vec![],
            interp: Some(Interp { shape, show: true }),
            frame: IDENTITY,
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
                tool: cur.tool.model(),
                color: cur.color,
                width: cur.width,
                points: finish_points(&cur.points, view_scale.max(0.05)),
                interp: shape.map(|shape| Interp { shape, show: true }),
                frame: IDENTITY,
            }),
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op)
    }

    pub fn cancel_stroke(&self) {
        self.state.lock().unwrap().current = None;
    }

    // ----- eraser -----

    /// Start a stroke eraser gesture: every object touched is deleted whole.
    pub fn begin_erase(&self) {
        let mut st = self.state.lock().unwrap();
        st.erasing = Some(HashSet::new());
        st.partial = None;
    }

    /// Mark items touched by the segment; returns true when something new was hit.
    pub fn erase_segment(&self, ax: f64, ay: f64, bx: f64, by: f64, radius: f64) -> bool {
        let mut st = self.state.lock().unwrap();
        let State {
            doc, erasing, partial, ..
        } = &mut *st;
        let page = doc.page();
        let Some(set) = erasing.as_mut() else { return false };
        let (a, b) = (Point::xy(ax, ay), Point::xy(bx, by));
        if let Some((path, r)) = partial.as_mut() {
            path.push((a, b));
            *r = r.max(radius);
        }
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
        let mut st = self.state.lock().unwrap();
        st.erasing = None;
        st.partial = None;
    }

    pub fn end_erase(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(set) = st.erasing.take() else { return false };
        if let Some((path, r)) = st.partial.take() {
            let op = editing::partial_erase_op(&mut st, &set, &path, r);
            return st.commit(op);
        }
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

    // ----- selection -----

    /// Ids of items whose bounds intersect the rectangle (x0, y0)–(x1, y1), with the rest
    /// of their groups, in drawing order.
    pub fn select_rect(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<String> {
        let r = nb_model::Rect {
            x: x0.min(x1),
            y: y0.min(y1),
            w: (x1 - x0).abs(),
            h: (y1 - y0).abs(),
        };
        let st = self.state.lock().unwrap();
        let page = st.doc.page();
        page.with_groups(&nb_model::select::rect(&page.items, &r))
    }

    /// Bounding box of the given items as [x, y, w, h], or empty if none exist.
    pub fn bounds_of(&self, ids: Vec<String>) -> Vec<f32> {
        let st = self.state.lock().unwrap();
        let mut b: Option<(f64, f64, f64, f64)> = None;
        for it in st.doc.page().items.iter().filter(|it| ids.contains(&it.id)) {
            let r = it.bounds();
            b = Some(match b {
                None => (r.x, r.y, r.x + r.w, r.y + r.h),
                Some((a, c, d, e)) => (a.min(r.x), c.min(r.y), d.max(r.x + r.w), e.max(r.y + r.h)),
            });
        }
        b.map(|(x0, y0, x1, y1)| vec![x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32])
            .unwrap_or_default()
    }

    /// Move items by (dx, dy) as one undoable step. Raw ink is translated, never rewritten.
    pub fn move_items(&self, ids: Vec<String>, dx: f64, dy: f64) -> bool {
        let mut st = self.state.lock().unwrap();
        let moved: Vec<Item> = st
            .doc
            .page()
            .items
            .iter()
            .filter(|it| ids.contains(&it.id))
            .map(|it| it.translated(dx, dy))
            .collect();
        let op = Op::update(st.doc.page(), moved);
        st.commit(op)
    }

    /// Delete items as one undoable step (they go to the page's trash).
    pub fn delete_items(&self, ids: Vec<String>) -> bool {
        let mut st = self.state.lock().unwrap();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let op = Op::remove(st.doc.page(), &refs);
        st.commit(op)
    }

    /// Make this page a copy of `template` (P1-08): its paper, size and background, and its
    /// objects added on top of anything already here (new ids; images included). One undo
    /// step for the objects. Returns the number of objects copied.
    pub fn copy_content_from(&self, template: std::sync::Arc<PageDoc>) -> u32 {
        if std::ptr::eq(self, template.as_ref()) {
            return 0;
        }
        let (items, assets, paper, size, background, pdf) = {
            let src = template.state.lock().unwrap();
            let items = src.doc.page().items.clone();
            let assets: Vec<(String, Vec<u8>)> = src
                .doc
                .asset_hashes()
                .into_iter()
                .filter_map(|h| src.doc.asset(&h).map(|b| (h, b)))
                .collect();
            let background = src.doc.background();
            (
                items,
                assets,
                src.doc.template().to_string(),
                src.doc.size().to_string(),
                background,
                src.doc.pdf_source().map(|p| (p, src.doc.pdf_text_layer())),
            )
        };
        let mut st = self.state.lock().unwrap();
        st.doc.set_template(&paper);
        st.doc.set_size(&size);
        if let Some((p, text)) = &pdf {
            st.doc.set_pdf_source(Some(p), Some(text));
        }
        if let Some(bg) = &background {
            if let Some((_, bytes)) = assets.iter().find(|(h, _)| h == bg) {
                st.doc.set_background(Some((bg, bytes)));
            }
        }
        for (hash, bytes) in &assets {
            if Some(hash) != background.as_ref() && st.doc.asset(hash).is_none() {
                st.doc.put_asset(hash, bytes);
            }
        }
        let n = items.len() as u32;
        let mut copies = Vec::with_capacity(items.len());
        for it in items {
            st.seq += 1;
            let mut c = it;
            c.id = new_id(st.seq);
            copies.push(c);
        }
        let op = Op::add(st.doc.page(), copies);
        st.commit(op);
        n
    }

    /// Copy items to the app clipboard (P1-15), with the images they show, so they can be
    /// pasted on any page of any notebook. Returns the number of items copied.
    pub fn copy_items(&self, ids: Vec<String>) -> u32 {
        let st = self.state.lock().unwrap();
        let items: Vec<Item> = st
            .doc
            .page()
            .items
            .iter()
            .filter(|it| ids.contains(&it.id))
            .cloned()
            .collect();
        let assets = items
            .iter()
            .filter_map(|it| match &it.content {
                Content::Image { asset, .. } | Content::Attachment { asset, .. } => {
                    st.doc.asset(asset).map(|b| (asset.clone(), b))
                }
                _ => None,
            })
            .collect();
        let groups = st
            .doc
            .page()
            .groups
            .iter()
            .filter(|(id, _)| ids.contains(id))
            .map(|(a, b)| (a.clone(), b.clone()))
            .collect();
        let n = items.len() as u32;
        *CLIPBOARD.lock().unwrap() = Some(Clip { items, assets, groups });
        n
    }

    /// Paste the clipboard, offset by (dx, dy), on top of the page. Returns the new ids.
    pub fn paste_items(&self, dx: f64, dy: f64) -> Vec<String> {
        let Some(clip) = CLIPBOARD.lock().unwrap().clone() else {
            return vec![];
        };
        let mut st = self.state.lock().unwrap();
        for (hash, bytes) in &clip.assets {
            if st.doc.asset(hash).is_none() {
                st.doc.put_asset(hash, bytes);
            }
        }
        let mut copies = Vec::with_capacity(clip.items.len());
        let mut renamed = vec![];
        for it in clip.items {
            st.seq += 1;
            let mut c = it.translated(dx, dy);
            c.id = new_id(st.seq);
            renamed.push((it.id, c.id.clone()));
            copies.push(c);
        }
        let new_ids = copies.iter().map(|c| c.id.clone()).collect();
        let op = Op::add(st.doc.page(), copies);
        editing::commit_copies(&mut st, op, &renamed, &clip.groups);
        new_ids
    }

    /// Bring items to the top of the page (`front`) or send them to the bottom.
    pub fn reorder_items(&self, ids: Vec<String>, front: bool) -> bool {
        let mut st = self.state.lock().unwrap();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let changed = st.doc.reorder(&refs, front);
        st.persist();
        changed
    }

    /// Copy items, offset by (dx, dy), on top of the page. Returns the new ids.
    pub fn duplicate_items(&self, ids: Vec<String>, dx: f64, dy: f64) -> Vec<String> {
        let mut st = self.state.lock().unwrap();
        let originals: Vec<Item> = st
            .doc
            .page()
            .items
            .iter()
            .filter(|it| ids.contains(&it.id))
            .cloned()
            .collect();
        let groups = st.doc.page().groups.clone();
        let mut copies = Vec::with_capacity(originals.len());
        let mut renamed = vec![];
        for it in originals {
            st.seq += 1;
            let mut c = it.translated(dx, dy);
            c.id = new_id(st.seq);
            renamed.push((it.id, c.id.clone()));
            copies.push(c);
        }
        let new_ids = copies.iter().map(|c| c.id.clone()).collect();
        let op = Op::add(st.doc.page(), copies);
        editing::commit_copies(&mut st, op, &renamed, &groups);
        new_ids
    }

    // ----- images -----

    pub fn image_blocks(&self) -> Vec<ImageBlock> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .filter_map(|it| match &it.content {
                Content::Image { rect, asset } => Some(ImageBlock {
                    key: item_key(it),
                    id: it.id.clone(),
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.w as f32,
                    height: rect.h as f32,
                    hash: asset.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Add an image (already downsized/encoded by the UI) in a box at (x, y). One undo step.
    pub fn add_image(&self, x: f64, y: f64, width: f64, height: f64, bytes: Vec<u8>) -> String {
        use sha2::Digest;
        let hash: String = sha2::Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut st = self.state.lock().unwrap();
        st.seq += 1;
        let id = new_id(st.seq);
        st.doc.put_asset(&hash, &bytes);
        let item = Item {
            id: id.clone(),
            content: Content::Image {
                rect: nb_model::Rect {
                    x,
                    y,
                    w: width,
                    h: height,
                },
                asset: hash,
            },
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op);
        id
    }

    /// Hash of the page background image (an imported PDF page), if any.
    pub fn background_hash(&self) -> Option<String> {
        self.state.lock().unwrap().doc.background()
    }

    /// Set the page background from encoded image bytes (empty clears it).
    pub fn set_background_image(&self, bytes: Vec<u8>) {
        use sha2::Digest;
        let mut st = self.state.lock().unwrap();
        if bytes.is_empty() {
            st.doc.set_background(None);
        } else {
            let hash: String = sha2::Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            st.doc.set_background(Some((&hash, &bytes)));
        }
        st.persist();
    }

    pub fn image_bytes(&self, hash: String) -> Option<Vec<u8>> {
        self.state.lock().unwrap().doc.asset(&hash)
    }

    // ----- plots -----

    pub fn plot_blocks(&self) -> Vec<PlotBlock> {
        let st = self.state.lock().unwrap();
        st.doc.page().items.iter().filter_map(plot_block).collect()
    }

    /// Add a plot box at (x, y) of `width` × `height` for the window [x0, x1] × [y0, y1].
    #[allow(clippy::too_many_arguments)] // flat FFI signature
    pub fn add_plot(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        exprs: Vec<String>,
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
    ) -> Result<String, CoreError> {
        check_window(x0, x1, y0, y1)?;
        let mut st = self.state.lock().unwrap();
        st.seq += 1;
        let id = new_id(st.seq);
        let item = Item {
            id: id.clone(),
            content: Content::Plot {
                rect: nb_model::Rect {
                    x,
                    y,
                    w: width,
                    h: height,
                },
                exprs,
                x0,
                x1,
                y0,
                y1,
            },
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op);
        Ok(id)
    }

    /// Change a plot's expressions and window. No expressions deletes the plot.
    pub fn set_plot(
        &self,
        id: String,
        exprs: Vec<String>,
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
    ) -> Result<bool, CoreError> {
        let mut st = self.state.lock().unwrap();
        let Some(old) = st.doc.page().items.iter().find(|i| i.id == id).cloned() else {
            return Ok(false);
        };
        if exprs.iter().all(|e| e.trim().is_empty()) {
            let op = Op::remove(st.doc.page(), &[id.as_str()]);
            return Ok(st.commit(op));
        }
        check_window(x0, x1, y0, y1)?;
        let Content::Plot { rect, .. } = &old.content else {
            return Ok(false);
        };
        let item = Item {
            id,
            content: Content::Plot {
                rect: rect.clone(),
                exprs,
                x0,
                x1,
                y0,
                y1,
            },
        };
        let op = Op::update(st.doc.page(), vec![item]);
        Ok(st.commit(op))
    }

    /// The lines of plot `id` with free point `name` moved to page position (x, y), snapped
    /// to `snap` window units (0: no snapping). `None` if there is no such plot or point.
    /// Use with `preview_plot` while dragging and `set_plot` on release (one undo step).
    pub fn move_plot_point(&self, id: String, name: String, x: f64, y: f64, snap: f64) -> Option<Vec<String>> {
        let st = self.state.lock().unwrap();
        let it = st.doc.page().items.iter().find(|i| i.id == id)?;
        let Content::Plot {
            rect,
            exprs,
            x0,
            x1,
            y0,
            y1,
        } = &it.content
        else {
            return None;
        };
        let mut wx = x0 + (x - rect.x) / rect.w * (x1 - x0);
        let mut wy = y1 - (y - rect.y) / rect.h * (y1 - y0);
        if snap > 0.0 {
            wx = (wx / snap).round() * snap;
            wy = (wy / snap).round() * snap;
        }
        let mut lines = exprs.clone();
        let line = lines.iter_mut().enumerate().find(|(i, l)| {
            let base = nb_math::plot::split_style_suffix(l).0;
            matches!(nb_math::geo::parse_line(&base, *i), Ok(Some((n, nb_math::geo::Def::Free(..)))) if n == name)
        })?;
        // Keep the point's style suffix ([red], …).
        let suffix = nb_math::plot::split_style_suffix(line.1).1;
        *line.1 = format!(
            "{name} = ({}, {}){suffix}",
            nb_math::analysis::fmt_num(wx),
            nb_math::analysis::fmt_num(wy)
        );
        Some(lines)
    }

    /// Plot `id` drawn with `exprs` instead of its stored lines (not saved): for previews.
    pub fn preview_plot(&self, id: String, exprs: Vec<String>) -> Option<PlotBlock> {
        let st = self.state.lock().unwrap();
        let mut it = st.doc.page().items.iter().find(|i| i.id == id)?.clone();
        if let Content::Plot { exprs: e, .. } = &mut it.content {
            *e = exprs;
        }
        plot_block(&it)
    }

    /// Topmost plot containing (x, y), if any.
    pub fn plot_at(&self, x: f64, y: f64) -> Option<String> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .rev()
            .find(|it| matches!(it.content, Content::Plot { .. }) && it.bounds().contains(x, y))
            .map(|it| it.id.clone())
    }

    // ----- maths blocks -----

    pub fn math_blocks(&self) -> Vec<MathBlock> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .filter_map(|it| match &it.content {
                Content::Math {
                    rect,
                    latex,
                    size,
                    color,
                } => Some(MathBlock {
                    key: item_key(it),
                    id: it.id.clone(),
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.w as f32,
                    height: rect.h as f32,
                    latex: latex.clone(),
                    size: *size as f32,
                    color: *color,
                }),
                _ => None,
            })
            .collect()
    }

    /// Add a maths block (top-left at x, y; size measured by the UI's renderer). Returns its id.
    #[allow(clippy::too_many_arguments)] // flat FFI signature
    pub fn add_math(&self, x: f64, y: f64, width: f64, height: f64, latex: String, size: f64, color: u32) -> String {
        let mut st = self.state.lock().unwrap();
        st.seq += 1;
        let id = new_id(st.seq);
        let item = Item {
            id: id.clone(),
            content: Content::Math {
                rect: nb_model::Rect {
                    x,
                    y,
                    w: width,
                    h: height,
                },
                latex,
                size,
                color,
            },
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op);
        id
    }

    /// Replace a maths block's LaTeX and measured size. Empty LaTeX deletes the block.
    pub fn set_math(&self, id: String, latex: String, width: f64, height: f64) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(old) = st.doc.page().items.iter().find(|i| i.id == id).cloned() else {
            return false;
        };
        if latex.trim().is_empty() {
            let op = Op::remove(st.doc.page(), &[id.as_str()]);
            return st.commit(op);
        }
        let Content::Math { rect, size, color, .. } = &old.content else {
            return false;
        };
        let item = Item {
            id,
            content: Content::Math {
                rect: nb_model::Rect {
                    w: width,
                    h: height,
                    ..rect.clone()
                },
                latex,
                size: *size,
                color: *color,
            },
        };
        let op = Op::update(st.doc.page(), vec![item]);
        st.commit(op)
    }

    /// Topmost maths block containing (x, y), if any.
    pub fn math_at(&self, x: f64, y: f64) -> Option<String> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .rev()
            .find(|it| matches!(it.content, Content::Math { .. }) && it.bounds().contains(x, y))
            .map(|it| it.id.clone())
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
                    spans,
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
                    spans: editing::spans_to_utf16(text, spans),
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
                spans: vec![],
            },
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op);
        id
    }

    /// Replace the text of a block (and its measured height). Empty text deletes the block.
    /// Formatting is kept where it still fits; use `set_rich_text` to change it.
    pub fn set_text(&self, id: String, text: String, height: f64) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(old) = st.doc.page().items.iter().find(|i| i.id == id).cloned() else {
            return false;
        };
        if text.trim().is_empty() {
            let op = Op::remove(st.doc.page(), &[id.as_str()]);
            return st.commit(op);
        }
        let Content::Text {
            rect,
            size,
            color,
            spans,
            ..
        } = &old.content
        else {
            return false;
        };
        let spans = nb_model::normalize_spans(spans, text.chars().count() as u32);
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
                spans,
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
                partial: None,
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
    fn select_move_duplicate_delete_undo() {
        let doc = PageDoc::new_in_memory();
        draw(&doc, &[(10.0, 10.0), (60.0, 12.0)], false);
        draw(&doc, &[(500.0, 500.0), (560.0, 520.0)], false);
        doc.add_text(20.0, 40.0, 100.0, 20.0, "Note".into(), 16.0, 0);
        let sel = doc.select_rect(0.0, 0.0, 200.0, 100.0);
        assert_eq!(sel.len(), 2, "the far stroke is not selected");
        let b0 = doc.bounds_of(sel.clone());
        assert!(doc.move_items(sel.clone(), 100.0, 50.0));
        let b1 = doc.bounds_of(sel.clone());
        assert!((b1[0] - b0[0] - 100.0).abs() < 0.01 && (b1[1] - b0[1] - 50.0).abs() < 0.01);
        assert!(doc.undo(), "a move is one undo step");
        assert_eq!(doc.bounds_of(sel.clone()), b0);
        let copies = doc.duplicate_items(sel.clone(), 20.0, 20.0);
        assert_eq!((copies.len(), doc.item_count()), (2, 5));
        assert!(doc.delete_items(copies));
        assert_eq!(doc.item_count(), 3);
        assert!(doc.bounds_of(vec!["missing".into()]).is_empty());
    }

    #[test]
    fn math_blocks_add_edit_find_delete() {
        let doc = PageDoc::new_in_memory();
        let id = doc.add_math(50.0, 50.0, 120.0, 30.0, "a^2+b^2=c^2".into(), 20.0, 0);
        assert_eq!(doc.math_at(60.0, 60.0), Some(id.clone()));
        assert_eq!(doc.text_at(60.0, 60.0), None, "maths is not text");
        assert!(doc.set_math(id.clone(), "\\frac{a}{b}".into(), 40.0, 50.0));
        let m = &doc.math_blocks()[0];
        assert_eq!((m.latex.as_str(), m.width, m.height), ("\\frac{a}{b}", 40.0, 50.0));
        assert_eq!(
            doc.select_rect(0.0, 0.0, 100.0, 100.0),
            vec![id.clone()],
            "maths can be selected"
        );
        assert!(doc.set_math(id, "".into(), 0.0, 0.0));
        assert!(doc.math_blocks().is_empty());
    }

    #[test]
    fn drag_a_free_point_of_a_construction() {
        let doc = PageDoc::new_in_memory();
        let lines: Vec<String> = ["A = (0, 0)", "B = (2, 0) [red]", "M = midpoint(A, B)"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // Box 0..100 for window −5..5: 10 px per unit, y up.
        let id = doc
            .add_plot(0.0, 0.0, 100.0, 100.0, lines, -5.0, 5.0, -5.0, 5.0)
            .unwrap();
        assert!(doc.plot_blocks()[0].markers.iter().filter(|m| m.kind == "free").count() == 2);
        let moved = doc.move_plot_point(id.clone(), "B".into(), 91.0, 29.0, 0.5).unwrap();
        assert_eq!(moved[1], "B = (4, 2) [red]", "style kept");
        let preview = doc.preview_plot(id.clone(), moved.clone()).unwrap();
        let m = preview.markers.iter().find(|m| m.label == "M").unwrap();
        assert!(
            (m.x - 70.0).abs() < 1e-3 && (m.y - 40.0).abs() < 1e-3,
            "M = (2, 1) → page (70, 40)"
        );
        assert_eq!(
            doc.plot_blocks()[0].exprs[1],
            "B = (2, 0) [red]",
            "preview does not save"
        );
        assert!(
            doc.move_plot_point(id.clone(), "M".into(), 0.0, 0.0, 0.0).is_none(),
            "M is not free"
        );
        doc.set_plot(id, moved, -5.0, 5.0, -5.0, 5.0).unwrap();
        assert_eq!(doc.plot_blocks()[0].exprs[1], "B = (4, 2) [red]");
    }

    #[test]
    fn plot_analysis_markers_and_areas_in_page_coordinates() {
        let doc = PageDoc::new_in_memory();
        let lines = [
            "f(x) = x^2 - 1",
            "roots f",
            "area f from -1 to 1",
            "tangent f at 1",
            "(cos t, sin t)",
        ];
        doc.add_plot(
            100.0,
            100.0,
            200.0,
            200.0,
            lines.iter().map(|s| s.to_string()).collect(),
            -2.0,
            2.0,
            -2.0,
            2.0,
        )
        .unwrap();
        let p = &doc.plot_blocks()[0];
        assert!(p.errors.iter().all(String::is_empty), "{:?}", p.errors);
        let roots: Vec<(f32, f32)> = p
            .markers
            .iter()
            .filter(|m| m.kind == "root")
            .map(|m| (m.x, m.y))
            .collect();
        // x = ±1, y = 0 → page (150, 200) and (250, 200).
        assert_eq!(roots.len(), 2);
        assert!((roots[0].0 - 150.0).abs() < 0.01 && (roots[0].1 - 200.0).abs() < 0.01);
        assert_eq!(p.shades.len(), 1);
        assert_eq!(p.shades[0].label, "∫ = -1.333");
        assert_eq!(p.dashed, [false, false, false, true, false]);
        assert!(p.markers.iter().any(|m| m.label == "y = 2x − 2"));
        assert!(!p.curves[4].is_empty(), "parametric circle drawn");
    }

    #[test]
    fn plots_add_sample_edit_delete() {
        let doc = PageDoc::new_in_memory();
        let id = doc
            .add_plot(
                100.0,
                100.0,
                400.0,
                300.0,
                vec!["y = x^2 - 2".into(), "1/x".into(), "sin(".into()],
                -5.0,
                5.0,
                -5.0,
                5.0,
            )
            .unwrap();
        let p = &doc.plot_blocks()[0];
        assert_eq!(p.curves.len(), 3);
        assert_eq!(p.curves[0].len(), 1, "a parabola is one polyline");
        assert!(p.curves[1].len() >= 2, "1/x is split at its pole");
        assert!(
            p.curves[2].is_empty() && !p.errors[2].is_empty(),
            "bad expression reported, not drawn"
        );
        // The vertex of x^2-2 at (0,-2) maps to the box: x centre, y = 100 + (5-(-2))*30 = 310.
        let pts = &p.curves[0][0];
        let (mut best, mut by) = (f32::MAX, 0.0f32);
        for xy in pts.chunks(2) {
            if (xy[0] - 300.0).abs() < best {
                best = (xy[0] - 300.0).abs();
                by = xy[1];
            }
        }
        assert!((by - 310.0).abs() < 2.0, "vertex at y={by}");
        assert!(
            doc.add_plot(0.0, 0.0, 10.0, 10.0, vec!["x".into()], 1.0, 1.0, 0.0, 1.0)
                .is_err(),
            "empty window refused"
        );
        assert!(doc
            .set_plot(id.clone(), vec!["cos(x)".into()], -3.0, 3.0, -2.0, 2.0)
            .unwrap());
        assert_eq!(doc.plot_blocks()[0].exprs, vec!["cos(x)".to_string()]);
        assert_eq!(doc.plot_at(150.0, 150.0), Some(id.clone()));
        assert!(
            doc.set_plot(id, vec![], 0.0, 1.0, 0.0, 1.0).unwrap(),
            "no expression deletes"
        );
        assert!(doc.plot_blocks().is_empty());
    }

    #[test]
    fn earlier_versions_are_recorded_on_open_and_restorable() {
        let dir = std::env::temp_dir().join(format!("kahie-ffi-ver-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.kpage").to_string_lossy().to_string();
        {
            let d = PageDoc::open(path.clone()).unwrap();
            assert!(d.versions().is_empty(), "nothing earlier than now");
            d.add_text(10.0, 10.0, 100.0, 20.0, "first".into(), 20.0, 0);
            d.save().unwrap();
        }
        {
            // Session 2 starts from "first"; edit it away.
            let d = PageDoc::open(path.clone()).unwrap();
            let id = d.text_blocks()[0].id.clone();
            d.delete_items(vec![id]);
            d.add_text(10.0, 50.0, 100.0, 20.0, "second".into(), 20.0, 0);
        }
        let d = PageDoc::open(path).unwrap();
        let v = d.versions();
        // Newest first: the start of session 2 ("first"), then the empty new page.
        assert_eq!(v.iter().map(|v| v.items).collect::<Vec<_>>(), [1, 0]);
        assert!(d.restore_version(v[0].index).unwrap());
        let texts: Vec<String> = d.text_blocks().into_iter().map(|t| t.text).collect();
        assert_eq!(texts, ["first"]);
        assert!(d.undo(), "restoring is undoable");
        assert_eq!(d.text_blocks()[0].text, "second");
        // The state that was replaced is now in the history too.
        assert!(d.versions().iter().any(|x| x.items == 1));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_page_can_be_made_from_a_template() {
        let t = PageDoc::new_in_memory();
        t.set_template("grid".into());
        t.set_page_size("a4-landscape".into());
        t.add_text(40.0, 40.0, 300.0, 30.0, "Lesson plan".into(), 28.0, 0);
        t.add_image(40.0, 100.0, 50.0, 50.0, b"\x89PNG logo".to_vec());
        let p = PageDoc::new_in_memory();
        assert_eq!(p.copy_content_from(t.clone()), 2);
        assert_eq!(
            (p.template(), p.page_size()),
            ("grid".to_string(), "a4-landscape".to_string())
        );
        assert_eq!(p.text_blocks()[0].text, "Lesson plan");
        assert_ne!(p.text_blocks()[0].id, t.text_blocks()[0].id, "new ids");
        let h = p.image_blocks()[0].hash.clone();
        assert_eq!(p.image_bytes(h), Some(b"\x89PNG logo".to_vec()));
        assert_eq!(t.copy_content_from(t.clone()), 0, "not onto itself");
    }

    #[test]
    fn copy_paste_between_pages_carries_images() {
        let a = PageDoc::new_in_memory();
        let b = PageDoc::new_in_memory();
        let bytes = b"\x89PNG clip".to_vec();
        let img = a.add_image(10.0, 20.0, 100.0, 50.0, bytes.clone());
        let id = a.add_text(200.0, 200.0, 100.0, 30.0, "x = 2".into(), 20.0, 0);
        assert_eq!(a.copy_items(vec![img, id, "missing".into()]), 2);
        assert_eq!(clipboard_count(), 2);
        let pasted = b.paste_items(0.0, 0.0);
        assert_eq!(pasted.len(), 2);
        let im = &b.image_blocks()[0];
        assert_eq!((im.x, im.y), (10.0, 20.0));
        assert_eq!(b.image_bytes(im.hash.clone()), Some(bytes), "image bytes come along");
        assert_eq!(b.text_blocks()[0].text, "x = 2");
        // Pasting again on the same page gives new ids; one undo removes one paste.
        let again = a.paste_items(24.0, 24.0);
        assert!(again.iter().all(|i| !pasted.contains(i)));
        assert_eq!(a.image_blocks().len(), 2);
        assert!(a.undo());
        assert_eq!(a.image_blocks().len(), 1);
    }

    #[test]
    fn images_are_content_addressed_undoable_and_persisted() {
        let dir = std::env::temp_dir().join(format!("kahie-ffi-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.kpage").to_string_lossy().to_string();
        let bytes = b"\x89PNG fake image".to_vec();
        {
            let doc = PageDoc::open(path.clone()).unwrap();
            let id = doc.add_image(10.0, 20.0, 200.0, 100.0, bytes.clone());
            let img = &doc.image_blocks()[0];
            assert_eq!(img.hash.len(), 64);
            assert_eq!(doc.image_bytes(img.hash.clone()), Some(bytes.clone()));
            doc.add_image(300.0, 20.0, 50.0, 50.0, bytes.clone());
            assert_eq!(
                doc.image_blocks()[0].hash,
                doc.image_blocks()[1].hash,
                "same bytes, same asset"
            );
            assert!(doc.undo());
            assert_eq!(doc.image_blocks().len(), 1, "adding an image is one undo step");
            assert_eq!(doc.select_rect(0.0, 0.0, 50.0, 50.0), vec![id]);
        }
        let doc = PageDoc::open(path).unwrap();
        let h = doc.image_blocks()[0].hash.clone();
        assert_eq!(doc.image_bytes(h), Some(bytes));
        std::fs::remove_dir_all(dir).unwrap();
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
