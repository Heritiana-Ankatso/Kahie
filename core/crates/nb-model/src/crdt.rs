//! A page stored in a Loro document (ADR-0001, decision D6; measurements in
//! `docs/spikes/p0-03-loro.md`).
//!
//! Layout of the Loro document:
//!
//! | Root container | Type | Content |
//! |---|---|---|
//! | `meta` | map | `id`, `template`, `size` (absent: A4), `background`, `pdf_asset` + `pdf_page` + `pdf_text` (PDF page, P1-21/23) |
//! | `order` | movable list | object ids, bottom to top (z-order) |
//! | `objects` | map | object id → map of fields (below) |
//! | `groups` | map | object id → group id (P1-16; `""` = not grouped) |
//! | `assets` | map | SHA-256 (hex) → bytes of images and attachments |
//!
//! Object fields: `t` (`"stroke"` or `"block"`), `deleted` (bool, trash tombstone).
//! Strokes: `tool`, `color`, `width`, `ink` (packed points, [`crate::inkcodec`],
//! **immutable once written** — ADR-0003), `xf` (optional list `[a, b, c, d]`: linear part
//! of the transform applied to the ink, P1-15; absent = identity), `dx`/`dy` (translation
//! applied after it), `interp` (JSON of [`Interp`] or null). Other objects: `json` (JSON of
//! [`Content`]). Text blocks also have `rt`, a Loro rich text (P1-17) holding the text and
//! its marks (`bold`, `italic`, `underline`, `color`, `h`, `list`); it wins over the text
//! in `json`, which is kept for older readers.
//!
//! The plain [`Page`] returned by [`CrdtPage::page`] is a materialised view rebuilt
//! after every change; decoded ink is cached per object id because ink never changes.

use crate::{
    inkcodec, normalize_spans, Content, Interp, Item, Linear, Op, Page, PdfSource, Stroke, TextSpan, Tool, IDENTITY,
};
use loro::event::{Diff, DiffEvent};
use loro::{
    Container, ContainerTrait, ExpandType, ExportMode, Frontiers, Index, LoroDoc, LoroMap, LoroText, LoroValue,
    StyleConfig, StyleConfigMap, Subscription, TextDelta, UndoManager, UpdateOptions, ValueOrContainer, VersionVector,
};
use nb_ink::Point;
use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

const META: &str = "meta";
const ORDER: &str = "order";
const OBJECTS: &str = "objects";
/// Image bytes by SHA-256 (hex). Stored in the CRDT so images sync and export with the page.
const ASSETS: &str = "assets";
const GROUPS: &str = "groups";
/// Rich text mark keys (P1-17).
const MARKS: [&str; 6] = ["bold", "italic", "underline", "color", "h", "list"];

/// Errors from loading or merging CRDT data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelError(pub String);

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ModelError {}

fn err(e: impl std::fmt::Display) -> ModelError {
    ModelError(e.to_string())
}

/// One page as a CRDT document, with per-user undo/redo.
pub struct CrdtPage {
    doc: LoroDoc,
    undo: UndoManager,
    view: Page,
    ink_cache: HashMap<String, Arc<Vec<Point>>>,
    /// Version already handed out by [`CrdtPage::take_unsaved`].
    saved: VersionVector,
    /// Decoded objects (`deleted` flag, item), kept up to date from Loro change events so a
    /// rebuild only re-reads objects that changed.
    objects: HashMap<String, (bool, Option<Item>)>,
    /// Object ids changed since the last rebuild, filled by the event subscription.
    dirty: Arc<Mutex<HashSet<String>>>,
    _events: Subscription,
}

