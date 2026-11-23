//! PDF support in the core (P1-20 … P1-24): page geometry and the **text layer**.
//!
//! Pages are *rasterised* by the platform (Android's `PdfRenderer`, which is PDFium inside
//! the OS; see `docs/spikes/p1-20-pdf.md` for why PDFium is not linked directly). The core
//! reads the PDF itself only to know its pages and their text: [`PdfDoc::page_info`] gives
//! the displayed size of a page (crop box, `/Rotate` applied) and [`PdfDoc::words`] the
//! words on it with their boxes, for selection, copy and search.
//!
//! Word boxes are **normalised** to the displayed page: `(0, 0)` is the top-left corner and
//! `(1, 1)` the bottom-right, whatever the page size or rotation, so the app maps them onto
//! however it fits the page.
//!
//! Text extraction follows the PDF text model (ISO 32000-1 §9): the graphics state stack
//! (`q`/`Q`/`cm`), text objects (`BT`/`ET`, `Tm`, `Td`, `TD`, `T*`, `Tc`, `Tw`, `Tz`, `TL`,
//! `Ts`, `Tf`), showing operators (`Tj`, `TJ`, `'`, `"`) and form XObjects (`Do`). Glyph
//! advances come from the font's `/Widths` (simple fonts) or `/W` (CID fonts), with a
//! per-font default otherwise. Characters come from `/ToUnicode` or the font encoding
//! (through `lopdf`). It is meant for search and copy, not for exact layout.

use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};
use std::collections::HashMap;

pub mod testgen;

/// Error from reading a PDF.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfError(pub String);

impl std::fmt::Display for PdfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PdfError {}

fn err(e: impl std::fmt::Display) -> PdfError {
    PdfError(e.to_string())
}

/// Displayed size of a page, in PDF points (1/72 in), after `/Rotate`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageInfo {
    pub width_pt: f64,
    pub height_pt: f64,
    /// `/Rotate` normalised to 0, 90, 180 or 270.
    pub rotate: u16,
}

/// A word of the text layer; the box is normalised to the displayed page (see crate docs).
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// Content streams above this size (decompressed) are not read for text.
const MAX_CONTENT: usize = 64 << 20;
/// Nesting limit for form XObjects.
const MAX_FORM_DEPTH: usize = 8;
/// Upper bound of glyphs read from one page (pathological files).
const MAX_GLYPHS: usize = 200_000;

/// A parsed PDF document.
pub struct PdfDoc {
    doc: Document,
    pages: Vec<ObjectId>,
}

impl PdfDoc {
    /// Parse a PDF from memory. Never panics on malformed input (panics inside the parser
    /// are caught and returned as errors).
    pub fn load(bytes: &[u8]) -> Result<Self, PdfError> {
        let doc = std::panic::catch_unwind(|| Document::load_mem(bytes))
            .map_err(|_| PdfError("the PDF parser failed on this file".into()))?
            .map_err(err)?;
        let pages: Vec<ObjectId> = doc.get_pages().into_values().collect();
        if pages.is_empty() {
            return Err(PdfError("the PDF has no pages".into()));
        }
        Ok(Self { doc, pages })
    }

    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Displayed size and rotation of page `index` (0-based).
    pub fn page_info(&self, index: usize) -> Option<PageInfo> {
        let id = *self.pages.get(index)?;
        let (bx, rotate) = self.page_box(id);
        let (w, h) = (bx[2] - bx[0], bx[3] - bx[1]);
        Some(if rotate % 180 == 90 {
            PageInfo {
                width_pt: h,
                height_pt: w,
                rotate,
            }
        } else {
            PageInfo {
                width_pt: w,
                height_pt: h,
                rotate,
            }
        })
    }

