//! P0-09 — Typst maths layout fed by MiTeX (LaTeX → Typst). Spike code.
//! `cargo run --release` → prints a report fragment, writes sheet.svg and sheet.pdf.
use std::time::Instant;
use typst_layout::PagedDocument;
use typst_as_lib::TypstEngine;

const PRELUDE: &str = include_str!("../prelude.typ");

/// Symbol modifiers renamed in Typst 0.14+ (cannot be aliased with `#let`).
fn fixup(typ: &str) -> String {
    typ.replace("angle.l", "chevron.l").replace("angle.r", "chevron.r")
}

fn engine(src: String) -> TypstEngine<typst_as_lib::TypstTemplateMainFile> {
    TypstEngine::builder().main_file(src).fonts(typst_assets::fonts()).build()
}

fn main() {
    let formulas: Vec<String> =
        std::fs::read_to_string("formulas.txt").unwrap().lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    let mut converted = vec![];
    let mut convert_ms = 0.0;
    for f in &formulas {
        let t = Instant::now();
        let typ = fixup(&mitex::convert_math(f, None).unwrap_or_else(|e| panic!("{f}: {e}")));
        convert_ms += t.elapsed().as_secs_f64() * 1000.0;
        converted.push(typ);
    }
    // One document per formula: cold first, then warm (incremental compilation cache).
    let mut fails = vec![];
    let mut ok = vec![false; formulas.len()];
    let mut times = vec![];
    for (i, (f, typ)) in formulas.iter().zip(&converted).enumerate() {
        let src = format!("#set page(width: auto, height: auto, margin: 3pt)\n#set text(size: 14pt)\n{PRELUDE}\n$ {typ} $\n");
        let t = Instant::now();
        let r = engine(src).compile::<PagedDocument>();
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        match r.output {
            Ok(doc) => {
                let _svg = typst_svg::svg(&doc.pages()[0], &typst_svg::SvgOptions::default());
                times.push(ms);
                ok[i] = true;
            }
            Err(e) => {
                let msg = format!("{e:?}");
                let m = msg.split("message: \"").nth(1).and_then(|r| r.split('"').next()).unwrap_or(&msg).to_string();
                fails.push(format!("`{f}` → `{typ}`: {m}"));
            }
        }
    }
    times.sort_by(f64::total_cmp);
    // Reference sheet: all formulas, numbered, two columns.
    let mut body = String::new();
    for (i, (f, typ)) in formulas.iter().zip(&converted).enumerate() {
        if !ok[i] {
            body.push_str(&format!("[{}], [#raw({:?})], [_not supported yet_],\n", i + 1, f));
            continue;
        }
        body.push_str(&format!("[{}], [#raw({:?})], [$ {typ} $],\n", i + 1, f));
    }
    let sheet = format!(
        "#set page(paper: \"a4\", margin: 1.2cm)\n#set text(size: 9pt)\n{PRELUDE}\n= Kahie — P0-09 reference formulas (LaTeX → MiTeX → Typst)\n#table(columns: (auto, 1fr, 1fr), stroke: 0.3pt + gray, inset: 4pt,\n[*\\#*], [*LaTeX*], [*Rendered*],\n{body})\n"
    );
    let t = Instant::now();
    let doc: PagedDocument = engine(sheet).compile().output.expect("sheet compiles");
    let sheet_ms = t.elapsed().as_secs_f64() * 1000.0;
    let pdf = typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()).expect("pdf");
    std::fs::write("sheet.pdf", &pdf).unwrap();
    let svg = typst_svg::svg_merged(&doc, &typst_svg::SvgOptions::default(), typst::layout::Abs::pt(10.0));
    std::fs::write("sheet.svg", &svg).unwrap();

    println!("| Measure | Value |\n|---|---|");
    println!("| Formulas | {} |", formulas.len());
    println!("| LaTeX → Typst conversion (MiTeX), total | {convert_ms:.1} ms |");
    println!("| Compiled and laid out | {} / {} |", times.len(), formulas.len());
    if !times.is_empty() {
        println!("| Per formula (new engine each time), median / p95 / max | {:.1} / {:.1} / {:.1} ms |", times[times.len() / 2], times[times.len() * 95 / 100], times[times.len() - 1]);
    }
    println!("| Whole 100-formula sheet (layout) | {sheet_ms:.0} ms, {} page(s) |", doc.pages().len());
    println!("| Sheet PDF / SVG size | {:.0} KiB / {:.0} KiB |", pdf.len() as f64 / 1024.0, svg.len() as f64 / 1024.0);
    for f in &fails {
        println!("\nFAIL {f}");
    }
}
