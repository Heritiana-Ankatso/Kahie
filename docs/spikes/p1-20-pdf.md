# P1-20 … P1-24 — PDF in notebooks: decisions and measurements

**Date:** 2026-11-21 · **Status:** implemented (core tested on desktop; Android code compiled, not run on a device)

## Decision: render with Android's `PdfRenderer`, read text with `lopdf`

The backlog asks for PDFium (P1-20). PDFium was **not** linked directly:

| Option | Result |
|---|---|
| Build PDFium from source (gn/ninja, `depot_tools`) | Not possible here: needs Google's download hosts (`dl.google.com`, `chromium.googlesource.com`), blocked in this environment and heavy in CI (~1 h). |
| Prebuilt PDFium binaries (`pdfium-binaries`) + `pdfium-render` | Adds ~5–6 MB per ABI to the APK, a second copy of what Android already ships, and a native library to keep patched. |
| **Android `android.graphics.pdf.PdfRenderer`** (chosen) | It *is* PDFium, built into the OS (API 21+, our minimum is 31), patched by system updates, no APK cost, renders any page region at any scale (`Page.render(bitmap, clip, matrix, mode)`). |

What `PdfRenderer` lacks on our minimum SDK is text: the text APIs (`PdfRenderer.Page.getTextContents`, selection) arrive with Android 15 / SDK extensions only. The text layer therefore comes from the core, with the pure-Rust **`lopdf`** crate (MIT, `default-features = false`; its dependencies are MIT/Apache/BSD and pass `cargo deny`). `nb-pdf` interprets content streams itself (text and graphics state, `Tj`/`TJ`/`'`/`"`, form XObjects, `/Widths` and CID `/W`, `/ToUnicode` and encodings via `lopdf`) and returns words with boxes normalised to the displayed page (crop box and `/Rotate` applied). Other crates considered: `pdf-extract` (MIT, built on lopdf, gives text but no word boxes), `pdf` (MIT, larger API, no text layout helpers).

Revisit when the minimum SDK reaches 35 (platform text APIs) or if `PdfRenderer` proves too limited on reference devices (P0-15): `pdfium-render` would then replace it behind the same `PdfPages` interface in the app.

## Storage (P1-21)

- The original PDF is kept **once per notebook**: bytes in the notebook's asset store `assets.kpage` (a page database with only an `assets` map: saved, synced with the relay under the id `assets`, exported), plus a plain copy `pdf/<sha256>.pdf` for `PdfRenderer` (rebuilt from the store on demand, e.g. on a device that received the notebook by sync).
- Each notebook page that shows a PDF page stores `pdf_source` (`asset` = SHA-256, `page_index`) and its text layer in its own CRDT (`meta.pdf_asset`, `meta.pdf_page`, `meta.pdf_text`). `document.json` has the optional `pdf` field; `.kahie` files carry the PDF once as `assets/<sha256>`.
- Import runs in two steps (first page, then the rest) so a long PDF opens at once.
- Pages are sized like the PDF page (A4/A5/Letter when within 2 mm, else a custom size, scaled into 50–1000 mm when needed); the PDF page is fitted and centred.
- Pages imported before this change (raster background per page) keep working unchanged.

## Rendering on demand (P1-21)

`PdfPages.kt` renders on a background thread: a whole-page bitmap at the zoom in use (bucketed by powers of √2, at most 2048 px on the long side), and, when zoomed in further, a **detail tile** of the visible region only, at screen resolution. Bitmaps live in an LRU cache (1/8 of the heap). Until a render is ready the previous resolution (or a light placeholder) is drawn. Exports render each page synchronously at 2×.

## Measurements (desktop, release build, `nb-bench`)

| Step | Time |
|---|---|
| Parse a generated 300-page PDF, size and text of its first page | ~0.9 ms |
| Import its first page into a notebook (store the PDF once, create the page with its text layer) | ~10 ms |
| Import all 300 pages | ~1.2 s (done in the background after the first page is shown) |

The app imports page 1 alone, opens it, then imports the other pages after it, so the first page of a long PDF is writable almost at once; its rendering time on a device (budget 250 ms per A4 page) needs P0-15.

## Text layer (P1-23)

- `PageDoc.pdfText()` (plain text, for copy and search indexing), `pdfWords()` (words + boxes, for highlights) and `pdfTextIn(x0, y0, x1, y1)` (text under a selection rectangle, normalised coordinates).
- In the app: *More → PDF text* shows the page's text, selectable, with *Copy all*.

## Corpus (P1-24)

Real-world PDFs cannot be downloaded in CI, so `nb_pdf::testgen` generates **240** documents covering page sizes (A4, Letter, A5, A3 landscape, small square, long strip), `/Rotate` 0/90/180/270, crop boxes, inherited media boxes, four font set-ups (Standard, WinAnsi + `/Widths`, `/Differences`, Type 0 + `/ToUnicode`), kerning in `TJ`, text in form XObjects, scaled `cm`, compression, object and cross-reference streams, images, 1–7 pages and trailing garbage. Every page must give exactly its expected words with boxes inside the page. Damaged files (truncated, bytes flipped: 200 variants) must never panic. A directory of real PDFs can be checked with `KAHIE_PDF_CORPUS=<dir> cargo test -p nb-pdf real_world` (for a nightly job once a corpus is available).

Local check against `pdftotext` (poppler) on 11 real PDFs (LibreOffice and Typst output, a ReportLab-style showcase, the repository's sample files): word counts identical on the 6 LibreOffice documents, within 2–7 % on the others (differences in symbol glyphs such as ★); one file in the set is not a PDF and is rejected cleanly.

**Not covered on desktop:** rasterisation (it runs in Android's PDFium); render correctness of the corpus must be checked on a device (P0-15) or in the emulator job.
