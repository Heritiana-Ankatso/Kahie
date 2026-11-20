# P0-07 — CAS evaluation: Giac

- **Status:** Partial — correctness and speed measured on desktop with Ubuntu's Giac 1.9 (`libgiac-dev`); Android arm64 build and size pending
- **Date:** 2026-11-20
- **Decision context:** D1 (GPL-3) allows Giac; D7 chose it. This spike checks it against a curriculum task set.
- **Code:** [`spikes/p0-07/`](../../spikes/p0-07) — C++ shim (`src/shim.cpp`, C ABI over `giac::gen`/`protecteval`) called from Rust; task set `tasks.txt`; `cargo run --release` (needs `libgiac-dev`)

## Method

88 lycée / first-year university tasks in 10 categories (simplification, solving, inequalities, derivatives, integrals, limits, series and sums, matrices, complex numbers, probability). Each answer is checked **inside Giac** against a reference (`simplify(result − expected) = 0`, sorted value lists, or exact printed form for inequalities). The harness was verified to reject deliberately wrong references (3/3 rejected).

## Results (desktop container, Intel Xeon 2.3 GHz)

| Category | Passed | Mean time (ms) |
|---|---|---|
| complex | 5/5 | 0.0 |
| derivative | 10/10 | 0.0 |
| inequality | 4/4 | 0.2 |
| integral | 12/12 | 0.1 |
| limit | 8/8 | 0.1 |
| matrix | 8/8 | 0.1 |
| proba | 3/3 | 0.0 |
| series | 8/8 | 0.1 |
| simplify | 16/16 | 0.1 |
| solve | 14/14 | 0.1 |
| **Total** | **88/88** | median 0.0, p95 0.3, max 0.6 |

All times in milliseconds per task; context creation is negligible.

| Other measure | Value |
|---|---|
| Library size (Ubuntu build, x86-64, stripped) | 22 MB, plus PARI, NTL, GSL, MPFR, MPFI, GF2X |
| Licence | GPL-3.0-or-later (compatible with D1) |

## Notes for integration

- **Output format:** Giac prints lists as `list[…]` and matrices as `matrix[…]`; never re-parse printed output. Keep results as Giac values inside the core and convert to our expression tree / LaTeX (`latex(…)` is available in Giac) at the boundary.
- **Definite integrals** print a warning on stderr ("No checks were made for singular points…"): capture Giac's log and surface it as a caveat, not an error.
- **Lenient parser (important):** Giac silently "repairs" malformed input — `factor(x^2` returns `x^2`, `integrate(x,` returns `undef` — instead of raising an error. `nb_math::cas` therefore checks bracket balance before calling Giac and treats `undef` and Giac error strings as errors, so the user never gets a wrong answer for a typo.
- **Implemented:** `nb_math::cas::{CasBackend, GiacCas}` behind the cargo feature `giac` (`cargo test -p nb-math --features giac` with `libgiac-dev`); calls are serialised because Giac keeps one global context.
- **Safety:** use `protecteval` and a per-call time limit (Giac has `timeout`) so a hard request cannot freeze the app; run CAS calls off the UI thread (UniFFI async).
- **`CasBackend` trait (draft)** for `nb-math`:

```rust
pub trait CasBackend: Send {
    /// Evaluate a Giac-syntax command; result as canonical Giac text and LaTeX.
    fn eval(&mut self, input: &str, timeout_ms: u32) -> Result<CasResult, CasError>;
}
pub struct CasResult { pub text: String, pub latex: String, pub warnings: Vec<String> }
pub enum CasError { Syntax(String), Timeout, Unsupported(String), Internal(String) }
```

LaTeX input from the user goes LaTeX → our expression tree (`nb-math`) → Giac syntax, so the CAS is swappable.

## Remaining work

1. **Android arm64 build in CI:** cross-compile GMP, MPFR and Giac with the NDK, with PARI, NTL, GSL, FLTK and GLPK disabled; target ≤ 12 MB added to the APK (to be measured). A minimal Giac without PARI may fail some number-theory tasks: rerun this suite on the Android build.
2. Grow the task set to the planned 150 (geometry, statistics, sequences, ODEs).

## Recommendation

Giac passes **88/88** with sub-millisecond latency: keep it (D7). The deciding risk is now the Android binary size, which the cross-compile task must measure early.
