# WS4 — Maths engine (expressions, CAS, plotting, typesetting)

**Lead role:** Computational maths engineer · **Phases:** 0–3 · **Blocked by decisions:** D1, D7

## Mission

Reliable, offline mathematical computation and beautiful typesetting — the deterministic foundation that drawing (WS5) and AI (WS6) rely on.

## Scope

- Canonical expression tree (`nb-math`) + parsers (LaTeX subset, linear "calculator" syntax).
- Structured maths editor (typed input with templates: fractions, roots, matrices…).
- CAS operations: simplify, expand, factor, solve (equations, systems, inequalities), differentiate, integrate, limits, series, matrices.
- Plotting: explicit `y=f(x)`, parametric, polar `r=f(θ)`, implicit `F(x,y)=0`, inequalities/area shading, sequences.
- Coordinate systems: Cartesian, polar; axes, grids, scales, units (π-based ticks).
- Derived objects for functions: tangent, normal, roots, extrema, inflection points, asymptotes, intersections, areas between curves.
- Typesetting of maths and rich text for display and export.

## Technical approach

| Topic | Approach |
|---|---|
| CAS | `CasBackend` trait. Candidates: **SymEngine** (MIT, C++; strong core, limited solve/integrate → supplement with our own routines) or **Giac** (GPL, very complete). Decision D7 after spike P0-07 |
| Numerics | Rust: robust root finding (bracketing + Newton), interval arithmetic for plotting and certified intersections |
| Plotter | Adaptive sampling driven by interval arithmetic: detects discontinuities/poles, avoids false connecting lines; implicit curves by marching squares + refinement |
| Asymptotes | Symbolic limits via CAS; numeric fallback with explicit "approximate" flag |
| Typesetting | Embed **Typst** (Rust, Apache-2.0) math layout, or MicroTeX; decided by P0-09. OpenType MATH fonts (e.g. Latin Modern Math, STIX Two) bundled |
| Editor | Structured editor over the expression tree (cursor model like established maths editors); LaTeX source view in Advanced mode |
| Precision | Exact arithmetic (rationals, radicals, π) where the CAS allows; numeric display with configurable significant digits |

## Validation suite

- **Curriculum test suite** (≥ 300 tasks): secondary school + first-year university (algebra, analysis, trigonometry, linear algebra, sequences), each with expected exact result. Required to pass for M2.
- Plotter golden tests: tricky functions (`tan x`, `sin(1/x)`, `1/x`, `x^x`, `floor`, `√(x²−1)`, implicit `x³+y³=3xy`).

## Deliverables by phase

| Phase | Deliverable |
|---|---|
| 0 | P0-07 CAS spike, P0-09 typesetting spike |
| 2 | Expression model, maths editor, CAS integration, plotter, coordinate systems, derived objects |
| 3 | Integration with Maths HWR (recognised expression → live, computable object) |
| 5 | Tool API exposed to AI (WS6) |
