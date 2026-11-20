//! A page stored in a Loro document (ADR-0001, decision D6; measurements in
//! `docs/spikes/p0-03-loro.md`).
//!
//! Layout of the Loro document:
//!
//! | Root container | Type | Content |
//! |---|---|---|
//! | `meta` | map | `id`, `template` |
//! | `order` | movable list | object ids, bottom to top (z-order) |
//! | `objects` | map | object id → map of fields (below) |
//!
//! Object fields: `t` (`"stroke"` or `"block"`), `deleted` (bool, trash tombstone).
//! Strokes: `tool`, `color`, `width`, `ink` (packed points, [`crate::inkcodec`],
//! **immutable once written** — ADR-0003), `dx`/`dy` (translation applied to the ink),
//! `interp` (JSON of [`Interp`] or null). Other objects: `json` (JSON of [`Content`]).
//!
//! The plain [`Page`] returned by [`CrdtPage::page`] is a materialised view rebuilt
//! after every change; decoded ink is cached per object id because ink never changes.

use crate::{inkcodec, Content, Interp, Item, Op, Page, Stroke, Tool};
use loro::event::{Diff, DiffEvent};
use loro::{
    Container, ContainerTrait, ExportMode, Index, LoroDoc, LoroMap, LoroValue, Subscription, UndoManager,
    ValueOrContainer, VersionVector,
};
use nb_ink::Point;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

const META: &str = "meta";
const ORDER: &str = "order";
const OBJECTS: &str = "objects";
/// Image bytes by SHA-256 (hex). Stored in the CRDT so images sync and export with the page.
const ASSETS: &str = "assets";

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
        }
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

    fn patch_view(&mut self, op: &Op) {
        match op {
            Op::Add(entries) => {
                let mut sorted = entries.clone();
                sorted.sort_by_key(|(i, _)| *i);
                for (i, mut item) in sorted {
                    if let Content::Stroke(s) = &mut item.content {
                        // Same values a rebuild would decode from the stored ink.
                        s.points = inkcodec::quantize(&s.points);
                        self.ink_cache.insert(item.id.clone(), Arc::new(s.points.clone()));
                    }
                    let at = i.min(self.view.items.len());
                    self.view.items.insert(at, item);
                }
            }
            Op::Remove(entries) => {
                self.view.items.retain(|it| !entries.iter().any(|(_, e)| e.id == it.id));
            }
            Op::Update { .. } => self.rebuild(),
        }
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
                m.insert("tool", tool_name(s.tool)).expect("insert");
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
                // Ink is immutable: only a translation of the raw points is representable.
                let (dx, dy) = match (a.points.first(), b.points.first()) {
                    (Some(p), Some(q)) => (q.x - p.x, q.y - p.y),
                    _ => (0.0, 0.0),
                };
                debug_assert!(
                    a.points.len() == b.points.len()
                        && a.points
                            .iter()
                            .zip(&b.points)
                            .all(|(p, q)| { (q.x - p.x - dx).abs() < 1e-6 && (q.y - p.y - dy).abs() < 1e-6 }),
                    "stroke update must be a pure translation (ADR-0003)"
                );
                let (ox, oy) = (num(m.get("dx")), num(m.get("dy")));
                if dx != 0.0 || dy != 0.0 {
                    m.insert("dx", ox + dx).expect("insert");
                    m.insert("dy", oy + dy).expect("insert");
                }
                if a.color != b.color {
                    m.insert("color", i64::from(b.color)).expect("insert");
                }
                if a.width != b.width {
                    m.insert("width", b.width).expect("insert");
                }
                if a.tool != b.tool {
                    m.insert("tool", tool_name(b.tool)).expect("insert");
                }
                if a.interp != b.interp {
                    m.insert("interp", interp_value(&b.interp)).expect("insert");
                }
            }
            (_, other) => {
                m.insert("json", serde_json::to_string(other).expect("serialise"))
                    .expect("insert");
            }
        }
    }

    // ---------- materialisation ----------

    fn rebuild(&mut self) {
        let meta = self.doc.get_map(META).get_value();
        let id = str_field(&meta, "id").unwrap_or_default();
        let template = str_field(&meta, "template").unwrap_or_else(|| "lined".into());
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
        self.view = Page { id, template, items };
        #[cfg(debug_assertions)]
        {
            let full = self.rebuild_full();
            debug_assert_eq!(self.view, full, "cached view diverged from the document");
        }
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
        let items = order
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
        Page {
            id: str_field(&meta, "id").unwrap_or_default(),
            template: str_field(&meta, "template").unwrap_or_else(|| "lined".into()),
            items,
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
                let points = raw.iter().map(|p| Point::new(p.x + dx, p.y + dy, p.p)).collect();
                let interp = match field(fields, "interp") {
                    Some(LoroValue::String(s)) => serde_json::from_str::<Interp>(s).ok(),
                    _ => None,
                };
                Content::Stroke(Stroke {
                    tool: if str_field(fields, "tool").as_deref() == Some("highlighter") {
                        Tool::Highlighter
                    } else {
                        Tool::Pen
                    },
                    color: match field(fields, "color") {
                        Some(LoroValue::I64(c)) => *c as u32,
                        _ => 0,
                    },
                    width: num_v(field(fields, "width")),
                    points,
                    interp,
                })
            }
            _ => serde_json::from_str(&str_field(fields, "json")?).ok()?,
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

fn tool_name(t: Tool) -> &'static str {
    match t {
        Tool::Pen => "pen",
        Tool::Highlighter => "highlighter",
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
