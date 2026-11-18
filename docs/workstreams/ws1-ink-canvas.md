# WS1 — Ink & canvas

**Lead role:** Android/ink engineer · **Phases:** 0–3 · **Related ADRs:** 0002, 0003

## Mission

Make writing and drawing feel like paper: minimal latency, natural strokes, smooth navigation on pages and infinite canvases.

## Scope

- Stylus input pipeline (pressure, tilt, timestamps, hover, side button, eraser end).
- Palm rejection and stylus-only drawing mode; finger = navigate by default.
- Wet-ink rendering (front-buffered), motion prediction.
- Stroke processing: smoothing, simplification, variable-width outline generation (`nb-ink`).
- Tools: pen, pencil, highlighter, brush, stroke/partial eraser, lasso selection, shape-free transform.
- Page view: fixed pages (scroll vertical/horizontal), infinite canvas (pan/zoom), tiling, LOD.
- Selection & transform: move, scale, rotate, copy/paste, duplicate, group.
- Instruments' input projection (with WS5).

## Technical approach

| Topic | Approach |
|---|---|
| Wet ink | Android low-latency front-buffered rendering (`androidx.graphics` low-latency APIs; evaluate the Jetpack Ink library) |
| Prediction | Platform motion prediction API; predicted segment drawn then replaced |
| Stroke model | Raw points stored (immutable, ADR-0003); rendered outline computed in `nb-ink` (pressure → width curve, Catmull-Rom/Bézier smoothing) |
| Partial eraser | Does **not** mutate strokes: creates new strokes for remaining segments and tombstones the original (history keeps it) |
| Hit testing | R-tree in `nb-scene`; precise distance-to-outline test |
| Rendering | Display list from core → Skia; tile cache per zoom level; vector re-render on zoom settle |
| Desktop input | Pen tablets via JVM/Skiko pointer events (pressure where available); mouse fallback |

## Deliverables by phase

| Phase | Deliverable |
|---|---|
| 0 | Latency spike & measurement harness (P0-02); rendering spike (P0-05) |
| 1 | Production ink pipeline, tools, eraser, lasso, page/infinite navigation |
| 2 | Instrument-constrained input, snapping hooks |
| 3 | Gesture shortcuts (scribble-to-erase, circle-to-select) — optional, behind setting |

## Quality metrics

- Latency budgets: [overview §6](../architecture/overview.md#6-performance-budgets).
- Stroke fidelity: rendered stroke deviates ≤ 0.5 px from raw centreline at 100 % zoom.
- Palm rejection: ≤ 1 stray mark per 30 min writing session in usability tests.

## Risks

R1 (latency on mid/low-end devices), R11 (fragmented stylus support across vendors) — see [risks.md](../risks.md).
