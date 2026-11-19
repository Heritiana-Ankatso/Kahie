# WS5 — Geometry & intelligent mathematical drawing

**Lead role:** Computational geometry engineer · **Phases:** 2–3 · **Related ADRs:** 0003, 0004

## Mission

A drawing engine that understands what the user is constructing: from freehand strokes to exact, dynamic, animatable mathematical constructions — always as reversible suggestions.

## Scope

1. Construction graph and geometric primitives (`nb-geo`).
2. Constraint solver (`nb-solve`) for snapping, alignment and geometric constraints.
3. Shape beautification (using WS3 primitives).
4. Relationship inference and suggestions.
5. Curve → function suggestion.
6. Graphs, trees, networks, flowcharts: recognition and layout.
7. Virtual instruments: ruler, compass, protractor, set squares, geometric templates.
8. Professional styling: dashes, dots, thickness, colours, transparency, patterns, hatching, layers, transformations.
9. Step-by-step construction animation.
10. Interaction modes: **Free** (no correction), **Suggest** (default in Advanced), **Auto-correct**.

## Technical approach

### 5.1 Construction graph & solver
- DAG of definitions (ADR-0004). Operations library: midpoint, perpendicular/parallel line, perpendicular bisector, angle bisector, intersection (all pairs of line/circle/conic/function), tangent(s) from point, circle through 3 points, incircle/circumcircle, reflection, rotation, translation, homothety, locus.
- Solver: variational geometric constraint solver (Newton–Raphson / Levenberg–Marquardt with DOF analysis), ported or adapted from **PlaneGCS** (LGPL) or written in Rust. Must detect over-/under-constrained systems and report conflicting constraints.

### 5.2 Relationship inference (the "understanding" part)
Pipeline on each pen-up:
1. Primitive from WS3 (e.g. "circle, centre ≈ (3.1, 2.0), r ≈ 1.9").
2. **Hypothesis generation** against nearby objects: coincident, point-on, parallel, perpendicular, equal length, tangent, concentric, midpoint, symmetric, passes-through-vertices (circumcircle), touches-all-sides (incircle), angle bisector, etc.
3. **Scoring:** geometric residual normalised by drawing scale and stylus noise model × prior (frequency of the relation in curriculum constructions) × context (existing objects, recent actions).
4. **Presentation:** top-k hypotheses above threshold as small chips ("Circumscribed circle?", "⟂"); Auto-correct applies the best one automatically with an undo toast.
5. **Accepted** → becomes a definition or constraint in the graph; solver repositions; source strokes hidden (ADR-0003).

Phase 3 option: learn the scorer weights from accept/reject events collected **on-device only** (no telemetry).

### 5.3 Curve → function
- Map stroke into the active coordinate system; resample uniformly.
- Fit candidate families: polynomials (deg ≤ 4), `a·e^{bx}+c`, `a·ln(bx+c)+d`, `a·sin(bx+c)+d`, `a/(x−b)+c`, rational (low degree), `a·√(x−b)+c`, `|x|`-based.
- Rank by BIC: `BIC = n·ln(RSS/n) + k·ln(n)`; boost by detected features (symmetry, roots on grid points, extrema, asymptotes, "nice" parameter values like integers/π multiples).
- Present top 3 with preview overlay; accepted → `Plot` object (exact function), strokes hidden.

### 5.4 Diagrams
- Nodes = closed shapes/text clusters; edges = open strokes whose endpoints land near nodes (arrowheads detected).
- Diagram types inferred: tree (acyclic, single root), DAG/flowchart (decision diamonds), general graph/network.
- Layout algorithms: tidy tree (Reingold–Tilford/Walker), layered (Sugiyama) for flowcharts, force-directed for networks, circular. Layout is a suggestion; user can pin nodes.

### 5.5 Instruments
- Overlay objects manipulated with fingers (move/rotate) while the stylus draws along them.
- Ruler: stylus projected onto edge → exact segment; shows length.
- Compass: set radius by two points, sweep → exact arc/circle.
- Protractor: measure/construct angles; set squares (45° and 30/60°); templates (circles, ellipses, polygons).
- Output objects enter the construction graph with exact definitions.

### 5.6 Animation
- Replay construction steps; per-step highlight of inputs, captions, speed control, step-through for teaching; export as sequence of PDF pages (and later video/GIF).

## Validation

- **Construction suite** (≥ 100 standard constructions) reproducible with instruments and with freehand + suggestions.
- Relation suggestion precision ≥ 80 % at the presentation threshold; acceptance ≥ 60 % in pilot (M3).
- Curve→function: correct family in top 3 for ≥ 85 % of a 200-sketch test set drawn by teachers.

## Deliverables by phase

| Phase | Deliverable |
|---|---|
| 2 | Construction graph, solver, instruments, beautification (Free/Suggest/Auto modes for primitives), styling, layers, transformations, coordinate systems with WS4 |
| 3 | Relationship inference, curve→function, diagram recognition & layout, construction animation |
