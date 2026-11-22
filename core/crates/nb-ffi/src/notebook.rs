//! `NotebookDoc`: the section/page outline of one notebook directory, for the app.
//!
//! Layout of a notebook directory:
//! - `notebook.db` — SQLite log of the outline CRDT ([`nb_model::notebook::CrdtNotebook`]);
//! - `<page-id>.kpage` — one SQLite page database per page ([`crate::PageDoc`]).
//!
//! Directories written before the outline existed hold `page-NNNN.json` files: they are
//! adopted in name order on first open and renamed to `.kpage` (contents migrate when
//! the page is opened).

use crate::{peer_id, CoreError};
use loro::TreeID;
use nb_model::notebook::{CrdtNotebook, NodeKind};
use nb_store::Store;
use std::path::PathBuf;
use std::sync::Mutex;

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutlineKind {
    Section,
    Page,
}

/// One row of the outline, in display order.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct OutlineEntry {
    /// Opaque node id (use with `rename`, `move_node`, `delete_node`, `restore_node`).
    pub node: String,
    pub kind: OutlineKind,
    pub title: String,
    /// For pages: id of the page, see `page_path`.
    pub page_id: Option<String>,
    pub depth: u32,
}

pub(crate) struct State {
    pub(crate) nb: CrdtNotebook,
    pub(crate) store: Store,
    dir: PathBuf,
    error: Option<String>,
}

impl State {
    pub(crate) fn persist(&mut self) {
        let Some(update) = self.nb.take_unsaved() else { return };
        let r = self.store.append_update(&update).and_then(|_| {
            if self.store.pending_updates()? >= 200 {
                self.store.compact(&self.nb.snapshot())?;
            }
            Ok(())
        });
        if let Err(e) = r {
            self.error.get_or_insert(e.to_string());
        }
    }

    /// Remove a trashed node for good with the files of its pages; returns the page count.
    fn remove_for_good(&mut self, node: TreeID) -> u32 {
        let pages = self.nb.page_ids_under(node);
        for page in &pages {
            for ext in ["kpage", "kpage-wal", "kpage-shm"] {
                let _ = std::fs::remove_file(self.dir.join(format!("{page}.{ext}")));
            }
        }
        self.nb.purge(node);
        pages.len() as u32
    }

    fn node(&self, id: &str) -> Option<TreeID> {
        TreeID::try_from(id).ok()
    }
}

#[derive(uniffi::Object)]
pub struct NotebookDoc {
    pub(crate) state: Mutex<State>,
}

/// Seconds since 1970.
pub(crate) fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn fmt_err(e: impl std::fmt::Display) -> CoreError {
    CoreError::Format { detail: e.to_string() }
}

fn new_page_id() -> String {
    // Unique across devices (hashed time + process) and within a session (counter).
    static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("p{:012x}{n:x}", peer_id() >> 16)
}

fn entry(n: nb_model::notebook::Node) -> OutlineEntry {
    OutlineEntry {
        node: n.id.to_string(),
        kind: if n.kind == NodeKind::Section {
            OutlineKind::Section
        } else {
            OutlineKind::Page
        },
        title: n.title,
        page_id: n.page_id,
        depth: n.depth as u32,
    }
}

impl NotebookDoc {
    /// Title, visible outline and directory, for exports.
    pub(crate) fn export_parts(&self) -> (String, Vec<nb_model::notebook::Node>, PathBuf) {
        let st = self.state.lock().unwrap();
        (st.nb.title(), st.nb.outline(), st.dir.clone())
    }
}

#[uniffi::export]
impl NotebookDoc {
    /// Open (or create) the notebook stored in directory `dir`.
    #[uniffi::constructor]
    pub fn open(dir: String, title: String) -> Result<std::sync::Arc<Self>, CoreError> {
        Self::open_inner(dir, title, true)
    }

    /// Open a directory for a notebook that will arrive through sync: when it is new, it
    /// starts with no page and no title (they come from the other devices).
    #[uniffi::constructor]
    pub fn join(dir: String) -> Result<std::sync::Arc<Self>, CoreError> {
        Self::open_inner(dir, String::new(), false)
    }
}

