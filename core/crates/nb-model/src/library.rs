//! The library: folders holding notebooks, as a Loro movable tree (P1-01).
//!
//! Each node is a **folder** (holds folders and notebooks) or a **notebook** (a leaf naming
//! the notebook's directory, `dir`). Concurrent moves converge without cycles (Loro's
//! movable tree; see the property test below and `docs/spikes/p0-03-loro.md`).
//!
//! Node metadata map: `kind` (`"folder"`/`"notebook"`), `title` (folder name, or the
//! notebook's last known title, a cache for the library screen), `dir` (notebooks),
//! `opened` (seconds since 1970 when last opened on this device; recent first),
//! `deleted` (trash tombstone; the node stays in place so restore is exact) and
//! `deleted_at` (seconds since 1970, for the 30-day purge).

use crate::crdt::ModelError;
use loro::{ExportMode, LoroDoc, LoroTree, LoroValue, TreeID, TreeParentId, VersionVector};

const TREE: &str = "tree";
const META: &str = "meta";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibKind {
    Folder,
    Notebook,
}

/// One node of the library.
#[derive(Clone, Debug, PartialEq)]
pub struct LibNode {
    pub id: TreeID,
    pub kind: LibKind,
    pub title: String,
    /// For notebooks: the notebook's directory name.
    pub dir: Option<String>,
    /// Last opened (seconds since 1970), 0 if never.
    pub opened: i64,
    /// When it was put in the trash (seconds since 1970), if it is in the trash.
    pub deleted_at: Option<i64>,
    pub depth: usize,
}

pub struct CrdtLibrary {
    doc: LoroDoc,
    saved: VersionVector,
}

impl CrdtLibrary {
    pub fn new(peer: u64) -> Self {
        let doc = LoroDoc::new();
        doc.set_peer_id(peer).expect("valid peer id");
        doc.get_map(META).insert("kind", "library").expect("insert");
        doc.get_tree(TREE).enable_fractional_index(0);
        doc.commit();
        Self {
            doc,
            saved: VersionVector::default(),
        }
    }

    pub fn load(chunks: &[&[u8]], peer: u64) -> Result<Self, ModelError> {
        let doc = LoroDoc::new();
        for c in chunks {
            doc.import(c).map_err(|e| ModelError(e.to_string()))?;
        }
        doc.set_peer_id(peer).map_err(|e| ModelError(e.to_string()))?;
        if doc.get_map(META).get("kind").is_none() {
            return Err(ModelError("not a Kahie library".into()));
        }
        doc.get_tree(TREE).enable_fractional_index(0);
        let saved = doc.oplog_vv();
        Ok(Self { doc, saved })
    }

    /// Changes not yet handed out for persistence, each returned once.
    pub fn take_unsaved(&mut self) -> Option<Vec<u8>> {
        let now = self.doc.oplog_vv();
        if now == self.saved {
            return None;
        }
        let bytes = self.doc.export(ExportMode::updates(&self.saved)).expect("export");
        self.saved = now;
        Some(bytes)
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

    fn tree(&self) -> LoroTree {
        self.doc.get_tree(TREE)
    }

    fn create(&mut self, parent: Option<TreeID>, kind: LibKind, title: &str, dir: Option<&str>) -> Option<TreeID> {
        if let Some(p) = parent {
            if self.kind(p) != Some(LibKind::Folder) {
                return None;
            }
        }
        let t = self.tree();
        let id = t.create(TreeParentId::from(parent)).ok()?;
        let m = t.get_meta(id).expect("meta");
        m.insert("kind", if kind == LibKind::Folder { "folder" } else { "notebook" })
            .expect("insert");
        m.insert("title", title).expect("insert");
        if let Some(d) = dir {
            m.insert("dir", d).expect("insert");
        }
        m.insert("deleted", false).expect("insert");
        self.doc.commit();
        Some(id)
    }

    /// Add a folder at the end of `parent` (top level when `None`). `None` if `parent` is not a folder.
    pub fn add_folder(&mut self, parent: Option<TreeID>, title: &str) -> Option<TreeID> {
        self.create(parent, LibKind::Folder, title, None)
    }

    /// Add the notebook stored in directory `dir` at the end of `parent`.
    pub fn add_notebook(&mut self, parent: Option<TreeID>, dir: &str, title: &str) -> Option<TreeID> {
        self.create(parent, LibKind::Notebook, title, Some(dir))
    }

    /// Node of the notebook in directory `dir` (in the trash or not).
    pub fn find_notebook(&self, dir: &str) -> Option<TreeID> {
        self.all()
            .into_iter()
            .find(|n| n.dir.as_deref() == Some(dir))
            .map(|n| n.id)
    }

    /// Set a folder's name, or a notebook's cached title.
    pub fn rename(&mut self, node: TreeID, title: &str) {
        if self.node(node, 0).is_some_and(|n| n.title == title) {
            return;
        }
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("title", title).expect("insert");
            self.doc.commit();
        }
    }

