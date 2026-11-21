# WS2 — Documents, PDF, export, search

**Lead role:** Rust core engineer · **Phases:** 0–2 · **Related ADRs:** 0001, 0006

## Mission

A robust, never-lose-data document system with the library, page types, templates, PDFs, images, search and standard-format export.

## Scope

- Document model (`nb-model`), storage (`nb-store`), `.kahie` format.
- Library: folders, notebooks, sections, pages; reorder/move; trash with restore.
- Page kinds & templates: fixed sizes, infinite canvas, backgrounds, template gallery (teaching, research, study, personal).
- Typed rich text blocks; images (insert, crop, rotate).
- PDF: import, render, annotate, insert pages, export flattened or with annotations.
- Undo/redo (per-user, per-document), autosave, crash recovery, local backup.
- Search: typed text, recognised handwriting, maths (normalised), notebook/page titles.
- Export: PDF (vector), SVG, PNG, Markdown (+ LaTeX maths), LaTeX (+ TikZ for diagrams/geometry), `.kahie`, JSON.

## Technical approach

| Topic | Approach |
|---|---|
| Undo | Command-based undo over CRDT: each user command records inverse ops; undo applies inverses (only own changes) |
| Autosave | Every committed command is durable within 1 s (SQLite WAL); no "save" button |
| Backup | Scheduled local `.kahie` export to user-chosen folder (Android Storage Access Framework); Drive backup in Phase 4 |
| PDF render | PDFium (BSD) via Rust bindings; per-page tiles; text layer extracted for search/selection |
| PDF annotate | Annotations are ordinary objects on a page with `pdf_source`; PDF itself never modified |
| PDF export | Display list → PDF writer (vector paths, embedded fonts, original PDF pages as XObjects) |
| SVG/PNG | Display list → SVG writer / rasteriser |
| Markdown | Text blocks + `$…$` maths; ink/diagrams exported as embedded SVG/PNG |
| LaTeX | Document skeleton + `amsmath` maths; geometry/plots/diagrams as **TikZ/pgfplots** generated from construction graph |
| Search | SQLite FTS5 per notebook + global index; maths indexed as normalised LaTeX token sequences and canonical expression strings; Phase 5 adds embeddings |

## Deliverables by phase

| Phase | Deliverable |
|---|---|
| 0 | Model v0, storage v0, `.kahie` spec draft (P0-11…P0-13) |
| 1 | Library, pages, templates, text, images, PDF, undo/autosave, search (text/HWR), PDF/PNG/SVG export, `.kahie` export/import |
| 2 | Markdown, LaTeX/TikZ export; maths search |

## Quality metrics

- Zero data loss under kill -9 / power-cut fuzzing (1 000 randomised kill tests per release).
- Export fidelity: golden-image diff ≤ 0.5 % pixel difference between on-screen render and exported PDF raster.
- PDF compatibility: corpus of ≥ 200 real-world PDFs (textbooks, scanned exams, forms) opens without error.
