use super::testgen::{corpus, generate, FontKind, Spec};
use super::*;

fn texts(words: &[Word]) -> Vec<String> {
    words.iter().map(|w| w.text.clone()).collect()
}

/// P1-24: every generated document opens, has the right pages and sizes, and every page
/// yields exactly the expected words with boxes inside the page.
#[test]
fn generated_corpus_opens_and_yields_its_text() {
    let specs = corpus();
    assert!(specs.len() >= 200);
    let mut failures = vec![];
    for spec in &specs {
        let bytes = generate(spec);
        let pdf = match PdfDoc::load(&bytes) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{}: {e}", spec.name));
                continue;
            }
        };
        if pdf.page_count() != spec.pages {
            failures.push(format!("{}: {} pages", spec.name, pdf.page_count()));
            continue;
        }
        let info = pdf.page_info(0).unwrap();
        let (w, h) = if spec.rotate % 180 == 90 {
            (spec.size.1, spec.size.0)
        } else {
            spec.size
        };
        if (info.width_pt - w).abs() > 0.5 || (info.height_pt - h).abs() > 0.5 {
            failures.push(format!("{}: size {info:?}", spec.name));
        }
        for k in 0..spec.pages {
            let words = pdf.words(k);
            if texts(&words) != spec.expected_words(k) {
                failures.push(format!("{} p{k}: {:?}", spec.name, texts(&words)));
            }
            for wd in &words {
                let inside = [wd.x0, wd.y0, wd.x1, wd.y1].iter().all(|v| (-0.01..=1.01).contains(v));
                if !inside || wd.x1 <= wd.x0 || wd.y1 <= wd.y0 {
                    failures.push(format!("{} p{k}: box of {:?} {wd:?}", spec.name, wd.text));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn word_boxes_follow_rotation() {
    for rotate in [0, 90, 180, 270] {
        let mut spec = Spec::simple("rot", 1);
        spec.rotate = rotate;
        let pdf = PdfDoc::load(&generate(&spec)).unwrap();
        let first = &pdf.words(0)[0];
        assert_eq!(first.text, "Kahie");
        let (cx, cy) = ((first.x0 + first.x1) / 2.0, (first.y0 + first.y1) / 2.0);
        // The text sits near the top-left corner of the unrotated page.
        match rotate {
            0 => assert!(cx < 0.3 && cy < 0.1, "{first:?}"),
            90 => assert!(cx > 0.9 && cy < 0.3, "{first:?}"),
            180 => assert!(cx > 0.7 && cy > 0.9, "{first:?}"),
            _ => assert!(cx < 0.1 && cy > 0.7, "{first:?}"),
        }
    }
}

#[test]
fn word_box_matches_the_drawn_position() {
    let spec = Spec::simple("pos", 1);
    let pdf = PdfDoc::load(&generate(&spec)).unwrap();
    let w = &pdf.words(0)[0];
    // "Kahie" at x = 20 pt, baseline 30 pt below the top, 12 pt Times with 500/1000 widths.
    let (pw, ph) = (595.0f32, 842.0f32);
    assert!((w.x0 * pw - 20.0).abs() < 0.5, "{w:?}");
    assert!((w.x1 * pw - (20.0 + 5.0 * 6.0)).abs() < 0.5, "{w:?}");
    let baseline = 30.0 / ph;
    assert!(w.y0 < baseline && w.y1 > baseline, "{w:?}");
    assert!((w.y1 - w.y0) * ph > 10.0 && (w.y1 - w.y0) * ph < 14.0, "{w:?}");
}

#[test]
fn plain_text_joins_words_and_lines() {
    let pdf = PdfDoc::load(&generate(&Spec::simple("text", 2))).unwrap();
    assert_eq!(pdf.text(1), "Kahie corpus\npage 2\nélève\nhello world");
}

#[test]
fn each_font_kind_decodes_accents() {
    for font in [FontKind::WinAnsi, FontKind::Differences, FontKind::Type0] {
        let mut spec = Spec::simple("font", 1);
        spec.font = font;
        let pdf = PdfDoc::load(&generate(&spec)).unwrap();
        assert!(texts(&pdf.words(0)).contains(&"élève".to_string()), "{font:?}");
    }
}

#[test]
fn large_document_parses_quickly() {
    let pdf_bytes = generate(&Spec::simple("big", 300));
    let t = std::time::Instant::now();
    let pdf = PdfDoc::load(&pdf_bytes).unwrap();
    assert_eq!(pdf.page_count(), 300);
    let _ = pdf.page_info(0).unwrap();
    let _ = pdf.words(0);
    // Debug builds on a CI runner: generous; the release benchmark tracks the real number.
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
}

/// Damaged files (truncated, bytes flipped) give an error or fewer words, never a panic.
#[test]
fn damaged_files_never_panic() {
    let good = generate(&Spec::simple("dmg", 3));
    let mut seed = 12345u64;
    for k in 0..200 {
        let mut b = good.clone();
        if k % 2 == 0 {
            b.truncate(b.len() * (k % 97 + 1) / 100);
        } else {
            for _ in 0..8 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                let i = (seed % b.len() as u64) as usize;
                b[i] = (seed >> 32) as u8;
            }
        }
        if let Ok(pdf) = PdfDoc::load(&b) {
            for i in 0..pdf.page_count() {
                let _ = pdf.page_info(i);
                let _ = pdf.words(i);
            }
        }
    }
    assert!(PdfDoc::load(b"").is_err());
    assert!(PdfDoc::load(b"%PDF-1.4\n%%EOF").is_err());
}

/// Real-world PDFs (nightly or local): every `*.pdf` under `KAHIE_PDF_CORPUS` must open and
/// its pages must have a size and readable text (or none). Skipped when unset.
#[test]
fn real_world_corpus_from_environment() {
    let Ok(dir) = std::env::var("KAHIE_PDF_CORPUS") else {
        return;
    };
    let mut n = 0;
    let mut failures = vec![];
    for entry in std::fs::read_dir(&dir)
        .expect("KAHIE_PDF_CORPUS is a directory")
        .flatten()
    {
        let p = entry.path();
        if p.extension().is_none_or(|e| !e.eq_ignore_ascii_case("pdf")) {
            continue;
        }
        n += 1;
        let bytes = std::fs::read(&p).unwrap();
        match PdfDoc::load(&bytes) {
            Ok(pdf) => {
                for i in 0..pdf.page_count() {
                    if pdf.page_info(i).is_none_or(|s| s.width_pt < 1.0 || s.height_pt < 1.0) {
                        failures.push(format!("{}: page {i} has no size", p.display()));
                    }
                    let _ = pdf.words(i);
                }
            }
            Err(e) => failures.push(format!("{}: {e}", p.display())),
        }
    }
    println!("{n} PDFs checked, {} failures", failures.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
