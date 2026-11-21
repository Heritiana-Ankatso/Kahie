//! Generated PDF corpus (P1-24) for tests and benchmarks.
//!
//! Real-world PDFs cannot be fetched from CI, so the corpus is generated: [`corpus`] gives
//! 240 small documents that vary everything the text layer and page geometry depend on —
//! page size, `/Rotate`, crop box, inherited media box, simple and composite fonts
//! (Standard, WinAnsi, `/Differences`, Type 0 with `/ToUnicode`), kerning in `TJ`, text in
//! form XObjects, scaled coordinate systems, compression, object and cross-reference
//! streams, images, empty pages and trailing garbage. Each comes with the words every page
//! must yield. A directory of real PDFs can be checked too (`KAHIE_PDF_CORPUS`, see the
//! tests).

use lopdf::{dictionary, Document, Object, ObjectId, SaveOptions, Stream, StringFormat};

/// How a test document encodes its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontKind {
    /// Helvetica with its built-in (Standard) encoding, no `/Widths`.
    Standard,
    /// Times-Roman with `/WinAnsiEncoding` and explicit `/Widths`.
    WinAnsi,
    /// Courier, WinAnsi base with `/Differences` remapping one code to `eacute`.
    Differences,
    /// Type 0 font, `Identity-H`, two-byte codes mapped back with a `/ToUnicode` CMap.
    Type0,
}

/// What to put in a generated document.
#[derive(Clone, Debug, PartialEq)]
pub struct Spec {
    pub name: String,
    /// Media box width and height in points (before `/Rotate`).
    pub size: (f64, f64),
    pub rotate: i64,
    pub pages: usize,
    pub font: FontKind,
    pub compress: bool,
    pub object_streams: bool,
    /// Media box larger than the page, with a crop box offset by 50 pt.
    pub crop: bool,
    /// Media box only on the page tree root.
    pub inherit: bool,
    /// Text drawn inside a form XObject.
    pub form: bool,
    /// Text drawn in a coordinate system scaled by 0.5 (`cm`).
    pub scaled: bool,
    /// An image XObject on the page as well.
    pub image: bool,
    /// Bytes after `%%EOF`.
    pub trailing_garbage: bool,
}

impl Spec {
    pub fn simple(name: &str, pages: usize) -> Self {
        Spec {
            name: name.into(),
            size: (595.0, 842.0),
            rotate: 0,
            pages,
            font: FontKind::WinAnsi,
            compress: true,
            object_streams: false,
            crop: false,
            inherit: false,
            form: false,
            scaled: false,
            image: false,
            trailing_garbage: false,
        }
    }

    /// Words page `index` must yield, in order.
    pub fn expected_words(&self, index: usize) -> Vec<String> {
        let mut w: Vec<String> = vec!["Kahie".into(), "corpus".into(), "page".into(), (index + 1).to_string()];
        if self.font != FontKind::Standard {
            w.push("élève".into());
        }
        w.push("hello".into());
        w.push("world".into());
        w
    }
}

/// Sizes used by the corpus: A4, Letter, A5, A3 landscape, a small square, a long strip.
const SIZES: [(f64, f64); 6] = [
    (595.0, 842.0),
    (612.0, 792.0),
    (420.0, 595.0),
    (1191.0, 842.0),
    (200.0, 200.0),
    (1000.0, 300.0),
];
const FONTS: [FontKind; 4] = [
    FontKind::Standard,
    FontKind::WinAnsi,
    FontKind::Differences,
    FontKind::Type0,
];

/// The generated corpus: 240 specifications covering every combination of size, rotation
/// and font, with the other options spread over them.
pub fn corpus() -> Vec<Spec> {
    (0..240)
        .map(|i: usize| {
            let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 32;
            let bit = |b: u32| h >> b & 1 == 1;
            Spec {
                name: format!("gen-{i:03}"),
                size: SIZES[i % 6],
                rotate: [0, 90, 180, 270][(i / 6) % 4],
                pages: [1, 1, 2, 3, 7][(h % 5) as usize],
                font: FONTS[(i / 24) % 4],
                compress: (i / 96).is_multiple_of(2),
                object_streams: (i / 96).is_multiple_of(2) && bit(3),
                crop: bit(4),
                inherit: bit(5),
                form: bit(6),
                scaled: bit(7),
                image: bit(8),
                trailing_garbage: bit(9) && bit(10),
            }
        })
        .collect()
}

