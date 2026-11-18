# Kahie — Intelligent Notebook: Delivery Plan

| | |
|---|---|
| **Status** | Draft v1 — for team review |
| **Date** | 2026-11-18 |
| **Scope** | Whole product, from foundations to offline AI |
| **Audience** | Engineering, design, QA, product |

---

## TL;DR

- We are building an **offline-first, stylus-first notebook and mathematical workspace**, Android tablets first.
- The product combines five proven categories: ink notebook, dynamic geometry, computer algebra, on-device AI, real-time collaboration. **The integration is the hard part; each piece has known solutions.**
- Architecture: **Rust core** (document model, CRDT, geometry, solver, maths, export) + **Kotlin / Compose Multiplatform UI**, with every online or ML component behind a swappable interface.
- Delivery in **six phases**. Phase 0 (3 months) is dedicated to de-risking spikes; their results confirm or overturn the proposed ADRs.
- Indicative effort: **notebook MVP beta at ~month 12**, full vision at **~month 30–36**, with a core team of 7–9.

---

## 1. Product goal

> A beginner should be able to use it without instructions, while an expert should never feel constrained by it.

Original product description: [`product/app-description.md`](product/app-description.md).

### 1.1 Target users (initial)

| Persona | Primary needs |
|---|---|
| **Secondary-school teacher (maths)** | Prepare lessons, project and construct figures live, annotate PDFs, export worksheets |
| **University student** | Lecture notes, handwritten maths converted to LaTeX, search, revision |
| **Researcher** | Infinite canvas, typeset maths, references, LaTeX/TikZ export |
| **Non-technical user** | Open the app and write; nothing else required |

### 1.2 Success criteria for v1.0

