# Phase 0 — Foundations & de-risking (months 0–3)

**Milestone M0 exit criteria**
- All spikes (P0-02 … P0-10) delivered a written report in `docs/spikes/` with a recommendation.
- ADR-0001 … 0006 moved to *Accepted* (or superseded).
- Decisions D1–D5 resolved by the product owner.
- Repo, CI, core skeleton, model v0 and storage v0 merged on `main`.
- Ink prototype meets latency budget on reference devices.

**Spike rules:** time-boxed; output = report (context, method, measurements, recommendation) + throwaway or reusable prototype code in a clearly marked directory (`spikes/<id>/`). Spike code is not merged into production crates without review.

| ID | Title | WS | Size | Depends on |
|---|---|---|---|---|
| P0-01 | Repository, build and CI | All | M | — |
| P0-02 | Ink latency spike | WS1 | M | P0-15 |
| P0-03 | CRDT library evaluation | WS2/WS7 | M | — |
| P0-04 | Rust ↔ Kotlin FFI spike | WS1/WS2 | S | P0-01 |
| P0-05 | Rendering pipeline spike | WS1/WS2 | M | P0-04 |
| P0-06 | Handwriting recognition benchmark | WS3 | L | D2, D5, P0-15 |
| P0-07 | CAS evaluation | WS4 | M | D1 |
| P0-08 | On-device LLM benchmark | WS6 | M | D4, D5, P0-15 |
| P0-09 | Typesetting engine evaluation | WS4 | S | — |
| P0-10 | Firebase cost & latency simulation | WS7 | S | P0-03 |
| P0-11 | Document model v0 | WS2 | L | P0-03 |
| P0-12 | Storage v0 | WS2 | M | P0-11 |
| P0-13 | `.kahie` format specification draft | WS2 | S | P0-11 |
| P0-14 | UX research & first-run prototype | WS8 | L | — |
| P0-15 | Reference device lab | QA | S | D4 |
| P0-16 | Handwriting corpus collection protocol | WS3/QA | S | — |

---

