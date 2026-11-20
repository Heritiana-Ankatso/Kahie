# P0-04 — Rust ↔ Kotlin FFI

- **Status:** Partial — core-side timings done; JNA call overhead on a reference tablet pending (P0-15)
- **Date:** 2026-11-20
- **Code:** `core/crates/nb-ffi` (UniFFI 0.32, Kotlin bindings over JNA). Timing test: `cargo test --release -p nb-ffi -- --ignored --nocapture`.

## Method

- Page already holding 200 strokes of 500 points; 50 further 500-point strokes, median and p95.
- Input sent the way the app now sends it: `add_points` once per input event (4 samples, as a 240 Hz stylus at 60 fps), then `end_stroke` at pen-up.
- Desktop container (Intel Xeon 2.3 GHz). Not a tablet: expect 2–4× slower on a mid-range ARM device (estimate).

## Results

| Step | In memory | Durable (SQLite WAL, fsync) |
|---|---|---|
| 499 points via `add_points` (whole stroke, median) | 0.007 ms | 0.013 ms |
| `end_stroke` (commit 500-point stroke), median | **0.063 ms** | **0.47 ms** |
| `end_stroke`, p95 | 0.097 ms | 0.73 ms |

Budget: committing a 500-point stroke ≤ 1 ms → **met** on desktop, including the synchronous durable write.

Before this spike, `end_stroke` cost 0.58 ms in memory and 1.0 ms durable: every change rebuilt the page view from the CRDT. Adds and removes now patch the view in place (verified against a full rebuild after every operation in debug builds and the randomised convergence tests).

## Other questions

| Question | Finding |
|---|---|
| Panic safety | UniFFI catches Rust panics at the boundary and raises `InternalException` in Kotlin; the app does not abort. Crash reporting of these (D8, opt-in) is P1-45 |
| Error mapping | `CoreError { Io, Format }` → `CoreException.Io/Format` with a `detail` field. A field named `message` breaks Kotlin compilation (clash with `Throwable.message`) |
| Async / cancellation | Not needed yet: all calls are short. Long operations (PDF import, export, recognition) will use UniFFI async functions; to prototype with the first of them |
| Per-point calls | Replaced by `add_points` (one call per input event), as the FFI guidelines require |

## Recommendation

Keep UniFFI + JNA. Measure the JNA per-call overhead on the reference tablets; if `add_points` exceeds 0.2 ms per call there, switch the hot path to a direct JNI or shared-buffer entry point.