    /// Words of page `index` in content order. Pages whose text cannot be read give an
    /// empty list, never an error: the page still renders, it just has no text layer.
    pub fn words(&self, index: usize) -> Vec<Word> {
        let Some(&id) = self.pages.get(index) else {
            return vec![];
        };
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.words_inner(id))).unwrap_or_default()
    }

    /// Plain text of page `index`: words joined by spaces, lines by newlines.
    pub fn text(&self, index: usize) -> String {
        words_to_text(&self.words(index))
    }

    // ---------- geometry ----------

    /// Crop box (intersected with the media box) and rotation, inherited from parents.
    fn page_box(&self, id: ObjectId) -> ([f64; 4], u16) {
        let mut media = None;
        let mut crop = None;
        let mut rotate = None;
        let mut cur = Some(id);
        let mut guard = 0;
        while let (Some(pid), true) = (cur, guard < 64) {
            guard += 1;
            let Ok(d) = self.doc.get_dictionary(pid) else { break };
            if media.is_none() {
                media = self.rect(d, b"MediaBox");
            }
            if crop.is_none() {
                crop = self.rect(d, b"CropBox");
            }
            if rotate.is_none() {
                rotate = d.get_deref(b"Rotate", &self.doc).ok().and_then(|o| o.as_float().ok());
            }
            cur = d.get(b"Parent").and_then(Object::as_reference).ok();
        }
        let media = media.unwrap_or([0.0, 0.0, 612.0, 792.0]);
        let mut b = match crop {
            Some(c) => [
                c[0].max(media[0]),
                c[1].max(media[1]),
                c[2].min(media[2]),
                c[3].min(media[3]),
            ],
            None => media,
        };
        if b[2] - b[0] < 1.0 || b[3] - b[1] < 1.0 {
            b = media;
        }
        let r = rotate.unwrap_or(0.0).round() as i64;
        (b, (((r % 360) + 360) % 360 / 90 * 90) as u16)
    }

    fn rect(&self, d: &Dictionary, key: &[u8]) -> Option<[f64; 4]> {
        let a = d.get_deref(key, &self.doc).ok()?.as_array().ok()?;
        if a.len() != 4 {
            return None;
        }
        let mut v = [0.0; 4];
        for (i, o) in a.iter().enumerate() {
            let o = self.doc.dereference(o).ok()?.1;
            v[i] = f64::from(o.as_float().ok()?);
        }
        let r = [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])];
        r.iter().all(|x| x.is_finite()).then_some(r)
    }

    // ---------- text ----------

    fn words_inner(&self, id: ObjectId) -> Vec<Word> {
        let (bx, rotate) = self.page_box(id);
        let resources = self.page_resources(id);
        let content = self
            .doc
            .get_page_content_with_limit(id, MAX_CONTENT)
            .unwrap_or_default();
        let mut run = TextRun {
            pdf: self,
            fonts: HashMap::new(),
            glyphs: vec![],
        };
        run.run(&content, resources, Matrix::IDENTITY, 0);
        let glyphs = run.glyphs;

        // Map user space to the normalised displayed page.
        let (w, h) = (bx[2] - bx[0], bx[3] - bx[1]);
        let norm = |x: f64, y: f64| -> (f64, f64) {
            let (u, v) = (x - bx[0], y - bx[1]);
            match rotate {
                90 => (v / h, u / w),
                180 => ((w - u) / w, v / h),
                270 => ((h - v) / h, (w - u) / w),
                _ => (u / w, (h - v) / h),
            }
        };
        let mut words: Vec<Word> = vec![];
        let mut cur: Option<(Word, Glyph)> = None;
        for g in glyphs {
            let (ax, ay) = norm(g.quad[0].0, g.quad[0].1);
            let mut b = [ax, ay, ax, ay];
            for &(x, y) in &g.quad[1..] {
                let (nx, ny) = norm(x, y);
                b = [b[0].min(nx), b[1].min(ny), b[2].max(nx), b[3].max(ny)];
            }
            if g.text.chars().all(char::is_whitespace) {
                if let Some((wd, _)) = cur.take() {
                    words.push(wd);
                }
                continue;
            }
            if let Some((wd, last)) = &mut cur {
                if last.continues(&g) {
                    wd.text.push_str(&g.text);
                    wd.x0 = wd.x0.min(b[0] as f32);
                    wd.y0 = wd.y0.min(b[1] as f32);
                    wd.x1 = wd.x1.max(b[2] as f32);
                    wd.y1 = wd.y1.max(b[3] as f32);
                    *last = g;
                    continue;
                }
                let (done, _) = cur.take().expect("current word");
                words.push(done);
            }
            cur = Some((
                Word {
                    text: g.text.clone(),
                    x0: b[0] as f32,
                    y0: b[1] as f32,
                    x1: b[2] as f32,
                    y1: b[3] as f32,
                },
                g,
            ));
        }
        if let Some((wd, _)) = cur {
            words.push(wd);
        }
        words.retain(|w| !w.text.trim().is_empty());
        words
    }

    fn page_resources(&self, id: ObjectId) -> Option<&Dictionary> {
        let mut cur = Some(id);
        let mut guard = 0;
        while let (Some(pid), true) = (cur, guard < 64) {
            guard += 1;
            let Ok(d) = self.doc.get_dictionary(pid) else { break };
            if let Ok(r) = d.get_deref(b"Resources", &self.doc).and_then(Object::as_dict) {
                return Some(r);
            }
            cur = d.get(b"Parent").and_then(Object::as_reference).ok();
        }
        None
    }

    fn deref_dict<'a>(&'a self, d: &'a Dictionary, key: &[u8]) -> Option<&'a Dictionary> {
        d.get_deref(key, &self.doc).ok()?.as_dict().ok()
    }
}