    /// Record that a notebook was opened at `now` (seconds). The last one opened always sorts
    /// first, even if several are opened within the same second.
    pub fn touch(&mut self, node: TreeID, now: i64) {
        let latest = self.all().iter().filter(|n| n.id != node).map(|n| n.opened).max();
        let t = now.max(latest.unwrap_or(0) + 1);
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("opened", t).expect("insert");
            self.doc.commit();
        }
    }

    /// Move `node` to the end of folder `parent` (top level when `None`). Refuses cycles and
    /// notebooks as parents.
    pub fn move_node(&mut self, node: TreeID, parent: Option<TreeID>) -> bool {
        if let Some(p) = parent {
            if self.kind(p) != Some(LibKind::Folder) {
                return false;
            }
        }
        let t = self.tree();
        let p: TreeParentId = parent.into();
        let n = t.children_num(p).unwrap_or(0);
        // Already the last child of `parent`: nothing to do (mov_to would count it twice).
        if t.parent(node) == Some(p) && t.children(p).and_then(|c| c.last().copied()) == Some(node) {
            return true;
        }
        if t.mov_to(node, p, n).is_err() {
            return false;
        }
        self.doc.commit();
        true
    }

    /// Put `node` (and everything in it) in the trash at time `now` (seconds).
    pub fn delete(&mut self, node: TreeID, now: i64) {
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("deleted", true).expect("insert");
            m.insert("deleted_at", now).expect("insert");
            self.doc.commit();
        }
    }

    pub fn restore(&mut self, node: TreeID) {
        if let Ok(m) = self.tree().get_meta(node) {
            m.insert("deleted", false).expect("insert");
            m.delete("deleted_at").expect("delete");
            self.doc.commit();
        }
    }

    /// Remove `node` and everything in it for good.
    pub fn purge(&mut self, node: TreeID) {
        if self.tree().delete(node).is_ok() {
            self.doc.commit();
        }
    }

    /// Notebook directories of `node` and everything in it (trashed or not).
    pub fn dirs_under(&self, node: TreeID) -> Vec<String> {
        let mut out = vec![];
        if let Some(n) = self.node(node, 0) {
            out.extend(n.dir);
        }
        for c in self.tree().children(TreeParentId::Node(node)).unwrap_or_default() {
            out.extend(self.dirs_under(c));
        }
        out
    }

    /// Visible children of `parent` (top level when `None`): folders in tree order, then
    /// notebooks, most recently opened first.
    pub fn children(&self, parent: Option<TreeID>) -> Vec<LibNode> {
        let p: TreeParentId = parent.into();
        let mut kids: Vec<LibNode> = self
            .tree()
            .children(p)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| self.node(id, 0))
            .filter(|n| n.deleted_at.is_none())
            .collect();
        // Stable sort: folders keep their order.
        kids.sort_by(|a, b| match (a.kind, b.kind) {
            (LibKind::Folder, LibKind::Folder) => std::cmp::Ordering::Equal,
            (LibKind::Folder, LibKind::Notebook) => std::cmp::Ordering::Less,
            (LibKind::Notebook, LibKind::Folder) => std::cmp::Ordering::Greater,
            (LibKind::Notebook, LibKind::Notebook) => b.opened.cmp(&a.opened),
        });
        kids
    }

    /// Every visible folder, depth-first (for "move to folder" choices).
    pub fn folders(&self) -> Vec<LibNode> {
        self.walk(false)
            .into_iter()
            .filter(|n| n.kind == LibKind::Folder)
            .collect()
    }

    /// Every visible notebook (in any visible folder), most recently opened first.
    pub fn recent(&self) -> Vec<LibNode> {
        let mut v: Vec<LibNode> = self
            .walk(false)
            .into_iter()
            .filter(|n| n.kind == LibKind::Notebook)
            .collect();
        v.sort_by_key(|n| std::cmp::Reverse(n.opened));
        v
    }

    /// Folders from the top level down to `node` (excluded).
    pub fn ancestors(&self, node: TreeID) -> Vec<LibNode> {
        let t = self.tree();
        let mut out = vec![];
        let mut cur = t.parent(node);
        while let Some(TreeParentId::Node(p)) = cur {
            if let Some(n) = self.node(p, 0) {
                out.push(n);
            }
            cur = t.parent(p);
        }
        out.reverse();
        out
    }

    /// Whether `node` exists and neither it nor a folder above it is in the trash.
    pub fn is_visible(&self, node: TreeID) -> bool {
        let Some(n) = self.node(node, 0) else { return false };
        n.deleted_at.is_none() && self.ancestors(node).iter().all(|a| a.deleted_at.is_none())
    }

    /// Trashed nodes (top-most trashed ancestor only), most recently deleted first.
    pub fn trash(&self) -> Vec<LibNode> {
        let mut v = self.walk(true);
        v.sort_by_key(|n| std::cmp::Reverse(n.deleted_at));
        v
    }

    /// Trashed nodes deleted at or before `cutoff` (seconds).
    pub fn expired(&self, cutoff: i64) -> Vec<LibNode> {
        self.trash()
            .into_iter()
            .filter(|n| n.deleted_at.is_some_and(|t| t <= cutoff))
            .collect()
    }

    /// Every node (visible or trashed), depth-first.
    pub fn all(&self) -> Vec<LibNode> {
        let mut out = vec![];
        self.walk_all(TreeParentId::Root, 0, &mut out);
        out
    }

    fn walk_all(&self, parent: TreeParentId, depth: usize, out: &mut Vec<LibNode>) {
        for id in self.tree().children(parent).unwrap_or_default() {
            if let Some(n) = self.node(id, depth) {
                out.push(n);
            }
            self.walk_all(TreeParentId::Node(id), depth + 1, out);
        }
    }

    fn walk(&self, trash: bool) -> Vec<LibNode> {
        let mut out = vec![];
        self.walk_in(TreeParentId::Root, 0, trash, &mut out);
        out
    }

    fn walk_in(&self, parent: TreeParentId, depth: usize, trash: bool, out: &mut Vec<LibNode>) {
        for id in self.tree().children(parent).unwrap_or_default() {
            let Some(n) = self.node(id, depth) else { continue };
            match (trash, n.deleted_at.is_some()) {
                (false, false) => {
                    out.push(n);
                    self.walk_in(TreeParentId::Node(id), depth + 1, false, out);
                }
                (true, true) => out.push(n),
                (true, false) => self.walk_in(TreeParentId::Node(id), depth + 1, true, out),
                (false, true) => {}
            }
        }
    }

    fn kind(&self, node: TreeID) -> Option<LibKind> {
        self.node(node, 0).map(|n| n.kind)
    }

    fn node(&self, id: TreeID, depth: usize) -> Option<LibNode> {
        let LoroValue::Map(m) = self.tree().get_meta(id).ok()?.get_value() else {
            return None;
        };
        let s = |k: &str| match m.get(k) {
            Some(LoroValue::String(s)) => Some(s.to_string()),
            _ => None,
        };
        let i = |k: &str| match m.get(k) {
            Some(LoroValue::I64(v)) => Some(*v),
            Some(LoroValue::Double(v)) => Some(*v as i64),
            _ => None,
        };
        let kind = match s("kind")?.as_str() {
            "folder" => LibKind::Folder,
            _ => LibKind::Notebook,
        };
        let deleted = matches!(m.get("deleted"), Some(LoroValue::Bool(true)));
        Some(LibNode {
            id,
            kind,
            title: s("title").unwrap_or_default(),
            dir: s("dir"),
            opened: i("opened").unwrap_or(0),
            // A tombstone without a date (should not happen) counts as deleted at time 0.
            deleted_at: deleted.then(|| i("deleted_at").unwrap_or(0)),
            depth,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(l: &CrdtLibrary, parent: Option<TreeID>) -> Vec<String> {
        l.children(parent).into_iter().map(|n| n.title).collect()
    }

    #[test]
    fn folders_hold_notebooks_recent_first_and_go_to_the_trash() {
        let mut l = CrdtLibrary::new(1);
        let maths = l.add_folder(None, "Maths").unwrap();
        let seconde = l.add_folder(Some(maths), "2nde").unwrap();
        let a = l.add_notebook(None, "nb-0001", "Cours").unwrap();
        let b = l.add_notebook(None, "nb-0002", "Exercices").unwrap();
        let c = l.add_notebook(Some(seconde), "nb-0003", "Contrôles").unwrap();
        assert!(l.add_folder(Some(a), "x").is_none(), "a notebook holds nothing");
        l.touch(a, 100);
        l.touch(b, 200);
        assert_eq!(names(&l, None), ["Maths", "Exercices", "Cours"]);
        l.touch(a, 300);
        assert_eq!(names(&l, None), ["Maths", "Cours", "Exercices"]);
        assert_eq!(
            l.recent().iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
            ["Cours", "Exercices", "Contrôles"]
        );
        // Move, rename, refuse cycles and notebooks as parents.
        assert!(l.move_node(b, Some(maths)));
        assert_eq!(names(&l, Some(maths)), ["2nde", "Exercices"]);
        assert!(!l.move_node(maths, Some(seconde)), "no cycles");
        assert!(!l.move_node(c, Some(a)));
        assert!(l.move_node(b, Some(maths)), "moving to the same place is a no-op");
        l.rename(seconde, "Seconde");
        assert_eq!(
            l.ancestors(c).iter().map(|n| n.title.as_str()).collect::<Vec<_>>(),
            ["Maths", "Seconde"]
        );
        assert_eq!(l.folders().iter().map(|n| n.depth).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(l.find_notebook("nb-0003"), Some(c));
        // Trash: a folder takes its content with it; restore is exact.
        l.delete(maths, 1_000);
        assert_eq!(names(&l, None), ["Cours"]);
        assert!(!l.is_visible(c) && l.is_visible(a));
        let t = l.trash();
        assert_eq!((t.len(), t[0].deleted_at), (1, Some(1_000)));
        let mut dirs = l.dirs_under(maths);
        dirs.sort();
        assert_eq!(dirs, ["nb-0002", "nb-0003"]);
        assert!(l.expired(999).is_empty());
        assert_eq!(l.expired(1_000).len(), 1);
        l.restore(maths);
        assert!(l.trash().is_empty() && l.is_visible(c));
        l.delete(a, 5);
        l.purge(a);
        assert!(l.find_notebook("nb-0001").is_none());
        // Persisted and reloaded.
        let q = CrdtLibrary::load(&[&l.snapshot()], 2).unwrap();
        assert_eq!(q.all(), l.all());
        assert!(CrdtLibrary::load(&[b"x"], 3).is_err());
    }

    struct Rng(u64);
    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }
    }

    fn sync(a: &mut CrdtLibrary, b: &mut CrdtLibrary) {
        let (va, vb) = (a.version(), b.version());
        let (ua, ub) = (a.updates_since(&vb), b.updates_since(&va));
        a.import(&ub).unwrap();
        b.import(&ua).unwrap();
    }

    /// Property test (P1-01): replicas applying random concurrent creates, moves, renames,
    /// deletes and restores, gossiping partially, end identical once fully synced, and the
    /// tree has no cycle (every node is reachable from the top level exactly once).
    #[test]
    fn concurrent_moves_converge_without_cycles() {
        for seed in 1..=40u64 {
            let mut r = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
            let mut base = CrdtLibrary::new(100);
            for i in 0..3 {
                let f = base.add_folder(None, &format!("F{i}")).unwrap();
                base.add_notebook(Some(f), &format!("nb-{i}"), "n");
            }
            let mut reps: Vec<CrdtLibrary> = (0..3)
                .map(|k| CrdtLibrary::load(&[&base.snapshot()], k + 1).unwrap())
                .collect();
            for round in 0..12 {
                for (k, rep) in reps.iter_mut().enumerate() {
                    for i in 0..r.below(6) {
                        let all = rep.all();
                        let folders: Vec<TreeID> =
                            all.iter().filter(|n| n.kind == LibKind::Folder).map(|n| n.id).collect();
                        let pick = |r: &mut Rng| all[r.below(all.len())].id;
                        let folder = |r: &mut Rng| {
                            let i = r.below(folders.len() + 1);
                            folders.get(i).copied()
                        };
                        match r.below(7) {
                            0 => {
                                let p = folder(&mut r);
                                rep.add_folder(p, &format!("f{k}{round}{i}"));
                            }
                            1 => {
                                let p = folder(&mut r);
                                rep.add_notebook(p, &format!("d{k}{round}{i}"), "n");
                            }
                            2 | 3 => {
                                let (n, p) = (pick(&mut r), folder(&mut r));
                                rep.move_node(n, p);
                            }
                            4 => {
                                let n = pick(&mut r);
                                rep.rename(n, &format!("t{k}{round}{i}"));
                            }
                            5 => {
                                let n = pick(&mut r);
                                rep.delete(n, round);
                            }
                            _ => {
                                let n = pick(&mut r);
                                rep.restore(n);
                            }
                        }
                    }
                }
                let (a, b) = (r.below(3), r.below(3));
                if a != b {
                    let (lo, hi) = (a.min(b), a.max(b));
                    let (left, right) = reps.split_at_mut(hi);
                    sync(&mut left[lo], &mut right[0]);
                }
            }
            for _ in 0..2 {
                for (a, b) in [(0, 1), (1, 2), (0, 2)] {
                    let (left, right) = reps.split_at_mut(b);
                    sync(&mut left[a], &mut right[0]);
                }
            }
            let all = reps[0].all();
            assert_eq!(all, reps[1].all(), "seed {seed}");
            assert_eq!(all, reps[2].all(), "seed {seed}");
            assert_eq!(reps[0].trash(), reps[2].trash(), "seed {seed}");
            let unique: std::collections::HashSet<TreeID> = all.iter().map(|n| n.id).collect();
            assert_eq!(unique.len(), all.len(), "each node once, seed {seed}");
            let nodes = reps[0].tree().nodes().len();
            assert_eq!(all.len(), nodes, "every node reachable from the top, seed {seed}");
            for n in &all {
                if let Some(TreeParentId::Node(p)) = reps[0].tree().parent(n.id) {
                    assert_eq!(
                        reps[0].kind(p),
                        Some(LibKind::Folder),
                        "only folders hold nodes, seed {seed}"
                    );
                }
            }
        }
    }
}
