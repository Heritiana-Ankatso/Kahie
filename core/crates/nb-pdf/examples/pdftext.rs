//! Print the page sizes and text layer of PDF files (debugging the text extraction).
//! `cargo run -p nb-pdf --example pdftext -- file.pdf [--words]`

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let words = args.iter().any(|a| a == "--words");
    for path in args.iter().filter(|a| !a.starts_with("--")) {
        let bytes = std::fs::read(path).expect("read file");
        let t = std::time::Instant::now();
        match nb_pdf::PdfDoc::load(&bytes) {
            Ok(pdf) => {
                println!("# {path}: {} pages, parsed in {:?}", pdf.page_count(), t.elapsed());
                for i in 0..pdf.page_count() {
                    let info = pdf.page_info(i).expect("page");
                    let t = std::time::Instant::now();
                    let list = pdf.words(i);
                    println!(
                        "## page {} ({:.0} x {:.0} pt, rotate {}): {} words in {:?}",
                        i + 1,
                        info.width_pt,
                        info.height_pt,
                        info.rotate,
                        list.len(),
                        t.elapsed()
                    );
                    if words {
                        for w in &list {
                            println!("{:.3} {:.3} {:.3} {:.3} {}", w.x0, w.y0, w.x1, w.y1, w.text);
                        }
                    } else {
                        println!("{}", nb_pdf::words_to_text(&list));
                    }
                }
            }
            Err(e) => println!("# {path}: error {e}"),
        }
    }
}
