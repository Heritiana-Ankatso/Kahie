//! Editing features of a page (Phase 1, E1.3–E1.5): pencil and brush with tilt, partial
//! eraser, lasso selection, scale/rotate, groups, rich text, attachments, image crop/rotate
//! and the infinite canvas. Kept apart from `lib.rs` so the core page API stays readable.

use crate::{item_key, new_id, render_stroke, Current, InkTool, PageDoc, RenderItem, State};
use nb_ink::{erase_pieces, nib_radius, Point};
use nb_model::{normalize_spans, Content, Interp, Item, Op, Similarity, Stroke, TextSpan, IDENTITY};
use std::collections::{BTreeMap, HashSet};

/// Formatting of a run of a text block (P1-17). Offsets are UTF-16 code units, as Android
/// text uses; the model counts characters.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct RichSpan {
    pub start: u32,
    pub end: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub color: Option<u32>,
    /// Paragraph heading level, 1–3 (0: none).
    pub heading: u8,
    /// Paragraph list: 1 bullet, 2 numbered (0: none).
    pub list: u8,
}

/// An attached file (P1-19); its bytes come from `PageDoc.image_bytes(hash)`.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct AttachmentBlock {
    pub key: String,
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub name: String,
    pub mime: String,
    pub hash: String,
}

/// Stroke radius of `tool` for base `width`, pressure and tilt (radians): the response curve
/// the core draws with (P1-10), for the UI's wet ink.
#[uniffi::export]
pub fn ink_radius(tool: InkTool, width: f64, pressure: f64, tilt: f64) -> f64 {
    nib_radius(tool.model().nib(), width, pressure, tilt)
}

/// UTF-16 offset of each char boundary of `text` (`len + 1` entries).
fn utf16_offsets(text: &str) -> Vec<u32> {
    let mut v = Vec::with_capacity(text.len() + 1);
    let mut at = 0u32;
    v.push(0);
    for c in text.chars() {
        at += c.len_utf16() as u32;
        v.push(at);
    }
    v
}

pub(crate) fn spans_to_utf16(text: &str, spans: &[TextSpan]) -> Vec<RichSpan> {
    if spans.is_empty() {
        return vec![];
    }
    let offs = utf16_offsets(text);
    let at = |i: u32| offs[(i as usize).min(offs.len() - 1)];
    spans
        .iter()
        .map(|s| RichSpan {
            start: at(s.start),
            end: at(s.end),
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            color: s.color,
            heading: s.heading,
            list: s.list,
        })
        .collect()
}