/// Join words into text: a space between words on a line, a newline between lines.
pub fn words_to_text(words: &[Word]) -> String {
    let mut s = String::new();
    let mut prev: Option<&Word> = None;
    for w in words {
        if let Some(p) = prev {
            let ph = (p.y1 - p.y0).max(1e-4);
            let same_line = ((w.y0 + w.y1) - (p.y0 + p.y1)).abs() / 2.0 < ph * 0.6;
            s.push(if same_line { ' ' } else { '\n' });
        }
        s.push_str(&w.text);
        prev = Some(w);
    }
    s
}

/// Affine matrix `[a b c d e f]` as in PDF (`x' = a x + c y + e`, `y' = b x + d y + f`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Matrix([f64; 6]);

impl Matrix {
    const IDENTITY: Matrix = Matrix([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);

    /// `self × m` (apply `self` first, then `m`).
    fn then(&self, m: &Matrix) -> Matrix {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = m.0;
        Matrix([
            a * a2 + b * c2,
            a * b2 + b * d2,
            c * a2 + d * c2,
            c * b2 + d * d2,
            e * a2 + f * c2 + e2,
            e * b2 + f * d2 + f2,
        ])
    }

    fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }

    fn translate(tx: f64, ty: f64) -> Matrix {
        Matrix([1.0, 0.0, 0.0, 1.0, tx, ty])
    }
}

/// One shown glyph: its text, its quadrilateral in user space and where the next glyph of
/// the same word would start.
#[derive(Clone, Debug)]
struct Glyph {
    text: String,
    quad: [(f64, f64); 4],
    /// Origin of the glyph and of the next one, and the font height, in user space.
    origin: (f64, f64),
    next: (f64, f64),
    height: f64,
}

impl Glyph {
    /// Whether `g` follows this glyph in the same word (close to where it would be).
    fn continues(&self, g: &Glyph) -> bool {
        let h = self.height.max(g.height).max(1e-3);
        let (dx, dy) = (g.origin.0 - self.next.0, g.origin.1 - self.next.1);
        (dx * dx + dy * dy).sqrt() < h * 0.15
    }
}

/// Font data needed for text: how to decode codes and how far each glyph advances.
struct Font<'a> {
    encoding: Option<Encoding<'a>>,
    two_byte: bool,
    /// Glyph widths in 1/1000 text space unit, by code.
    widths: HashMap<u32, f64>,
    default_width: f64,
    /// Type 3 fonts use their own matrix; the others 1/1000.
    scale: f64,
}

