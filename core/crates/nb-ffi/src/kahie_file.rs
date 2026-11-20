//! `.kahie` export and import (docs/format/kahie-v0.md, P1-35).
//!
//! Export writes a ZIP with `manifest.json`, `document.json` (readable without Kahie),
//! `library.crdt` (outline) and `pages/<id>.crdt` (one Loro snapshot per page).
//! Import creates a new notebook directory from such a file.

use crate::notebook::NotebookDoc;
use crate::{peer_id, CoreError, PageDoc};
use nb_model::crdt::CrdtPage;
use nb_model::notebook::CrdtNotebook;
use nb_model::{DocumentJson, NotebookInfo, DOCUMENT_FORMAT, DOCUMENT_VERSION};
use nb_store::Store;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const FORMAT: &str = "kahie";
const VERSION: &str = "0.1.0";

fn io(e: impl std::fmt::Display) -> CoreError {
    CoreError::Io { detail: e.to_string() }
}

fn fmt(e: impl std::fmt::Display) -> CoreError {
    CoreError::Format { detail: e.to_string() }
}

/// Write the notebook stored in `notebook_dir` to `out_path` as a `.kahie` file.
#[uniffi::export]
pub fn export_kahie(notebook_dir: String, out_path: String) -> Result<(), CoreError> {
    let nb = NotebookDoc::open(notebook_dir, String::new())?;
    let (library, title, page_ids) = {
        let st = nb.state.lock().unwrap();
        (st.nb.snapshot(), st.nb.title(), st.nb.page_ids())
    };
    let mut pages = Vec::with_capacity(page_ids.len());
    let mut snapshots = Vec::with_capacity(page_ids.len());
    let mut assets: std::collections::BTreeMap<String, Vec<u8>> = Default::default();
    for id in &page_ids {
        let doc = PageDoc::open(nb.page_path(id.clone()))?;
        let st = doc.state.lock().unwrap();
        let mut page = st.doc.page().clone();
        page.id = id.clone();
        pages.push(page);
        snapshots.push(st.doc.snapshot());
        for h in st.doc.asset_hashes() {
            if let Some(b) = st.doc.asset(&h) {
                assets.entry(h).or_insert(b);
            }
        }
    }
    let manifest = serde_json::json!({
        "format": FORMAT,
        "version": VERSION,
        "created_by": concat!("Kahie core ", env!("CARGO_PKG_VERSION")),
        "notebook": { "id": "notebook", "title": title },
        "pages": page_ids,
        "spec": "https://github.com/Heritiana-Ankatso/Kahie/blob/main/docs/format/kahie-v0.md",
    });
    let document = DocumentJson {
        format: DOCUMENT_FORMAT.into(),
        version: DOCUMENT_VERSION.into(),
        notebook: NotebookInfo {
            id: "notebook".into(),
            title,
        },
        pages,
    };

    // Write next to the target, then rename: a failed export never leaves half a file.
    let out = PathBuf::from(&out_path);
    let tmp = out.with_extension("kahie.tmp");
    {
        let file = std::fs::File::create(&tmp).map_err(io)?;
        let mut z = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let mut put = |name: &str, bytes: &[u8]| -> Result<(), CoreError> {
            z.start_file(name, opts).map_err(io)?;
            z.write_all(bytes).map_err(io)
        };
        put("manifest.json", &serde_json::to_vec_pretty(&manifest).map_err(fmt)?)?;
        put("document.json", &serde_json::to_vec_pretty(&document).map_err(fmt)?)?;
        put("library.crdt", &library)?;
        for (id, snap) in page_ids.iter().zip(&snapshots) {
            put(&format!("pages/{id}.crdt"), snap)?;
        }
        // Images also as plain files, so they are usable without Kahie (format spec §1).
        for (hash, bytes) in &assets {
            put(&format!("assets/{hash}"), bytes)?;
        }
        z.finish().map_err(io)?;
    }
    std::fs::rename(&tmp, &out).map_err(io)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Import a `.kahie` file as a new notebook directory under `notebooks_root`.
/// Returns the new directory.
#[uniffi::export]
pub fn import_kahie(path: String, notebooks_root: String) -> Result<String, CoreError> {
    let file = std::fs::File::open(&path).map_err(io)?;
    let mut z = zip::ZipArchive::new(file).map_err(|e| fmt(format!("not a .kahie file: {e}")))?;
    let mut read = |name: &str| -> Result<Option<Vec<u8>>, CoreError> {
        match z.by_name(name) {
            Ok(mut f) => {
                // Refuse absurd sizes (zip bombs) before allocating.
                if f.size() > 512 << 20 {
                    return Err(fmt(format!("{name} is too large")));
                }
                let mut b = Vec::with_capacity(f.size() as usize);
                f.read_to_end(&mut b).map_err(io)?;
                Ok(Some(b))
            }
            Err(zip::result::ZipError::FileNotFound) => Ok(None),
            Err(e) => Err(fmt(e)),
        }
    };
    let manifest: serde_json::Value =
        serde_json::from_slice(&read("manifest.json")?.ok_or_else(|| fmt("manifest.json missing"))?).map_err(fmt)?;
    if manifest["format"] != FORMAT {
        return Err(fmt("not a Kahie notebook"));
    }
    let version = manifest["version"].as_str().unwrap_or("");
    if !version.starts_with("0.") {
        return Err(fmt(format!("format version {version} is newer than this app supports")));
    }
    let page_ids: Vec<String> = manifest["pages"]
        .as_array()
        .ok_or_else(|| fmt("manifest has no pages"))?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    if page_ids.iter().any(|id| !valid_id(id)) {
        return Err(fmt("invalid page id in manifest"));
    }
    let title = manifest["notebook"]["title"].as_str().unwrap_or("").to_string();

    // Load everything before touching the disk, so a bad file creates nothing.
    let library = match read("library.crdt")? {
        Some(b) => CrdtNotebook::load(&[&b], peer_id()).map_err(fmt)?,
        None => {
            let mut nb = CrdtNotebook::new("notebook", &title, peer_id());
            for id in &page_ids {
                nb.add_page(None, None, id, "");
            }
            nb
        }
    };
    let mut pages = vec![];
    for id in &page_ids {
        let b = read(&format!("pages/{id}.crdt"))?.ok_or_else(|| fmt(format!("pages/{id}.crdt missing")))?;
        pages.push((id.clone(), CrdtPage::from_snapshot(&b, peer_id()).map_err(fmt)?));
    }

    let root = Path::new(&notebooks_root);
    std::fs::create_dir_all(root).map_err(io)?;
    let dir = (1..)
        .map(|n| root.join(format!("imported-{n:04}")))
        .find(|d| !d.exists())
        .expect("free name");
    std::fs::create_dir_all(&dir).map_err(io)?;
    let mut store = Store::open(dir.join("notebook.db")).map_err(fmt)?;
    store.compact(&library.snapshot()).map_err(fmt)?;
    for (id, page) in &pages {
        let mut s = Store::open(dir.join(format!("{id}.kpage"))).map_err(fmt)?;
        s.compact(&page.snapshot()).map_err(fmt)?;
    }
    Ok(dir.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::InkTool;

    #[test]
    fn export_then_import_round_trips_and_document_json_is_readable() {
        let base = std::env::temp_dir().join(format!("kahie-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("src");
        let nb = NotebookDoc::open(src.to_string_lossy().into(), "Géométrie".into()).unwrap();
        let p1 = nb.page_ids()[0].clone();
        let p2 = nb.add_page(Some(p1.clone()));
        for (pid, x) in [(&p1, 0.0), (&p2, 50.0)] {
            let page = PageDoc::open(nb.page_path(pid.clone())).unwrap();
            page.begin_stroke(InkTool::Pen, 0x1d1d1f, 3.0, x, 10.0, 0.5);
            page.add_points(
                (1..40)
                    .flat_map(|k| [x + k as f64, 10.0 + k as f64 * 0.5, 0.5])
                    .collect(),
            );
            page.end_stroke(1.0, false);
        }
        let out = base.join("Géométrie.kahie");
        export_kahie(src.to_string_lossy().into(), out.to_string_lossy().into()).unwrap();

        // document.json is plain JSON matching the published model.
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        let mut s = String::new();
        z.by_name("document.json").unwrap().read_to_string(&mut s).unwrap();
        let doc: DocumentJson = serde_json::from_str(&s).unwrap();
        if let Ok(dump) = std::env::var("KAHIE_DUMP_DOCUMENT_JSON") {
            std::fs::write(dump, &s).unwrap();
        }
        assert_eq!(doc.notebook.title, "Géométrie");
        assert_eq!(doc.pages.len(), 2);
        assert_eq!(doc.pages[1].items.len(), 1);

        let dir = import_kahie(
            out.to_string_lossy().into(),
            base.join("notebooks").to_string_lossy().into(),
        )
        .unwrap();
        let imported = NotebookDoc::open(dir, String::new()).unwrap();
        assert_eq!(imported.title(), "Géométrie");
        assert_eq!(imported.page_ids(), [p1.clone(), p2.clone()]);
        let a = PageDoc::open(nb.page_path(p2.clone())).unwrap();
        let b = PageDoc::open(imported.page_path(p2)).unwrap();
        assert_eq!(a.layout(), b.layout());

        std::fs::write(base.join("bad.kahie"), b"not a zip").unwrap();
        assert!(import_kahie(
            base.join("bad.kahie").to_string_lossy().into(),
            base.join("n2").to_string_lossy().into()
        )
        .is_err());
        assert!(!base.join("n2").exists(), "a bad file creates nothing");
        std::fs::remove_dir_all(base).unwrap();
    }
}