fn string(font: FontKind, text: &str) -> Object {
    match font {
        FontKind::Type0 => Object::String(
            text.chars().flat_map(|c| (c as u16).to_be_bytes()).collect(),
            StringFormat::Hexadecimal,
        ),
        FontKind::Differences => Object::String(
            text.chars()
                .map(|c| match c {
                    'é' => 0x80,
                    c if (c as u32) < 256 => c as u8,
                    _ => b'?',
                })
                .collect(),
            StringFormat::Literal,
        ),
        _ => Object::String(
            text.chars()
                .map(|c| if (c as u32) < 256 { c as u8 } else { b'?' })
                .collect(),
            StringFormat::Literal,
        ),
    }
}

fn font_object(doc: &mut Document, font: FontKind) -> ObjectId {
    match font {
        FontKind::Standard => doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        }),
        FontKind::WinAnsi => {
            let widths: Vec<Object> = (32..=255u32)
                .map(|c| Object::Integer(if c == 32 { 250 } else { 500 }))
                .collect();
            doc.add_object(dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Times-Roman",
                "Encoding" => "WinAnsiEncoding", "FirstChar" => 32, "Widths" => widths,
            })
        }
        FontKind::Differences => {
            let enc = doc.add_object(dictionary! {
                "Type" => "Encoding", "BaseEncoding" => "WinAnsiEncoding",
                "Differences" => vec![Object::Integer(128), Object::Name(b"eacute".to_vec())],
            });
            doc.add_object(dictionary! {
                "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier", "Encoding" => enc,
            })
        }
        FontKind::Type0 => {
            let cmap = b"/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
1 beginbfrange\n<0000> <00FF> <0000>\nendbfrange\n\
endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
                .to_vec();
            let to_unicode = doc.add_object(Stream::new(dictionary! {}, cmap));
            let cid = doc.add_object(dictionary! {
                "Type" => "Font", "Subtype" => "CIDFontType2", "BaseFont" => "Helvetica",
                "CIDSystemInfo" => dictionary! { "Registry" => Object::string_literal("Adobe"), "Ordering" => Object::string_literal("Identity"), "Supplement" => 0 },
                "DW" => 600,
                "W" => vec![Object::Integer(32), Object::Array(vec![Object::Integer(280)])],
            });
            doc.add_object(dictionary! {
                "Type" => "Font", "Subtype" => "Type0", "BaseFont" => "Helvetica",
                "Encoding" => "Identity-H", "DescendantFonts" => vec![Object::Reference(cid)],
                "ToUnicode" => to_unicode,
            })
        }
    }
}

/// A string operand as it appears in a content stream.
fn operand(o: &Object) -> String {
    match o {
        Object::String(b, StringFormat::Hexadecimal) => {
            format!("<{}>", b.iter().map(|x| format!("{x:02X}")).collect::<String>())
        }
        Object::String(b, _) => {
            let mut out = String::from("(");
            for &x in b {
                match x {
                    b'(' | b')' | b'\\' => {
                        out.push('\\');
                        out.push(x as char);
                    }
                    0x20..=0x7e => out.push(x as char),
                    _ => out.push_str(&format!("\\{x:03o}")),
                }
            }
            out.push(')');
            out
        }
        _ => String::new(),
    }
}

