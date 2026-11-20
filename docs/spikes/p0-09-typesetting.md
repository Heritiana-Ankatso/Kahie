# P0-09 — Typesetting engine: Typst maths layout fed by MiTeX

- **Status:** Partial — quality, coverage and speed measured on desktop; Android binary size not yet acceptable (see below); MicroTeX not evaluated
- **Date:** 2026-11-20
- **Code:** [`spikes/p0-09/`](../../spikes/p0-09) — `cargo run --release` reproduces the table and writes the sheet
- **Sample sheet:** [`p0-09-sheet.pdf`](p0-09-sheet.pdf) (99 reference formulas, LaTeX source next to the rendering)

## Approach

Users type **LaTeX** (as in Kahie Web). The pipeline is:

1. **MiTeX 0.2** (Rust, Apache-2.0) converts LaTeX maths to Typst maths syntax.
2. A small **prelude** (`spikes/p0-09/prelude.typ`, written for Kahie) defines the helper functions MiTeX emits (`mitexsqrt`, `pmatrix`, `textmath`, …) and aliases symbols renamed in Typst 0.14+ (`diff`, `sect`); one string fix-up handles renamed symbol modifiers (`angle.l` → `chevron.l`).
3. **Typst 0.15** (Rust, Apache-2.0) lays out the formula with the New Computer Modern Math font and exports SVG or PDF. Typst frames can also be walked directly to produce our own display list (glyph outlines + rules).

## Results (desktop container, Intel Xeon 2.3 GHz)

| Measure | Value |
|---|---|
| Formulas | 99 |
| LaTeX → Typst conversion (MiTeX), total | 1.1 ms |
| Compiled and laid out | 99 / 99 |
| Per formula (new engine each time), median / p95 / max | 4.7 / 7.2 / 13.1 ms |
| Whole 100-formula sheet (layout) | 36 ms, 3 page(s) |
| Sheet PDF / SVG size | 153 KiB / 679 KiB |

| Binary size (x86-64, release, stripped, all Typst fonts embedded) | 50 MB, of which fonts 9.6 MB |

**Visual quality:** close to LaTeX (same Computer Modern design, correct limits, fractions, matrices, cases, big operators). One defect seen: `\|\vec{u}\|` renders its delimiters incorrectly (row 27 of the sheet).

## Assessment

| Criterion | Result |
|---|---|
| Coverage of the 99 reference formulas | **99 / 99** after the prelude and aliases |
| Layout time | 4.7 ms median per formula with a fresh engine; a whole 3-page sheet in 36 ms → fine for interactive editing |
| Output to display list + PDF | SVG and PDF directly; display list via frame walk (to implement) |
| Binary size on Android | **Too large as built** (50 MB on x86-64 with every Typst feature and font). Needs: only the maths font (NewCMMath, ≈ 0.6 MB), no PDF/bibliography/syntax-highlighting/hyphenation data in the app build, `opt-level = "s"` + LTO. Target ≤ 8 MB added to the APK |
| Licence | Typst, MiTeX: Apache-2.0 → compatible with GPL-3 (D1) |

## Recommendation

Adopt **Typst + MiTeX** for maths typesetting (quality, coverage, speed are all good; Rust-native, fits the core). Before production, a size-reduction task: build a minimal Typst configuration for arm64 and measure APK growth; if it cannot reach ≈ 8 MB, fall back to MicroTeX (C++) for on-screen rendering and keep Typst for PDF export. Maintain the prelude/fix-up list with a regression test over the reference formulas.