impl NotebookDoc {
    fn open_inner(dir: String, title: String, default_page: bool) -> Result<std::sync::Arc<Self>, CoreError> {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).map_err(|e| CoreError::Io { detail: e.to_string() })?;
        let store = Store::open(dir.join("notebook.db")).map_err(fmt_err)?;
        let loaded = store.load().map_err(fmt_err)?;
        let mut chunks: Vec<&[u8]> = loaded.snapshot.iter().map(Vec::as_slice).collect();
        chunks.extend(loaded.updates.iter().map(Vec::as_slice));
        let fresh = chunks.is_empty();
        let nb = if fresh && !default_page {
            CrdtNotebook::empty(peer_id())
        } else if fresh {
            CrdtNotebook::new("notebook", &title, peer_id())
        } else {
            CrdtNotebook::load(&chunks, peer_id()).map_err(fmt_err)?
        };
        let mut st = State {
            nb,
            store,
            dir,
            error: None,
        };
        if fresh {
            // Adopt pages written before the outline existed.
            let mut legacy: Vec<PathBuf> = std::fs::read_dir(&st.dir)
                .map_err(|e| CoreError::Io { detail: e.to_string() })?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect();
            legacy.sort();
            for p in &legacy {
                let id = p.file_stem().and_then(|s| s.to_str()).unwrap_or("page").to_string();
                std::fs::rename(p, st.dir.join(format!("{id}.kpage")))
                    .map_err(|e| CoreError::Io { detail: e.to_string() })?;
                st.nb.add_page(None, None, &id, "");
            }
            if legacy.is_empty() && default_page {
                st.nb.add_page(None, None, &new_page_id(), "");
            }
            st.nb.clear_undo();
        }
        st.persist();
        Ok(std::sync::Arc::new(Self { state: Mutex::new(st) }))
    }
}

#[uniffi::export]
impl NotebookDoc {
    pub fn title(&self) -> String {
        self.state.lock().unwrap().nb.title()
    }

    pub fn set_title(&self, title: String) {
        let mut st = self.state.lock().unwrap();
        st.nb.set_title(&title);
        st.persist();
    }

    /// Visible outline (sections and pages, depth-first).
    pub fn outline(&self) -> Vec<OutlineEntry> {
        self.state.lock().unwrap().nb.outline().into_iter().map(entry).collect()
    }

    /// Visible page ids in reading order.
    pub fn page_ids(&self) -> Vec<String> {
        self.state.lock().unwrap().nb.page_ids()
    }

    /// File of a page, to pass to `PageDoc.open`.
    pub fn page_path(&self, page_id: String) -> String {
        let st = self.state.lock().unwrap();
        st.dir.join(format!("{page_id}.kpage")).to_string_lossy().into_owned()
    }

    /// Insert a new page right after `after_page_id` (at the end when `None` or unknown).
    /// Returns the new page id.
    pub fn add_page(&self, after_page_id: Option<String>) -> String {
        let mut st = self.state.lock().unwrap();
        let id = new_page_id();
        let outline = st.nb.outline();
        let anchor = after_page_id.and_then(|a| {
            outline
                .iter()
                .find(|n| n.page_id.as_deref() == Some(a.as_str()))
                .cloned()
        });
        match anchor {
            Some(a) => {
                let (parent, index) = st.nb.position(a.id).unwrap_or((None, usize::MAX));
                st.nb.add_page(parent, Some(index.saturating_add(1)), &id, "");
            }
            None => {
                st.nb.add_page(None, None, &id, "");
            }
        }
        st.persist();
        id
    }

    pub fn add_section(&self, title: String) -> String {
        let mut st = self.state.lock().unwrap();
        let id = st.nb.add_section(None, &title).to_string();
        st.persist();
        id
    }