impl CrdtPage {
    /// New empty page. `peer` must be unique per device session (use a random u64).
    pub fn new(id: &str, peer: u64) -> Self {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).expect("valid peer id");
        let meta = doc.get_map(META);
        meta.insert("id", id).expect("insert");
        meta.insert("template", "lined").expect("insert");
        doc.commit();
        Self::wrap(doc)
    }

    /// Import a page from the pre-CRDT JSON model (migration path).
    pub fn from_page(page: &Page, peer: u64) -> Self {
        let mut p = Self::new(&page.id, peer);
        p.doc
            .get_map(META)
            .insert("template", page.template.as_str())
            .expect("insert");
        if page.size != "a4" {
            p.doc.get_map(META).insert("size", page.size.as_str()).expect("insert");
        }
        if let Some(pdf) = &page.pdf {
            p.write_pdf_source(Some(pdf), None);
        }
        for (i, item) in page.items.iter().enumerate() {
            p.write_new(i, item);
        }
        p.doc.commit();
        p.undo = Self::undo_manager(&p.doc); // migration is not undoable
        p.rebuild();
        p
    }

    /// Load from bytes produced by [`CrdtPage::snapshot`].
    pub fn from_snapshot(bytes: &[u8], peer: u64) -> Result<Self, ModelError> {
        Self::load(&[bytes], peer)
    }

    /// Load from a snapshot and/or updates, applied in order (e.g. from `nb-store`).
    /// Everything loaded counts as saved.
    pub fn load(chunks: &[&[u8]], peer: u64) -> Result<Self, ModelError> {
        let doc = LoroDoc::new();
        for c in chunks {
            doc.import(c).map_err(err)?;
        }
        doc.set_peer_id(peer).map_err(err)?;
        if doc.get_map(META).get("id").is_none() {
            return Err(ModelError("not a Kahie page".into()));
        }
        let mut p = Self::wrap(doc);
        p.saved = p.doc.oplog_vv();
        Ok(p)
    }

    /// Changes not yet handed out for persistence (local edits and merged remote ones),
    /// or `None` when there are none. Each change is returned once.
    pub fn take_unsaved(&mut self) -> Option<Vec<u8>> {
        let now = self.doc.oplog_vv();
        if now == self.saved {
            return None;
        }
        let bytes = self
            .doc
            .export(ExportMode::updates(&self.saved))
            .expect("export updates");
        self.saved = now;
        Some(bytes)
    }

    fn undo_manager(doc: &LoroDoc) -> UndoManager {
        let mut u = UndoManager::new(doc);
        u.set_max_undo_steps(500);
        u.set_merge_interval(0);
        u
    }

    fn wrap(doc: LoroDoc) -> Self {
        let mut styles = StyleConfigMap::new();
        for k in MARKS {
            styles.insert(k.into(), StyleConfig::new().expand(ExpandType::After));
        }
        doc.config_text_style(styles);
        let undo = Self::undo_manager(&doc);
        let dirty: Arc<Mutex<HashSet<String>>> = Arc::default();
        let objects_id = doc.get_map(OBJECTS).id();
        let d = dirty.clone();
        let events = doc.subscribe_root(Arc::new(move |e: DiffEvent| {
            let mut set = d.lock().unwrap();
            for c in &e.events {
                // A change inside one object's map. Each path entry is (container, its key in
                // its parent), so the object id is on the entry right after `objects`.
                for w in c.path.windows(2) {
                    if w[0].0 == objects_id {
                        if let Index::Key(k) = &w[1].1 {
                            set.insert(k.to_string());
                        }
                    }
                }
                // Keys added to or removed from `objects` itself.
                if *c.target == objects_id {
                    if let Diff::Map(m) = &c.diff {
                        set.extend(m.updated.keys().map(|k| k.to_string()));
                    }
                }
            }
        }));
        let mut p = Self {
            doc,
            undo,
            view: Page::new(""),
            ink_cache: HashMap::new(),
            saved: VersionVector::default(),
            objects: HashMap::new(),
            dirty,
            _events: events,
        };
        p.rebuild();
        p
    }

    // ---------- persistence and sync ----------

    /// Full snapshot (state + history).
    pub fn snapshot(&self) -> Vec<u8> {
        self.doc.export(ExportMode::Snapshot).expect("export snapshot")
    }

    /// Current version, to ask a peer for what we are missing.
    pub fn version(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    /// Changes the holder of `since` does not have yet.
    pub fn updates_since(&self, since: &VersionVector) -> Vec<u8> {
        self.doc.export(ExportMode::updates(since)).expect("export updates")
    }

    /// Sync upload: the changes the relay does not have, given `relay` (the version known to
    /// be on the relay, persisted by the caller). Advances `relay`. Unlike
    /// [`CrdtPage::take_unsaved`], changes received from the relay are never sent back, while
    /// local changes from earlier sessions (other peer ids) are.
    pub fn sync_outgoing(&self, relay: &mut VersionVector) -> Option<Vec<u8>> {
        sync_outgoing(&self.doc, relay)
    }

    /// Sync download: import an update received from the relay and record it in `relay`.
    pub fn sync_incoming(&mut self, bytes: &[u8], relay: &mut VersionVector) -> Result<(), ModelError> {
        sync_incoming(&self.doc, bytes, relay)?;
        self.rebuild();
        Ok(())
    }

    /// Merge updates or a snapshot from another replica.
    pub fn import(&mut self, bytes: &[u8]) -> Result<(), ModelError> {
        self.doc.import(bytes).map_err(err)?;
        self.rebuild();
        Ok(())
    }

    // ---------- assets (images) ----------

    /// Store bytes under their content hash (written once; identical images share it).
    /// Not committed on its own: it is part of the next change (normally the item that uses
    /// it), so adding an image is a single undo step.
    pub fn put_asset(&mut self, hash: &str, bytes: &[u8]) {
        let assets = self.doc.get_map(ASSETS);
        if assets.get(hash).is_none() {
            assets
                .insert(hash, LoroValue::Binary(bytes.to_vec().into()))
                .expect("insert asset");
        }
    }

    /// [`CrdtPage::put_asset`] as a change of its own (a notebook's asset store, P1-21).
    pub fn store_asset(&mut self, hash: &str, bytes: &[u8]) {
        self.put_asset(hash, bytes);
        self.doc.commit();
    }

    pub fn asset(&self, hash: &str) -> Option<Vec<u8>> {
        match self.doc.get_map(ASSETS).get(hash) {
            Some(ValueOrContainer::Value(LoroValue::Binary(b))) => Some(b.to_vec()),
            _ => None,
        }
    }

    /// Page background (e.g. an imported PDF page): an asset hash drawn under everything,
    /// not an item, so it cannot be erased or moved by accident.
    pub fn background(&self) -> Option<String> {
        match self.doc.get_map(META).get("background") {
            Some(ValueOrContainer::Value(LoroValue::String(s))) if !s.is_empty() => Some(s.to_string()),
            _ => None,
        }
    }

    /// Set (with its bytes) or clear the page background, as one change.
    pub fn set_background(&mut self, asset: Option<(&str, &[u8])>) {
        match asset {
            Some((hash, bytes)) => {
                self.put_asset(hash, bytes);
                self.doc.get_map(META).insert("background", hash).expect("insert");
            }
            None => {
                self.doc.get_map(META).insert("background", "").expect("insert");
            }
        }
        self.doc.commit();
    }

    /// PDF page shown under this page (P1-21), if any. The PDF itself is a notebook asset.
    pub fn pdf_source(&self) -> Option<PdfSource> {
        pdf_field(&self.doc.get_map(META).get_value())
    }

    /// Text layer of the PDF page (P1-23): JSON written by the app's PDF import (words with
    /// boxes normalised to the page), empty when the PDF page has no text.
    pub fn pdf_text_layer(&self) -> String {
        str_field(&self.doc.get_map(META).get_value(), "pdf_text").unwrap_or_default()
    }

    /// Show a PDF page under this page (or none), with its text layer, as one change.
    pub fn set_pdf_source(&mut self, src: Option<&PdfSource>, text_layer: Option<&str>) {
        self.write_pdf_source(src, text_layer);
        self.doc.commit();
        self.rebuild();
    }

    fn write_pdf_source(&mut self, src: Option<&PdfSource>, text_layer: Option<&str>) {
        let meta = self.doc.get_map(META);
        match src {
            Some(s) => {
                meta.insert("pdf_asset", s.asset.as_str()).expect("insert");
                meta.insert("pdf_page", i64::from(s.page_index)).expect("insert");
            }
            None => {
                meta.insert("pdf_asset", "").expect("insert");
            }
        }
        meta.insert("pdf_text", text_layer.unwrap_or("")).expect("insert");
    }

    /// Hashes of all stored assets (for export).
    pub fn asset_hashes(&self) -> Vec<String> {
        match self.doc.get_map(ASSETS).get_value() {
            LoroValue::Map(m) => {
                let mut v: Vec<String> = m.keys().cloned().collect();
                v.sort();
                v
            }
            _ => vec![],
        }
    }

    // ---------- reading ----------

    /// Materialised page (visible objects in z-order).
    pub fn page(&self) -> &Page {
        &self.view
    }

    /// Deleted objects that can still be restored.
    pub fn trash(&self) -> Vec<Item> {
        let LoroValue::Map(objs) = self.doc.get_map(OBJECTS).get_deep_value() else {
            return vec![];
        };
        let mut out: Vec<Item> = objs
            .iter()
            .filter(|(_, v)| matches!(field(v, "deleted"), Some(LoroValue::Bool(true))))
            .filter_map(|(id, v)| self.decode_item(id, v))
            .collect();
        out.sort_by(|a, b| a.id.cmp(&b.id));
        out
    }

    // ---------- commands ----------

    /// Apply a command built against [`CrdtPage::page`]. Returns false if it changed nothing.
    pub fn apply(&mut self, op: &Op) -> bool {
        if op.is_empty() {
            return false;
        }
        // Adds and removes update the view in place (the common, latency-critical case:
        // pen-up and eraser). Anything else, or an order list that does not mirror the
        // view one-to-one (possible after concurrent restores), falls back to a rebuild.
        let incremental =
            matches!(op, Op::Add(_) | Op::Remove(_)) && self.doc.get_movable_list(ORDER).len() == self.view.items.len();
        self.write_op(op);
        self.doc.commit();
        if incremental {
            self.patch_view(op);
            #[cfg(debug_assertions)]
            {
                let patched = self.view.clone();
                self.rebuild();
                debug_assert_eq!(patched, self.view, "incremental view diverged from rebuild");
            }
        } else {
            self.rebuild();
        }
        true
    }

    fn write_op(&mut self, op: &Op) {
        match op {
            Op::Add(entries) => {
                let mut sorted = entries.clone();
                sorted.sort_by_key(|(i, _)| *i);
                for (i, item) in &sorted {
                    self.write_new(*i, item);
                }
            }
            Op::Remove(entries) => {
                let ids: Vec<&str> = entries.iter().map(|(_, it)| it.id.as_str()).collect();
                self.remove_ids(&ids);
            }
            Op::Update { before, after } => {
                for (old, new) in before.iter().zip(after) {
                    self.write_update(old, new);
                }
            }
            Op::Batch(ops) => ops.iter().for_each(|o| self.write_op(o)),
        }
    }

    fn patch_view(&mut self, op: &Op) {
        match op {
            Op::Add(entries) => {
                let mut sorted = entries.clone();
                sorted.sort_by_key(|(i, _)| *i);
                for (i, mut item) in sorted {
                    match &mut item.content {
                        Content::Stroke(s) => {
                            // Same values a rebuild would decode from the stored ink.
                            s.points = inkcodec::quantize(&s.points);
                            s.frame = IDENTITY;
                            self.ink_cache.insert(item.id.clone(), Arc::new(s.points.clone()));
                        }
                        Content::Text { text, spans, .. } => {
                            *spans = normalize_spans(spans, text.chars().count() as u32);
                        }
                        _ => {}
                    }
                    let at = i.min(self.view.items.len());
                    self.view.items.insert(at, item);
                }
            }
            Op::Remove(entries) => {
                self.view.items.retain(|it| !entries.iter().any(|(_, e)| e.id == it.id));
            }
            Op::Update { .. } | Op::Batch(_) => return self.rebuild(),
        }
        if !self.view.groups.is_empty() || !self.doc.get_map(GROUPS).is_empty() {
            self.view.groups = self.groups(&self.view.items);
        }
    }

    // ---------- groups (P1-16) ----------

    /// Put `ids` in one new group `group` (or take them out of any group with `None`), as
    /// one undo step. Returns false if nothing changed.
    pub fn set_group(&mut self, ids: &[&str], group: Option<&str>) -> bool {
        let want = group.unwrap_or("");
        let changed = self.write_groups(ids.iter().map(|id| (*id, want)));
        if changed {
            self.doc.commit();
            self.rebuild();
        }
        changed
    }

    /// Apply `op` and assign groups (item id, group id) in the same undo step, e.g. pasting
    /// a copied group as a new group.
    pub fn apply_grouped(&mut self, op: &Op, groups: &[(&str, &str)]) -> bool {
        if op.is_empty() {
            return false;
        }
        self.write_op(op);
        self.write_groups(groups.iter().copied());
        self.doc.commit();
        self.rebuild();
        true
    }

    fn write_groups<'a>(&mut self, pairs: impl Iterator<Item = (&'a str, &'a str)>) -> bool {
        let groups = self.doc.get_map(GROUPS);
        let mut changed = false;
        for (id, want) in pairs {
            let cur = match groups.get(id) {
                Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
                _ => String::new(),
            };
            if cur != want {
                groups.insert(id, want).expect("insert");
                changed = true;
            }
        }
        changed
    }

    /// Restore objects from the trash to the top of the page.
    pub fn restore(&mut self, ids: &[&str]) -> bool {
        let objects = self.doc.get_map(OBJECTS);
        let order = self.doc.get_movable_list(ORDER);
        let mut changed = false;
        for id in ids {
            if let Some(m) = child_map(&objects, id) {
                if matches!(m.get("deleted"), Some(ValueOrContainer::Value(LoroValue::Bool(true)))) {
                    m.insert("deleted", false).expect("insert");
                    order.push(*id).expect("push");
                    changed = true;
                }
            }
        }
        if changed {
            self.doc.commit();
            self.rebuild();
        }
        changed
    }

    // ---------- history (P1-28) ----------

    /// The current version, encoded, to keep as a local checkpoint.
    pub fn frontiers(&self) -> Vec<u8> {
        self.doc.oplog_frontiers().encode()
    }

    /// The page as it was at an encoded version from [`CrdtPage::frontiers`].
    pub fn page_at(&self, frontiers: &[u8]) -> Result<Page, ModelError> {
        let f = Frontiers::decode(frontiers).map_err(err)?;
        let fork = self.doc.fork_at(&f).map_err(err)?;
        Ok(Self::wrap(fork).view)
    }

    /// Make the page's objects look as they did at `frontiers` again. This is a new change
    /// (one undo step, synced like any edit), not a rewind of history. Returns false if the
    /// page already looks like that.
    pub fn revert_to(&mut self, frontiers: &[u8]) -> Result<bool, ModelError> {
        let old = self.page_at(frontiers)?;
        if old.items == self.view.items {
            return Ok(false);
        }
        let current: HashMap<String, Item> = self.view.items.iter().map(|it| (it.id.clone(), it.clone())).collect();
        let wanted: HashSet<&str> = old.items.iter().map(|it| it.id.as_str()).collect();
        let gone: Vec<&str> = current
            .keys()
            .map(String::as_str)
            .filter(|id| !wanted.contains(id))
            .collect();
        self.remove_ids(&gone);
        let objects = self.doc.get_map(OBJECTS);
        let order = self.doc.get_movable_list(ORDER);
        for it in &old.items {
            if let Some(cur) = current.get(&it.id) {
                if cur != it {
                    self.write_update(cur, it);
                }
                continue;
            }
            // Deleted since: bring the object back, then put it as it was.
            match child_map(&objects, &it.id) {
                Some(m) => {
                    m.insert("deleted", false).expect("insert");
                    order.push(it.id.as_str()).expect("push");
                    let stored = self.decode_item(&it.id, &m.get_deep_value());
                    if let Some(stored) = stored.filter(|s| s != it) {
                        self.write_update(&stored, it);
                    }
                }
                None => self.write_new(order.len(), it),
            }
        }
        // Same stacking order as then.
        for (j, it) in old.items.iter().enumerate() {
            let at = (0..order.len()).find(
                |&i| matches!(order.get(i), Some(ValueOrContainer::Value(LoroValue::String(s))) if s.as_str() == it.id),
            );
            if let Some(i) = at {
                if i != j {
                    order.mov(i, j).expect("mov");
                }
            }
        }
        self.doc.commit();
        self.rebuild();
        Ok(true)
    }

    /// Z-order (P1-16): move `ids` to the top of the page (`front`) or to the bottom,
    /// keeping their relative order. One undo step. Returns false if nothing moved.
    pub fn reorder(&mut self, ids: &[&str], front: bool) -> bool {
        let order = self.doc.get_movable_list(ORDER);
        let id_at = |i: usize| match order.get(i) {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => Some(s.to_string()),
            _ => None,
        };
        let n = order.len();
        let picked: Vec<usize> = (0..n)
            .filter(|&i| id_at(i).is_some_and(|s| ids.contains(&s.as_str())))
            .collect();
        if picked.is_empty() {
            return false;
        }
        let k = picked.len();
        // Already in place?
        let in_place = if front {
            picked.iter().enumerate().all(|(j, &i)| i == n - k + j)
        } else {
            picked.iter().enumerate().all(|(j, &i)| i == j)
        };
        if in_place {
            return false;
        }
        if front {
            // Move from the top-most down so earlier indices stay valid.
            for (j, &i) in picked.iter().enumerate().rev() {
                order.mov(i, n - k + j).expect("mov");
            }
        } else {
            for (j, &i) in picked.iter().enumerate() {
                order.mov(i, j).expect("mov");
            }
        }
        self.doc.commit();
        self.rebuild();
        true
    }

    pub fn template(&self) -> &str {
        &self.view.template
    }

    pub fn set_template(&mut self, template: &str) {
        if template == self.view.template {
            return;
        }
        self.doc.get_map(META).insert("template", template).expect("insert");
        self.doc.commit();
        self.rebuild();
    }

    /// Paper size name (see `nb_model::PAGE_SIZES` and `nb_model::custom_size`).
    pub fn size(&self) -> &str {
        &self.view.size
    }

    pub fn set_size(&mut self, size: &str) {
        if size == self.view.size || !crate::valid_size(size) {
            return;
        }
        self.doc.get_map(META).insert("size", size).expect("insert");
        self.doc.commit();
        self.rebuild();
    }

    // ---------- undo (local changes only; remote changes are never undone) ----------

    pub fn undo(&mut self) -> bool {
        let done = self.undo.undo().unwrap_or(false);
        if done {
            self.rebuild();
        }
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.undo.redo().unwrap_or(false);
        if done {
            self.rebuild();
        }
        done
    }

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    // ---------- writing ----------

    fn write_new(&mut self, index: usize, item: &Item) {
        let objects = self.doc.get_map(OBJECTS);
        let order = self.doc.get_movable_list(ORDER);
        let m = objects
            .insert_container(&item.id, LoroMap::new())
            .expect("insert object");
        match &item.content {
            Content::Stroke(s) => {
                m.insert("t", "stroke").expect("insert");
                m.insert("tool", s.tool.name()).expect("insert");
                m.insert("color", i64::from(s.color)).expect("insert");
                m.insert("width", s.width).expect("insert");
                m.insert("ink", LoroValue::Binary(inkcodec::encode(&s.points).into()))
                    .expect("insert");
                m.insert("dx", 0.0).expect("insert");
                m.insert("dy", 0.0).expect("insert");
                m.insert("interp", interp_value(&s.interp)).expect("insert");
            }
            other => {
                m.insert("t", "block").expect("insert");
                m.insert("json", serde_json::to_string(other).expect("serialise"))
                    .expect("insert");
                if let Content::Text { text, spans, .. } = other {
                    let rt = m.insert_container("rt", LoroText::new()).expect("insert text");
                    write_rich(&rt, text, spans);
                }
            }
        }
        m.insert("deleted", false).expect("insert");
        let at = index.min(order.len());
        order.insert(at, item.id.as_str()).expect("insert order");
    }

    fn remove_ids(&mut self, ids: &[&str]) {
        let objects = self.doc.get_map(OBJECTS);
        let order = self.doc.get_movable_list(ORDER);
        // Delete from the end so earlier indices stay valid.
        let mut idx: Vec<usize> = (0..order.len())
            .filter(|&i| matches!(order.get(i), Some(ValueOrContainer::Value(LoroValue::String(s))) if ids.contains(&s.as_str())))
            .collect();
        idx.sort_unstable_by(|a, b| b.cmp(a));
        for i in idx {
            order.delete(i, 1).expect("delete order");
        }
        for id in ids {
            if let Some(m) = child_map(&objects, id) {
                m.insert("deleted", true).expect("insert");
            }
        }
    }

    fn write_update(&mut self, old: &Item, new: &Item) {
        let Some(m) = child_map(&self.doc.get_map(OBJECTS), &new.id) else {
            return;
        };
        match (&old.content, &new.content) {
            (Content::Stroke(a), Content::Stroke(b)) => {
                // Ink is immutable: only a transform of the stored points is representable,
                // its linear part in `xf` (`b.frame`) and its translation in `dx`/`dy`.
                let raw = match self.ink_cache.get(&new.id) {
                    Some(p) => p.clone(),
                    None => match m.get("ink") {
                        Some(ValueOrContainer::Value(LoroValue::Binary(bytes))) => {
                            Arc::new(inkcodec::decode(&bytes).unwrap_or_default())
                        }
                        _ => Arc::new(vec![]),
                    },
                };
                let f = b.frame;
                let (dx, dy) = match (raw.first(), b.points.first()) {
                    (Some(r), Some(q)) => (q.x - (f[0] * r.x + f[2] * r.y), q.y - (f[1] * r.x + f[3] * r.y)),
                    _ => (num(m.get("dx")), num(m.get("dy"))),
                };
                debug_assert!(
                    raw.len() == b.points.len()
                        && raw.iter().zip(&b.points).all(|(r, q)| {
                            (f[0] * r.x + f[2] * r.y + dx - q.x).abs() < 1e-3
                                && (f[1] * r.x + f[3] * r.y + dy - q.y).abs() < 1e-3
                        }),
                    "stroke update must be a transform of the stored ink (ADR-0003)"
                );
                if a.frame != b.frame {
                    let xf = if f == IDENTITY {
                        LoroValue::Null
                    } else {
                        LoroValue::from(f.to_vec())
                    };
                    m.insert("xf", xf).expect("insert");
                }
                let (ox, oy) = (num(m.get("dx")), num(m.get("dy")));
                if (dx - ox).abs() > 1e-9 || (dy - oy).abs() > 1e-9 {
                    m.insert("dx", dx).expect("insert");
                    m.insert("dy", dy).expect("insert");
                }
                if a.color != b.color {
                    m.insert("color", i64::from(b.color)).expect("insert");
                }
                if a.width != b.width {
                    m.insert("width", b.width).expect("insert");
                }
                if a.tool != b.tool {
                    m.insert("tool", b.tool.name()).expect("insert");
                }
                if a.interp != b.interp {
                    m.insert("interp", interp_value(&b.interp)).expect("insert");
                }
            }
            (_, other) => {
                m.insert("json", serde_json::to_string(other).expect("serialise"))
                    .expect("insert");
                if let Content::Text { text, spans, .. } = other {
                    let rt = match m.get("rt") {
                        Some(ValueOrContainer::Container(Container::Text(t))) => t,
                        _ => m.insert_container("rt", LoroText::new()).expect("insert text"),
                    };
                    write_rich(&rt, text, spans);
                }
            }
        }
    }

    // ---------- materialisation ----------

    fn rebuild(&mut self) {
        let meta = self.doc.get_map(META).get_value();
        let id = str_field(&meta, "id").unwrap_or_default();
        let template = str_field(&meta, "template").unwrap_or_else(|| "lined".into());
        let size = str_field(&meta, "size").unwrap_or_else(|| "a4".into());
        let LoroValue::List(order) = self.doc.get_movable_list(ORDER).get_value() else {
            unreachable!("root list")
        };
        let objects = self.doc.get_map(OBJECTS);
        let dirty: HashSet<String> = self.dirty.lock().unwrap().drain().collect();
        for oid in &dirty {
            self.refresh_object(&objects, oid);
        }
        // Unchanged items move from the previous view instead of being cloned.
        let mut previous: HashMap<String, Item> = std::mem::take(&mut self.view.items)
            .into_iter()
            .map(|it| (it.id.clone(), it))
            .collect();
        let mut seen = HashSet::new();
        let mut items = Vec::with_capacity(order.len());
        for v in order.iter() {
            let LoroValue::String(oid) = v else { continue };
            // Concurrent restores can list an id twice; keep its first occurrence only.
            if !seen.insert(oid.as_str()) {
                continue;
            }
            if !dirty.contains(oid.as_str()) {
                if let Some(item) = previous.remove(oid.as_str()) {
                    items.push(item);
                    continue;
                }
            }
            if !self.objects.contains_key(oid.as_str()) {
                self.refresh_object(&objects, oid);
            }
            if let Some((false, Some(item))) = self.objects.get(oid.as_str()) {
                items.push(item.clone());
            }
        }
        let groups = self.groups(&items);
        self.view = Page {
            id,
            template,
            size,
            pdf: pdf_field(&meta),
            items,
            groups,
        };
        #[cfg(debug_assertions)]
        {
            let full = self.rebuild_full();
            debug_assert_eq!(self.view, full, "cached view diverged from the document");
        }
    }

    /// Group of each grouped item in `items`.
    fn groups(&self, items: &[Item]) -> BTreeMap<String, String> {
        let LoroValue::Map(g) = self.doc.get_map(GROUPS).get_value() else {
            return BTreeMap::new();
        };
        if g.is_empty() {
            return BTreeMap::new();
        }
        items
            .iter()
            .filter_map(|it| match g.get(&it.id) {
                Some(LoroValue::String(s)) if !s.is_empty() => Some((it.id.clone(), s.to_string())),
                _ => None,
            })
            .collect()
    }

    /// Re-read one object from the document into the caches.
    fn refresh_object(&mut self, objects: &LoroMap, oid: &str) {
        let Some(m) = child_map(objects, oid) else {
            self.objects.remove(oid);
            self.ink_cache.remove(oid);
            return;
        };
        let fields = m.get_value();
        if !self.ink_cache.contains_key(oid) {
            if let Some(LoroValue::Binary(b)) = field(&fields, "ink") {
                if let Ok(p) = inkcodec::decode(b) {
                    self.ink_cache.insert(oid.to_string(), Arc::new(p));
                }
            }
        }
        let deleted = matches!(field(&fields, "deleted"), Some(LoroValue::Bool(true)));
        let item = self.decode_item(oid, &fields);
        self.objects.insert(oid.to_string(), (deleted, item));
    }

    /// Reference implementation without caches (debug check of [`CrdtPage::rebuild`]).
    #[cfg(debug_assertions)]
    fn rebuild_full(&self) -> Page {
        let meta = self.doc.get_map(META).get_value();
        let LoroValue::Map(objs) = self.doc.get_map(OBJECTS).get_deep_value() else {
            unreachable!()
        };
        let LoroValue::List(order) = self.doc.get_movable_list(ORDER).get_value() else {
            unreachable!()
        };
        let mut seen = HashSet::new();
        let items: Vec<Item> = order
            .iter()
            .filter_map(|v| match v {
                LoroValue::String(s) if seen.insert(s.to_string()) => Some(s.to_string()),
                _ => None,
            })
            .filter_map(|oid| {
                let f = objs.get(&oid)?;
                if matches!(field(f, "deleted"), Some(LoroValue::Bool(true))) {
                    return None;
                }
                self.decode_item(&oid, f)
            })
            .collect();
        let groups = self.groups(&items);
        Page {
            id: str_field(&meta, "id").unwrap_or_default(),
            template: str_field(&meta, "template").unwrap_or_else(|| "lined".into()),
            size: str_field(&meta, "size").unwrap_or_else(|| "a4".into()),
            pdf: pdf_field(&meta),
            items,
            groups,
        }
    }

    fn decode_item(&self, id: &str, fields: &LoroValue) -> Option<Item> {
        let content = match str_field(fields, "t")?.as_str() {
            "stroke" => {
                let raw = match self.ink_cache.get(id) {
                    Some(p) => p.clone(),
                    None => {
                        let Some(LoroValue::Binary(b)) = field(fields, "ink") else {
                            return None;
                        };
                        Arc::new(inkcodec::decode(b).ok()?)
                    }
                };
                let (dx, dy) = (num_v(field(fields, "dx")), num_v(field(fields, "dy")));
                let frame = frame_v(field(fields, "xf"));
                let f = frame;
                let points = if f == IDENTITY {
                    raw.iter()
                        .map(|p| Point {
                            x: p.x + dx,
                            y: p.y + dy,
                            ..*p
                        })
                        .collect()
                } else {
                    raw.iter()
                        .map(|p| Point {
                            x: f[0] * p.x + f[2] * p.y + dx,
                            y: f[1] * p.x + f[3] * p.y + dy,
                            ..*p
                        })
                        .collect()
                };
                let interp = match field(fields, "interp") {
                    Some(LoroValue::String(s)) => serde_json::from_str::<Interp>(s).ok(),
                    _ => None,
                };
                Content::Stroke(Stroke {
                    tool: Tool::from_name(str_field(fields, "tool").as_deref().unwrap_or("pen")),
                    color: match field(fields, "color") {
                        Some(LoroValue::I64(c)) => *c as u32,
                        _ => 0,
                    },
                    width: num_v(field(fields, "width")),
                    points,
                    interp,
                    frame,
                })
            }
            _ => {
                let mut c: Content = serde_json::from_str(&str_field(fields, "json")?).ok()?;
                if let Content::Text { text, spans, .. } = &mut c {
                    if let Some(rt) = child_map(&self.doc.get_map(OBJECTS), id).and_then(|m| match m.get("rt") {
                        Some(ValueOrContainer::Container(Container::Text(t))) => Some(t),
                        _ => None,
                    }) {
                        (*text, *spans) = read_rich(&rt);
                    }
                }
                c
            }
        };
        Some(Item {
            id: id.to_string(),
            content,
        })
    }
}

