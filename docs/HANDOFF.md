# Hand-off

**Date:** 2026-11-24 · **Working branch:** `ccr-121ac074-e1jgb6` at `1bacf67` · **`main`:** `c9527ec` (PR #5)

This page is for whoever picks up Kahie next, whether a person or an agent session. It covers the current state, what changed in the last session, how to work on the code, and what remains. `docs/STATUS.md` keeps the detailed, item-by-item status. `docs/user-guide.md` describes every feature from the user's side.

## 1. Summary

- **Product:** Kahie is a native Android notebook for teachers and students (Android 12+). It combines handwriting, typed text, maths, plots, dynamic geometry, PDF annotation, and sync through a self-hostable relay with optional end-to-end encryption. The app UI is Kotlin; everything else runs in a Rust core (`core/`) called through UniFFI. The CAS is Giac.
- **Phase 0** (spikes): done on desktop. Device measurements are still pending because no test devices are available.
- **Phase 1** (notebook app): complete except handwriting recognition, which needs a corpus. P1-42 (desktop) was dropped; P1-47 (pilot) needs the owner.
- **Phase 2** (maths layer):
  - Done: E2.3–E2.7, E2.12, E2.13.
  - Partly done: E2.8–E2.11.
  - Not started: E2.1, E2.2, layers.
  - The exit criteria are **not met** yet: the curriculum suite has 118 of the required 300 tasks, and the 100 constructions with instruments do not exist.
- **CI:** both workflows are green on `1bacf67`: `ci` run 78 and `android` run 56 (APK build plus emulator smoke test).
- **Not tested on a real device.** Every UI change has only been compiled and smoke-tested on an emulator.

## 2. Branches and merging

| Ref | Content |
|---|---|
| `main` (`c9527ec`) | Everything up to PR #5 (Phase 1 and the Phase 2 maths layer). |
| `ccr-121ac074-e1jgb6` (`1bacf67`) | `main`, plus PR #6 (image formats, merged into this branch), plus 6 commits from 2026-11-24. **7 commits ahead of `main`; no open PR.** |

Commits on the branch that are not yet on `main`:

| Commit | Change |
|---|---|
| `7e90e8d` | Insert → Image: SVG/SVGZ kept as vector, TIFF decoded by the core, clear message for unsupported formats (PR #6). |
| `845740d` | Search covers the text layer of imported PDF pages (P1-23; search index version 2). |
| `fb1b90c` | Select tool: dragging over PDF text opens a dialog to copy the selected text (P1-23). |
| `0b7bb69` | Infinite canvas: PDF/PNG/SVG/LaTeX exports cropped to the content (P1-19). |
| `8f00138` | Auto-width text boxes (P1-17). `set_rich_text` gained a `width` argument. |
| `6eea2d5` | Shape tidying modes, snapping and instruments (E2.8, E2.9, E2.10). |
| `1bacf67` | Lycée curriculum test suite for `nb-math`, 118 tasks. |

**Merging rules (keep them):**
- **Always squash-merge into `main`.** An old commit on the branch (`a7ad8c9`) contains about 300 MiB of build output, so a plain merge would bring it into `main`.
- **After each squash-merge, record `main` in the branch** with `git merge -s ours origin/main` (as in `eb34e46`). Later PRs then show only new work.
- **Push only to `ccr-121ac074-e1jgb6`, and never force-push.** Open or merge PRs only when the owner asks.

**Next merge:** open a PR from `ccr-121ac074-e1jgb6` to `main`, wait for the checks, squash-merge, then run the `-s ours` merge described above.

## 3. What the last session built (details)

### Drawing aids (E2.8–E2.10)

| Part | Where | Behaviour |
|---|---|---|
| Tidy modes | `nb-ffi/src/tools.rs` (`TidyMode`, `end_stroke_tidy`, `set_shapes_shown`, `shapes_shown`) | **Free:** shapes are tidied only by the hold gesture. **Suggest:** the recognised shape is stored as `Interp { show: false }` and a "Tidy shape" chip appears for 4 s. **Auto:** the shape is shown at once. The raw ink is always kept (ADR-0003), and switching between ink and shape is one undo step. |
| Snapping | `nb_recog::snap` | Shapes snap to key points of other tidied shapes (ends, midpoints, corners, centres). Otherwise they snap to the grid of `grid`/`dots` paper (24 units) or `graph` paper (5 mm). Lines within 6° of a multiple of 15° are turned to that angle. |
| Instruments | `nb_recog::instruments`; state in `State.instrument` (per view, not saved) | **Ruler:** strokes follow its edge, clamped to its length. **Compass:** strokes follow the circle. **Protractor:** strokes become rays at whole degrees; the direction is fixed once the pen is 24 units from the centre. A stroke is guided only if it starts within the capture distance (16 dp). Guided strokes are never re-recognised. Readings use 1 cm = 794/21 page units. |
| Android | `Instruments.kt` (overlay, finger drag/turn/open), `DrawingAidsUi.kt` (menus, chip, selection entries), small hooks in `InkView.kt` | Settings live in the preferences `tidyMode` and `snapping`; the old `autoShapes` preference is migrated. Menu entries: *More → Shapes and snapping* (the wand button in Advanced mode) and *More → Instruments*. Guided strokes bypass the Jetpack Ink wet layer, so the drawn line matches the projected points. |

### Curriculum suite

- **Files:** `core/crates/nb-math/tests/curriculum.rs` with `tests/curriculum/{seconde,premiere,terminale}.json` (40 + 40 + 38 tasks).
- **Run it:** `cargo test -p nb-math --test curriculum -- --nocapture` prints the pass count per file and in total. Current result: **118/118**.
- **Check kinds:** `eval`, `derivative`, `roots`, `inflections`, `vertical_asymptotes`, `extrema`, `intersections`, `tangent`, `asymptote`, `integral`, `construction`, `constraints`. The module documentation in `curriculum.rs` describes the format.
- **Expected values were written from the mathematics, not taken from the engine.** A deliberately wrong expectation was confirmed to fail.
- **Gaps:** a task the engine cannot do yet gets `"gap": "<reason>"`. Such tasks are run and reported but do not fail the test, and the test fails if a gap starts to pass.

### Other changes

- **PDF search hits** have kind `PdfText` and no block id. The search index version was raised to 2, so old caches are rebuilt automatically.
- **Infinite-canvas export area** comes from `Page::extent()`, the content bounds plus a 40-unit margin.
  - Kotlin side: `PageRenderer.exportArea`.
  - Rust side: `docexport.rs` uses the extent width for LaTeX figure widths.
- **Auto-width text boxes:** a box counts as auto-width while its width equals the natural width of its text (within 1.5 px). This uses `RichText.naturalWidth`; there is no stored flag, so the document schema is unchanged.

## 4. How to work on it

| Task | Command / place |
|---|---|
| Core checks (run before every push) | in `core/`: `cargo fmt`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked` |
| Kotlin compile check without the Android SDK | `app/android/ci/kt-check.sh <cache-dir>`: about 1 min after a one-time 270 MB download; also validates the resource XML |
| Accessibility check | `python3 app/android/ci/a11y-check.py` (also runs in `ci`) |
| APK plus emulator smoke test | `.github/workflows/android.yml`, about 17 min. **Each push touching `core/**` or `app/android/**` cancels the running build, so batch pushes.** Pushes that only touch docs do not trigger it. |
| CAS libraries | `.github/workflows/giac-android.yml` publishes `giac-libs-<target>`. `android` downloads the latest successful run. |
| Smoke-test failures | `app/android/ci/smoke.sh` prints a step trail, crash lines and Android's exit reason. The job-log API returns only the last ~100 lines. |
| Installable APK | The `kahie-release-apk` artifact of the latest `android` run, signed with a stable test key (PR #4), so updates install over each other. Latest: https://github.com/Heritiana-Ankatso/Kahie/actions/runs/45315099107 |

Conventions:
- `docs/engineering/conventions.md`
- ADRs in `docs/adr/`. Notable: ADR-0003, immutable ink with interpretations.
- Open decisions in `docs/decisions-open.md`.
- UI strings exist in three languages: `values`, `values-fr`, `values-mg`. **Every new string needs all three.** No bare apostrophes (use `’` or `\'`).

## 5. What remains

### Needs the owner

- **Real-device testing.** Check in particular: instruments (dragging, turning, guided strokes), the tidy chip, auto-width text, PDF text selection, and exports of the infinite canvas.
- **Native-speaker review of the Malagasy UI** (`values-mg/`). All of it is a draft, including the 13 strings added on 2026-11-24 under "Drawing aids".
- Devices for the Phase 0 measurements (P0-02/03/04/08, P0-15); the handwriting corpus (P0-06/16, D2); the trademark check (D10); the pilot (P1-47).
- **Scheduled tasks:** a cloud session created by the agent cannot push to this repository, so the 15:00 task of 2026-11-23 failed. To run unattended work, create the scheduled task from claude.ai with this repository selected, so it can push.

### Next engineering steps, in order

1. **Merge to `main`** following §2 (only if the owner asks).
2. **Curriculum suite to 300 tasks** (Phase 2 exit criterion).
   - Add symbolic tasks, which need a CAS check kind with the `giac` feature, and sequences/probability tasks.
   - Any task the engine fails should get a `gap` entry and then a fix.
3. **100 instrument constructions** (exit criterion): an automated suite that drives `PageDoc` with guided strokes and checks the resulting geometry.
4. **Finish E2.9–E2.10:**
   - smart guides (alignment lines while moving)
   - snapping while moving or resizing objects
   - tangent snapping
   - set squares
   - showing the instrument's position persistently across pages (today it is per view)
5. **E2.11 layers, E2.1 maths editor, E2.2 typesetting** (Typst + MiTeX spike done in P0-09).
6. **P0-05:** Android tile cache from `nb-render` display lists. Justify it with device frame rates first.
7. **Handwriting recognition** (P1-29–31), once the corpus exists.

### Known limitations

- Images and PDF backgrounds live in the page CRDT. This is fine for worksheets, but large PDFs will need a separate asset store.
- Horizontal page mode shows one page at a time.
- The instrument is not saved and is not synced; this is intentional.
- Snapping applies to tidied shapes only, not to raw ink or to blocks.
