//! Notebook structure as a Loro movable tree (data-model §1, P1-01/P1-03).
//!
//! Each node is a **section** (can contain sections and pages) or a **page** (a leaf whose
//! content lives in its own [`crate::crdt::CrdtPage`] document, keyed by `page_id`).
//! Concurrent moves converge without cycles (verified in `docs/spikes/p0-03-loro.md`).
//!
//! Node metadata map: `kind` (`"section"`/`"page"`), `title`, `page_id` (pages),
//! `deleted` (trash tombstone; the node stays in place so restore is exact).

use crate::crdt::ModelError;
use loro::{ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, UndoManager, VersionVector};

const TREE: &str = "tree";
const META: &str = "meta";
/// Commit origin of housekeeping changes, excluded from undo.
const SYS_ORIGIN: &str = "sys:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Section,
    Page,
}

/// One entry of the outline, in display order.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub id: TreeID,
    pub kind: NodeKind,
    pub title: String,
    /// For pages: the id of the page document.
    pub page_id: Option<String>,
    pub depth: usize,
}

pub struct CrdtNotebook {
    doc: LoroDoc,
    undo: UndoManager,
    saved: VersionVector,
}

impl CrdtNotebook {
    pub fn new(id: &str, title: &str, peer: u64) -> Self {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).expect("valid peer id");
        let meta = doc.get_map(META);
        meta.insert("id", id).expect("insert");
        meta.insert("title", title).expect("insert");
        doc.get_tree(TREE).enable_fractional_index(0);
        doc.commit();
        Self::wrap(doc)
    }

    /// A replica with no content of its own, to be filled by sync. Writing nothing matters:
    /// a local title or id would compete (last writer wins) with the real one.
    pub fn empty(peer: u64) -> Self {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).expect("valid peer id");
        doc.get_tree(TREE).enable_fractional_index(0);
        Self::wrap(doc)
    }

    pub fn load(chunks: &[&[u8]], peer: u64) -> Result<Self, ModelError> {
        let doc = LoroDoc::new();
        for c in chunks {
            doc.import(c).map_err(|e| ModelError(e.to_string()))?;
        }
        doc.set_peer_id(peer).map_err(|e| ModelError(e.to_string()))?;
        if doc.get_map(META).get("id").is_none() {
            return Err(ModelError("not a Kahie notebook".into()));
        }
        let mut nb = Self::wrap(doc);
        nb.saved = nb.doc.oplog_vv();
        Ok(nb)
    }

    fn wrap(doc: LoroDoc) -> Self {
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(0);
        // Housekeeping (trash dates) is not something the user can undo.
        undo.add_exclude_origin_prefix(SYS_ORIGIN);
        undo.set_max_undo_steps(200);
        Self {
            doc,
            undo,
            saved: VersionVector::default(),
        }
    }

    /// Changes not yet handed out for persistence, each returned once (see `CrdtPage`).
    pub fn take_unsaved(&mut self) -> Option<Vec<u8>> {
        let now = self.doc.oplog_vv();
        if now == self.saved {
            return None;
        }
        let bytes = self.doc.export(ExportMode::updates(&self.saved)).expect("export");
        self.saved = now;
        Some(bytes)
    }

    /// Sync upload (see `CrdtPage::sync_outgoing`).
    pub fn sync_outgoing(&self, relay: &mut VersionVector) -> Option<Vec<u8>> {
        crate::crdt::sync_outgoing(&self.doc, relay)
    }

    /// Sync download (see `CrdtPage::sync_incoming`).
    pub fn sync_incoming(&mut self, bytes: &[u8], relay: &mut VersionVector) -> Result<(), ModelError> {
        crate::crdt::sync_incoming(&self.doc, bytes, relay)
    }

    /// Forget the undo history (e.g. after a migration that must not be undoable).
    pub fn clear_undo(&mut self) {
        self.undo.clear();
    }

    /// Parent section and index among its siblings.
    pub fn position(&self, node: TreeID) -> Option<(Option<TreeID>, usize)> {
        let t = self.tree();
        let parent = t.parent(node)?;
        let siblings = t.children(parent)?;
        let index = siblings.iter().position(|&s| s == node)?;
        let parent = match parent {
            TreeParentId::Node(p) => Some(p),
            _ => None,
        };
        Some((parent, index))
    }

    /// Number of children of `parent` (top level when `None`), trashed ones included.
    pub fn child_count(&self, parent: Option<TreeID>) -> usize {
        let p: TreeParentId = parent.into();
        self.tree().children_num(p).unwrap_or(0)
    }

    /// The `index`-th child of `parent` (trashed ones included).
    pub fn child_at(&self, parent: Option<TreeID>, index: usize) -> Option<TreeID> {
        let p: TreeParentId = parent.into();
        self.tree().children(p)?.get(index).copied()
    }

    fn tree(&self) -> LoroTree {
        self.doc.get_tree(TREE)
    }

    pub fn snapshot(&self) -> Vec<u8> {
        self.doc.export(ExportMode::Snapshot).expect("export")
    }

    pub fn version(&self) -> VersionVector {
        self.doc.oplog_vv()
    }

    pub fn updates_since(&self, v: &VersionVector) -> Vec<u8> {
        self.doc.export(ExportMode::updates(v)).expect("export")
    }

    pub fn import(&mut self, bytes: &[u8]) -> Result<(), ModelError> {
        self.doc
            .import(bytes)
            .map(|_| ())
            .map_err(|e| ModelError(e.to_string()))
    }

    pub fn title(&self) -> String {
        match self.doc.get_map(META).get_value() {
            LoroValue::Map(m) => match m.get("title") {
                Some(LoroValue::String(s)) => s.to_string(),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }

    pub fn set_title(&mut self, title: &str) {
        self.doc.get_map(META).insert("title", title).expect("insert");
        self.doc.commit();
    }

    fn create(
        &mut self,
        parent: Option<TreeID>,
        index: Option<usize>,
        kind: NodeKind,
        title: &str,
        page_id: Option<&str>,
    ) -> TreeID {
        let t = self.tree();
        let p: TreeParentId = parent.into();
        let id = match index {
            Some(i) => t.create_at(p, i.min(t.children_num(p).unwrap_or(0))).expect("create"),
            None => t.create(p).expect("create"),
        };
        let m = t.get_meta(id).expect("meta");
        m.insert("kind", if kind == NodeKind::Section { "section" } else { "page" })
            .expect("insert");
        m.insert("title", title).expect("insert");
        if let Some(pid) = page_id {
            m.insert("page_id", pid).expect("insert");
        }
        m.insert("deleted", false).expect("insert");
        self.doc.commit();
        id
    }

    /// Add a section under `parent` (root when `None`), at the end.
    pub fn add_section(&mut self, parent: Option<TreeID>, title: &str) -> TreeID {
        self.create(parent, None, NodeKind::Section, title, None)
    }

    /// Add a page under `parent` at `index` (end when `None`).
    pub fn add_page(&mut self, parent: Option<TreeID>, index: Option<usize>, page_id: &str, title: &str) -> TreeID {
        self.create(parent, index, NodeKind::Page, title, Some(page_id))
    }

    pub fn rename(&mut self, node: TreeID, title: &str) {
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("title", title).expect("insert");
            self.doc.commit();
        }
    }

    /// Move `node` under `parent` at `index` among its siblings. Refuses cycles and pages as parents.
    pub fn move_node(&mut self, node: TreeID, parent: Option<TreeID>, index: usize) -> bool {
        if let Some(p) = parent {
            if self.kind(p) != Some(NodeKind::Section) {
                return false;
            }
        }
        let t = self.tree();
        let p: TreeParentId = parent.into();
        let n = t.children_num(p).unwrap_or(0);
        if t.mov_to(node, p, index.min(n)).is_err() {
            return false;
        }
        self.doc.commit();
        true
    }

    /// Put `node` (and everything under it) in the trash.
    pub fn delete(&mut self, node: TreeID) {
        self.set_deleted(node, true);
    }

    /// Put `node` in the trash at time `now` (seconds since 1970), for the 30-day purge (P1-04).
    pub fn delete_at(&mut self, node: TreeID, now: i64) {
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("deleted", true).expect("insert");
            m.insert("deleted_at", now).expect("insert");
            self.doc.commit();
        }
    }

    pub fn restore(&mut self, node: TreeID) {
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("deleted", false).expect("insert");
            if m.get("deleted_at").is_some() {
                m.delete("deleted_at").expect("delete");
            }
            self.doc.commit();
        }
    }

    fn set_deleted(&mut self, node: TreeID, v: bool) {
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("deleted", v).expect("insert");
            self.doc.commit();
        }
    }

    /// When `node` was put in the trash (seconds), if it is trashed and dated.
    pub fn deleted_at(&self, node: TreeID) -> Option<i64> {
        match self.meta_value(node)? {
            LoroValue::Map(m) => match (m.get("deleted"), m.get("deleted_at")) {
                (Some(LoroValue::Bool(true)), Some(LoroValue::I64(t))) => Some(*t),
                _ => None,
            },
            _ => None,
        }
    }

    /// Trashed nodes deleted at or before `cutoff` (seconds). Trashed nodes without a date
    /// (deleted before dates were recorded) are dated `now` first, so they expire later.
    pub fn expired(&mut self, cutoff: i64, now: i64) -> Vec<Node> {
        let undated: Vec<TreeID> = self
            .trash()
            .iter()
            .map(|n| n.id)
            .filter(|&id| self.deleted_at(id).is_none())
            .collect();
        if !undated.is_empty() {
            for id in undated {
                if let Ok(m) = self.tree().get_meta(id) {
                    m.insert("deleted_at", now).expect("insert");
                }
            }
            self.doc.set_next_commit_origin(SYS_ORIGIN);
            self.doc.commit();
        }
        self.trash()
            .into_iter()
            .filter(|n| self.deleted_at(n.id).is_some_and(|t| t <= cutoff))
            .collect()
    }

    /// Move `node` right before (`after == false`) or after `sibling`, under the same parent
    /// as `sibling` (drag-and-drop reordering). Refuses cycles.
    pub fn move_next_to(&mut self, node: TreeID, sibling: TreeID, after: bool) -> bool {
        if node == sibling {
            return false;
        }
        let t = self.tree();
        let r = if after {
            t.mov_after(node, sibling)
        } else {
            t.mov_before(node, sibling)
        };
        if r.is_err() {
            return false;
        }
        self.doc.commit();
        true
    }

    pub fn undo(&mut self) -> bool {
        self.undo.undo().unwrap_or(false)
    }

    pub fn redo(&mut self) -> bool {
        self.undo.redo().unwrap_or(false)
    }

    fn meta_value(&self, node: TreeID) -> Option<LoroValue> {
        self.tree().get_meta(node).ok().map(|m| m.get_value())
    }

    fn kind(&self, node: TreeID) -> Option<NodeKind> {
        match self.meta_value(node)? {
            LoroValue::Map(m) => match m.get("kind") {
                Some(LoroValue::String(s)) if s.as_str() == "section" => Some(NodeKind::Section),
                Some(LoroValue::String(_)) => Some(NodeKind::Page),
                _ => None,
            },
            _ => None,
        }
    }

    /// Visible outline in display order (depth-first), skipping deleted nodes and their children.
    pub fn outline(&self) -> Vec<Node> {
        let mut out = vec![];
        self.walk(TreeParentId::Root, 0, &mut out, false);
        out
    }

    /// Page ids of `node` and everything under it (trashed or not).
    pub fn page_ids_under(&self, node: TreeID) -> Vec<String> {
        let mut out = vec![];
        if let Some(LoroValue::Map(m)) = self.meta_value(node) {
            if let Some(LoroValue::String(p)) = m.get("page_id") {
                out.push(p.to_string());
            }
        }
        for c in self.tree().children(TreeParentId::Node(node)).unwrap_or_default() {
            out.extend(self.page_ids_under(c));
        }
        out
    }

    /// Remove `node` and everything under it for good (emptying the trash).
    pub fn purge(&mut self, node: TreeID) {
        if self.tree().delete(node).is_ok() {
            self.doc.commit();
        }
    }

    /// Deleted nodes (top-most deleted ancestor only), for the trash view.
    pub fn trash(&self) -> Vec<Node> {
        let mut all = vec![];
        self.walk(TreeParentId::Root, 0, &mut all, true);
        all
    }

    /// Visible page ids in reading order.
    pub fn page_ids(&self) -> Vec<String> {
        self.outline().into_iter().filter_map(|n| n.page_id).collect()
    }

    fn walk(&self, parent: TreeParentId, depth: usize, out: &mut Vec<Node>, trash: bool) {
        let t = self.tree();
        for id in t.children(parent).unwrap_or_default() {
            let Some(LoroValue::Map(m)) = self.meta_value(id) else {
                continue;
            };
            let s = |k: &str| match m.get(k) {
                Some(LoroValue::String(s)) => Some(s.to_string()),
                _ => None,
            };
            let deleted = matches!(m.get("deleted"), Some(LoroValue::Bool(true)));
            let node = Node {
                id,
                kind: if s("kind").as_deref() == Some("section") {
                    NodeKind::Section
                } else {
                    NodeKind::Page
                },
                title: s("title").unwrap_or_default(),
                page_id: s("page_id"),
                depth,
            };
            match (trash, deleted) {
                (false, false) => {
                    out.push(node);
                    self.walk(TreeParentId::Node(id), depth + 1, out, false);
                }
                (true, true) => out.push(node),
                (true, false) => self.walk(TreeParentId::Node(id), depth + 1, out, true),
                (false, true) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(n: &CrdtNotebook) -> Vec<String> {
        n.outline()
            .iter()
            .map(|x| format!("{}{}", "  ".repeat(x.depth), x.title))
            .collect()
    }

    #[test]
    fn sections_pages_move_rename_delete_restore_undo() {
        let mut nb = CrdtNotebook::new("nb", "Maths", 1);
        let ch1 = nb.add_section(None, "Chapitre 1");
        let p1 = nb.add_page(Some(ch1), None, "p1", "Cours");
        nb.add_page(Some(ch1), None, "p2", "Exercices");
        let ch2 = nb.add_section(None, "Chapitre 2");
        assert_eq!(titles(&nb), ["Chapitre 1", "  Cours", "  Exercices", "Chapitre 2"]);
        assert!(nb.move_node(p1, Some(ch2), 0));
        assert!(!nb.move_node(ch1, Some(p1), 0), "pages cannot contain nodes");
        assert!(!nb.move_node(ch2, Some(ch2), 0), "no cycles");
        nb.rename(ch2, "Chapitre 2 — Géométrie");
        assert_eq!(
            titles(&nb),
            ["Chapitre 1", "  Exercices", "Chapitre 2 — Géométrie", "  Cours"]
        );
        nb.delete(ch1);
        assert_eq!(nb.page_ids(), ["p1"]);
        assert_eq!(
            nb.trash().iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
            ["Chapitre 1"]
        );
        nb.restore(ch1);
        assert_eq!(nb.page_ids(), ["p2", "p1"]);
        assert!(nb.undo()); // undo restore
        assert_eq!(nb.page_ids(), ["p1"]);
        let mut q = CrdtNotebook::load(&[&nb.snapshot()], 2).unwrap();
        assert_eq!(q.outline(), nb.outline());
        assert_eq!(q.title(), "Maths");
        q.set_title("Maths 1re");
        assert!(CrdtNotebook::load(&[b"x"], 3).is_err());
    }

    #[test]
    fn concurrent_reorders_converge() {
        let mut a = CrdtNotebook::new("nb", "N", 1);
        let s = a.add_section(None, "S");
        let pages: Vec<_> = (0..6)
            .map(|i| a.add_page(Some(s), None, &format!("p{i}"), &format!("{i}")))
            .collect();
        let mut b = CrdtNotebook::load(&[&a.snapshot()], 2).unwrap();
        a.move_node(pages[0], Some(s), 5);
        b.move_node(pages[0], Some(s), 2);
        b.move_node(pages[5], None, 0);
        a.add_page(Some(s), Some(1), "pa", "A");
        b.add_page(Some(s), Some(1), "pb", "B");
        let (va, vb) = (a.version(), b.version());
        let (ua, ub) = (a.updates_since(&vb), b.updates_since(&va));
        a.import(&ub).unwrap();
        b.import(&ua).unwrap();
        assert_eq!(a.outline(), b.outline());
        assert_eq!(a.page_ids().len(), 8);
    }

    #[test]
    fn drag_reorder_and_dated_trash() {
        let mut nb = CrdtNotebook::new("nb", "N", 1);
        let s = nb.add_section(None, "S");
        let a = nb.add_page(None, None, "a", "A");
        let b = nb.add_page(None, None, "b", "B");
        let c = nb.add_page(Some(s), None, "c", "C");
        assert_eq!(titles(&nb), ["S", "  C", "A", "B"]);
        assert!(nb.move_next_to(b, s, false), "before the section");
        assert_eq!(titles(&nb), ["B", "S", "  C", "A"]);
        assert!(nb.move_next_to(a, c, false), "into the section, before C");
        assert_eq!(titles(&nb), ["B", "S", "  A", "  C"]);
        assert!(nb.move_next_to(b, c, true), "after C");
        assert_eq!(titles(&nb), ["S", "  A", "  C", "  B"]);
        assert!(!nb.move_next_to(s, a, false), "a section cannot go inside itself");
        assert!(!nb.move_next_to(a, a, true));
        // Dated trash: only items past the cutoff expire; undated ones get dated now.
        nb.delete_at(a, 100);
        nb.delete(c); // no date (as before dates were recorded)
        assert_eq!(nb.deleted_at(a), Some(100));
        assert_eq!(nb.expired(150, 1_000).iter().map(|n| n.id).collect::<Vec<_>>(), [a]);
        assert_eq!(nb.deleted_at(c), Some(1_000));
        assert_eq!(nb.expired(1_000, 2_000).len(), 2);
        // Dating is not an undo step: undo restores C (the last user action).
        assert!(nb.undo());
        assert_eq!(nb.trash().iter().map(|n| n.id).collect::<Vec<_>>(), [a]);
        nb.restore(a);
        assert_eq!(nb.deleted_at(a), None);
        assert!(nb.trash().is_empty());
    }
}