fn spans_from_utf16(text: &str, spans: &[RichSpan]) -> Vec<TextSpan> {
    let offs = utf16_offsets(text);
    // First char boundary at or after a UTF-16 offset.
    let idx = |u: u32| offs.partition_point(|&o| o < u) as u32;
    let model: Vec<TextSpan> = spans
        .iter()
        .map(|s| TextSpan {
            start: idx(s.start),
            end: idx(s.end),
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            color: s.color,
            heading: s.heading.min(3),
            list: s.list.min(2),
        })
        .collect();
    normalize_spans(&model, (offs.len() - 1) as u32)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// The partial eraser's command: each touched stroke is removed (it stays in the page's
/// trash, its ink untouched) and the pieces left of it are added at its place in the
/// stacking order, all as one undo step. Blocks and recognised shapes are erased whole.
pub(crate) fn partial_erase_op(st: &mut State, set: &HashSet<String>, path: &[(Point, Point)], r: f64) -> Op {
    let page = st.doc.page().clone();
    let mut ops = vec![];
    let touched: Vec<usize> = (0..page.items.len())
        .filter(|&i| set.contains(&page.items[i].id))
        .collect();
    // From the top down, so lower indices stay valid while the batch runs.
    for i in touched.into_iter().rev() {
        let it = &page.items[i];
        let pieces = match &it.content {
            Content::Stroke(s) if !matches!(s.interp, Some(Interp { show: true, .. })) => {
                match erase_pieces(&s.points, path, r + s.width / 2.0, 0.5) {
                    None => continue,
                    Some(p) => p.into_iter().map(|pts| (s, pts)).collect(),
                }
            }
            _ => vec![],
        };
        ops.push(Op::Remove(vec![(i, it.clone())]));
        let mut adds = vec![];
        for (k, (s, pts)) in pieces.into_iter().enumerate() {
            st.seq += 1;
            adds.push((
                i + k,
                Item {
                    id: new_id(st.seq),
                    content: Content::Stroke(Stroke {
                        points: pts,
                        interp: None,
                        frame: IDENTITY,
                        ..s.clone()
                    }),
                },
            ));
        }
        if !adds.is_empty() {
            ops.push(Op::Add(adds));
        }
    }
    Op::Batch(ops)
}

/// Add pasted or duplicated copies; copies of grouped items form new groups (one per
/// original group), in the same undo step.
pub(crate) fn commit_copies(st: &mut State, op: Op, renamed: &[(String, String)], groups: &BTreeMap<String, String>) {
    let mut fresh: BTreeMap<&str, String> = BTreeMap::new();
    let mut pairs: Vec<(String, String)> = vec![];
    for (old, new) in renamed {
        if let Some(g) = groups.get(old) {
            let gid = match fresh.get(g.as_str()) {
                Some(n) => n.clone(),
                None => {
                    st.seq += 1;
                    let n = format!("g{}", new_id(st.seq));
                    fresh.insert(g, n.clone());
                    n
                }
            };
            pairs.push((new.clone(), gid));
        }
    }
    let refs: Vec<(&str, &str)> = pairs.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    st.doc.apply_grouped(&op, &refs);
    st.persist();
}

#[uniffi::export]
impl PageDoc {
    // ----- ink with tilt (P1-10) -----

    /// Start a stroke with the pen's tilt (radians from perpendicular, `AXIS_TILT`).
    #[allow(clippy::too_many_arguments)] // flat FFI signature
    pub fn begin_stroke_tilt(&self, tool: InkTool, color: u32, width: f64, x: f64, y: f64, pressure: f64, tilt: f64) {
        let mut st = self.state.lock().unwrap();
        st.current = Some(Current {
            tool,
            color,
            width,
            points: vec![Point::with_tilt(x, y, pressure, tilt)],
            snapped: None,
        });
    }

    /// Like `add_points` with tilt: `xypt` = x0, y0, p0, t0, x1, …; one call per input event.
    /// Returns the accepted points as x, y, smoothed pressure, smoothed tilt.
    pub fn add_points_tilt(&self, xypt: Vec<f64>) -> Vec<f32> {
        let mut st = self.state.lock().unwrap();
        let Some(cur) = st.current.as_mut() else { return vec![] };
        let mut accepted = Vec::with_capacity(xypt.len());
        for c in xypt.chunks_exact(4) {
            let last = *cur.points.last().unwrap();
            if (c[0] - last.x).hypot(c[1] - last.y) < 0.2 {
                continue;
            }
            let p = Point::with_tilt(c[0], c[1], last.p * 0.6 + c[2] * 0.4, last.t * 0.6 + c[3] * 0.4);
            cur.points.push(p);
            accepted.extend([p.x as f32, p.y as f32, p.p as f32, p.t as f32]);
        }
        accepted
    }

    // ----- partial eraser (P1-11) -----

    /// Start a partial eraser gesture: only the parts of strokes under the eraser go; the
    /// original strokes are kept in the trash, never modified (ADR-0003). Feed it with
    /// `erase_segment` and finish with `end_erase`, like the stroke eraser.
    pub fn begin_partial_erase(&self) {
        let mut st = self.state.lock().unwrap();
        st.erasing = Some(HashSet::new());
        st.partial = Some((vec![], 0.0));
    }

    /// During a partial eraser gesture: what is left of the strokes being erased (their own
    /// keys are in `erasing_keys`, hidden by the UI meanwhile).
    pub fn erase_preview(&self) -> Vec<RenderItem> {
        let st = self.state.lock().unwrap();
        let (Some(set), Some((path, r))) = (&st.erasing, &st.partial) else {
            return vec![];
        };
        let mut out = vec![];
        for it in st.doc.page().items.iter().filter(|it| set.contains(&it.id)) {
            let Content::Stroke(s) = &it.content else { continue };
            if matches!(s.interp, Some(Interp { show: true, .. })) {
                continue;
            }
            match erase_pieces(&s.points, path, r + s.width / 2.0, 0.5) {
                None => out.push(render_stroke(item_key(it), s)),
                Some(pieces) => {
                    for (k, pts) in pieces.into_iter().enumerate() {
                        let piece = Stroke {
                            points: pts,
                            ..s.clone()
                        };
                        out.push(render_stroke(format!("{}~{k}", it.id), &piece));
                    }
                }
            }
        }
        out
    }

    // ----- selection (P1-14, P1-15, P1-16) -----

    /// Items selected by the lasso `xy` (x0, y0, x1, y1, …, a closed path), with the rest of
    /// their groups, in drawing order.
    pub fn select_lasso(&self, xy: Vec<f64>) -> Vec<String> {
        let pts: Vec<(f64, f64)> = xy.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        let st = self.state.lock().unwrap();
        let page = st.doc.page();
        page.with_groups(&nb_model::select::lasso(&page.items, pts))
    }

    /// Scale by `scale` and rotate by `angle` (radians, clockwise) about (cx, cy), then move
    /// by (dx, dy), as one undoable step. Ink keeps its stored points (ADR-0003); blocks stay
    /// upright and scale with their text.
    #[allow(clippy::too_many_arguments)] // flat FFI signature
    pub fn transform_items(
        &self,
        ids: Vec<String>,
        cx: f64,
        cy: f64,
        scale: f64,
        angle: f64,
        dx: f64,
        dy: f64,
    ) -> bool {
        if !(scale.is_finite() && scale > 0.0 && angle.is_finite() && dx.is_finite() && dy.is_finite()) {
            return false;
        }
        let t = Similarity {
            cx,
            cy,
            scale: scale.clamp(0.02, 50.0),
            angle,
            dx,
            dy,
        };
        let mut st = self.state.lock().unwrap();
        let after: Vec<Item> = st
            .doc
            .page()
            .items
            .iter()
            .filter(|it| ids.contains(&it.id))
            .map(|it| it.transformed(&t))
            .collect();
        let op = Op::update(st.doc.page(), after);
        st.commit(op)
    }

    /// Group the items (one undo step); they are then selected together.
    pub fn group_items(&self, ids: Vec<String>) -> bool {
        if ids.len() < 2 {
            return false;
        }
        let mut st = self.state.lock().unwrap();
        st.seq += 1;
        let gid = format!("g{}", new_id(st.seq));
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let changed = st.doc.set_group(&refs, Some(&gid));
        st.persist();
        changed
    }

    /// Take the items out of their groups (one undo step).
    pub fn ungroup_items(&self, ids: Vec<String>) -> bool {
        let mut st = self.state.lock().unwrap();
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let changed = st.doc.set_group(&refs, None);
        st.persist();
        changed
    }

    /// Whether any of the items is in a group.
    pub fn grouped(&self, ids: Vec<String>) -> bool {
        let st = self.state.lock().unwrap();
        ids.iter().any(|id| st.doc.page().groups.contains_key(id))
    }

    /// Plain text of the selected text blocks, top to bottom (for the system clipboard).
    pub fn selection_text(&self, ids: Vec<String>) -> String {
        let st = self.state.lock().unwrap();
        let mut blocks: Vec<(f64, f64, &str)> = st
            .doc
            .page()
            .items
            .iter()
            .filter(|it| ids.contains(&it.id))
            .filter_map(|it| match &it.content {
                Content::Text { rect, text, .. } => Some((rect.y, rect.x, text.as_str())),
                _ => None,
            })
            .collect();
        blocks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        blocks.iter().map(|b| b.2).collect::<Vec<_>>().join("\n\n")
    }

    // ----- infinite canvas (P1-06) -----

    pub fn is_infinite(&self) -> bool {
        self.state.lock().unwrap().doc.size() == nb_model::INFINITE
    }

    /// Area to show for "zoom to fit" and to export, as [x, y, w, h] in page units: the
    /// paper, or the content of an infinite canvas with a margin.
    pub fn page_extent(&self) -> Vec<f32> {
        let st = self.state.lock().unwrap();
        let r = st.doc.page().extent();
        vec![r.x as f32, r.y as f32, r.w as f32, r.h as f32]
    }

    /// Bounding box of everything on the page as [x, y, w, h], or empty for an empty page.
    pub fn content_bounds(&self) -> Vec<f32> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .content_bounds()
            .map(|r| vec![r.x as f32, r.y as f32, r.w as f32, r.h as f32])
            .unwrap_or_default()
    }

    // ----- rich text (P1-17) -----

    /// Add a rich text block (offsets of `spans` in UTF-16 units); returns its id.
    #[allow(clippy::too_many_arguments)] // flat FFI signature
    pub fn add_rich_text(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        text: String,
        spans: Vec<RichSpan>,
        size: f64,
        color: u32,
    ) -> String {
        let spans = spans_from_utf16(&text, &spans);
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
                spans,
            },
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op);
        id
    }

    /// Replace a text block's text and formatting (and measured height), merged as a
    /// character diff into the block's CRDT text. Empty text deletes the block.
    pub fn set_rich_text(&self, id: String, text: String, spans: Vec<RichSpan>, height: f64) -> bool {
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
        let spans = spans_from_utf16(&text, &spans);
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

    // ----- attachments (P1-19) and images (P1-18) -----

    pub fn attachment_blocks(&self) -> Vec<AttachmentBlock> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .filter_map(|it| match &it.content {
                Content::Attachment {
                    rect,
                    asset,
                    name,
                    mime,
                } => Some(AttachmentBlock {
                    key: item_key(it),
                    id: it.id.clone(),
                    x: rect.x as f32,
                    y: rect.y as f32,
                    width: rect.w as f32,
                    height: rect.h as f32,
                    name: name.clone(),
                    mime: mime.clone(),
                    hash: asset.clone(),
                }),
                _ => None,
            })
            .collect()
    }

    /// Attach a file (stored by content hash, synced and exported with the page) in a box at
    /// (x, y). One undo step. Returns its id.
    #[allow(clippy::too_many_arguments)] // flat FFI signature
    pub fn add_attachment(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        bytes: Vec<u8>,
        name: String,
        mime: String,
    ) -> String {
        let hash = sha256_hex(&bytes);
        let mut st = self.state.lock().unwrap();
        st.seq += 1;
        let id = new_id(st.seq);
        st.doc.put_asset(&hash, &bytes);
        let item = Item {
            id: id.clone(),
            content: Content::Attachment {
                rect: nb_model::Rect {
                    x,
                    y,
                    w: width,
                    h: height,
                },
                asset: hash,
                name,
                mime,
            },
        };
        let op = Op::add(st.doc.page(), vec![item]);
        st.commit(op);
        id
    }

    /// Topmost attachment containing (x, y), if any.
    pub fn attachment_at(&self, x: f64, y: f64) -> Option<String> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .rev()
            .find(|it| matches!(it.content, Content::Attachment { .. }) && it.bounds().contains(x, y))
            .map(|it| it.id.clone())
    }

    /// Replace an image's picture (after crop or rotation by the UI) and its box size,
    /// keeping its top-left corner. One undo step; the previous picture stays stored, so
    /// undo brings it back.
    pub fn replace_image(&self, id: String, bytes: Vec<u8>, width: f64, height: f64) -> bool {
        if bytes.is_empty() || !(width > 0.0 && height > 0.0) {
            return false;
        }
        let hash = sha256_hex(&bytes);
        let mut st = self.state.lock().unwrap();
        let Some(old) = st.doc.page().items.iter().find(|i| i.id == id).cloned() else {
            return false;
        };
        let Content::Image { rect, .. } = &old.content else {
            return false;
        };
        let item = Item {
            id,
            content: Content::Image {
                rect: nb_model::Rect {
                    w: width,
                    h: height,
                    ..rect.clone()
                },
                asset: hash.clone(),
            },
        };
        st.doc.put_asset(&hash, &bytes);
        let op = Op::update(st.doc.page(), vec![item]);
        st.commit(op)
    }

    /// Topmost image containing (x, y), if any.
    pub fn image_at(&self, x: f64, y: f64) -> Option<String> {
        let st = self.state.lock().unwrap();
        st.doc
            .page()
            .items
            .iter()
            .rev()
            .find(|it| matches!(it.content, Content::Image { .. }) && it.bounds().contains(x, y))
            .map(|it| it.id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageDoc;

    fn draw(doc: &PageDoc, tool: InkTool, pts: &[(f64, f64)]) {
        doc.begin_stroke_tilt(tool, 0x1d1d1f, 3.0, pts[0].0, pts[0].1, 0.5, 0.8);
        let mut xypt = vec![];
        for &(x, y) in &pts[1..] {
            xypt.extend([x, y, 0.6, 0.8]);
        }
        doc.add_points_tilt(xypt);
        doc.end_stroke(1.0, false);
    }

    fn line_pts(y: f64) -> Vec<(f64, f64)> {
        (0..=50).map(|i| (i as f64 * 4.0, y)).collect()
    }

    #[test]
    fn pencil_and_brush_with_tilt() {
        let doc = PageDoc::new_in_memory();
        draw(&doc, InkTool::Pencil, &line_pts(0.0));
        draw(&doc, InkTool::Brush, &line_pts(50.0));
        let keys = doc.layout();
        let g = doc.geometry(keys[0].clone()).unwrap();
        assert!(g.alpha < 1.0, "pencil is drawn slightly transparent");
        // Tilted pencil is wider than an upright one.
        let width = |g: &RenderItem| {
            let ys: Vec<f32> = g.coords.chunks(2).map(|c| c[1]).collect();
            ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min)
        };
        assert!(width(&g) > 3.0, "{}", width(&g));
        assert!(ink_radius(InkTool::Pencil, 3.0, 0.5, 0.8) > ink_radius(InkTool::Pencil, 3.0, 0.5, 0.0));
    }

    #[test]
    fn partial_erase_splits_a_stroke_in_one_undo_step() {
        let doc = PageDoc::new_in_memory();
        draw(&doc, InkTool::Pen, &line_pts(0.0));
        draw(&doc, InkTool::Pen, &line_pts(100.0));
        doc.add_text(0.0, 200.0, 50.0, 20.0, "t".into(), 12.0, 0);
        assert_eq!(doc.item_count(), 3);
        doc.begin_partial_erase();
        assert!(doc.erase_segment(100.0, -20.0, 100.0, 20.0, 4.0));
        assert_eq!(doc.erasing_keys().len(), 1);
        let preview = doc.erase_preview();
        assert_eq!(preview.len(), 2, "two pieces shown while erasing");
        assert!(doc.end_erase());
        assert_eq!(doc.item_count(), 4, "the cut stroke became two");
        let st = doc.state.lock().unwrap();
        assert_eq!(st.doc.trash().len(), 1, "the original is kept in the trash");
        drop(st);
        assert!(doc.undo());
        assert_eq!(doc.item_count(), 3);
        // The stroke eraser still deletes whole strokes.
        doc.begin_erase();
        doc.erase_segment(100.0, 90.0, 100.0, 110.0, 4.0);
        assert!(doc.end_erase());
        assert_eq!(doc.item_count(), 2);
    }

    #[test]
    fn lasso_group_transform_copy() {
        let doc = PageDoc::new_in_memory();
        draw(&doc, InkTool::Pen, &line_pts(0.0));
        draw(&doc, InkTool::Pen, &line_pts(300.0));
        let t = doc.add_text(10.0, 20.0, 100.0, 20.0, "hello".into(), 12.0, 0);
        let lasso = [-10.0, -10.0, 220.0, -10.0, 220.0, 60.0, -10.0, 60.0];
        let sel = doc.select_lasso(lasso.to_vec());
        assert_eq!(sel.len(), 2, "first stroke and the text");
        assert!(sel.contains(&t));
        assert!(doc.group_items(sel.clone()));
        assert!(doc.grouped(vec![t.clone()]));
        // Selecting one member selects the group.
        assert_eq!(doc.select_rect(0.0, 15.0, 30.0, 30.0).len(), 2);
        assert_eq!(doc.selection_text(sel.clone()), "hello");
        assert!(doc.transform_items(sel.clone(), 100.0, 0.0, 2.0, std::f64::consts::FRAC_PI_2, 0.0, 0.0));
        let b = doc.bounds_of(vec![sel[0].clone()]);
        assert!(b[3] > 300.0, "rotated a quarter turn and doubled: now tall {b:?}");
        assert!(doc.undo());
        assert!(doc.bounds_of(vec![sel[0].clone()])[2] > 190.0, "one undo step");
        assert!(
            !doc.transform_items(sel.clone(), 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
            "zero scale refused"
        );
        // Duplicates of a group form a new group.
        let copies = doc.duplicate_items(sel.clone(), 10.0, 10.0);
        assert_eq!(copies.len(), 2);
        assert!(doc.grouped(copies.clone()));
        let rect_sel = doc.select_rect(25.0, 35.0, 26.0, 36.0);
        assert_eq!(rect_sel.len(), 4, "both groups touched: {rect_sel:?}");
        assert!(doc.ungroup_items(sel.clone()));
        assert!(!doc.grouped(sel));
    }

    #[test]
    fn rich_text_spans_cross_the_ffi_in_utf16() {
        let doc = PageDoc::new_in_memory();
        // "é" is one UTF-16 unit, "𝑥" two.
        let text = "é𝑥 bold".to_string();
        let span = RichSpan {
            start: 4,
            end: 8,
            bold: true,
            italic: false,
            underline: false,
            color: Some(0xff0000),
            heading: 0,
            list: 0,
        };
        let id = doc.add_rich_text(0.0, 0.0, 100.0, 20.0, text.clone(), vec![span.clone()], 14.0, 0);
        let b = &doc.text_blocks()[0];
        assert_eq!(b.spans, vec![span.clone()]);
        let st = doc.state.lock().unwrap();
        let Content::Text { spans, .. } = &st.doc.page().items[0].content else {
            unreachable!()
        };
        assert_eq!((spans[0].start, spans[0].end), (3, 7), "chars in the model");
        drop(st);
        let heading = RichSpan {
            start: 0,
            end: 8,
            bold: false,
            color: None,
            heading: 1,
            ..span
        };
        assert!(doc.set_rich_text(id.clone(), text.clone() + "!", vec![heading], 30.0));
        assert_eq!(doc.text_blocks()[0].spans[0].heading, 1);
        assert!(doc.undo());
        assert!(doc.text_blocks()[0].spans[0].bold);
        // Plain set_text keeps formatting that still fits.
        assert!(doc.set_text(id, "é𝑥 bo".into(), 20.0));
        assert_eq!(doc.text_blocks()[0].spans[0].end, 6);
    }

    #[test]
    fn attachments_and_image_replacement() {
        let doc = PageDoc::new_in_memory();
        let a = doc.add_attachment(
            10.0,
            10.0,
            180.0,
            48.0,
            b"%PDF-1.7".to_vec(),
            "cours.pdf".into(),
            "application/pdf".into(),
        );
        let blocks = doc.attachment_blocks();
        assert_eq!(
            (blocks[0].name.as_str(), blocks[0].mime.as_str()),
            ("cours.pdf", "application/pdf")
        );
        assert_eq!(doc.image_bytes(blocks[0].hash.clone()).unwrap(), b"%PDF-1.7");
        assert_eq!(doc.attachment_at(20.0, 20.0), Some(a.clone()));
        // Copy/paste carries the file.
        assert_eq!(doc.copy_items(vec![a]), 1);
        let other = PageDoc::new_in_memory();
        other.paste_items(0.0, 0.0);
        assert_eq!(other.image_bytes(blocks[0].hash.clone()).unwrap(), b"%PDF-1.7");

        let img = doc.add_image(0.0, 100.0, 200.0, 100.0, vec![1, 2, 3]);
        assert_eq!(doc.image_at(10.0, 110.0), Some(img.clone()));
        assert!(doc.replace_image(img.clone(), vec![4, 5], 100.0, 200.0));
        let ib = doc.image_blocks();
        assert_eq!((ib[0].width, ib[0].height, ib[0].y), (100.0, 200.0, 100.0));
        assert_eq!(doc.image_bytes(ib[0].hash.clone()).unwrap(), vec![4, 5]);
        assert!(doc.undo());
        assert_eq!(
            doc.image_bytes(doc.image_blocks()[0].hash.clone()).unwrap(),
            vec![1, 2, 3]
        );
    }

    #[test]
    fn infinite_canvas_extent() {
        let doc = PageDoc::new_in_memory();
        doc.set_page_size(nb_model::INFINITE.into());
        assert!(doc.is_infinite());
        draw(&doc, InkTool::Pen, &[(-2000.0, -500.0), (5000.0, 900.0)]);
        let e = doc.page_extent();
        assert!(e[0] < -2000.0 && e[0] + e[2] > 5000.0, "{e:?}");
        assert_eq!(doc.content_bounds().len(), 4);
    }
}
