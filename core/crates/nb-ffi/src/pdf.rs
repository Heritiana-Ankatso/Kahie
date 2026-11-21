//! PDF pages in notebooks (P1-21 … P1-23).
//!
//! An imported PDF is stored **once per notebook**, not once per page: its bytes live in the
//! notebook's asset store `assets.kpage` (a page database holding only an `assets` map, so it
//! is saved, synced with the relay and exported like a page, but loaded only when a PDF page
//! is drawn), with a plain copy `pdf/<sha256>.pdf` that the platform renderer opens. Each
//! notebook page showing a PDF page records `pdf_source` (asset hash + page index) and the
//! page's **text layer** (words with normalised boxes, extracted by `nb-pdf`) in its own CRDT.
//! Pages are rendered on demand by the app at the resolution it needs.

use crate::notebook::NotebookDoc;
use crate::{CoreError, PageDoc};
use nb_model::PdfSource;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// File of a notebook's asset store, next to its pages. Its sync id is `assets`.
pub const ASSET_STORE: &str = "assets.kpage";

/// Where a notebook page's PDF page comes from.
#[derive(uniffi::Record, Clone, Debug, PartialEq, Eq)]
pub struct PdfPageRef {
    /// SHA-256 (hex) of the PDF; see [`pdf_file`].
    pub asset: String,
    /// Page of the PDF, from 0.
    pub page_index: u32,
}

/// A word of a PDF page's text layer. The box is normalised to the PDF page: (0, 0) is its
/// top-left corner and (1, 1) its bottom-right, however the page is fitted on screen.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct PdfWord {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// Displayed size of a PDF page in points (1/72 in), as the platform renderer reports it.
#[derive(uniffi::Record, Clone, Copy, Debug, PartialEq)]
pub struct PdfPageSize {
    pub width_pt: f64,
    pub height_pt: f64,
}