/// Changes in `doc` that `relay` lacks; advances `relay` (shared by pages and notebooks).
pub(crate) fn sync_outgoing(doc: &LoroDoc, relay: &mut VersionVector) -> Option<Vec<u8>> {
    let now = doc.oplog_vv();
    if now.iter().all(|(peer, c)| relay.get(peer).is_some_and(|r| r >= c)) {
        return None;
    }
    let bytes = doc.export(ExportMode::updates(relay)).expect("export updates");
    for (peer, c) in now.iter() {
        if relay.get(peer).is_none_or(|r| r < c) {
            relay.insert(*peer, *c);
        }
    }
    Some(bytes)
}

/// Import a relay update into `doc` and record what was applied in `relay`.
pub(crate) fn sync_incoming(doc: &LoroDoc, bytes: &[u8], relay: &mut VersionVector) -> Result<(), ModelError> {
    let status = doc.import(bytes).map_err(err)?;
    for (peer, (_, end)) in status.success.iter() {
        if relay.get(peer).is_none_or(|r| r < end) {
            relay.insert(*peer, *end);
        }
    }
    Ok(())
}

/// The `xf` field: four numbers, anything else is the identity.
fn frame_v(v: Option<&LoroValue>) -> Linear {
    let Some(LoroValue::List(l)) = v else {
        return IDENTITY;
    };
    if l.len() != 4 {
        return IDENTITY;
    }
    let mut f = IDENTITY;
    for (i, x) in l.iter().enumerate() {
        f[i] = num_v(Some(x));
    }
    f
}

