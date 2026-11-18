# Phase 1 — Notebook MVP (months 3–12)

**Milestones**
- **M1α (≈ month 9):** feature-complete for the epics below; internal dogfooding.
- **M1β (≈ month 12):** 4-week pilot with ≥ 10 teachers/students; cold-start UX test passed; no open data-loss bug; performance budgets met on reference devices.

**Out of scope for Phase 1:** maths recognition, geometry engine, sync, AI (later phases). The architecture must not preclude them (ADRs).

| Epic | Title | WS |
|---|---|---|
| E1.1 | Library & notebook structure | WS2, WS8 |
| E1.2 | Pages & templates | WS2, WS1 |
| E1.3 | Ink tools | WS1 |
| E1.4 | Selection & transformation | WS1 |
| E1.5 | Text & images | WS2 |
| E1.6 | PDF read & annotate | WS2 |
| E1.7 | Undo, autosave, recovery, backup | WS2 |
| E1.8 | Text handwriting recognition & search | WS3, WS2 |
| E1.9 | Export & portable format | WS2 |
| E1.10 | Simple-mode shell & onboarding | WS8, WS1 |
| E1.11 | Desktop build (preview) | WS1 |
| E1.12 | Quality, performance, accessibility | QA, all |

---

## E1.1 — Library & notebook structure

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-01 | Library tree CRDT (folders, notebooks) | M | Create/rename/move/delete folders and notebooks; concurrent moves converge (property test) |
| P1-02 | Library screen | M | Grid/list of notebooks with covers & thumbnails; recent notebooks first; opens in ≤ 300 ms |
| P1-03 | Sections & page list | M | Nestable sections; page thumbnails sidebar; drag-to-reorder pages/sections; add page in one tap |
| P1-04 | Trash | S | Deleted notebooks/sections/pages/objects restorable for 30 days; "empty trash" with confirmation |

## E1.2 — Pages & templates

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-05 | Fixed page sizes | S | A3/A4/A5/Letter/custom, portrait/landscape; vertical and horizontal scroll modes |
| P1-06 | Infinite canvas | M | Pan/zoom 5 %–3 200 %; meets 50 000-stroke budget; "zoom to fit" |
| P1-07 | Page backgrounds | S | Plain, ruled, squared (5 mm), dotted, graph (mm), Cornell, music staff; colour; vector rendered & exported |
| P1-08 | Template system & gallery | M | Page and notebook templates; built-in set: Lesson plan, Lecture notes, Exercise sheet, Research canvas, Journal, Weekly planner; user can save any page as template |

## E1.3 — Ink tools

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-09 | Production wet-ink pipeline | L | From P0-02 recommendation; latency budget met; prediction artefacts invisible in user test |
| P1-10 | Pen, pencil, highlighter, brush | M | Pressure/tilt response curves; highlighter renders beneath ink; 12 default colours + custom; 5 width presets |
| P1-11 | Stroke eraser & partial eraser | M | Partial eraser implemented without mutating strokes (WS1 approach); stylus eraser end & side-button mapping |
| P1-12 | Palm rejection & input modes | S | Stylus draws, finger navigates (default); "finger drawing" toggle for devices without stylus |
| P1-13 | Stroke outline generation (`nb-ink`) | M | Smoothing + variable width; fidelity ≤ 0.5 px from raw centreline |

## E1.4 — Selection & transformation

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-14 | Lasso & rectangle selection | M | Selects mixed objects; selection handles; hit test ≤ 16 ms with 10 000 objects on page |
| P1-15 | Move/scale/rotate/duplicate/delete | M | Transform as single undoable command; copy/paste across pages & notebooks; system clipboard for text/images |
| P1-16 | Group/ungroup, z-order | S | Fractional-index ordering; bring forward/back |

## E1.5 — Text & images

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-17 | Rich text blocks | L | CRDT rich text: bold/italic/underline, headings, lists, colour; keyboard + on-screen keyboard; auto-width & fixed-width boxes |
| P1-18 | Image insertion | M | From gallery, camera, files, clipboard; crop/rotate; content-addressed storage; large images downsampled for display only |
| P1-19 | Document insertion | S | Insert PDF pages as new pages; insert other files as attachments (opened externally) |

