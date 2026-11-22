# Engineering conventions

## 1. Branching and reviews

- Trunk-based: short-lived feature branches → PR → `main` (protected).
- Branch names: `<type>/<ticket-id>-<short-desc>`, e.g. `feat/P1-11-partial-eraser`, `spike/P0-03-crdt`.
- Commit messages: imperative mood, reference ticket ID (`P1-11: implement partial eraser without stroke mutation`).
- Every PR: ≥ 1 approving review; ≥ 2 for `nb-model`, `nb-store`, `nb-sync`, FFI or data-format changes.
- Documentation updated **in the same PR** as the behaviour it describes.

## 2. Definition of Ready (ticket)

- Goal and acceptance criteria written and testable.
- Dependencies resolved or explicitly mocked.
- UX spec available (for user-facing tickets).
- Size ≤ L (XL must be split).

## 3. Definition of Done

- Acceptance criteria met and demonstrated on a reference device (for user-facing work).
- Tests added (see §4); CI green, including offline job and performance benchmarks.
- No new lint warnings; licence check passes.
- Accessibility labels for new UI.
- Docs/ADRs updated; changelog entry for user-visible changes.

## 4. Testing strategy

| Level | Tooling | Required for |
|---|---|---|
| Unit | `cargo test`, JUnit/Kotlin test | All crates/modules |
| Property-based | `proptest` | CRDT convergence, undo/redo invariants, ADR-0003 ink preservation, solver invariants, format round-trips |
| Fuzzing | `cargo-fuzz` | PDF import, `.kahie` import, LaTeX/expression parsers |
| Golden images | Display-list rasteriser in CI; device screenshots nightly | Rendering, export |
| Curriculum suites | Custom runners | CAS (≥ 300 tasks), constructions (≥ 100), curve fitting (200 sketches) |
| Recognition evaluation | `ml/` evaluation scripts on frozen sets | Any model/engine change |
| UI / instrumentation | Compose UI tests, Android instrumented tests | Critical flows (first run, write/erase, export) |
| Kill / crash tests | Harness from P0-12 | Storage, autosave |
| Soak / network-partition | Sync simulator | Phase 4 onwards |

## 5. Performance regression

- Benchmarks for every budget in [overview §6](../architecture/overview.md#6-performance-budgets) (criterion for Rust; macrobenchmark for Android).
- CI fails on > 10 % regression versus `main` baseline; nightly runs on physical reference devices.
- **Implemented (P1-43):** `core/crates/nb-bench` (`cargo run --release -p nb-bench -- --check`), CI job `bench`. Desktop-measurable budgets: open a 2 000-stroke page (≤ 400 ms), cached tile p95 on a 50 000-stroke canvas (≤ 18 ms), durable stroke commit (≤ 1 s), shape recognition (≤ 50 ms), 300-page PDF parse + first page text and first page imported (≤ 2 s); full 300-page import tracked for regressions. Not measurable here: wet-ink latency, cold start, fps on device, HWR (not built), constraint solving (not built), search (other work stream), APK size (android job).
- Noise handling: medians of 5–21 repetitions, 3 interleaved rounds (best kept), values normalised by a calibration workload of the same round, failing metrics re-measured twice. Metrics dominated by disk flushes or tail percentiles (`commit_stroke_durable`, `tile_50k_strokes_p95`, PDF imports) use a 35 % band: identical runs vary by up to ±30 % on a shared machine.
- The baseline (`nb-bench/baseline.json`) is compared only when it was recorded on the same runner type (`KAHIE_BENCH_RUNNER`; CI sets `github-ubuntu-latest`). To (re)arm the check after an intended change or a runner change: take the `bench` artifact of a green CI run on `main` and commit its `bench.json` as `baseline.json`. Until then CI checks the absolute budgets only.
- Golden images (P1-44): `core/crates/nb-render/tests/golden.rs` rasterises display lists (tiny-skia) and compares PNG + SVG goldens; `UPDATE_GOLDEN=1` regenerates them; failures upload actual and diff images (`golden-failures` artifact). Device screenshot goldens need the device lab (P0-15).

## 6. Rust ↔ Kotlin FFI guidelines

- Coarse-grained calls: commit whole strokes/commands, never per point.
- Core → UI notifications are batched change sets per frame.
- No Rust panics across FFI: catch at boundary, map to typed errors, report.
- Ownership: large buffers passed as byte arrays; no long-lived references across the boundary.
- All long operations async and cancellable.
- Hot input path: one call per input event (`add_points`), never per sample. Pen-up commit ≤ 1 ms (P0-04: 0.06 ms in memory, 0.47 ms durable on desktop).
- Error variants must not have a field named `message` (clashes with Kotlin `Throwable.message`); use `detail`.
- Check generated bindings compile before pushing: generate them with `uniffi-bindgen` and compile with `kotlinc -cp jna.jar` (no Android SDK needed).
- Check the whole app's Kotlin compiles before pushing: `app/android/ci/kt-check.sh` (no Android SDK needed; compiles against Robolectric's `android-all`, with stubs for `R` and `androidx.ink`). It does not replace the CI APK build (resources, manifest, AGP, linking).

## 7. Privacy and offline rules

- The core (`core/`) must not depend on any networking crate (enforced by `cargo-deny` bans).
- Network access only in `adapters/` and only when the user enabled the corresponding feature.
- CI runs the full test suite in a network-isolated container; any outbound connection fails the build.
- No analytics SDKs. Crash reporting per D8 (opt-in, no content).
- No user content in logs.
- ML models and datasets: licence and provenance documented in `ml/MODEL_CARDS.md`.

## 8. Code style

- Rust: `rustfmt` defaults, `clippy -D warnings`, public items documented.
- Kotlin: ktlint / detekt; Compose guidelines; no business logic in UI layer.
- Python (`ml/`): ruff + black; pinned environments.

## 9. Versioning and releases

- Semantic versioning for the app and for the `.kahie` format (independently).
- Format readers support all older format versions (migration tests with stored fixtures in `testdata/formats/`).
- Release channels: internal → pilot → beta → stable.
