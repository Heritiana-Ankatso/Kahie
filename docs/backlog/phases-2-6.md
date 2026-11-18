# Phases 2–6 — Epic-level backlog

Epics are refined into tickets (same format as [phase-1.md](phase-1.md)) during the last iteration before each phase starts. Estimates are indicative.

---

## Phase 2 — Maths layer (months 10–18) · Milestone M2

**Exit criteria:** curriculum test suite (≥ 300 tasks) passes; construction suite (≥ 100) reproducible with instruments; LaTeX/TikZ export compiles with `pdflatex` for all suite documents.

| Epic | Title | WS | Key deliverables |
|---|---|---|---|
| E2.1 | Expression model & maths editor | WS4 | Canonical expression tree, LaTeX/linear parsers, structured editor, `MathBlock` objects |
| E2.2 | Typesetting | WS4 | Chosen engine (P0-09) integrated; maths fonts; rendering in display list and export |
| E2.3 | CAS integration | WS4 | `CasBackend` implementation (D7); evaluate/simplify/solve/diff/integrate/limit in UI; exact & numeric modes |
| E2.4 | Coordinate systems & plotting | WS4/WS5 | Cartesian & polar frames placed on pages; explicit/parametric/polar/implicit plots; area shading; robust plotter |
| E2.5 | Function analysis objects | WS4 | Tangent, normal, roots, extrema, inflection, asymptotes, intersections, areas |
| E2.6 | Construction graph | WS5 | `nb-geo` primitives and operations library; dynamic dragging; undefined-state handling |
| E2.7 | Constraint solver | WS5 | `nb-solve`; DOF analysis; conflict reporting; ≤ 16 ms for 100 constraints |
| E2.8 | Shape beautification | WS3/WS5 | Primitive recognition v1; Free / Suggest / Auto modes; originals recoverable |
| E2.9 | Snapping & alignment | WS5 | Grid, object, angle, tangent snapping; smart guides; toggle per mode |
| E2.10 | Virtual instruments | WS5/WS1 | Ruler, compass, protractor, set squares, templates |
| E2.11 | Professional styling & layers | WS5/WS2 | Dashes, dots, patterns, hatching, opacity, arrowheads; layer panel; transformations (reflection, rotation, homothety) |
| E2.12 | Markdown & LaTeX/TikZ export | WS2 | Markdown with maths; LaTeX document with TikZ/pgfplots for geometry and plots |
| E2.13 | Maths search | WS2 | Index typed/converted maths; structural query ("x^2 + ?") |

## Phase 3 — Intelligence (months 15–24) · Milestone M3

**Exit criteria:** recognition targets met (WS3, WS5); suggestion acceptance ≥ 60 % in pilot; Advanced mode usability study SUS ≥ 75 with expert users; Simple mode cold-start test still passes.

| Epic | Title | WS | Key deliverables |
|---|---|---|---|
| E3.1 | Maths handwriting recognition | WS3 | `MathHwr` engine (D2); n-best UI; recognised expression becomes live computable `MathBlock` |
| E3.2 | Ink segmentation & classification | WS3 | Text / maths / drawing grouping; "maths pen" explicit mode |
| E3.3 | Relationship inference | WS5 | Hypothesis generation, scoring, suggestion chips, Auto-correct with undo toast |
| E3.4 | Curve → function | WS5/WS4 | Family fitting, BIC ranking, feature cues, top-3 preview |
| E3.5 | Diagram recognition & layout | WS5 | Nodes/edges detection; tree, flowchart, network layouts; pinning |
| E3.6 | Construction animation | WS5 | Step replay, captions, step-through presentation mode, export as page sequence |
| E3.7 | Advanced mode complete | WS8 | Construction protocol panel, constraints panel, CAS panel, customisable toolbar |
| E3.8 | Presentation mode | WS8/WS1 | Full-screen teaching mode for projection; laser pointer; hide UI chrome |

## Phase 4 — Sync & collaboration (months 14–22, parallel) · Milestone M4

**Exit criteria:** soak test (3 devices × 5 users × 72 h, random partitions) with no divergence or loss; collaboration latency ≤ 500 ms p95 on 4G; Security Rules test suite green; app fully functional with sync disabled.

| Epic | Title | WS | Key deliverables |
|---|---|---|---|
| E4.1 | Identity & sign-in | WS7 | `AuthProvider` (Firebase Auth + Google Sign-In); anonymous-to-account migration |
| E4.2 | Personal sync (Drive) | WS7 | `SyncTransport` over Drive appDataFolder; compaction; conflict-free multi-device |
| E4.3 | Local folder / WebDAV sync | WS7 | Independence adapter (ADR-0006) |
| E4.4 | Real-time relay | WS7 | Firebase relay per P0-10; reconnection & offline queue |
| E4.5 | Sharing & permissions | WS7/WS8 | Invite by email/link; owner/editor/commenter/viewer; Security Rules + client enforcement |
| E4.6 | Presence | WS7/WS1 | Live cursors, selections, identities, follow-mode for teaching |
| E4.7 | Comments | WS7/WS2 | Anchored threads, mentions, resolve; commenter role |
| E4.8 | Revision history UI | WS7/WS8 | Timeline, per-object history, diff overlay, restore version, revert change, named versions |
| E4.9 | Drive backup & assets | WS7 | `BlobStore` implementations; backup schedule |

## Phase 5 — Offline AI (months 22–30) · Milestone M5

**Exit criteria:** tool-use benchmark ≥ 90 % with 0 unflagged numeric errors; latency targets on Standard tier; feature hidden cleanly on unsupported devices; pilot usefulness ≥ 4/5.

| Epic | Title | WS | Key deliverables |
|---|---|---|---|
| E5.1 | LLM runtime & model manager | WS6 | `LlmRuntime` implementations; download manager; device tiering |
| E5.2 | Context builder | WS6 | Page/selection/notebook serialisation incl. construction-graph DSL |
| E5.3 | Tool router & verification | WS6/WS4/WS5 | Tool schemas; verified vs generated marking |
| E5.4 | Assistant UI | WS6/WS8 | "Explain / Summarise / Help me solve / Check my construction" actions; panel in Advanced |
| E5.5 | Retrieval | WS6/WS2 | On-device embeddings; notebook-wide Q&A with page citations |
| E5.6 | Generators | WS6/WS4 | Examples, exercises with CAS-verified solutions, diagram generation via `geo.construct` |
| E5.7 | Organisation assistant | WS6 | Tag/section suggestions, linking related pages |
| E5.8 | LAN companion runtime (optional) | WS6 | Use a model running on the user's own PC, no external API |

## Phase 6 — Expansion (month 30+)

| Epic | Title | Notes |
|---|---|---|
| E6.1 | iPad / iOS app | Rust core via Swift bindings; depends on D3 |
| E6.2 | Web viewer | Core compiled to WASM; read-only + comments |
| E6.3 | Self-hosted sync relay | Open-source relay implementing `SyncTransport` |
| E6.4 | Vision model for diagram/photo understanding | Optional, high-tier devices |
| E6.5 | Notebook encryption | Per-notebook passphrase |
| E6.6 | Additional exports/imports | e.g. GeoGebra-compatible geometry export (if licence permits), OneNote/GoodNotes PDF import improvements |
| E6.7 | Classroom features | Distribute/collect worksheets, follow-the-teacher mode |