## E1.6 — PDF read & annotate

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-20 | PDFium integration | M | Rust bindings; Android arm64/x86_64 + desktop builds; licence notice |
| P1-21 | Open PDF as notebook | M | Each PDF page → page with `pdf_source`; tiles rendered on demand; 300-page PDF opens ≤ 2 s (first page) |
| P1-22 | Annotate & insert blank pages | S | Ink/text/highlights over PDF; insert blank/template pages between PDF pages |
| P1-23 | PDF text layer | M | Text selection & copy; text indexed for search |
| P1-24 | PDF corpus test | S | ≥ 200 real-world PDFs open and render without error (CI nightly) |

## E1.7 — Undo, autosave, recovery, backup

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-25 | Undo/redo | M | Per-user, per-notebook; ≥ 500 steps; visible buttons + two-finger tap / three-finger tap gestures |
| P1-26 | Autosave & crash recovery | M | Durable ≤ 1 s after commit; reopening after kill restores exact state; kill-test suite in CI |
| P1-27 | Local backup | M | Scheduled `.nbk` backups to user-chosen folder; restore from backup flow; integrity check |
| P1-28 | Page/notebook version snapshots (local) | S | "Restore this page as it was" from recent snapshots (pre-sync history UI) |

## E1.8 — Text handwriting recognition & search

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-29 | `TextHwr` integration (Android) | M | Engine per D2 outcome; language models downloaded on demand with clear size info; offline after download |
| P1-30 | "Convert to text" | M | On selection → `Interpretation` with alternatives; applied result is a `TextBlock`; original ink restorable (ADR-0003) |
| P1-31 | Background indexing of handwriting | M | Idle-time recognition of ink for search only (no visible conversion); battery-aware; resumable |
| P1-32 | Search | M | Global and in-notebook search over titles, typed text, recognised ink, PDF text; results with highlighted location on page; ≤ 300 ms across 100 notebooks |

## E1.9 — Export & portable format

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-33 | PDF export | M | Vector; page range; with/without backgrounds; annotated PDFs export original pages + annotations; golden diff ≤ 0.5 % |
| P1-34 | PNG & SVG export | S | Per page or selection; resolution option for PNG |
| P1-35 | `.nbk` export/import v1 | M | Implements format spec; round-trip test (export → import → identical document); `document.json` validates against schema |
| P1-36 | Share sheet integration | S | Android share intent for PDF/PNG; import `.nbk` and PDF via "open with" |

## E1.10 — Simple-mode shell & onboarding

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-37 | First-run experience | M | First launch → blank page with pen selected in ≤ 2 s; no account/permission prompts |
| P1-38 | Simple-mode toolbar | M | ≤ 7 items per WS8 spec; left-handed mode; toolbar position top/side |
| P1-39 | Design system | L | Tokens, components, icons, dark mode, scalable text; accessibility labels |
| P1-40 | Contextual tips & optional tutorials | M | One-time tips; 3 interactive tutorials (write & erase, organise notebooks, annotate PDF) |
| P1-41 | Mode switch scaffold | S | Simple/Advanced setting exists; Advanced exposes layers & full style options (more added in later phases) |

## E1.11 — Desktop build (preview)

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-42 | Desktop app (Windows/macOS/Linux) | L | Opens/edits same `.nbk` files; pen tablet pressure where supported; keyboard shortcuts; installers built in CI |

## E1.12 — Quality, performance, accessibility

| ID | Ticket | Size | Acceptance criteria |
|---|---|---|---|
| P1-43 | Performance CI | M | Benchmarks for all budgets in [overview §6](../architecture/overview.md#6-performance-budgets); regression > 10 % fails CI |
| P1-44 | Golden-image rendering tests | M | Display-list and export goldens on CI; device screenshot goldens nightly |
| P1-45 | Crash reporting (opt-in) | S | Per D8: off by default, no content, local log export for support |
| P1-46 | Accessibility audit | S | WCAG 2.2 AA for chrome; TalkBack labels; contrast checks |
| P1-47 | Pilot programme | M | Recruit ≥ 10 participants; feedback channel; weekly triage; pilot report at M1β |