- A first-time user writes on a page within **5 seconds** of first launch, with no account and no configuration.
- Stylus latency is perceived as "paper-like" (see performance budgets in [`architecture/overview.md`](architecture/overview.md#6-performance-budgets)).
- Zero data loss under crash, kill or power loss (autosave, journaled storage).
- Every intelligent feature is a **reversible suggestion**; original ink is never destroyed.
- The whole app is functional with networking disabled.

---

## 2. Guiding principles

These principles are binding. Any change requires an ADR.

1. **Local-first.** The device is the source of truth. Online features are optional adapters. → [ADR-0001](adr/0001-local-first-crdt.md)
2. **One shared core.** All deterministic logic lives in a portable Rust core. → [ADR-0002](adr/0002-rust-core-kotlin-ui.md)
3. **Raw ink is immutable.** Recognition produces *interpretations* linked to strokes, never replacements. → [ADR-0003](adr/0003-immutable-ink-interpretations.md)
4. **Constructions are graphs.** Geometry is stored as a dependency graph of definitions, not as final shapes. → [ADR-0004](adr/0004-construction-graph.md)
5. **AI orchestrates, algorithms compute.** The language model never produces mathematical results by itself. → [ADR-0005](adr/0005-llm-as-tool-orchestrator.md)
6. **No vendor lock-in.** Every external service sits behind an interface; data is exportable in open formats. → [ADR-0006](adr/0006-pluggable-adapters.md)
7. **Simple by default, powerful on demand.** Simple and Advanced modes share one engine; only exposure differs. → [`workstreams/ws8-ux.md`](workstreams/ws8-ux.md)

---

## 3. Scope

### 3.1 In scope (full vision)

Everything in the product description, delivered in phases (§4).

### 3.2 Explicit non-goals for v1.0

| Non-goal | Reason | Revisit |
|---|---|---|
| iOS / iPadOS app | Focus on Android stylus quality first | After M2 — see [D3](decisions-open.md#d3--platforms-beyond-android) |
| Full web editor | Web = read-only viewer at most in v1 | Phase 6 |
| Image-based AI (vision model) | Structured scene context is more reliable on-device | Phase 6 |
| Self-hosted sync server | Adapter interface prepared; implementation later | Phase 6 |
| 3D geometry | Out of scope of the description | Not planned |
| Audio/video recording in notes | Not in the description | Not planned |

---

## 4. Phases and milestones

| Phase | Months (indicative) | Milestone | Content | Exit criteria |
|---|---|---|---|---|
| **0 — Foundations** | 0–3 | **M0** | Spikes, ADRs accepted, repo/CI, document model v0, storage v0, ink prototype | All Phase 0 spikes reported; ADR-0001…0006 *Accepted* or replaced; ink prototype meets latency budget on reference devices |
| **1 — Notebook MVP** | 3–12 | **M1α** (m9), **M1β** (m12) | Library, pages, templates, ink tools, text, images, PDF annotation, undo/autosave, text HWR, search, PDF/PNG/SVG export, Simple mode | Pilot with ≥ 10 teachers/students for 4 weeks; no data-loss bug open; performance budgets met |
| **2 — Maths layer** | 10–18 | **M2** | Maths input & typesetting, CAS, plotting, coordinate systems, shape beautification, instruments, construction graph, constraint solver, LaTeX/MD/TikZ export | Curriculum test suite (≥ 300 tasks) passes; teacher can build standard constructions with instruments |
| **3 — Intelligence** | 15–24 | **M3** | Handwritten maths recognition, relation suggestions, curve→function, diagram recognition & layout, construction animation, Advanced mode | Recognition accuracy targets met (see WS3/WS5); suggestion acceptance rate ≥ 60 % in pilot |
| **4 — Sync & collaboration** | 14–22 (parallel) | **M4** | Drive personal sync, Firebase real-time collaboration, roles, comments, revision history UI | 3 devices × 5 users soak test 72 h with no divergence; offline-merge test suite green |
| **5 — Offline AI** | 22–30 | **M5** | LLM runtime, tool calling, explanations, summaries, exercise generation, notebook retrieval | Tool-use benchmark ≥ target; AI disabled gracefully on unsupported devices |
| **6 — Expansion** | 30+ | — | Desktop polish, web viewer, iOS, self-hosted sync, vision model | Per-item |

```
Month:   0    3    6    9    12   15   18   21   24   27   30   33
Phase 0  ████
Phase 1      ██████████████
Phase 2               ████████████
Phase 3                     ████████████
Phase 4                    ███████████
Phase 5                                    ████████████
Phase 6                                                ███████→
```

Overlaps are intentional: research-heavy tracks (recognition models, CAS integration) start before their phase formally begins.

Detailed backlog:
- [`backlog/phase-0.md`](backlog/phase-0.md) — ticket-level, ready to start
- [`backlog/phase-1.md`](backlog/phase-1.md) — ticket-level
- [`backlog/phases-2-6.md`](backlog/phases-2-6.md) — epic-level, refined at each phase start

---

## 5. Workstreams

| ID | Workstream | Lead role | Spec |
|---|---|---|---|
| WS1 | Ink & canvas | Android/ink engineer | [ws1-ink-canvas.md](workstreams/ws1-ink-canvas.md) |
| WS2 | Documents, PDF, export, search | Rust core engineer | [ws2-documents.md](workstreams/ws2-documents.md) |
| WS3 | Recognition (text, maths, shapes) | ML engineer | [ws3-recognition.md](workstreams/ws3-recognition.md) |
| WS4 | Maths engine (CAS, plotting, typesetting) | Computational maths engineer | [ws4-maths-engine.md](workstreams/ws4-maths-engine.md) |
| WS5 | Geometry & intelligent drawing | Computational geometry engineer | [ws5-geometry-drawing.md](workstreams/ws5-geometry-drawing.md) |
| WS6 | Offline AI assistant | ML engineer | [ws6-ai-assistant.md](workstreams/ws6-ai-assistant.md) |
| WS7 | Sync & collaboration | Backend/sync engineer | [ws7-sync-collaboration.md](workstreams/ws7-sync-collaboration.md) |
| WS8 | UX, design, onboarding | Product designer | [ws8-ux.md](workstreams/ws8-ux.md) |

---

## 6. Team

| Role | Headcount | Phase 0–1 focus | Phase 2+ focus |
|---|---|---|---|
| Tech lead / architect | 1 | ADRs, core model, reviews | Cross-workstream integration |
| Rust core engineers | 2 | Model, storage, rendering, export | Solver, sync protocol, history |
| Android / Kotlin engineers | 2 | Ink, UI shell, PDF view | Instruments, Advanced mode, collaboration UI |
| Computational geometry / maths engineer | 1 | CAS & typesetting spikes | Geometry engine, solver, plotting |
| ML engineer | 1 | HWR & LLM spikes, data collection | Maths HWR, shape/curve recognition, LLM |
| Backend / sync engineer | 0.5 → 1 | Firebase spike | Sync & collaboration |
| Product designer (education) | 1 | Personas, prototypes, usability tests | Progressive disclosure, onboarding |
| QA engineer | 1 | Device lab, test corpus, automation | Soak tests, recognition evaluation |

Buying a commercial handwriting SDK (decision [D2](decisions-open.md#d2--handwriting-recognition-buy-or-build)) would reduce ML workload by roughly one engineer-year.

---

## 7. Governance

- **Architecture changes**: via ADR ([`adr/`](adr/README.md)); tech lead approves.
- **Open decisions**: tracked in [`decisions-open.md`](decisions-open.md) with owner and deadline.
- **Risks**: tracked in [`risks.md`](risks.md); reviewed at each milestone.
- **Engineering conventions**: [`engineering/conventions.md`](engineering/conventions.md).
- **Planning cadence**: 2-week iterations; backlog refinement weekly; milestone review with go/no-go at each M.

---

## 8. Immediate next steps (weeks 1–2)

1. Review this plan; raise objections as PR comments or ADR amendments.
2. Product owner resolves decisions **D1–D5** (or sets deadlines) — several Phase 0 spikes depend on them.
3. Procure reference devices ([P0-15](backlog/phase-0.md#p0-15--reference-device-lab)).
4. Start tickets **P0-01** (repo & CI), **P0-02** (ink latency), **P0-03** (CRDT evaluation), **P0-14** (UX research) in parallel.