### P0-01 — Repository, build and CI
- **WS:** All · **Size:** M · **Owner:** Tech lead
- **Tasks**
  - Create monorepo layout per [overview §2](../architecture/overview.md#2-repository-layout-target).
  - Rust workspace with empty crates; `cargo fmt`, `clippy -D warnings`, `cargo test`.
  - Gradle (Kotlin DSL) for `app/android`, `app/desktop`, `app/shared-ui`; `cargo-ndk` integration for Android ABIs (arm64-v8a, x86_64 for emulator).
  - CI (GitHub Actions): Rust lint/test, Android build + unit tests, desktop build, licence scanning (`cargo-deny`), dependency audit.
  - CI job running all tests with **network disabled** (see [conventions §7](../engineering/conventions.md#7-privacy-and-offline-rules)).
- **Acceptance:** a PR touching any module triggers CI in < 15 min; `main` protected; a debug APK installs on a reference device and shows a blank Compose screen calling a Rust function.

### P0-02 — Ink latency spike
- **WS:** WS1 · **Size:** M · **Owner:** Android/ink engineer
- **Tasks**
  - Prototype wet-ink with front-buffered rendering and platform motion prediction; compare against plain Compose `Canvas` and the Jetpack Ink library.
  - Build a **latency measurement harness**: high-speed camera (≥ 240 fps) method + in-app timestamp instrumentation.
  - Test palm rejection behaviour and stylus features (pressure, tilt, hover, button) on each reference device.
- **Acceptance:** report with latency per device and approach; recommended approach meets ≤ 25 ms (high-end) / ≤ 35 ms (mid-range); list of device-specific quirks.

### P0-03 — CRDT library evaluation
- **WS:** WS2/WS7 · **Size:** M · **Owner:** Rust core engineer
- **Candidates:** Loro, Automerge, Yrs. **Loro chosen (D6);** run the scenarios on Loro to confirm it meets the budgets.
- **Benchmark scenarios**
  - Document with 100 000 strokes (≈ 150 points each): load time, memory, file size (with/without history).
  - Two replicas diverge offline for one simulated week (5 000 ops each) → merge time.
  - Movable tree for sections/pages: concurrent moves.
  - Rich text concurrent edits.
  - History: checkout version N, revert single change, attribution per actor.
  - Binary size on Android; FFI ergonomics; licence; maintenance activity.
- **Acceptance:** report with measurements on mid-range reference device and recommendation; ADR-0001 updated with choice.

### P0-04 — Rust ↔ Kotlin FFI spike
- **WS:** WS1/WS2 · **Size:** S · **Owner:** Rust core engineer
- **Tasks:** UniFFI bindings for a mock `commit_stroke(points)` and `subscribe_scene_changes`; measure round-trip and allocation; test async/cancellation; test panic safety (Rust panic must not crash the app silently — mapped to error + crash report).
- **Acceptance:** committing a 500-point stroke ≤ 1 ms; guidelines draft for FFI API design (batching, ownership, error mapping) added to conventions.

### P0-05 — Rendering pipeline spike
- **WS:** WS1/WS2 · **Size:** M · **Owner:** Rust core + Android engineer
- **Tasks:** core produces a display list for a tile; Android executes it on Skia; tile cache; LOD simplification; test with 50 000 synthetic strokes on infinite canvas; same display list → SVG/PDF writer prototype.
- **Acceptance:** ≥ 55 fps pan/zoom (p95 frame ≤ 18 ms) on mid-range device; exported PDF visually matches screen (golden diff ≤ 0.5 %); fallback analysis if targets missed (ADR-0002 fallback).

### P0-06 — Handwriting recognition benchmark
- **WS:** WS3 · **Size:** L · **Owner:** ML engineer · **Blocked by:** D2, D5
- **Tasks**
  - Text: ML Kit Digital Ink vs an open model on own corpus in French, English and Malagasy (D5): CER, latency, model size. MyScript is excluded by the GPL-3 licence (D1).
  - Maths: a baseline open model trained on CROHME + MathWriting (licence check for each dataset), evaluated on CROHME test + own corpus: exact match, top-3, symbol accuracy, latency on device after int8 quantisation.
  - Malagasy handwriting corpus: collection plan and size estimate.
- **Acceptance:** report with tables; buy/build recommendation for D2 with effort estimate for "build" path; recalibrated targets for WS3.

### P0-07 — CAS evaluation
- **WS:** WS4 · **Size:** M · **Owner:** Maths engineer · **Blocked by:** D1
- **Tasks (Giac chosen, D7):** build Giac for Android arm64; binary size; run a 150-task subset of the curriculum suite (simplify, solve, diff, integrate, limits, series, matrices, inequalities); measure correctness and time; document gaps.
- **Acceptance:** report on Giac; draft `CasBackend` trait.

### P0-08 — On-device LLM benchmark
- **WS:** WS6 · **Size:** M · **Owner:** ML engineer · **Blocked by:** D4, D5
- **Tasks:** llama.cpp and LiteRT-LM on all reference devices; 3–5 candidate models (permissive licences, 1–4B, 4-bit); measure tokens/s, time to first token, RAM peak, battery drain per 10 min, thermal throttling; evaluate tool-calling accuracy on 50 tasks using a mock tool API (CAS, geometry describe/check) and explanation quality (blind teacher rating on 20 prompts) in target languages.
- **Acceptance:** report; proposed device tiers and default model; verdict on ADR-0005 feasibility; licence review of chosen models.

### P0-09 — Typesetting engine evaluation
- **WS:** WS4 · **Size:** S · **Owner:** Maths engineer
- **Tasks:** embed Typst maths layout and MicroTeX; render 100 reference formulas; measure binary size, layout time, visual quality vs LaTeX reference; check output to display list + PDF.
- **Acceptance:** report + recommendation; sample rendered sheet committed to `docs/spikes/`.

### P0-10 — Firebase cost & latency simulation
- **WS:** WS7 · **Size:** S · **Owner:** Backend engineer
- **Tasks:** simulate a 30-student class session (one shared canvas, 60 min, realistic stroke rates) and personal sync for 1 000 users; compare Realtime Database vs Firestore: cost per month, p95 latency, free-tier headroom; Drive appDataFolder quota/rate limits for personal sync.
- **Acceptance:** cost/latency table; recommended relay; input for D9 (business model of collaboration).

### P0-11 — Document model v0
- **WS:** WS2 · **Size:** L · **Owner:** Rust core engineers
- **Tasks:** implement `nb-model` per [data-model.md](../architecture/data-model.md) on the chosen CRDT: Notebook/Section/Page/Layer, `InkStroke`, `TextBlock` (plain), `Image`, `Interpretation` skeleton; command API (add/move/delete/restore/transform); per-user undo; change notifications; packed ink encoding.
- **Acceptance:** unit + property tests (convergence of random concurrent commands across 3 replicas, undo/redo invariants, ADR-0003 stroke-preservation invariant); ink encoding ≤ 6 bytes/point average on corpus.

### P0-12 — Storage v0
- **WS:** WS2 · **Size:** M · **Owner:** Rust core engineer
- **Tasks:** `nb-store` with SQLite (WAL): update log, snapshots, assets table, migration framework; kill-test harness.
- **Acceptance:** 1 000 randomised kill-during-write tests with zero corruption and zero loss of committed commands; load 2 000-stroke page ≤ 400 ms.

### P0-13 — `.kahie` format specification draft
- **WS:** WS2 · **Size:** S · **Owner:** Rust core engineer
- **Tasks:** write `docs/format/kahie-v0.md` (container layout, manifest schema, JSON schema for `document.json`, versioning policy).
- **Acceptance:** reviewed by tech lead; JSON Schema file validates a sample export.

### P0-14 — UX research & first-run prototype
- **WS:** WS8 · **Size:** L · **Owner:** Product designer
- **Tasks:** interviews/observation with 5 teachers, 5 students, 3 non-technical users; personas and top-20 task list; Simple-mode toolbar and library prototypes; cold-start test on prototype.
- **Acceptance:** research report; validated Simple-mode toolbar; interaction spec for Phase 1 library and page view.

### P0-15 — Reference device lab
- **WS:** QA · **Size:** S · **Owner:** QA engineer · **Blocked by:** D4
- **Tasks:** procure 3 Android stylus tablets (high-end, mid-range, minimum-supported), 1 phone, 1 Windows laptop with pen, 1 desktop with pen tablet; set up device farm scripts (adb, screen recording, latency camera rig).
- **Acceptance:** devices documented in `docs/engineering/devices.md` with specs and OS versions.

### P0-16 — Handwriting corpus collection protocol
- **WS:** WS3/QA · **Size:** S · **Owner:** ML engineer
- **Tasks:** consent form (purpose, retention, withdrawal), anonymisation, collection app (export raw strokes + labels), storage policy (evaluation set in repo only if consent permits; otherwise private bucket); target sizes per language and content type (text, maths, shapes, curves).
- **Acceptance:** protocol approved by product owner; collection app usable by non-engineers.
