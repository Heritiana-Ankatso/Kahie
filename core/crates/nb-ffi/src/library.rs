//! `LibraryDoc`: folders holding the notebooks of one notebooks root directory (P1-01/P1-02).
//!
//! The library lives in `<root>/library.db` (SQLite log of [`CrdtLibrary`]); each notebook
//! is a sub-directory of `<root>` (see [`crate::NotebookDoc`]). Directories that appear
//! without a library entry (new, imported, joined) are added at the top level when the
//! library is opened or [`LibraryDoc::rescan`] is called; entries whose directory is gone
//! are dropped.

use crate::notebook::now_secs;
use crate::{peer_id, CoreError};
use loro::TreeID;
use nb_model::library::{CrdtLibrary, LibKind, LibNode};
use nb_model::notebook::CrdtNotebook;
use nb_store::Store;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibraryKind {
    Folder,
    Notebook,
}

/// One folder or notebook of the library.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct LibraryEntry {
    /// Opaque node id (use with `rename`, `move_to`, `delete_node`, `restore_node`, `list`).
    pub node: String,
    pub kind: LibraryKind,
    /// Folder name, or the notebook's title as last seen (may be empty: untitled).
    pub title: String,
    /// For notebooks: the directory name under the root.
    pub dir: Option<String>,
    /// Last opened on this device (seconds since 1970), 0 if never.
    pub opened_secs: i64,
    /// When it was put in the trash (seconds since 1970), for trash entries.
    pub deleted_secs: Option<i64>,
    /// Depth below the top level (for `folders`).
    pub depth: u32,
    /// For folders: number of visible folders and notebooks directly inside.
    pub count: u32,
}

struct State {
    lib: CrdtLibrary,
    store: Store,
    root: PathBuf,
    error: Option<String>,
}

impl State {
    fn persist(&mut self) {
        let Some(update) = self.lib.take_unsaved() else { return };
        let r = self.store.append_update(&update).and_then(|_| {
            if self.store.pending_updates()? >= 200 {
                self.store.compact(&self.lib.snapshot())?;
            }
            Ok(())
        });
        if let Err(e) = r {
            self.error.get_or_insert(e.to_string());
        }
    }

    fn node(&self, id: &str) -> Option<TreeID> {
        TreeID::try_from(id).ok()
    }

    fn entry(&self, n: LibNode) -> LibraryEntry {
        let count = if n.kind == LibKind::Folder {
            self.lib.children(Some(n.id)).len() as u32
        } else {
            0
        };
        LibraryEntry {
            node: n.id.to_string(),
            kind: match n.kind {
                LibKind::Folder => LibraryKind::Folder,
                LibKind::Notebook => LibraryKind::Notebook,
            },
            title: n.title,
            dir: n.dir,
            opened_secs: n.opened,
            deleted_secs: n.deleted_at,
            depth: n.depth as u32,
            count,
        }
    }

    /// Add entries for new notebook directories, drop entries whose directory is gone.
    fn rescan(&mut self) -> Result<(), CoreError> {
        let dirs: Vec<String> = std::fs::read_dir(&self.root)
            .map_err(|e| CoreError::Io { detail: e.to_string() })?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().to_str().map(str::to_string))
            .collect();
        let known = self.lib.all();
        for n in &known {
            if let Some(d) = &n.dir {
                if !dirs.contains(d) {
                    self.lib.purge(n.id);
                }
            }
        }
        let mut new: Vec<&String> = dirs
            .iter()
            .filter(|d| !known.iter().any(|n| n.dir.as_ref() == Some(*d)))
            .collect();
        new.sort();
        for d in new {
            let title = read_title(&self.root.join(d)).unwrap_or_default();
            self.lib.add_notebook(None, d, &title);
        }
        Ok(())
    }

    /// Remove a trashed node for good with its notebooks' directories; returns how many
    /// notebooks were removed.
    fn remove_for_good(&mut self, node: TreeID) -> u32 {
        let dirs = self.lib.dirs_under(node);
        for d in &dirs {
            if plain_name(d) {
                let _ = std::fs::remove_dir_all(self.root.join(d));
            }
        }
        self.lib.purge(node);
        dirs.len() as u32
    }
}

/// A directory name without path separators or `..` (never escape the root).
fn plain_name(d: &str) -> bool {
    !d.is_empty() && d != "." && d != ".." && !d.contains(['/', '\\'])
}