/// Text and normalised spans of a rich text.
fn read_rich(rt: &LoroText) -> (String, Vec<TextSpan>) {
    let mut text = String::new();
    let mut spans = vec![];
    let mut at = 0u32;
    for d in rt.to_delta() {
        let TextDelta::Insert { insert, attributes } = d else {
            continue;
        };
        let n = insert.chars().count() as u32;
        text.push_str(&insert);
        if let Some(attrs) = attributes {
            let on = |k: &str| matches!(attrs.get(k), Some(LoroValue::Bool(true)));
            let int = |k: &str| match attrs.get(k) {
                Some(LoroValue::I64(v)) => Some(*v),
                Some(LoroValue::Double(v)) => Some(*v as i64),
                _ => None,
            };
            spans.push(TextSpan {
                start: at,
                end: at + n,
                bold: on("bold"),
                italic: on("italic"),
                underline: on("underline"),
                color: int("color").map(|c| c as u32),
                heading: int("h").unwrap_or(0).clamp(0, 3) as u8,
                list: int("list").unwrap_or(0).clamp(0, 2) as u8,
            });
        }
        at += n;
    }
    let len = at;
    (text, normalize_spans(&spans, len))
}

/// Make `rt` hold `text` with `spans`: a character diff (so concurrent edits merge), then
/// marks reset where the formatting differs.
fn write_rich(rt: &LoroText, text: &str, spans: &[TextSpan]) {
    if rt.to_string() != text {
        rt.update(text, UpdateOptions::default()).expect("update text");
    }
    let len = text.chars().count();
    let want = normalize_spans(spans, len as u32);
    let (_, have) = read_rich(rt);
    if want == have || len == 0 {
        return;
    }
    for k in MARKS {
        rt.unmark(0..len, k).expect("unmark");
    }
    for s in &want {
        let r = s.start as usize..s.end as usize;
        if s.bold {
            rt.mark(r.clone(), "bold", true).expect("mark");
        }
        if s.italic {
            rt.mark(r.clone(), "italic", true).expect("mark");
        }
        if s.underline {
            rt.mark(r.clone(), "underline", true).expect("mark");
        }
        if let Some(c) = s.color {
            rt.mark(r.clone(), "color", i64::from(c)).expect("mark");
        }
        if s.heading > 0 {
            rt.mark(r.clone(), "h", i64::from(s.heading)).expect("mark");
        }
        if s.list > 0 {
            rt.mark(r, "list", i64::from(s.list)).expect("mark");
        }
    }
}

