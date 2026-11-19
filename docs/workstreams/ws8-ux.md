# WS8 — UX, design and onboarding

**Lead role:** Product designer (education) · **Phases:** 0–6 (continuous)

## Mission

Guarantee the non-negotiable principle: *a beginner can use it without instructions; an expert never feels constrained.*

## Design principles (operational)

1. **Zero-step start:** first launch opens a blank page with a pen selected. No account, no tutorial wall, no permission prompts until needed.
2. **Paper metaphors first:** notebook, page, pen, eraser, ruler — familiar names and icons; no jargon in Simple mode.
3. **One primary toolbar** of ≤ 7 items in Simple mode: pen, highlighter, eraser, lasso, undo, redo, "more".
4. **Progressive disclosure:** advanced capabilities appear *in context* (e.g. after drawing a circle near a triangle, a single chip appears), not in menus.
5. **Everything reversible:** undo always visible; destructive actions go to trash; recognition is a suggestion.
6. **Consistency:** same gestures and icons on every device; desktop adds keyboard shortcuts, never removes features.
7. **Accessibility:** WCAG 2.2 AA for UI chrome; scalable UI; colour-blind-safe default palettes; left-handed mode; screen-reader labels.

## Simple vs Advanced mode

| Aspect | Simple mode (default) | Advanced mode |
|---|---|---|
| Toolbar | ≤ 7 tools | Customisable, full tool set |
| Recognition | Convert on demand ("Convert" button on selection) | Live suggestions, auto-correct option |
| Geometry | Instruments available from "more" | Construction tools, constraints panel, construction protocol |
| Layers | Hidden (single layer) | Layer panel |
| Styles | Colour + thickness | Full style editor (dash, patterns, hatching, opacity) |
| Maths | Convert handwriting, plot a function | CAS panel, coordinate system settings, LaTeX source |
| Collaboration | Share button with simple roles | Full permissions, history browser |
| AI | "Explain / summarise" on selection | Assistant panel, tool transparency |

Same document model and capabilities; switching modes never changes or hides content.

## Research & validation plan

| When | Activity |
|---|---|
| Phase 0 | Personas, task analysis with 5 teachers + 5 students + 3 non-technical users; paper/interactive prototypes (P0-14) |
| Every iteration (from Phase 1) | Hallway tests on reference devices |
| Each milestone | Moderated usability study (≥ 8 participants incl. ≥ 3 novices); SUS score target ≥ 80 |
| M1β | 4-week classroom pilot with ≥ 10 participants |

**Key UX acceptance test ("cold start test"):** a participant who has never used a digital notebook, given only the tablet with the app open, writes a note, erases a mistake, adds a page and finds the note again — without help, within 3 minutes. Required to pass for M1β and every later milestone.

## Onboarding

- No mandatory tutorial. Optional guided tutorials (interactive, on a real page) per feature area.
- Contextual tips: shown once, dismissable, never blocking.
- Template-based starters: "Lesson", "Lecture notes", "Exercise sheet", "Research canvas", "Journal".

## Deliverables

Design system (components, icons, tokens) in Phase 1; interaction specs per feature delivered one iteration ahead of engineering.