/// Title of the notebook in `dir`, read without modifying it (`None` if it has no outline yet).
pub(crate) fn read_title(dir: &Path) -> Option<String> {
    let db = dir.join("notebook.db");
    if !db.exists() {
        return None;
    }
    let store = Store::open(&db).ok()?;
    let loaded = store.load().ok()?;
    let mut chunks: Vec<&[u8]> = loaded.snapshot.iter().map(Vec::as_slice).collect();
    chunks.extend(loaded.updates.iter().map(Vec::as_slice));
    CrdtNotebook::load(&chunks, 1).ok().map(|nb| nb.title())
}

#[derive(uniffi::Object)]
pub struct LibraryDoc {
    state: Mutex<State>,
}

#[uniffi::export]
impl LibraryDoc {
    /// Open (or create) the library of the notebooks under directory `root`.
    #[uniffi::constructor]
    pub fn open(root: String) -> Result<std::sync::Arc<Self>, CoreError> {
        let root = PathBuf::from(root);
        std::fs::create_dir_all(&root).map_err(|e| CoreError::Io { detail: e.to_string() })?;
        let fmt = |e: &dyn std::fmt::Display| CoreError::Format { detail: e.to_string() };
        let store = Store::open(root.join("library.db")).map_err(|e| fmt(&e))?;
        let loaded = store.load().map_err(|e| fmt(&e))?;
        let mut chunks: Vec<&[u8]> = loaded.snapshot.iter().map(Vec::as_slice).collect();
        chunks.extend(loaded.updates.iter().map(Vec::as_slice));
        let lib = if chunks.is_empty() {
            CrdtLibrary::new(peer_id())
        } else {
            CrdtLibrary::load(&chunks, peer_id()).map_err(|e| fmt(&e))?
        };
        let mut st = State {
            lib,
            store,
            root,
            error: None,
        };
        st.rescan()?;
        st.persist();
        Ok(std::sync::Arc::new(Self { state: Mutex::new(st) }))
    }
}

#[uniffi::export]
impl LibraryDoc {
    /// Pick up notebook directories created, imported or removed since the library was opened.
    pub fn rescan(&self) -> Result<(), CoreError> {
        let mut st = self.state.lock().unwrap();
        st.rescan()?;
        st.persist();
        Ok(())
    }

    /// Re-read every notebook's title from disk (titles can change through sync).
    pub fn refresh_titles(&self) {
        let mut st = self.state.lock().unwrap();
        for n in st.lib.all() {
            let Some(d) = &n.dir else { continue };
            if let Some(t) = read_title(&st.root.join(d)) {
                st.lib.rename(n.id, &t);
            }
        }
        st.persist();
    }

    /// Visible content of `folder` (top level when `None`): folders first, then notebooks,
    /// most recently opened first.
    pub fn list(&self, folder: Option<String>) -> Vec<LibraryEntry> {
        let st = self.state.lock().unwrap();
        let parent = folder.and_then(|f| st.node(&f));
        st.lib.children(parent).into_iter().map(|n| st.entry(n)).collect()
    }

    /// Every visible notebook, in any folder, most recently opened first.
    pub fn recent(&self) -> Vec<LibraryEntry> {
        let st = self.state.lock().unwrap();
        st.lib.recent().into_iter().map(|n| st.entry(n)).collect()
    }

    /// Every visible folder, depth-first (choices for "move to folder").
    pub fn folders(&self) -> Vec<LibraryEntry> {
        let st = self.state.lock().unwrap();
        st.lib.folders().into_iter().map(|n| st.entry(n)).collect()
    }

    /// Folders from the top level down to `node` (excluded), for a breadcrumb.
    pub fn path(&self, node: String) -> Vec<LibraryEntry> {
        let st = self.state.lock().unwrap();
        let Some(n) = st.node(&node) else { return vec![] };
        st.lib.ancestors(n).into_iter().map(|n| st.entry(n)).collect()
    }

    /// Folder holding the notebook in `dir` (`None` at the top level or if unknown).
    pub fn folder_of(&self, dir: String) -> Option<String> {
        let st = self.state.lock().unwrap();
        let n = st.lib.find_notebook(&dir)?;
        st.lib.ancestors(n).last().map(|f| f.id.to_string())
    }

