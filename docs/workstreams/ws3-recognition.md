# WS3 — Recognition (text, maths, shapes)

**Lead role:** ML engineer · **Phases:** 0–3 · **Related ADRs:** 0003, 0006 · **Blocked by decisions:** D2, D5

## Mission

Turn handwriting into editable, typeset text and mathematics, and strokes into clean geometric primitives — accurately, on-device, reversibly.

## Scope

1. **Text HWR** — handwriting → text (initial languages per D5).
2. **Maths HWR** — handwritten expressions → expression tree + LaTeX.
3. **Primitive recognition** — stroke → line, polyline, polygon, circle, arc, ellipse, point, arrow, free curve (feeds WS5).
4. **Segmentation** — grouping strokes into words, lines, expressions, shapes (text vs maths vs drawing classification).

## Technical approach

| Component | Approach | Alternatives |
|---|---|---|
| Text HWR (Android) | Google ML Kit Digital Ink Recognition (on-device, per-language model download) | MyScript iink SDK; in-house model |
| Text HWR (desktop) | In-house/open model via ONNX Runtime, or MyScript | — |
| Maths HWR | In-house online HWR model: stroke-sequence encoder → LaTeX token decoder (transformer), trained on **CROHME** + **MathWriting** + synthetic data + own corpus; quantised (int8) for mobile via ONNX Runtime / LiteRT | MyScript (commercial) |
| Ink classification | Lightweight classifier (text / maths / drawing) on stroke-group features; user can override via tool mode ("maths pen") | Explicit modes only |
| Primitives | Deterministic geometric fitting (least squares / RANSAC: line, circle, ellipse; Douglas–Peucker for polylines; corner detection) + small classifier for ambiguous cases | — |
| Post-processing | Maths: grammar-constrained decoding + CAS parse validation; n-best list exposed to UI | — |

## Data

- Public datasets: CROHME, MathWriting (check licences per use — ticket P0-06).
- **Own corpus** (P0-15): handwriting of teachers/students on reference devices, with written consent and anonymisation; stored in `testdata/` (evaluation) or private bucket (training).
- Evaluation sets frozen per release for regression tracking.

## Targets

| Task | Metric | Target (M3) |
|---|---|---|
| Text HWR (supported languages) | Character error rate | ≤ 5 % on own corpus |
| Maths HWR | Expression exact match | ≥ 65 % (CROHME-style test); ≥ 85 % top-3 |
| Maths HWR | Symbol accuracy | ≥ 95 % |
| Primitives | Classification accuracy | ≥ 97 % on primitive corpus |
| Latency | — | See [overview §6](../architecture/overview.md#6-performance-budgets) |

Targets are provisional and will be recalibrated after P0-06 benchmarks.

## Deliverables by phase

| Phase | Deliverable |
|---|---|
| 0 | P0-06 benchmark & buy/build recommendation; corpus collection protocol |
| 1 | Text HWR integration (convert to text, search indexing) |
| 2 | Primitive recognition v1 (feeds shape beautification) |
| 3 | Maths HWR v1, segmentation/classification, n-best UI integration |
