//! Notebook export to Markdown and LaTeX (E2.12).
//!
//! Typed content is exported natively: text as paragraphs, maths as `$$…$$` / `\[…\]`,
//! sections and page titles as headings. Plots become pgfplots figures in LaTeX (vector,
//! formulas kept) and pictures in Markdown. Handwriting and page backgrounds are exported as
//! one picture per page ("drawing"), which the app renders; inserted images are exported
//! as they are. The app writes `text` plus the listed pictures into one folder or ZIP.

use crate::notebook::NotebookDoc;
use crate::CoreError;
use nb_math::plot::{tex_escape, to_pgfplots, Window};
use nb_model::notebook::NodeKind;
use nb_model::{Content, Item, Rect};

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocFormat {
    Markdown,
    Latex,
}

/// A picture the exported text refers to, at `path` (relative, under `assets/`).
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DocImage {
    pub path: String,
    pub page_id: String,
    /// `drawing`: render the page's ink and background (transparent elsewhere), cropped to
    /// `crop` (page units, x, y, w, h); `plot`: render plot `block_id`; `image`: write `bytes`;
    /// `svg`: render the SVG picture of image block `block_id` as a one-page vector PDF
    /// (LaTeX's `\includegraphics` cannot read SVG; Markdown gets the SVG as an `image`).
    pub kind: String,
    pub block_id: String,
    pub crop: Vec<f32>,
    pub bytes: Option<Vec<u8>>,
}

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct DocExport {
    /// The Markdown (`notebook.md`) or LaTeX (`notebook.tex`) source.
    pub text: String,
    pub images: Vec<DocImage>,
}

/// One block of a page in reading order.
enum Block<'a> {
    Drawing(Rect),
    Item(&'a Item),
}

fn union(a: Option<Rect>, b: &Rect) -> Rect {
    match a {
        None => b.clone(),
        Some(a) => {
            let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
            let (x1, y1) = ((a.x + a.w).max(b.x + b.w), (a.y + a.h).max(b.y + b.h));
            Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            }
        }
    }
}

/// Whether `bytes` are an SVG document (text whose root element is `<svg>`, after an
/// optional BOM, XML declaration, comments and doctype). Inserted SVG pictures are stored as
/// their (decompressed) SVG text.
pub(crate) fn is_svg(bytes: &[u8]) -> bool {
    let b = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let head = &b[..b.len().min(4096)];
    let head = match std::str::from_utf8(head) {
        Ok(s) => s,
        // A multi-byte character cut at the 4096-byte limit.
        Err(e) => std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or(""),
    };
    let mut s = head.trim_start();
    loop {
        let skip = if s.starts_with("<?") {
            s.find("?>").map(|i| i + 2)
        } else if s.starts_with("<!--") {
            s.find("-->").map(|i| i + 3)
        } else if s.starts_with("<!") {
            s.find('>').map(|i| i + 1)
        } else {
            break;
        };
        match skip {
            Some(i) => s = s[i..].trim_start(),
            None => return false,
        }
    }
    s.strip_prefix("<svg")
        .and_then(|r| r.chars().next())
        .is_some_and(|c| c.is_whitespace() || c == '>' || c == '/')
}

fn ext(bytes: &[u8]) -> &'static str {
    if is_svg(bytes) {
        "svg"
    } else if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else {
        "bin"
    }
}

/// Markdown text: escape characters that would start formatting.
fn md_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if matches!(c, '\\' | '*' | '_' | '`' | '#' | '[' | ']' | '<' | '>' | '|') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

const PREAMBLE: &str = "\\documentclass[11pt,a4paper]{article}
\\usepackage[utf8]{inputenc}
\\usepackage[margin=2cm]{geometry}
\\usepackage{amsmath,amssymb}
\\usepackage{graphicx}
\\usepackage{pgfplots}
\\pgfplotsset{compat=1.16, trig format plots=rad}
\\setlength{\\parindent}{0pt}
\\setlength{\\parskip}{0.6em}
";

