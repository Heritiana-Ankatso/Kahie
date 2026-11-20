# Project status and handoff

Living file: updated at the end of every work session so the next one can resume without context.
Working branch: `ccr-121ac074-e1jgb6` (not yet merged to `main`; squash-merge recommended because commit `a7ad8c9` accidentally contained ~300 MiB of build output, removed in `cc09da9`).

**Last updated:** 2026-11-20

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
| P0-07 CAS (Giac) | Builds for Android (arm64 + x86_64), 88/88 on desktop | `docs/spikes/p0-07-giac.md`; `nb_math::cas` (feature `giac`); arm64 lib 23.7 MB / 10 MB gzipped; size reduction pending |
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

Built and smoke-tested on CI up to commit `85b5e83` (APK built, app ran on the emulator without crashing, ink appeared). Everything after that is **compiled only by CI, not yet confirmed** — see "CI state" below.

- Ink: pen/highlighter/eraser on the Rust core, Jetpack Ink wet layer, shape snapping, batched input (`add_points`), undo/redo + two-/three-finger tap.
- Storage: every edit saved immediately (SQLite per page, `.kpage`); legacy JSON pages migrate.
- Notebooks (switch/create/rename), Pages dialog (open, add, delete to trash, Trash restore, export page PNG).
- Blocks: Text (tap to add/edit), Maths (LaTeX → MathJax in hidden WebView → SVG → AndroidSVG Picture), Plot (functions of x, sampled in the core), Image (photos, stored in the page CRDT `assets` map), Select tool (rectangle select, move, duplicate, delete).
- Paper: lined, grid, dots, mm graph, Cornell, music, blank; PDF import as uneditable page background.
- Export/import: `.kahie` (ZIP, schema-valid `document.json`, `assets/<sha256>`), vector PDF of the notebook, PNG of a page.
- Sync: self-hosted relay (`nb-relay`), Share/Join/Settings, live sync of the open page + outline, background one-shot sync of other pages every 5 min, status dot.
- CAS: `cas_eval` FFI + Compute button in the text dialog (only in builds with feature `giac`).

## CI state (at handoff, 17:40 UTC 2026-11-20)

- `ci` (web, core tests, cargo-deny, offline tests with loopback only, schema): green.
- `giac-android` (arm64 + x86_64 matrix): **green**; publishes `giac-libs-<target>` artifacts.
- `android` (APK + emulator smoke): last finished run (85b5e83) built the APK; smoke failed only on a too-strict pixel threshold (142 dark px; threshold now 100, fixed in `ac15472`). **Run for `3a26664` is the first that builds all features above and links the CAS** (android.yml downloads giac-libs, `KAHIE_FEATURES=giac`, step summary "CAS: included"). Expect Kotlin compile errors and possibly link errors (static libc++/c++abi, `GIAC_LIB_DIR_<target>` in `nb-math/build.rs`) — fix them first.

How to iterate without the Android SDK here: generate bindings and compile them with kotlinc (`docs/engineering/conventions.md` §6; kotlinc 2.2.21 zip from GitHub releases, JNA jar from Maven Central). Push in batches: every push to `core/**` or `app/android/**` cancels the running `android` job (~35 min). To push only workflow/doc commits without cancelling it, reorder them first and push that commit (`git push origin <sha>:ccr-121ac074-e1jgb6`).

## Needs the owner (cannot be done by the agent)

- Buy/borrow reference devices (P0-15); run device measurements for P0-02/03/04/08.
- Approve the corpus protocol and consent text; organise collection sessions; native-speaker review of Malagasy prompts.
- Trademark check for "Kahie" (D10).
- Merge the working branch into `main` (squash).

## Next steps for the agent (in order)

1. Get the `android` run for `3a26664` (or later) green: fix Kotlin compile errors in the new features, CAS link errors, smoke test. Then mark the APK + CAS as working here.
2. Extend the smoke test: open each tool (text, maths, plot) via adb taps and check no crash; screenshot artifacts.
3. CAS size reduction (`-Os`, `-ffunction-sections`/`--gc-sections`, drop unused Giac modules); target ≤ 12 MB arm64 uncompressed.
4. Section UI in the Pages dialog (core supports sections); rename pages.
5. P0-05 Android tile cache using `nb-render` display lists (needs device fps to justify).
6. Relay per-document access control (roles) and optional end-to-end encryption.
7. Handwriting recognition (D2 pending P0-06 corpus).

Known limitations: images/backgrounds live in the page CRDT (fine for photos/worksheets; large PDFs will need a side asset store); the open page does not see edits made by the background sync pass until reopened; text blocks are plain text (no rich text yet).
