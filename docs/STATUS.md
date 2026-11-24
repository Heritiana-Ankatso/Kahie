# Project status and handoff

Living file: updated at the end of every work session so the next one can resume without context.
Working branch: `ccr-121ac074-e1jgb6`. Merged into `main` by squash: PR #2 (to `920e87d`) and PR #3 (to `9911acd`). Keep squash-merging (commit `a7ad8c9` on the branch holds ~300 MiB of build output). After a squash-merge, the branch is merged with `main` (as in `f253ab2`) so later PRs show only new work.

**Last updated:** 2026-11-22

## Decisions (all in `docs/decisions-open.md`)

GPL-3.0-or-later · Android only (12+, 6 GB) · FR/EN/MG for UI and handwriting · Loro · Giac · opt-in crash reports · self-hostable relay · `.kahie` · D2 (recognition engine) waits for P0-06.

## Phase 0 progress

| Item | State | Evidence |
|---|---|---|
| P0-01 Repo, build, CI | Done except Android unit tests | `.github/workflows/ci.yml` (web, core, cargo-deny, offline tests, schema), `android.yml` (APK) |
| P0-02 Ink latency spike | Prototype only | Jetpack Ink wet layer in `app/android/.../JetpackWetInk.kt`; needs devices (P0-15) |
| P0-03 CRDT validation | Done on desktop | `docs/spikes/p0-03-loro.md`; device run pending |
| P0-04 FFI spike | Done on desktop | `docs/spikes/p0-04-ffi.md`; JNA overhead on device pending |
| P0-05 Rendering pipeline | Core done on desktop | `docs/spikes/p0-05-render.md`; Android tile cache + device fps pending |
| P0-06 HWR benchmark | Blocked on corpus | Protocol + tool ready (P0-16) |
| P0-07 CAS (Giac) | Done on Android (arm64 + x86_64), 88/88 on desktop, runs in the APK | `docs/spikes/p0-07-giac.md`; `nb_math::cas` (feature `giac`); arm64 lib **11.0 MB** (5.6 MB gzipped), was 23.7 MB |
| P0-08 On-device LLM | Not started | Needs devices |
| P0-09 Typesetting | Done on desktop | `docs/spikes/p0-09-typesetting.md`: Typst + MiTeX, 99/99 formulas; APK size reduction pending |
| P0-10 Relay | Done (with compaction) | `docs/spikes/p0-10-relay.md`, `core/crates/nb-relay`, `docs/relay.md` |
| P0-11 Document model v0 | Done (page level) | `nb_model::crdt`, 15 tests incl. 3-replica convergence |
| P0-12 Storage v0 | Done | `nb-store`, 1 000-kill test, 2 000-stroke load 14 ms |
| P0-13 `.kahie` spec | Draft done | `docs/format/` |
| P0-14 UX research | Not started | Needs users |
| P0-15 Device lab | Not started | Needs hardware purchase (owner) |
| P0-16 Corpus protocol | Draft done | `docs/spikes/p0-16-corpus-protocol.md`, `app/web/collect.html` |

## Android app (what exists)

**Built and smoke-tested on CI with the CAS at commit `07b7d76` (run 44817442793), including z-order, copy/paste, page versions, paper sizes, SVG export and page templates:** APK with Giac linked (arm64 + x86_64, 11.0 MB arm64); on the emulator the app starts, ink appears, the text dialog's Compute button gives `2^100 = 1267650600228229401496703205376` (exact, computed by Giac on the device), and maths, plot and pages open without a crash. The smoke test failed intermittently twice before (silent exit after the ink check); it now prints the failing line, native crash lines and the visible UI on any failure — read those first if it fails again.