pub(crate) struct PageContent {
    pub id: String,
    pub title: String,
    pub depth: usize,
    pub width: f64,
    pub items: Vec<Item>,
    pub assets: Vec<(String, Vec<u8>)>,
    pub has_background: bool,
}

pub(crate) enum Entry {
    Section { title: String, depth: usize },
    Page(Box<PageContent>),
}

pub(crate) fn build(title: &str, entries: &[Entry], format: DocFormat) -> DocExport {
    let tex = format == DocFormat::Latex;
    let mut out = String::new();
    let mut images = vec![];
    if tex {
        out.push_str(PREAMBLE);
        out.push_str(&format!(
            "\\title{{{}}}\n\\date{{}}\n\\begin{{document}}\n\\maketitle\n\n",
            tex_escape(title)
        ));
    } else {
        out.push_str(&format!("# {}\n\n", md_escape(title)));
    }
    let mut page_no = 0;
    for e in entries {
        match e {
            Entry::Section { title, depth } => {
                if tex {
                    let cmd = if *depth == 0 { "section" } else { "subsection" };
                    out.push_str(&format!("\\{cmd}{{{}}}\n\n", tex_escape(title)));
                } else {
                    out.push_str(&format!("{} {}\n\n", "#".repeat(depth + 2), md_escape(title)));
                }
            }
            Entry::Page(p) => {
                page_no += 1;
                let heading = if p.title.is_empty() {
                    format!("Page {page_no}")
                } else {
                    p.title.clone()
                };
                if tex {
                    let cmd = if p.depth == 0 { "subsection*" } else { "subsubsection*" };
                    out.push_str(&format!("\\{cmd}{{{}}}\n\n", tex_escape(&heading)));
                } else {
                    out.push_str(&format!(
                        "{} {}\n\n",
                        "#".repeat((p.depth + 2).min(6)),
                        md_escape(&heading)
                    ));
                }
                page(p, tex, &mut out, &mut images);
            }
        }
    }
    if tex {
        out.push_str("\\end{document}\n");
    }
    DocExport { text: out, images }
}