#[derive(Clone)]
struct TextState {
    ctm: Matrix,
    font: Option<Vec<u8>>,
    size: f64,
    char_spacing: f64,
    word_spacing: f64,
    hscale: f64,
    leading: f64,
    rise: f64,
}

struct TextRun<'a> {
    pdf: &'a PdfDoc,
    /// Fonts by dictionary identity (pointer) to avoid parsing them twice.
    fonts: HashMap<usize, Font<'a>>,
    glyphs: Vec<Glyph>,
}

fn num(o: &Object) -> f64 {
    o.as_float().map(f64::from).unwrap_or(0.0)
}

impl<'a> TextRun<'a> {
    fn font_dict(&self, resources: Option<&'a Dictionary>, name: &[u8]) -> Option<&'a Dictionary> {
        let fonts = self.pdf.deref_dict(resources?, b"Font")?;
        fonts.get_deref(name, &self.pdf.doc).ok()?.as_dict().ok()
    }

    fn load_font(&mut self, dict: &'a Dictionary) -> usize {
        let key = dict as *const Dictionary as usize;
        if self.fonts.contains_key(&key) {
            return key;
        }
        let doc = &self.pdf.doc;
        let subtype = dict.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"");
        let two_byte = subtype == b"Type0";
        let mut widths = HashMap::new();
        let mut default_width;
        let mut scale = 0.001;
        if two_byte {
            default_width = 1000.0;
            if let Some(cid) = dict
                .get_deref(b"DescendantFonts", doc)
                .ok()
                .and_then(|o| o.as_array().ok())
                .and_then(|a| a.first())
                .and_then(|o| doc.dereference(o).ok())
                .and_then(|(_, o)| o.as_dict().ok())
            {
                if let Ok(dw) = cid.get_deref(b"DW", doc) {
                    default_width = num(dw);
                }
                if let Ok(w) = cid.get_deref(b"W", doc).and_then(Object::as_array) {
                    // [c [w1 w2 …]] or [c_first c_last w]
                    let mut i = 0;
                    while i < w.len() && widths.len() < 65_536 {
                        let first = num(doc.dereference(&w[i]).map(|x| x.1).unwrap_or(&Object::Null)) as u32;
                        match w.get(i + 1).map(|o| doc.dereference(o).map(|x| x.1)) {
                            Some(Ok(Object::Array(list))) => {
                                for (k, o) in list.iter().enumerate() {
                                    widths.insert(first + k as u32, num(o));
                                }
                                i += 2;
                            }
                            Some(Ok(last)) => {
                                let last = num(last) as u32;
                                let v = w.get(i + 2).map(num).unwrap_or(default_width);
                                for c in first..=last.min(first.saturating_add(65_535)) {
                                    widths.insert(c, v);
                                }
                                i += 3;
                            }
                            _ => break,
                        }
                    }
                }
            }
        } else {
            let first = dict.get_deref(b"FirstChar", doc).map(num).unwrap_or(0.0) as u32;
            if let Ok(w) = dict.get_deref(b"Widths", doc).and_then(Object::as_array) {
                for (k, o) in w.iter().enumerate().take(256) {
                    let v = doc.dereference(o).map(|x| num(x.1)).unwrap_or(0.0);
                    widths.insert(first + k as u32, v);
                }
            }
            if subtype == b"Type3" {
                if let Ok(m) = dict.get_deref(b"FontMatrix", doc).and_then(Object::as_array) {
                    scale = m.first().map(num).unwrap_or(0.001).abs();
                }
            }
            let base = dict
                .get_deref(b"BaseFont", doc)
                .and_then(Object::as_name)
                .unwrap_or(b"");
            if base.starts_with(b"Courier") {
                default_width = 600.0;
            } else if widths.is_empty() {
                default_width = 520.0;
            } else {
                default_width = widths.values().copied().filter(|v| *v > 0.0).sum::<f64>()
                    / widths.values().filter(|v| **v > 0.0).count().max(1) as f64;
            }
            if widths.is_empty() {
                // Standard 14 fonts without /Widths: Helvetica-like widths for ASCII.
                for c in 32u32..127 {
                    widths.insert(c, standard_width(c as u8, base));
                }
            }
        }
        let encoding = dict.get_font_encoding_with_limit(doc, MAX_CONTENT).ok();
        self.fonts.insert(
            key,
            Font {
                encoding,
                two_byte,
                widths,
                default_width,
                scale,
            },
        );
        key
    }