    /// New folder at the end of `parent` (top level when `None`). Returns its node id.
    pub fn add_folder(&self, parent: Option<String>, title: String) -> Option<String> {
        let mut st = self.state.lock().unwrap();
        let p = parent.and_then(|p| st.node(&p));
        let id = st.lib.add_folder(p, &title)?;
        st.persist();
        Some(id.to_string())
    }

    /// Record that the notebook in `dir` (title `title`) was opened now, adding it to the
    /// library in `folder` if it is not there yet.
    pub fn note_opened(&self, dir: String, title: String, folder: Option<String>) {
        let mut st = self.state.lock().unwrap();
        let n = match st.lib.find_notebook(&dir) {
            Some(n) => n,
            None => {
                let parent = folder.and_then(|f| st.node(&f));
                let added = st.lib.add_notebook(parent, &dir, &title);
                match added.or_else(|| st.lib.add_notebook(None, &dir, &title)) {
                    Some(n) => n,
                    None => return,
                }
            }
        };
        st.lib.rename(n, &title);
        st.lib.touch(n, now_secs());
        st.persist();
    }

    /// Update the title shown for the notebook in `dir` (after renaming it).
    pub fn set_notebook_title(&self, dir: String, title: String) {
        let mut st = self.state.lock().unwrap();
        if let Some(n) = st.lib.find_notebook(&dir) {
            st.lib.rename(n, &title);
            st.persist();
        }
    }

    /// Rename a folder.
    pub fn rename(&self, node: String, title: String) {
        let mut st = self.state.lock().unwrap();
        if let Some(n) = st.node(&node) {
            st.lib.rename(n, &title);
            st.persist();
        }
    }

    /// Move a folder or notebook into `folder` (top level when `None`). False for a cycle.
    pub fn move_to(&self, node: String, folder: Option<String>) -> bool {
        let mut st = self.state.lock().unwrap();
        let Some(n) = st.node(&node) else { return false };
        let p = match folder {
            Some(f) => match st.node(&f) {
                Some(f) => Some(f),
                None => return false,
            },
            None => None,
        };
        let ok = st.lib.move_node(n, p);
        st.persist();
        ok
    }

    /// Node id of the notebook in `dir`.
    pub fn node_of(&self, dir: String) -> Option<String> {
        self.state
            .lock()
            .unwrap()
            .lib
            .find_notebook(&dir)
            .map(|n| n.to_string())
    }

    /// Whether the notebook in `dir` is in the library and not in the trash.
    pub fn is_visible(&self, dir: String) -> bool {
        let st = self.state.lock().unwrap();
        st.lib.find_notebook(&dir).is_some_and(|n| st.lib.is_visible(n))
    }

    /// Put a folder (with its content) or a notebook in the trash.
    pub fn delete_node(&self, node: String) {
        let mut st = self.state.lock().unwrap();
        if let Some(n) = st.node(&node) {
            st.lib.delete(n, now_secs());
            st.persist();
        }
    }

    pub fn restore_node(&self, node: String) {
        let mut st = self.state.lock().unwrap();
        if let Some(n) = st.node(&node) {
            st.lib.restore(n);
            st.persist();
        }
    }

    /// Trashed folders and notebooks, most recently deleted first.
    pub fn trash(&self) -> Vec<LibraryEntry> {
        let st = self.state.lock().unwrap();
        st.lib.trash().into_iter().map(|n| st.entry(n)).collect()
    }

    /// Delete everything in the trash for good, with the notebooks' files. Returns the number
    /// of notebooks removed.
    pub fn empty_trash(&self) -> u32 {
        let mut st = self.state.lock().unwrap();
        let trash = st.lib.trash();
        let removed = trash.iter().map(|n| st.remove_for_good(n.id)).sum();
        st.persist();
        removed
    }

    /// Delete for good what has been in the trash for `max_age_days` days or more (P1-04).
    /// Returns the number of notebooks removed.
    pub fn purge_expired(&self, max_age_days: u32) -> u32 {
        let mut st = self.state.lock().unwrap();
        let expired = st.lib.expired(now_secs() - i64::from(max_age_days) * 86_400);
        let removed = expired.iter().map(|n| st.remove_for_good(n.id)).sum();
        st.persist();
        removed
    }