fn page(p: &PageContent, tex: bool, out: &mut String, images: &mut Vec<DocImage>) {
    // Ink (and the page background) as one drawing, placed at its top in reading order.
    let mut drawing: Option<Rect> = None;
    for it in &p.items {
        if let Content::Stroke(_) = it.content {
            drawing = Some(union(drawing, &it.bounds()));
        }
    }
    if p.has_background {
        drawing = Some(Rect {
            x: 0.0,
            y: 0.0,
            w: p.width,
            h: p.width * 1.414,
        });
    }
    let mut blocks: Vec<(f64, f64, Block)> = p
        .items
        .iter()
        .filter(|it| !matches!(it.content, Content::Stroke(_)))
        .map(|it| {
            let b = it.bounds();
            (b.y, b.x, Block::Item(it))
        })
        .collect();
    if let Some(d) = &drawing {
        blocks.push((d.y, d.x, Block::Drawing(d.clone())));
    }
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    // Width on paper as a fraction of the text width.
    let frac = |w: f64| (w / p.width.max(1.0)).clamp(0.1, 1.0);
    for (_, _, b) in blocks {
        match b {
            Block::Drawing(r) => {
                let path = format!("assets/{}-drawing.png", p.id);
                images.push(DocImage {
                    path: path.clone(),
                    page_id: p.id.clone(),
                    kind: "drawing".into(),
                    block_id: String::new(),
                    crop: vec![r.x as f32, r.y as f32, r.w as f32, r.h as f32],
                    bytes: None,
                });
                if tex {
                    out.push_str(&format!(
                        "\\begin{{center}}\\includegraphics[width={:.2}\\linewidth]{{{path}}}\\end{{center}}\n\n",
                        frac(r.w)
                    ));
                } else {
                    out.push_str(&format!("![]({path})\n\n"));
                }
            }
            Block::Item(it) => {
                match &it.content {
                    Content::Text { text, .. } => {
                        for para in text.split("\n\n") {
                            if para.trim().is_empty() {
                                continue;
                            }
                            if tex {
                                let lines: Vec<String> = para.lines().map(tex_escape).collect();
                                out.push_str(&lines.join("\\\\\n"));
                            } else {
                                let lines: Vec<String> = para.lines().map(md_escape).collect();
                                out.push_str(&lines.join("  \n"));
                            }
                            out.push_str("\n\n");
                        }
                    }
                    Content::Math { latex, .. } if !latex.trim().is_empty() => {
                        if tex {
                            out.push_str(&format!("\\[\n{}\n\\]\n\n", latex.trim()));
                        } else {
                            out.push_str(&format!("$$\n{}\n$$\n\n", latex.trim()));
                        }
                    }
                    Content::Plot {
                        rect,
                        exprs,
                        x0,
                        x1,
                        y0,
                        y1,
                    } => {
                        if tex {
                            let w = Window {
                                x0: *x0,
                                x1: *x1,
                                y0: *y0,
                                y1: *y1,
                            };
                            let cm = (frac(rect.w) * 17.0).max(6.0);
                            out.push_str("\\begin{center}\n");
                            out.push_str(&to_pgfplots(exprs, &w, cm));
                            out.push_str("\\end{center}\n\n");
                        } else {
                            let path = format!("assets/{}-{}.png", p.id, it.id);
                            images.push(DocImage {
                                path: path.clone(),
                                page_id: p.id.clone(),
                                kind: "plot".into(),
                                block_id: it.id.clone(),
                                crop: vec![],
                                bytes: None,
                            });
                            out.push_str(&format!("![]({path})\n\n"));
                            let lines: Vec<String> =
                                exprs.iter().map(|e| format!("`{}`", e.replace('`', "'"))).collect();
                            out.push_str(&format!("*{}*\n\n", lines.join(" · ")));
                        }
                    }
                    Content::Image { rect, asset } => {
                        let Some((_, bytes)) = p.assets.iter().find(|(h, _)| h == asset) else {
                            continue;
                        };
                        let svg_pdf = tex && is_svg(bytes);
                        let path = if svg_pdf {
                            format!("assets/{asset}.pdf")
                        } else {
                            format!("assets/{asset}.{}", ext(bytes))
                        };
                        if !images.iter().any(|i| i.path == path) {
                            images.push(DocImage {
                                path: path.clone(),
                                page_id: p.id.clone(),
                                kind: if svg_pdf { "svg" } else { "image" }.into(),
                                block_id: it.id.clone(),
                                crop: vec![],
                                bytes: if svg_pdf { None } else { Some(bytes.clone()) },
                            });
                        }
                        if tex {
                            out.push_str(&format!("\\begin{{center}}\\includegraphics[width={:.2}\\linewidth]{{{path}}}\\end{{center}}\n\n", frac(rect.w)));
                        } else {
                            out.push_str(&format!("![]({path})\n\n"));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[uniffi::export]
impl NotebookDoc {
    /// The notebook as Markdown or LaTeX (E2.12): the text and the pictures it refers to.
    pub fn export_document(&self, format: DocFormat) -> Result<DocExport, CoreError> {
        let (title, outline, dir) = self.export_parts();
        let mut entries = vec![];
        for n in outline {
            match n.kind {
                NodeKind::Section => entries.push(Entry::Section {
                    title: n.title,
                    depth: n.depth,
                }),
                NodeKind::Page => {
                    let id = n.page_id.unwrap_or_default();
                    let path = dir.join(format!("{id}.kpage"));
                    let (items, assets, width, has_background) = if path.exists() {
                        let doc = crate::PageDoc::open(path.to_string_lossy().into())?;
                        let st = doc.state.lock().unwrap();
                        let page = st.doc.page();
                        let mut assets = vec![];
                        for it in &page.items {
                            if let Content::Image { asset, .. } = &it.content {
                                if let Some(b) = st.doc.asset(asset) {
                                    assets.push((asset.clone(), b));
                                }
                            }
                        }
                        let (w, _) = nb_model::page_dims(st.doc.size());
                        (
                            page.items.clone(),
                            assets,
                            w,
                            st.doc.background().is_some() || st.doc.pdf_source().is_some(),
                        )
                    } else {
                        (vec![], vec![], 794.0, false)
                    };
                    entries.push(Entry::Page(Box::new(PageContent {
                        id,
                        title: n.title,
                        depth: n.depth,
                        width,
                        items,
                        assets,
                        has_background,
                    })));
                }
            }
        }
        Ok(build(&title, &entries, format))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PageDoc;

    /// A 1×1 PNG, standing in for the pictures the app renders.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00,
        0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00,
        0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF,
        0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn sample(dir: &std::path::Path) -> std::sync::Arc<NotebookDoc> {
        let nb = NotebookDoc::open(dir.to_string_lossy().into(), "Fonctions & dérivées".into()).unwrap();
        let first = nb.page_ids()[0].clone();
        let p = PageDoc::open(nb.page_path(first.clone())).unwrap();
        p.add_text(
            40.0,
            40.0,
            500.0,
            40.0,
            "Étude de f : 50 % des élèves_ici\nLigne 2".into(),
            16.0,
            0,
        );
        p.add_math(40.0, 100.0, 200.0, 40.0, "f(x) = \\frac{x^2 - 1}{2}".into(), 18.0, 0);
        p.add_plot(
            40.0,
            200.0,
            360.0,
            270.0,
            [
                "f(x) = x^2 - 1",
                "roots f",
                "extrema f",
                "tangent f at 1",
                "area f from -1 to 1",
                "(cos t, sin t)",
                "r = 1 + cos θ",
                "asymptotes f",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            -3.0,
            3.0,
            -2.0,
            4.0,
        )
        .unwrap();
        p.begin_stroke(crate::InkTool::Pen, 0, 3.0, 50.0, 600.0, 0.5);
        p.add_points(vec![80.0, 620.0, 0.5, 120.0, 640.0, 0.5]);
        p.end_stroke(1.0, false);
        p.save().unwrap();
        let sec = nb.add_section("Chapitre 1".into());
        let second = nb.add_page(None);
        let node = nb
            .outline()
            .into_iter()
            .find(|e| e.page_id.as_deref() == Some(&second))
            .unwrap()
            .node;
        nb.move_node(node, Some(sec), 0);
        nb
    }

    #[test]
    fn markdown_export() {
        let dir = std::env::temp_dir().join(format!("kahie-md-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nb = sample(&dir);
        let md = nb.export_document(DocFormat::Markdown).unwrap();
        assert!(md.text.starts_with("# Fonctions & dérivées\n"));
        assert!(md.text.contains("Étude de f : 50 % des élèves\\_ici  \nLigne 2"));
        assert!(md.text.contains("$$\nf(x) = \\frac{x^2 - 1}{2}\n$$"));
        assert!(md.text.contains("## Chapitre 1") && md.text.contains("### Page 2"));
        let kinds: Vec<&str> = md.images.iter().map(|i| i.kind.as_str()).collect();
        assert_eq!(
            kinds,
            ["plot", "drawing"],
            "reading order: plot (y=200) before ink (y=600)"
        );
        assert_eq!(md.images[1].crop.len(), 4);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn svg_detection() {
        assert!(is_svg(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"));
        assert!(is_svg(b"\xEF\xBB\xBF  <?xml version=\"1.0\"?>\n<!-- c -->\n<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"x\">\n<svg>"));
        assert!(!is_svg(b"<svgfoo/>"));
        assert!(!is_svg(b"<html><svg/></html>"));
        assert!(!is_svg(b"\x89PNG\r\n"));
        assert!(!is_svg(b"<!-- unterminated"));
    }

    /// Inserted SVG pictures: Markdown gets the SVG file; LaTeX a one-page PDF the app
    /// renders (`\includegraphics` cannot read SVG).
    #[test]
    fn svg_images_export() {
        let svg =
            b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 4 2\"><rect width=\"4\" height=\"2\"/></svg>"
                .to_vec();
        let dir = std::env::temp_dir().join(format!("kahie-svg-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nb = NotebookDoc::open(dir.to_string_lossy().into(), "SVG".into()).unwrap();
        let first = nb.page_ids()[0].clone();
        let p = PageDoc::open(nb.page_path(first)).unwrap();
        let block = p.add_image(40.0, 40.0, 200.0, 100.0, svg.clone());
        p.add_image(40.0, 200.0, 50.0, 50.0, PNG.to_vec());
        p.save().unwrap();
        let hash = p.image_blocks()[0].hash.clone();

        let md = nb.export_document(DocFormat::Markdown).unwrap();
        let s = &md.images[0];
        assert_eq!(
            (s.kind.as_str(), s.path.clone()),
            ("image", format!("assets/{hash}.svg"))
        );
        assert_eq!(s.bytes.as_deref(), Some(&svg[..]));
        assert!(md.text.contains(&format!("![](assets/{hash}.svg)")));
        assert!(md.images[1].path.ends_with(".png"));

        let tex = nb.export_document(DocFormat::Latex).unwrap();
        let s = &tex.images[0];
        assert_eq!((s.kind.as_str(), s.path.clone()), ("svg", format!("assets/{hash}.pdf")));
        assert_eq!((s.bytes.clone(), s.block_id.clone()), (None, block));
        assert!(tex.text.contains(&format!("{{assets/{hash}.pdf}}")));
        assert_eq!(tex.images[1].kind, "image");
        std::fs::remove_dir_all(dir).ok();
    }

    /// Compiles the LaTeX export with pdflatex when it is installed (CI installs it and sets
    /// KAHIE_REQUIRE_PDFLATEX=1 so a missing pdflatex fails instead of skipping).
    #[test]
    fn latex_export_compiles_with_pdflatex() {
        let dir = std::env::temp_dir().join(format!("kahie-tex-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let nb = sample(&dir.join("nb"));
        let tex = nb.export_document(DocFormat::Latex).unwrap();
        assert!(tex.text.contains("\\begin{axis}") && tex.text.contains("\\section{Chapitre 1}"));
        assert!(tex.text.contains("50 \\% des élèves\\_ici\\\\\nLigne 2"));
        let out = dir.join("out");
        std::fs::create_dir_all(out.join("assets")).unwrap();
        std::fs::write(out.join("notebook.tex"), &tex.text).unwrap();
        for img in &tex.images {
            std::fs::write(out.join(&img.path), img.bytes.as_deref().unwrap_or(PNG)).unwrap();
        }
        let run = std::process::Command::new("pdflatex")
            .args(["-interaction=nonstopmode", "-halt-on-error", "notebook.tex"])
            .current_dir(&out)
            .output();
        match run {
            Err(_) => {
                assert!(
                    std::env::var("KAHIE_REQUIRE_PDFLATEX").is_err(),
                    "pdflatex is required but missing"
                );
                eprintln!("pdflatex not installed: compile check skipped");
            }
            Ok(o) => {
                let log = String::from_utf8_lossy(&o.stdout);
                assert!(
                    o.status.success(),
                    "pdflatex failed:\n{}",
                    &log[log.len().saturating_sub(3000)..]
                );
                assert!(out.join("notebook.pdf").exists());
            }
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