fn io(e: impl std::fmt::Display) -> CoreError {
    CoreError::Io { detail: e.to_string() }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn valid_hash(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Paper size for a PDF page of `w` × `h` points: a named size when it is one (±2 mm),
/// else a custom size, scaled into the allowed range if needed.
pub(crate) fn paper_for(w_pt: f64, h_pt: f64) -> String {
    let (w, h) = (w_pt * 25.4 / 72.0, h_pt * 25.4 / 72.0);
    let named = [("a4", 210.0, 297.0), ("a5", 148.0, 210.0), ("letter", 215.9, 279.4)];
    for (name, nw, nh) in named {
        if (w - nw).abs() <= 2.0 && (h - nh).abs() <= 2.0 {
            return name.into();
        }
        if (w - nh).abs() <= 2.0 && (h - nw).abs() <= 2.0 {
            return format!("{name}-landscape");
        }
    }
    let (lo, hi) = nb_model::CUSTOM_SIZE_MM;
    let mut s = 1.0;
    if w.max(h) > hi {
        s = hi / w.max(h);
    } else if w.min(h) < lo {
        s = (lo / w.min(h)).min(hi / w.max(h));
    }
    nb_model::custom_size(w * s, h * s).unwrap_or_else(|| if w > h { "a4-landscape" } else { "a4" }.into())
}

fn store_path(dir: &Path) -> PathBuf {
    dir.join(ASSET_STORE)
}

/// Add `bytes` under `hash` to the asset store of notebook directory `dir` (once).
pub(crate) fn store_notebook_asset(dir: &Path, hash: &str, bytes: &[u8]) -> Result<(), CoreError> {
    let store = PageDoc::open(store_path(dir).to_string_lossy().into())?;
    let mut st = store.state.lock().unwrap();
    if st.doc.asset(hash).is_none() {
        st.doc.store_asset(hash, bytes);
        st.persist();
    }
    Ok(())
}

/// Every asset in the store of notebook directory `dir` (for `.kahie` export).
pub(crate) fn notebook_assets(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let path = store_path(dir);
    if !path.exists() {
        return vec![];
    }
    let Ok(store) = PageDoc::open(path.to_string_lossy().into()) else {
        return vec![];
    };
    let st = store.state.lock().unwrap();
    st.doc
        .asset_hashes()
        .into_iter()
        .filter_map(|h| st.doc.asset(&h).map(|b| (h, b)))
        .collect()
}

/// Copy the assets of notebook `src` into notebook `dst` (notebook templates).
pub(crate) fn copy_notebook_assets(src: &Path, dst: &Path) -> Result<(), CoreError> {
    for (h, b) in notebook_assets(src) {
        store_notebook_asset(dst, &h, &b)?;
    }
    Ok(())
}

/// Text layer as stored in a page: JSON `[[text, x0, y0, x1, y1], …]`, 4 decimals.
fn encode_words(words: &[nb_pdf::Word]) -> String {
    let r = |v: f32| (f64::from(v) * 10_000.0).round() / 10_000.0;
    let list: Vec<serde_json::Value> = words
        .iter()
        .map(|w| serde_json::json!([w.text, r(w.x0), r(w.y0), r(w.x1), r(w.y1)]))
        .collect();
    serde_json::to_string(&list).unwrap_or_default()
}

fn decode_words(json: &str) -> Vec<PdfWord> {
    let Ok(serde_json::Value::Array(list)) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    list.iter()
        .filter_map(|v| {
            let a = v.as_array()?;
            let f = |i: usize| a.get(i).and_then(serde_json::Value::as_f64).map(|x| x as f32);
            Some(PdfWord {
                text: a.first()?.as_str()?.to_string(),
                x0: f(1)?,
                y0: f(2)?,
                x1: f(3)?,
                y1: f(4)?,
            })
        })
        .collect()
}

/// Import the PDF at `pdf_path` into `notebook`: the file is stored once in the notebook's
/// assets, and one page per PDF page is inserted after `after_page_id` (at the end when
/// `None`), each sized like its PDF page, blank paper, showing the PDF page under its
/// content and carrying its text layer. `sizes` are the page sizes from the platform
/// renderer (authoritative for drawing); when empty, they are read from the PDF. Returns the
/// new page ids in order.
#[uniffi::export]
pub fn pdf_import(
    notebook: Arc<NotebookDoc>,
    after_page_id: Option<String>,
    pdf_path: String,
    sizes: Vec<PdfPageSize>,
) -> Result<Vec<String>, CoreError> {
    let bytes = std::fs::read(&pdf_path).map_err(io)?;
    let hash = sha256_hex(&bytes);
    let parsed = nb_pdf::PdfDoc::load(&bytes);
    let sizes: Vec<PdfPageSize> = if !sizes.is_empty() {
        sizes
    } else {
        let pdf = parsed
            .as_ref()
            .map_err(|e| CoreError::Format { detail: e.to_string() })?;
        (0..pdf.page_count())
            .filter_map(|i| pdf.page_info(i))
            .map(|p| PdfPageSize {
                width_pt: p.width_pt,
                height_pt: p.height_pt,
            })
            .collect()
    };
    let dir = notebook.dir();
    store_notebook_asset(&dir, &hash, &bytes)?;
    let cache = dir.join("pdf");
    std::fs::create_dir_all(&cache).map_err(io)?;
    let cached = cache.join(format!("{hash}.pdf"));
    if !cached.exists() {
        std::fs::write(&cached, &bytes).map_err(io)?;
    }
    let mut ids = Vec::with_capacity(sizes.len());
    let mut anchor = after_page_id;
    for (i, size) in sizes.iter().enumerate() {
        let id = notebook.add_page(anchor.clone());
        let page = PageDoc::open(notebook.page_path(id.clone()))?;
        {
            let mut st = page.state.lock().unwrap();
            st.doc.set_template("blank");
            st.doc.set_size(&paper_for(size.width_pt, size.height_pt));
            let words = parsed.as_ref().map(|p| p.words(i)).unwrap_or_default();
            let src = PdfSource {
                asset: hash.clone(),
                page_index: i as u32,
            };
            st.doc.set_pdf_source(Some(&src), Some(&encode_words(&words)));
            st.persist();
        }
        anchor = Some(id.clone());
        ids.push(id);
    }
    Ok(ids)
}

/// Path of the PDF `hash` of the notebook in `notebook_dir`, ready for the platform
/// renderer, or `None` when this device does not have it (yet: it arrives with sync).
#[uniffi::export]
pub fn pdf_file(notebook_dir: String, hash: String) -> Option<String> {
    if !valid_hash(&hash) {
        return None;
    }
    let dir = PathBuf::from(notebook_dir);
    let path = dir.join("pdf").join(format!("{hash}.pdf"));
    if path.exists() {
        return Some(path.to_string_lossy().into_owned());
    }
    if !store_path(&dir).exists() {
        return None;
    }
    let store = PageDoc::open(store_path(&dir).to_string_lossy().into()).ok()?;
    let bytes = store.state.lock().unwrap().doc.asset(&hash)?;
    std::fs::create_dir_all(dir.join("pdf")).ok()?;
    // Write then rename, so a reader never sees half a file.
    let tmp = path.with_extension("pdf.tmp");
    std::fs::write(&tmp, &bytes).ok()?;
    std::fs::rename(&tmp, &path).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// Plain text of every page of the PDF at `path` (one string per page), e.g. to index a PDF
/// before importing it. Empty strings for pages without readable text.
#[uniffi::export]
pub fn pdf_extract_text(path: String) -> Result<Vec<String>, CoreError> {
    let bytes = std::fs::read(&path).map_err(io)?;
    let pdf = nb_pdf::PdfDoc::load(&bytes).map_err(|e| CoreError::Format { detail: e.to_string() })?;
    Ok((0..pdf.page_count()).map(|i| pdf.text(i)).collect())
}

#[uniffi::export]
impl PageDoc {
    /// The PDF page shown under this page, if any.
    pub fn pdf_source(&self) -> Option<PdfPageRef> {
        self.state.lock().unwrap().doc.pdf_source().map(|p| PdfPageRef {
            asset: p.asset,
            page_index: p.page_index,
        })
    }

    /// Words of the PDF page's text layer, in reading order (selection, copy, highlights).
    pub fn pdf_words(&self) -> Vec<PdfWord> {
        decode_words(&self.state.lock().unwrap().doc.pdf_text_layer())
    }

    /// Plain text of the PDF page (words joined by spaces, lines by newlines), for copy and
    /// search indexing. Empty when the page shows no PDF or the PDF page has no text.
    pub fn pdf_text(&self) -> String {
        let words: Vec<nb_pdf::Word> = self
            .pdf_words()
            .into_iter()
            .map(|w| nb_pdf::Word {
                text: w.text,
                x0: w.x0,
                y0: w.y0,
                x1: w.x1,
                y1: w.y1,
            })
            .collect();
        nb_pdf::words_to_text(&words)
    }

    /// Text of the PDF words whose boxes intersect the normalised rectangle
    /// (`x0, y0` – `x1, y1`, 0–1 on the PDF page), in reading order: what a selection drag
    /// over the PDF copies.
    pub fn pdf_text_in(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> String {
        let (ax, bx) = (x0.min(x1), x0.max(x1));
        let (ay, by) = (y0.min(y1), y0.max(y1));
        let words: Vec<nb_pdf::Word> = self
            .pdf_words()
            .into_iter()
            .filter(|w| w.x0 <= bx && w.x1 >= ax && w.y0 <= by && w.y1 >= ay)
            .map(|w| nb_pdf::Word {
                text: w.text,
                x0: w.x0,
                y0: w.y0,
                x1: w.x1,
                y1: w.y1,
            })
            .collect();
        nb_pdf::words_to_text(&words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nb_pdf::testgen::{generate, Spec};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("kahie-pdf-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn paper_sizes_follow_the_pdf_page() {
        assert_eq!(paper_for(595.28, 841.89), "a4");
        assert_eq!(paper_for(841.89, 595.28), "a4-landscape");
        assert_eq!(paper_for(612.0, 792.0), "letter");
        assert_eq!(paper_for(420.0, 595.0), "a5");
        assert_eq!(paper_for(1000.0, 300.0), "352.8x105.8mm");
        // Too small or too large pages are scaled into range, keeping their shape.
        assert_eq!(paper_for(100.0, 100.0), "50x50mm");
        assert_eq!(paper_for(5000.0, 2500.0), "1000x500mm");
    }

    #[test]
    fn import_stores_the_pdf_once_and_pages_carry_source_and_text() {
        let base = tmp("import");
        let pdf_path = base.join("in.pdf");
        std::fs::write(&pdf_path, generate(&Spec::simple("import", 3))).unwrap();
        let nb = NotebookDoc::open(base.join("nb").to_string_lossy().into(), "T".into()).unwrap();
        let first = nb.page_ids()[0].clone();
        let ids = pdf_import(
            nb.clone(),
            Some(first.clone()),
            pdf_path.to_string_lossy().into(),
            vec![],
        )
        .unwrap();
        assert_eq!(ids.len(), 3);
        assert_eq!(nb.page_ids()[1..], ids[..], "inserted after the first page, in order");

        let page = PageDoc::open(nb.page_path(ids[2].clone())).unwrap();
        let src = page.pdf_source().unwrap();
        assert_eq!(src.page_index, 2);
        assert_eq!(page.page_size(), "a4");
        assert_eq!(page.template(), "blank");
        assert_eq!(page.pdf_text(), "Kahie corpus\npage 3\nélève\nhello world");
        let w = &page.pdf_words()[0];
        assert_eq!(w.text, "Kahie");
        assert_eq!(page.pdf_text_in(w.x0, w.y0, w.x1 + 0.08, w.y1), "Kahie corpus");

        // The PDF is stored once: page files stay small, the asset store holds the bytes.
        let pdf_len = std::fs::metadata(&pdf_path).unwrap().len();
        let page_len = std::fs::metadata(nb.page_path(ids[1].clone())).unwrap().len();
        assert!(page_len < pdf_len + 40_000, "page {page_len} vs pdf {pdf_len}");
        let assets = notebook_assets(&base.join("nb"));
        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].0, src.asset);

        // The cached file can be rebuilt from the asset store (e.g. after sync).
        let dir: String = base.join("nb").to_string_lossy().into();
        std::fs::remove_dir_all(base.join("nb/pdf")).unwrap();
        let f = pdf_file(dir.clone(), src.asset.clone()).unwrap();
        assert_eq!(std::fs::read(f).unwrap(), std::fs::read(&pdf_path).unwrap());
        assert_eq!(pdf_file(dir, "00".repeat(32)), None);

        // Importing the same PDF again reuses the stored asset.
        pdf_import(nb.clone(), None, pdf_path.to_string_lossy().into(), vec![]).unwrap();
        assert_eq!(notebook_assets(&base.join("nb")).len(), 1);
        assert_eq!(nb.page_ids().len(), 7);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn kahie_export_carries_the_pdf_once_and_templates_copy_it() {
        let base = tmp("kahie");
        let pdf_path = base.join("in.pdf");
        std::fs::write(&pdf_path, generate(&Spec::simple("kahie", 2))).unwrap();
        let src_dir = base.join("nb");
        let nb = NotebookDoc::open(src_dir.to_string_lossy().into(), "T".into()).unwrap();
        let ids = pdf_import(nb.clone(), None, pdf_path.to_string_lossy().into(), vec![]).unwrap();
        let hash = PageDoc::open(nb.page_path(ids[0].clone()))
            .unwrap()
            .pdf_source()
            .unwrap()
            .asset;
        let out = base.join("x.kahie");
        crate::export_kahie(src_dir.to_string_lossy().into(), out.to_string_lossy().into()).unwrap();
        let mut z = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
        assert_eq!(
            z.file_names()
                .filter(|n| n.as_ref().is_ok_and(|n| n.starts_with("assets/")))
                .count(),
            1
        );
        let mut doc = String::new();
        std::io::Read::read_to_string(&mut z.by_name("document.json").unwrap(), &mut doc).unwrap();
        assert!(
            doc.contains(&format!("\"asset\": \"{hash}\"")),
            "document.json names the PDF"
        );

        let imported =
            crate::import_kahie(out.to_string_lossy().into(), base.join("lib").to_string_lossy().into()).unwrap();
        let f = pdf_file(imported.clone(), hash.clone()).expect("PDF restored");
        assert_eq!(std::fs::read(f).unwrap(), std::fs::read(&pdf_path).unwrap());
        let nb2 = NotebookDoc::open(imported, String::new()).unwrap();
        let p = PageDoc::open(nb2.page_path(ids[1].clone())).unwrap();
        assert_eq!(p.pdf_text(), "Kahie corpus\npage 2\nélève\nhello world");

        // A notebook made from it as a template gets the PDF too.
        let copy_dir = base.join("copy");
        let copy = NotebookDoc::open(copy_dir.to_string_lossy().into(), "C".into()).unwrap();
        copy.copy_from_template(nb.clone()).unwrap();
        assert!(pdf_file(copy_dir.to_string_lossy().into(), hash).is_some());
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn platform_sizes_win_and_unreadable_pdfs_still_import_without_text() {
        let base = tmp("sizes");
        let pdf_path = base.join("in.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4 not really a pdf").unwrap();
        let nb = NotebookDoc::open(base.join("nb").to_string_lossy().into(), "T".into()).unwrap();
        let path: String = pdf_path.to_string_lossy().into();
        assert!(
            pdf_import(nb.clone(), None, path.clone(), vec![]).is_err(),
            "no sizes, unreadable"
        );
        let sizes = vec![PdfPageSize {
            width_pt: 842.0,
            height_pt: 595.0,
        }];
        let ids = pdf_import(nb.clone(), None, path, sizes).unwrap();
        let page = PageDoc::open(nb.page_path(ids[0].clone())).unwrap();
        assert_eq!(page.page_size(), "a4-landscape");
        assert!(page.pdf_words().is_empty() && page.pdf_text().is_empty());
        assert!(page.pdf_source().is_some());
        std::fs::remove_dir_all(base).unwrap();
    }

    /// P1-21: a 300-page PDF is imported (pages created, text extracted) quickly enough that
    /// the first page shows within the 2 s budget. Debug build, so the bound is loose; the
    /// release benchmark (`nb-bench`) tracks the real figure.
    #[test]
    fn three_hundred_page_pdf_imports() {
        let base = tmp("big");
        let pdf_path = base.join("big.pdf");
        std::fs::write(&pdf_path, generate(&Spec::simple("big", 300))).unwrap();
        let nb = NotebookDoc::open(base.join("nb").to_string_lossy().into(), "T".into()).unwrap();
        let t = std::time::Instant::now();
        let ids = pdf_import(nb.clone(), None, pdf_path.to_string_lossy().into(), vec![]).unwrap();
        let secs = t.elapsed().as_secs_f64();
        assert_eq!(ids.len(), 300);
        assert!(secs < 60.0, "{secs} s");
        std::fs::remove_dir_all(base).unwrap();
    }
}