- Ink: pen/highlighter/eraser on the Rust core, Jetpack Ink wet layer, shape snapping, batched input (`add_points`), undo/redo + two-/three-finger tap.
- Storage: every edit saved immediately (SQLite per page, `.kpage`); legacy JSON pages migrate.
- Library (P1-01/02, `LibraryScreen.kt`, `nb_model::library`, FFI `LibraryDoc`): folders of notebooks (Loro tree, convergence property test), grid/list with covers, recent first, dated trash purged after 30 days (P1-04). Search (P1-32, `nb-ffi/src/search.rs`, `SearchScreen.kt`): titles, text, maths, plots; accent/case-insensitive; cached index; maths by structure (E2.13, `nb_math::mathsearch`). Page list with thumbnails and drag reorder (P1-03, `PagesScreen.kt`). Restore a notebook from a backup (P1-27). PDF export page range (P1-33). A3 paper (P1-05).
- Editing (P1-06/10/11/14–19, `EditingUi.kt`, `RichText.kt`, `nb-ffi/src/editing.rs`, `nb_model::select`): pencil and brush with pressure/tilt curves (`nb_ink::Nib`, ink codec v2 with tilt), 12 colours + custom, 5 widths; partial eraser (original stroke kept in history, pieces added, one undo step), stylus eraser end and side button; lasso; scale/rotate handles (ink keeps its points, a stored transform `xf`); groups (`groups` root map); rich text as Loro rich text (bold/italic/underline, H1–H3, lists, colour); insert from camera/clipboard, crop and rotate images; file attachments; infinite canvas page size (`infinite`, zoom 5–3 200 %). `.kahie` document version 0.2.0. Auto-width text boxes (a box is auto-width while its width equals its text's natural width). Known gaps: InkView does not draw through `nb-render` (50 000-stroke budget measured on desktop only).
- PDF, UI shell, quality (P1-20–24, 37–41, 43–46): PDF stored once per notebook (`assets` page, synced), pages rendered on demand with Android `PdfRenderer` (decision and reasons in `docs/spikes/p1-20-pdf.md`; no PDFium), text layer via `nb-pdf` (lopdf) with *More → PDF text*, 240 generated + 200 damaged PDFs in tests; Simple (7 items) / Advanced toolbar in `Toolbar.kt`, top or side, left-handed; design tokens, dark mode (paper stays white); first-run, 5 tips and 3 tutorials (`Coach.kt`); opt-in local crash log without content (`CrashReports.kt`); CI jobs `bench` (`nb-bench`, absolute budgets; regression check needs a baseline recorded on CI: commit the `bench.json` artifact as `core/crates/nb-bench/baseline.json`) and `a11y`; golden images for `nb-render` and SVG export. Gaps: PDFs over ~15 MB do not sync (relay 16 MB message cap); no drag-select of PDF text on the page; PDF text not yet in search; TalkBack audit on a device.
- Notebooks (switch/create/rename), Pages dialog (open, add, rename, delete to trash, Trash restore, export page PNG; sections: create, rename, delete, move the open page into one).
- Languages: English, French, Malagasy (draft); in-app language choice (Notebooks → More → Language) plus Android 13 per-app language.
- Blocks: Text (tap to add/edit), Maths (LaTeX → MathJax in hidden WebView → SVG → AndroidSVG Picture), Plot (functions of x, sampled in the core), Image (stored in the page CRDT `assets` map: JPEG, PNG, WebP, GIF first frame, HEIF/HEIC, AVIF, BMP, ICO, DNG downsized to ≤ 1600 px as PNG/JPEG; TIFF decoded by the core (`nb-ffi/src/raster.rs`) to PNG; SVG/SVGZ kept as SVG text and drawn as a vector `Picture`, `SvgImages.kt`), Select tool (rectangle select, move, copy/paste across pages and notebooks, duplicate, delete, bring to front / send to back).
- Templates (P1-08): page templates (`Templates.kt`, six built-in + saved) and **notebook templates** (`NotebookDoc::copy_from_template`: sections, page titles, pages with new ids and no sync state; three built-in: course, monthly planner, exercise book; save the current notebook as one).
- Page scrolling: vertical (default) or horizontal (whole page fits; a sideways pan past the page edge turns the page; `InkView.horizontal`, pref `horizontalPages`).
- Page history (P1-28): local checkpoints (on open, then every 10 min, 30 kept) in the page database; restoring is a new undoable, synced change (`CrdtPage::revert_to`).
- Paper: lined, grid, dots, mm graph, Cornell, music, blank; size per page (A4/A5/Letter, portrait/landscape, or custom `<w>x<h>mm`, 50–1000 mm; `Page.size`, `nb_model::custom_size`, `.kahie` field `size`); PDF import as uneditable page background (landscape pages kept landscape).
- Export/import: `.kahie` (ZIP, schema-valid `document.json`, `assets/<sha256>`), vector PDF of the notebook (each page at its size), PNG and SVG of a page (`SvgExport.kt`: paper, ink, plots, text, maths as embedded MathJax SVG, images as data URIs).
- Sync: self-hosted relay (`nb-relay`), Share/Join/Settings, live sync of the open page + outline, background one-shot sync of other pages every 5 min (reloads the open page if it changed), status dot. Per-notebook access control: Share shows an editor code and a read-only code (students); read-only notebooks are view-only (`nb_store::access`, `docs/relay.md`). **Optional end-to-end encryption** (asked at first Share, default on): codes `n….e…` / `n….v…`; updates sealed with ChaCha20-Poly1305 (`ring`) in `nb-ffi/src/sync.rs`, document id as AAD; the relay sees only derived credentials and ciphertext and never compacts sealed logs. Plain `w…`/`r…` codes and codes without keys stay unencrypted.
- CAS: `cas_eval` FFI + Compute button in the text dialog, plus operation buttons Solve/Factor/Expand/Simplify/d/dx/∫/≈ (`cas_command`, E2.3) — only in builds with feature `giac`.

## Phase 2 (maths layer) progress

All in the plot block's text lines (`nb_math::plot`, syntax in its module docs and in the user guide), so the document model, sync and `.kahie` are unchanged.

| Epic | State | Where |
|---|---|---|
| E2.3 CAS in the UI | Operations on the last line (exact or ≈) | `nb-ffi/src/cas.rs` `cas_command`, `MainActivity.withCasTools` |
| E2.4 Plotting | Functions, parametric `(x(t), y(t))`, polar `r = f(θ)`, points, area shading | `nb_math::plot` |
| E2.5 Function analysis | Roots, extrema, inflections, intersections, vertical/horizontal/oblique asymptotes, tangent/normal, integral and area between curves (numeric, 3 decimals) | `nb_math::analysis`, "Analyse…" button |
| E2.6 Construction graph | Points, midpoints, lines/segments/rays, circles, perpendicular/parallel, bisectors, intersections, point on object, polygons, distance/angle; undefined states propagate; drag free points with the Plot tool | `nb_math::geo`, `PageDoc.move_plot_point/preview_plot`, `InkView.handlePlot` |
| E2.7 Constraint solver | `constrain distance/angle/horizontal/vertical/parallel/perpendicular/equal`; Levenberg–Marquardt, DOF and conflicts; 100 constraints ≈ 10 ms release (budget 16 ms) | `nb_math::solve` |
| E2.11 Styling & transformations (part) | `[red, thick, dashed, hidden]`; rotate, dilate, translate, reflect any object | `nb_math::geo`, `plot.rs` |
| E2.12 Markdown & LaTeX export | ZIP with `notebook.md`/`.tex` + assets; plots as pgfplots; compiles with pdflatex (CI installs texlive, `KAHIE_REQUIRE_PDFLATEX=1`) | `nb-ffi/src/docexport.rs`, `DocExport.kt` |
| E2.13 Maths search | Structural queries `x^2 + ?`, `\frac{?}{2}` | `nb_math::mathsearch` |
| E2.8 Shape beautification (modes) | Free / Suggest / Auto; suggestions kept as a hidden interpretation (`Interp.show = false`) accepted with one tap; original ink always recoverable (`set_shapes_shown`, one undo step) | `nb-ffi/src/tools.rs`, `DrawingAidsUi.kt` |
| E2.9 Snapping (part) | Shapes snap to key points of other tidied shapes (ends, corners, midpoints, centres), to the paper grid and to 15° angles. Not done: smart guides, tangent snapping, snapping while moving objects | `nb_recog::snap` |
| E2.10 Virtual instruments (part) | Ruler (cm), compass, protractor (whole degrees): strokes starting on them are guided in the core; readings while drawing. Not done: set squares, instrument templates | `nb_recog::instruments`, `Instruments.kt` |
| E2.1 maths editor, E2.2 typesetting engine, E2.11 layers | Not started | |

Phase 2 exit criteria (300-task curriculum suite, 100 constructions with instruments) are not met: no curriculum suite exists yet.

## CI state (2026-11-23)

- **All green on `ed03b3f`**: `android` run 44 (APK with the CAS, emulator smoke test: launch, ink, text + CAS 2^100, maths, plot, pages) and `ci` run 64 (Rust tests incl. pdflatex, cargo-deny, `bench`, `a11y`, schema). Everything is merged on the branch: Phase 2 work, the three Phase 1 branches.
- Runs 39–43 failed in the smoke test only (no app crash): the rich-text dialog's field had no focus (fixed in `12be8f0`), then the test tapped the page while Simple mode's More menu was closing (fixed in `ed03b3f`). `smoke.sh` now prints a step trail, crash lines (native included) and Android's exit reason on any failure.

## CI state (older, 23:08 UTC 2026-11-20)

- `ci` (web, core tests, cargo-deny, offline tests with loopback only, schema): green.
- `giac-android` (arm64 + x86_64 matrix): **green**; publishes `giac-libs-<target>` artifacts.
- `android` (APK + emulator smoke): **green with the CAS** (run 31, 44832660472, commit `b9368fc`: E2E encryption, custom sizes, horizontal scrolling, notebook templates). `ci` run 47 green on the same commit. It takes Giac libraries from the latest successful `giac-android` run on the branch (`gh run list` + `actions/download-artifact`) and fails if they are missing. After `giac-android` publishes new libraries, start `android` by hand (`workflow_dispatch`) or push. ~16 min (APK 11 + smoke 5).
- The job-log API returns only the last ~100 lines: put anything you need to read at the end of a step that fails, or in a `::notice::` annotation.

**Local Kotlin check:** `app/android/ci/kt-check.sh <cache-dir>` compiles the whole app against Robolectric's `android-all` (stubs for `R`, `androidx.ink`) and validates resource XML, in ~1 min. It caught nothing in the Kotlin but found an unescaped `<` in `plot_bad_window` that would have failed aapt in CI.

How to iterate without the Android SDK here: generate bindings and compile them with kotlinc (`docs/engineering/conventions.md` §6; kotlinc 2.2.21 zip from GitHub releases, JNA jar from Maven Central). Push in batches: every push to `core/**` or `app/android/**` cancels the running `android` job (~35 min). To push only workflow/doc commits without cancelling it, reorder them first and push that commit (`git push origin <sha>:ccr-121ac074-e1jgb6`).

## Needs the owner (cannot be done by the agent)

- Buy/borrow reference devices (P0-15); run device measurements for P0-02/03/04/08.
- Approve the corpus protocol and consent text; organise collection sessions; native-speaker review of Malagasy prompts and of the **Malagasy UI draft** (`app/android/app/src/main/res/values-mg/strings.xml`, written without a native speaker).
- Trademark check for "Kahie" (D10).
- Test the APK on a real tablet (CI artifact `kahie-debug-apk` of the latest `android` run; zip, then install `app-debug.apk`).

## Next steps for the agent (in order)

1. (Done: CAS 11.0 MB arm64.) Further CAS savings only if APK size becomes a problem: drop unused Giac modules in `prepare-giac.py`.
2. Phase 1 in parallel branches (2026-11-21/22): organisation & search merged (`8b77c90`, `9911acd`); editing tools merged (`2af45b1`, `cd22389`). PDF/UI/quality merged (`4e3d969`…`c33f52a`). Phase 1 remaining: P1-29–31 handwriting (corpus), P1-42 desktop (dropped by D3), P1-47 pilot (owner); partial items listed under "Android app". Earlier P1 items that need no device. (Done: P1-05 incl. custom sizes, horizontal scrolling, P1-08 page + notebook templates, P1-34 SVG export, P1-15 copy/paste, P1-16 z-order, P1-28 page versions, P1-27 backup, P1-36 share, E2E encryption of sync.)
3. (Done: client-side compaction of encrypted logs, `KHC1` frames, `Store::compact_upto`.) Horizontal scrolling shows one page at a time (no neighbour preview); refine after device testing.
4. P0-05 Android tile cache using `nb-render` display lists (needs device fps to justify).
5. Handwriting recognition (D2 pending P0-06 corpus).

Known limitations: images/backgrounds live in the page CRDT (fine for photos/worksheets; large PDFs will need a side asset store); text blocks are plain text (no rich text yet).