    /// Compact the log and report any earlier write failure.
    pub fn save(&self) -> Result<(), CoreError> {
        let mut st = self.state.lock().unwrap();
        st.persist();
        if let Some(e) = st.error.take() {
            return Err(CoreError::Io { detail: e });
        }
        let snap = st.lib.snapshot();
        st.store
            .compact(&snap)
            .map_err(|e| CoreError::Io { detail: e.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NotebookDoc;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kahie-lib-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn library_adopts_notebooks_and_files_them_in_folders() {
        let root = tmp("adopt");
        let r = |d: &str| root.join(d).to_string_lossy().to_string();
        NotebookDoc::open(r("nb-0001"), "Maths 2nde".into()).unwrap();
        NotebookDoc::open(r("nb-0002"), "Physique".into()).unwrap();
        let lib = LibraryDoc::open(root.to_string_lossy().into()).unwrap();
        let titles = |v: Vec<LibraryEntry>| v.into_iter().map(|e| e.title).collect::<Vec<_>>();
        assert_eq!(titles(lib.list(None)), ["Maths 2nde", "Physique"]);
        // Recent first.
        lib.note_opened("nb-0002".into(), "Physique".into(), None);
        assert_eq!(titles(lib.list(None)), ["Physique", "Maths 2nde"]);
        // A folder; a notebook moved into it; the folder lists first.
        let f = lib.add_folder(None, "Lycée".into()).unwrap();
        let n1 = lib.node_of("nb-0001".into()).unwrap();
        assert!(lib.move_to(n1.clone(), Some(f.clone())));
        assert_eq!(titles(lib.list(None)), ["Lycée", "Physique"]);
        assert_eq!(lib.list(None)[0].count, 1);
        assert_eq!(titles(lib.list(Some(f.clone()))), ["Maths 2nde"]);
        assert_eq!(lib.folder_of("nb-0001".into()), Some(f.clone()));
        assert_eq!(titles(lib.path(n1.clone())), ["Lycée"]);
        assert!(!lib.move_to(f.clone(), Some(f.clone())), "no cycles");
        // A new notebook created in a folder is filed there when first opened.
        NotebookDoc::open(r("nb-0003"), "Chimie".into()).unwrap();
        lib.note_opened("nb-0003".into(), "Chimie".into(), Some(f.clone()));
        assert_eq!(titles(lib.list(Some(f.clone()))), ["Chimie", "Maths 2nde"]);
        assert_eq!(titles(lib.recent()), ["Chimie", "Physique", "Maths 2nde"]);
        // Titles changed elsewhere (sync) are picked up.
        NotebookDoc::open(r("nb-0002"), String::new())
            .unwrap()
            .set_title("Physique-chimie".into());
        lib.refresh_titles();
        assert!(titles(lib.list(None)).contains(&"Physique-chimie".to_string()));
        // Persisted.
        drop(lib);
        let lib = LibraryDoc::open(root.to_string_lossy().into()).unwrap();
        assert_eq!(titles(lib.list(Some(f.clone()))), ["Chimie", "Maths 2nde"]);
        // Trash: deleting the folder hides its notebooks; restore; empty removes the files.
        lib.delete_node(f.clone());
        assert!(!lib.is_visible("nb-0001".into()) && lib.is_visible("nb-0002".into()));
        assert_eq!(titles(lib.trash()), ["Lycée"]);
        assert_eq!(lib.purge_expired(30), 0, "kept for 30 days");
        lib.restore_node(f.clone());
        assert!(lib.is_visible("nb-0001".into()));
        lib.delete_node(n1);
        assert_eq!(lib.empty_trash(), 1);
        assert!(!root.join("nb-0001").exists() && root.join("nb-0003").exists());
        assert!(lib.trash().is_empty());
        // A directory removed behind the library's back disappears from it.
        std::fs::remove_dir_all(root.join("nb-0003")).unwrap();
        lib.rescan().unwrap();
        assert!(lib.list(Some(f.clone())).is_empty());
        let n2 = lib.node_of("nb-0002".into()).unwrap();
        lib.delete_node(n2);
        assert_eq!(lib.purge_expired(0), 1);
        assert!(!root.join("nb-0002").exists());
        lib.save().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn names_that_escape_the_root_are_never_deleted() {
        assert!(plain_name("nb-0001"));
        for bad in ["", ".", "..", "../x", "a/b", "a\\b"] {
            assert!(!plain_name(bad), "{bad}");
        }
    }
}