fn interp_value(i: &Option<Interp>) -> LoroValue {
    match i {
        Some(i) => LoroValue::from(serde_json::to_string(i).expect("serialise")),
        None => LoroValue::Null,
    }
}

fn child_map(objects: &LoroMap, id: &str) -> Option<LoroMap> {
    match objects.get(id) {
        Some(ValueOrContainer::Container(Container::Map(m))) => Some(m),
        _ => None,
    }
}

fn field<'a>(v: &'a LoroValue, key: &str) -> Option<&'a LoroValue> {
    match v {
        LoroValue::Map(m) => m.get(key),
        _ => None,
    }
}

fn pdf_field(meta: &LoroValue) -> Option<PdfSource> {
    let asset = str_field(meta, "pdf_asset").filter(|s| !s.is_empty())?;
    let page_index = num_v(field(meta, "pdf_page")).max(0.0) as u32;
    Some(PdfSource { asset, page_index })
}

fn str_field(v: &LoroValue, key: &str) -> Option<String> {
    match field(v, key) {
        Some(LoroValue::String(s)) => Some(s.to_string()),
        _ => None,
    }
}

fn num_v(v: Option<&LoroValue>) -> f64 {
    match v {
        Some(LoroValue::Double(d)) => *d,
        Some(LoroValue::I64(i)) => *i as f64,
        _ => 0.0,
    }
}

fn num(v: Option<ValueOrContainer>) -> f64 {
    match v {
        Some(ValueOrContainer::Value(v)) => num_v(Some(&v)),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests;