    pub fn rename(&self, node: String, title: String) {
        let mut st = self.state.lock().unwrap();
        if let Some(n) = st.node(&node) {
            st.nb.rename(n, &title);
            st.persist();
        }
    }

    /// Move `node` under section `parent` (top level when `None`) at `index`.
    pub fn move_node(&self, node: String, parent: Option<String>, index: u32) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(n) = st.node(&node) else { return false };
        let p = match parent {
            Some(p) => match st.node(&p) {
                Some(p) => Some(p),
                None => return false,
            },
            None => None,
        };
        let ok = st.nb.move_node(n, p, index as usize);
        st.persist();
        ok
    }

    /// Move `node` one place up (`delta` < 0) or down (`delta` > 0) among its siblings,
    /// skipping trashed ones. Returns false at either end.
    pub fn shift_node(&self, node: String, delta: i32) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(n) = st.node(&node) else { return false };
        let Some((parent, index)) = st.nb.position(n) else {
            return false;
        };
        let siblings = st.nb.child_count(parent);
        if delta == 0 {
            return false;
        }
        // Step over trashed siblings so the visible order changes.
        let visible: std::collections::HashSet<TreeID> = st.nb.outline().iter().map(|e| e.id).collect();
        let mut to = index as i64;
        loop {
            to += delta.signum() as i64;
            if to < 0 || to >= siblings as i64 {
                return false;
            }
            let at = st.nb.child_at(parent, to as usize);
            if at.is_some_and(|id| visible.contains(&id)) {
                break;
            }
        }
        let ok = st.nb.move_node(n, parent, to as usize);
        st.persist();
        ok
    }

    /// Put a node in the trash (its pages stay on disk until the trash is emptied, or for
    /// 30 days: see `purge_expired`).
    pub fn delete_node(&self, node: String) {
        let mut st = self.state.lock().unwrap();
        if let Some(n) = st.node(&node) {
            st.nb.delete_at(n, now_secs());
            st.persist();
        }
    }

    /// Drag and drop: move `node` right before (or after, when `after`) `sibling`, into
    /// `sibling`'s section. False if that would put a section inside itself.
    pub fn move_next_to(&self, node: String, sibling: String, after: bool) -> bool {
        let mut st = self.state.lock().unwrap();
        let (Some(n), Some(s)) = (st.node(&node), st.node(&sibling)) else {
            return false;
        };
        let ok = st.nb.move_next_to(n, s, after);
        st.persist();
        ok
    }

    /// Permanently remove what has been in the trash for `max_age_days` days or more, with
    /// its page files (P1-04). Returns the number of pages removed.
    pub fn purge_expired(&self, max_age_days: u32) -> u32 {
        let mut st = self.state.lock().unwrap();
        let now = now_secs();
        let expired = st.nb.expired(now - i64::from(max_age_days) * 86_400, now);
        let removed = expired.iter().map(|n| st.remove_for_good(n.id)).sum();
        st.persist();
        removed
    }

    pub fn restore_node(&self, node: String) {
        let mut st = self.state.lock().unwrap();
        if let Some(n) = st.node(&node) {
            st.nb.restore(n);
            st.persist();
        }
    }

    pub fn trash(&self) -> Vec<OutlineEntry> {
        self.state.lock().unwrap().nb.trash().into_iter().map(entry).collect()
    }

    /// Permanently remove everything in the trash and delete its page files. Returns the
    /// number of pages removed.
    pub fn empty_trash(&self) -> u32 {
        let mut st = self.state.lock().unwrap();
        let trash = st.nb.trash();
        let removed = trash.iter().map(|n| st.remove_for_good(n.id)).sum();
        st.persist();
        removed
    }

    /// Fill this notebook from `template` (P1-08 notebook templates): the same sections, page
    /// titles and order, each page a copy of the template's (paper, size, background,
    /// objects; new ids, no history and no sync state), appended at the end. If this
    /// notebook held only its initial empty page, that page is removed. Returns the number of
    /// pages copied.
    pub fn copy_from_template(&self, template: std::sync::Arc<NotebookDoc>) -> Result<u32, CoreError> {
        if std::ptr::eq(self, template.as_ref()) {
            return Ok(0);
        }
        let (outline, src_dir) = {
            let t = template.state.lock().unwrap();
            (t.nb.outline(), t.dir.clone())
        };
        let mut st = self.state.lock().unwrap();
        let before = st.nb.outline();
        let blank = match before.as_slice() {
            [only] if only.kind == NodeKind::Page => {
                let pid = only.page_id.clone().unwrap_or_default();
                let path = st.dir.join(format!("{pid}.kpage"));
                let empty = !path.exists()
                    || crate::PageDoc::open(path.to_string_lossy().into()).is_ok_and(|p| p.item_count() == 0);
                empty.then_some((only.id, pid))
            }
            _ => None,
        };
        // Parent section of the nodes at each depth.
        let mut parents: Vec<Option<TreeID>> = vec![None];
        let mut copied = 0;
        for e in outline {
            parents.truncate(e.depth + 1);
            let parent = parents.last().copied().flatten();
            match e.kind {
                NodeKind::Section => {
                    let id = st.nb.add_section(parent, &e.title);
                    parents.push(Some(id));
                }
                NodeKind::Page => {
                    let id = new_page_id();
                    st.nb.add_page(parent, None, &id, &e.title);
                    let src = src_dir.join(format!("{}.kpage", e.page_id.unwrap_or_default()));
                    let dst = crate::PageDoc::open(st.dir.join(format!("{id}.kpage")).to_string_lossy().into())?;
                    if src.exists() {
                        let src = crate::PageDoc::open(src.to_string_lossy().into())?;
                        dst.copy_content_from(src);
                    }
                    dst.save()?;
                    copied += 1;
                }
            }
        }
        if let (Some((node, pid)), true) = (blank, copied > 0) {
            st.nb.delete(node);
            st.nb.purge(node);
            for ext in ["kpage", "kpage-wal", "kpage-shm"] {
                let _ = std::fs::remove_file(st.dir.join(format!("{pid}.{ext}")));
            }
        }
        st.persist();
        Ok(copied)
    }

    pub fn undo(&self) -> bool {
        let mut st = self.state.lock().unwrap();
        let done = st.nb.undo();
        st.persist();
        done
    }

    /// Compact the log and report any earlier write failure.
    pub fn save(&self) -> Result<(), CoreError> {
        let mut st = self.state.lock().unwrap();
        st.persist();
        if let Some(e) = st.error.take() {
            return Err(CoreError::Io { detail: e });
        }
        let snap = st.nb.snapshot();
        st.store
            .compact(&snap)
            .map_err(|e| CoreError::Io { detail: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kahie-nb-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn notebook_from_template_copies_sections_titles_and_pages() {
        let (tdir, ndir) = (tmp("tpl-src"), tmp("tpl-dst"));
        let tpl = NotebookDoc::open(tdir.to_string_lossy().into(), "Modèle".into()).unwrap();
        let first = tpl.page_ids()[0].clone();
        let p = crate::PageDoc::open(tpl.page_path(first.clone())).unwrap();
        p.set_template("grid".into());
        p.set_page_size("a5".into());
        p.add_text(10.0, 10.0, 200.0, 30.0, "Objectifs".into(), 18.0, 0);
        p.save().unwrap();
        let sec = tpl.add_section("Séquence 1".into());
        let inner = tpl.add_page(None);
        let node = |nb: &NotebookDoc, pid: &str| {
            nb.outline()
                .into_iter()
                .find(|e| e.page_id.as_deref() == Some(pid))
                .unwrap()
                .node
        };
        tpl.move_node(node(&tpl, &inner), Some(sec.clone()), 0);
        tpl.rename(node(&tpl, &inner), "Exercices".into());

        let nb = NotebookDoc::open(ndir.to_string_lossy().into(), "2nde B".into()).unwrap();
        assert_eq!(nb.copy_from_template(tpl.clone()).unwrap(), 2);
        let shape = |nb: &NotebookDoc| {
            nb.outline()
                .into_iter()
                .map(|e| (e.kind, e.title, e.depth))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            shape(&nb),
            shape(&tpl),
            "same structure; the initial blank page is gone"
        );
        let ids = nb.page_ids();
        assert!(ids.iter().all(|id| !tpl.page_ids().contains(id)), "new page ids");
        let copy = crate::PageDoc::open(nb.page_path(ids[0].clone())).unwrap();
        assert_eq!(
            (copy.template(), copy.page_size()),
            ("grid".to_string(), "a5".to_string())
        );
        assert_eq!(copy.text_blocks()[0].text, "Objectifs");
        assert_eq!(nb.title(), "2nde B");
        // A notebook with content keeps it: the template is appended.
        assert_eq!(nb.copy_from_template(tpl.clone()).unwrap(), 2);
        assert_eq!(nb.page_ids().len(), 4);
        assert_eq!(nb.copy_from_template(nb.clone()).unwrap(), 0);
        std::fs::remove_dir_all(tdir).ok();
        std::fs::remove_dir_all(ndir).ok();
    }

    #[test]
    fn fresh_notebook_has_one_page_and_persists_edits() {
        let dir = tmp("fresh");
        let d = dir.to_string_lossy().to_string();
        let first;
        {
            let nb = NotebookDoc::open(d.clone(), "Maths".into()).unwrap();
            assert_eq!(nb.page_ids().len(), 1);
            first = nb.page_ids()[0].clone();
            let second = nb.add_page(Some(first.clone()));
            let third = nb.add_page(Some(first.clone()));
            assert_eq!(nb.page_ids(), [first.clone(), third.clone(), second.clone()]);
            let node = nb
                .outline()
                .iter()
                .find(|e| e.page_id.as_deref() == Some(&second))
                .unwrap()
                .node
                .clone();
            nb.delete_node(node.clone());
            assert_eq!(nb.trash().len(), 1);
            assert!(nb.page_path(first.clone()).ends_with(".kpage"));
            // No save(): edits must already be durable.
        }
        let nb = NotebookDoc::open(d, "ignored".into()).unwrap();
        assert_eq!(nb.title(), "Maths");
        assert_eq!(nb.page_ids().len(), 2);
        assert_eq!(nb.page_ids()[0], first);
        assert!(!nb.undo(), "undo history is per session");
        nb.set_title("Maths 1re".into());
        assert_eq!(nb.title(), "Maths 1re");
        assert!(nb.undo(), "renaming is undoable");
        assert_eq!(nb.title(), "Maths");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn legacy_page_files_are_adopted_in_order() {
        let dir = tmp("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        for n in [2, 1, 10] {
            std::fs::write(dir.join(format!("page-{n:04}.json")), r#"{"id":"x","items":[]}"#).unwrap();
        }
        let nb = NotebookDoc::open(dir.to_string_lossy().into(), "N".into()).unwrap();
        assert_eq!(nb.page_ids(), ["page-0001", "page-0002", "page-0010"]);
        let p = nb.page_path("page-0002".into());
        let page = crate::PageDoc::open(p).unwrap();
        assert_eq!(page.item_count(), 0, "legacy JSON content migrates on open");
        assert!(!dir.join("page-0002.json").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sections_group_pages_and_can_be_renamed_moved_and_deleted() {
        let dir = tmp("sections");
        let nb = NotebookDoc::open(dir.to_string_lossy().into(), "N".into()).unwrap();
        let a = nb.page_ids()[0].clone();
        let b = nb.add_page(Some(a.clone()));
        let sec = nb.add_section("Algebra".into());
        let node_of = |id: &str| {
            nb.outline()
                .into_iter()
                .find(|e| e.page_id.as_deref() == Some(id))
                .unwrap()
        };
        // The app moves a page to the end of a section with index u32::MAX.
        assert!(nb.move_node(node_of(&b).node, Some(sec.clone()), u32::MAX));
        let o = nb.outline();
        assert_eq!(o.len(), 3);
        assert_eq!(
            (o[1].kind, o[1].title.as_str(), o[1].depth),
            (OutlineKind::Section, "Algebra", 0)
        );
        assert_eq!((o[2].page_id.as_deref(), o[2].depth), (Some(b.as_str()), 1));
        // A page is not a valid parent.
        assert!(!nb.move_node(node_of(&a).node, Some(node_of(&b).node), 0));
        nb.rename(node_of(&b).node, "Équations".into());
        assert_eq!(node_of(&b).title, "Équations");
        // Back to the top level, then deleting the section leaves the pages.
        assert!(nb.move_node(node_of(&b).node, None, u32::MAX));
        nb.delete_node(sec.clone());
        assert_eq!(nb.page_ids(), [a.clone(), b.clone()]);
        assert_eq!(nb.trash().len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn trash_older_than_the_limit_is_purged_with_its_files() {
        let dir = tmp("purge");
        let nb = NotebookDoc::open(dir.to_string_lossy().into(), "N".into()).unwrap();
        let a = nb.page_ids()[0].clone();
        let b = nb.add_page(Some(a.clone()));
        let c = nb.add_page(Some(b.clone()));
        let node = |id: &str| {
            nb.outline()
                .into_iter()
                .find(|e| e.page_id.as_deref() == Some(id))
                .unwrap()
                .node
        };
        let path = nb.page_path(b.clone());
        crate::PageDoc::open(path.clone()).unwrap();
        nb.delete_node(node(&b));
        assert_eq!(nb.purge_expired(30), 0, "deleted just now: kept");
        assert_eq!(nb.trash().len(), 1);
        assert_eq!(nb.purge_expired(0), 1, "older than 0 days: purged");
        assert!(nb.trash().is_empty() && !std::path::Path::new(&path).exists());
        // Drag and drop between pages.
        assert!(nb.move_next_to(node(&c), node(&a), false));
        assert_eq!(nb.page_ids(), [c.clone(), a.clone()]);
        assert!(nb.move_next_to(node(&c), node(&a), true));
        assert_eq!(nb.page_ids(), [a.clone(), c.clone()]);
        assert!(!nb.move_next_to("bad".into(), node(&a), true));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn pages_shift_up_and_down() {
        let dir = tmp("shift");
        let nb = NotebookDoc::open(dir.to_string_lossy().into(), "N".into()).unwrap();
        let a = nb.page_ids()[0].clone();
        let b = nb.add_page(Some(a.clone()));
        let c = nb.add_page(Some(b.clone()));
        let node = |id: &str| {
            nb.outline()
                .into_iter()
                .find(|e| e.page_id.as_deref() == Some(id))
                .unwrap()
                .node
        };
        assert!(nb.shift_node(node(&c), -1));
        assert_eq!(nb.page_ids(), [a.clone(), c.clone(), b.clone()]);
        assert!(nb.shift_node(node(&a), 1));
        assert_eq!(nb.page_ids(), [c.clone(), a.clone(), b.clone()]);
        assert!(nb.shift_node(node(&a), 1));
        assert_eq!(nb.page_ids(), [c.clone(), b.clone(), a.clone()]);
        assert!(!nb.shift_node(node(&a), 1), "already last");
        assert!(!nb.shift_node(node(&c), -1), "already first");
        // A trashed page in between is stepped over.
        nb.delete_node(node(&b));
        assert!(nb.shift_node(node(&a), -1));
        assert_eq!(nb.page_ids(), [a.clone(), c.clone()]);
        // Emptying the trash removes the page and its file for good.
        let path = nb.page_path(b.clone());
        crate::PageDoc::open(path.clone()).unwrap();
        assert!(std::path::Path::new(&path).exists());
        assert_eq!(nb.empty_trash(), 1);
        assert!(nb.trash().is_empty() && !std::path::Path::new(&path).exists());
        assert_eq!(nb.page_ids(), [a.clone(), c.clone()]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