/// Content of page `k`: three lines near the top-left corner, in user space.
fn text_ops(spec: &Spec, k: usize, ox: f64, oy: f64, h: f64) -> Vec<u8> {
    let f = spec.font;
    let s = |t: &str| operand(&string(f, t));
    let kerned = |a: &str, b: &str, kern: i64| format!("[{} {kern} {}] TJ", s(a), s(b));
    let sc = if spec.scaled { 2.0 } else { 1.0 };
    let (x, y) = ((ox + 20.0) * sc, (oy + h - 30.0) * sc);
    let mut c = String::new();
    if spec.scaled {
        c.push_str("q 0.5 0 0 0.5 0 0 cm\n");
    }
    let fs = 12.0 * sc;
    c.push_str(&format!("BT /F1 {fs} Tf {x} {y} Td {} Tj\n", s("Kahie corpus")));
    c.push_str(&format!("{} TL T* {} Tj\n", fs * 1.4, s(&format!("page {}", k + 1))));
    if f != FontKind::Standard {
        c.push_str(&format!("T* {} Tj\n", s("élève")));
    }
    // "hello" and "world" on one line without a space character between them (a gap),
    // and "hello" itself kerned inside the word.
    c.push_str(&format!("0 -{} Td ", fs * 1.4));
    c.push_str(&kerned("hel", "lo", 30));
    c.push_str(&format!(" {} 0 Td ", fs * 4.0));
    c.push_str(&kerned("wor", "ld", -10));
    c.push_str("\nET\n");
    if spec.scaled {
        c.push_str("Q\n");
    }
    c.into_bytes()
}

/// Build the document described by `spec`.
pub fn generate(spec: &Spec) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font = font_object(&mut doc, spec.font);
    let (w, h) = spec.size;
    let off = if spec.crop { 50.0 } else { 0.0 };
    let media = |extra: f64| -> Vec<Object> {
        vec![
            0.into(),
            0.into(),
            Object::Real((w + extra) as f32),
            Object::Real((h + extra) as f32),
        ]
    };
    let image = spec.image.then(|| {
        // 2×2 RGB image.
        doc.add_object(Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 2,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        ))
    });
    let mut kids = vec![];
    for k in 0..spec.pages {
        let text = text_ops(spec, k, off, off, h);
        let mut xobjects = lopdf::Dictionary::new();
        let mut content = Vec::new();
        if let Some(img) = image {
            xobjects.set("Im1", img);
            content.extend_from_slice(format!("q 40 0 0 40 {} {} cm /Im1 Do Q\n", off + 10.0, off + 10.0).as_bytes());
        }
        if spec.form {
            // The form is drawn translated by (5, -5) and holds its own font resources.
            let form = doc.add_object(Stream::new(
                dictionary! {
                    "Type" => "XObject", "Subtype" => "Form",
                    "BBox" => vec![0.into(), 0.into(), Object::Real((w + off * 2.0) as f32), Object::Real((h + off * 2.0) as f32)],
                    "Matrix" => vec![1.into(), 0.into(), 0.into(), 1.into(), 5.into(), Object::Integer(-5)],
                    "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
                },
                text,
            ));
            xobjects.set("Fm1", form);
            content.extend_from_slice(b"/Fm1 Do\n");
        } else {
            content.extend_from_slice(&text);
        }
        let content_id = doc.add_object(Stream::new(dictionary! {}, content));
        let mut page = dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font }, "XObject" => xobjects },
        };
        if !spec.inherit {
            page.set("MediaBox", media(off * 2.0));
        }
        if spec.crop {
            page.set(
                "CropBox",
                vec![
                    Object::Real(off as f32),
                    Object::Real(off as f32),
                    Object::Real((w + off) as f32),
                    Object::Real((h + off) as f32),
                ],
            );
        }
        if spec.rotate != 0 {
            page.set("Rotate", spec.rotate);
        }
        kids.push(Object::Reference(doc.add_object(page)));
    }
    let mut pages = dictionary! { "Type" => "Pages", "Count" => spec.pages as i64, "Kids" => kids };
    if spec.inherit {
        pages.set("MediaBox", media(off * 2.0));
    }
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    if spec.compress {
        doc.compress();
    }
    let mut out = Vec::new();
    let opts = SaveOptions::builder()
        .use_object_streams(spec.object_streams)
        .use_xref_streams(spec.object_streams)
        .build();
    doc.save_with_options(&mut out, opts).expect("write PDF");
    if spec.trailing_garbage {
        out.extend_from_slice(b"\r\n\0\0garbage after the end of file\n");
    }
    out
}