    fn run(&mut self, content: &[u8], resources: Option<&'a Dictionary>, ctm: Matrix, depth: usize) {
        let Ok(ops) = lopdf::content::Content::decode(content) else {
            return;
        };
        let mut st = TextState {
            ctm,
            font: None,
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            hscale: 1.0,
            leading: 0.0,
            rise: 0.0,
        };
        let mut stack: Vec<TextState> = vec![];
        let (mut tm, mut tlm) = (Matrix::IDENTITY, Matrix::IDENTITY);
        for op in &ops.operations {
            if self.glyphs.len() >= MAX_GLYPHS {
                return;
            }
            let a = &op.operands;
            let n = |i: usize| a.get(i).map(num).unwrap_or(0.0);
            match op.operator.as_str() {
                "q" => stack.push(st.clone()),
                "Q" => {
                    if let Some(s) = stack.pop() {
                        st = s;
                    }
                }
                "cm" if a.len() == 6 => st.ctm = Matrix([n(0), n(1), n(2), n(3), n(4), n(5)]).then(&st.ctm),
                "BT" => {
                    tm = Matrix::IDENTITY;
                    tlm = Matrix::IDENTITY;
                }
                "Tf" => {
                    st.font = a.first().and_then(|o| o.as_name().ok()).map(<[u8]>::to_vec);
                    st.size = n(1);
                }
                "Tc" => st.char_spacing = n(0),
                "Tw" => st.word_spacing = n(0),
                "Tz" => st.hscale = n(0) / 100.0,
                "TL" => st.leading = n(0),
                "Ts" => st.rise = n(0),
                "Td" => {
                    tlm = Matrix::translate(n(0), n(1)).then(&tlm);
                    tm = tlm;
                }
                "TD" => {
                    st.leading = -n(1);
                    tlm = Matrix::translate(n(0), n(1)).then(&tlm);
                    tm = tlm;
                }
                "Tm" if a.len() == 6 => {
                    tlm = Matrix([n(0), n(1), n(2), n(3), n(4), n(5)]);
                    tm = tlm;
                }
                "T*" => {
                    tlm = Matrix::translate(0.0, -st.leading).then(&tlm);
                    tm = tlm;
                }
                "Tj" => {
                    if let Some(s) = a.first().and_then(|o| o.as_str().ok()) {
                        self.show(s, &st, &mut tm, resources);
                    }
                }
                "'" | "\"" => {
                    if op.operator == "\"" {
                        st.word_spacing = n(0);
                        st.char_spacing = n(1);
                    }
                    tlm = Matrix::translate(0.0, -st.leading).then(&tlm);
                    tm = tlm;
                    if let Some(s) = a.last().and_then(|o| o.as_str().ok()) {
                        self.show(s, &st, &mut tm, resources);
                    }
                }
                "TJ" => {
                    if let Some(list) = a.first().and_then(|o| o.as_array().ok()) {
                        for item in list {
                            match item {
                                Object::String(s, _) => self.show(s, &st, &mut tm, resources),
                                other => {
                                    let adj = -num(other) / 1000.0 * st.size * st.hscale;
                                    tm = Matrix::translate(adj, 0.0).then(&tm);
                                }
                            }
                        }
                    }
                }
                "Do" if depth < MAX_FORM_DEPTH => {
                    let Some(name) = a.first().and_then(|o| o.as_name().ok()) else {
                        continue;
                    };
                    let Some(xobjects) = resources.and_then(|r| self.pdf.deref_dict(r, b"XObject")) else {
                        continue;
                    };
                    let Ok(stream) = xobjects.get_deref(name, &self.pdf.doc).and_then(Object::as_stream) else {
                        continue;
                    };
                    if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Form") {
                        continue;
                    }
                    let matrix = stream
                        .dict
                        .get_deref(b"Matrix", &self.pdf.doc)
                        .and_then(Object::as_array)
                        .ok()
                        .filter(|m| m.len() == 6)
                        .map(|m| Matrix([num(&m[0]), num(&m[1]), num(&m[2]), num(&m[3]), num(&m[4]), num(&m[5])]))
                        .unwrap_or(Matrix::IDENTITY);
                    let inner = self.pdf.deref_dict(&stream.dict, b"Resources").or(resources);
                    let data = stream
                        .decompressed_content_with_limit(MAX_CONTENT)
                        .unwrap_or_else(|_| stream.content.clone());
                    self.run(&data, inner, matrix.then(&st.ctm), depth + 1);
                }
                _ => {}
            }
        }
    }

    /// Show string `s` with the current font, appending glyphs and advancing `tm`.
    fn show(&mut self, s: &[u8], st: &TextState, tm: &mut Matrix, resources: Option<&'a Dictionary>) {
        let Some(name) = &st.font else { return };
        let Some(dict) = self.font_dict(resources, name) else {
            return;
        };
        let key = self.load_font(dict);
        let font = &self.fonts[&key];
        let step = if font.two_byte { 2 } else { 1 };
        let fs = st.size;
        let th = st.hscale;
        let height = (fs * (st.ctm.0[3].hypot(st.ctm.0[2]) * tm.0[3].hypot(tm.0[2]))).abs();
        for code_bytes in s.chunks(step) {
            let code = code_bytes.iter().fold(0u32, |acc, b| acc << 8 | u32::from(*b));
            let text = match &font.encoding {
                Some(e) => Document::decode_text(e, code_bytes).unwrap_or_default(),
                None => code_bytes.iter().map(|&b| char::from(b)).collect(),
            };
            let text: String = text.chars().filter(|c| !c.is_control() || *c == ' ').collect();
            let w0 = font.widths.get(&code).copied().unwrap_or(font.default_width) * font.scale;
            let trm = Matrix([fs * th, 0.0, 0.0, fs, 0.0, st.rise]).then(tm).then(&st.ctm);
            let (desc, asc) = (-0.22, 0.78);
            let quad = [
                trm.apply(0.0, desc),
                trm.apply(w0, desc),
                trm.apply(w0, asc),
                trm.apply(0.0, asc),
            ];
            let space = step == 1 && code == 32;
            let tx = (w0 * fs + st.char_spacing + if space { st.word_spacing } else { 0.0 }) * th;
            let origin = trm.apply(0.0, 0.0);
            *tm = Matrix::translate(tx, 0.0).then(tm);
            let next = Matrix([fs * th, 0.0, 0.0, fs, 0.0, st.rise])
                .then(tm)
                .then(&st.ctm)
                .apply(0.0, 0.0);
            if !text.is_empty() {
                self.glyphs.push(Glyph {
                    text,
                    quad,
                    origin,
                    next,
                    height,
                });
            }
        }
    }
}

/// Approximate Helvetica / Times widths (1/1000 em) for the standard fonts without
/// `/Widths`; good enough to place words.
fn standard_width(c: u8, base: &[u8]) -> f64 {
    let times = base.starts_with(b"Times");
    match c {
        b' ' => 250.0,
        b'i' | b'j' | b'l' | b'.' | b',' | b'\'' | b'!' | b':' | b';' | b'|' => 260.0,
        b'f' | b't' | b'r' | b'I' | b'(' | b')' | b'[' | b']' | b'-' => 330.0,
        b'm' | b'w' => 780.0,
        b'M' | b'W' => 870.0,
        b'A'..=b'Z' => 680.0,
        b'0'..=b'9' => 556.0,
        _ if times => 480.0,
        _ => 540.0,
    }
}

#[cfg(test)]
mod tests;
