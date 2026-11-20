# Requirements traceability

Maps every requirement of the [product description](product/app-description.md) to its workstream, phase and backlog item. Update when the backlog changes. A requirement without a row is a planning bug.

## 1. Notebook and workspace

| Requirement | WS | Phase | Backlog |
|---|---|---|---|
| Unlimited notebooks, folders, sections, pages | WS2 | 1 | E1.1 (P1-01…P1-04) |
| Fixed pages (A4, A5…) and infinite canvases | WS2/WS1 | 1 | E1.2 (P1-05, P1-06) |
| Mixed handwriting, text, maths, drawings, diagrams, images | WS2 | 1–3 | E1.3, E1.5, E2.1, E3.5 |
| Handwriting → typeset text | WS3 | 1 | P1-29, P1-30 |
| Handwriting → typeset maths | WS3/WS4 | 3 | E3.1, E2.2 |
| Read and annotate PDFs; insert documents and images | WS2 | 1 | E1.6, P1-18, P1-19 |
| Search across notebooks, handwriting, text, maths | WS2/WS3 | 1–2 (5 semantic) | P1-31, P1-32, E2.13, E5.5 |
| Templates (teaching, research, study, personal) | WS2/WS8 | 1 | P1-07, P1-08 |
| Export PDF, SVG, PNG | WS2 | 1 | P1-33, P1-34 |
| Export Markdown, LaTeX, other standard formats | WS2 | 2 | E2.12, P1-35 |

## 2. Intelligent mathematical drawing

| Requirement | WS | Phase | Backlog |
|---|---|---|---|
| Recognise and construct geometric objects and relationships | WS5/WS3 | 2–3 | E2.6, E2.8, E3.3 |
| Recognise curves, suggest functions | WS5 | 3 | E3.4 |
| Tangents, normals, intersections, asymptotes, etc. | WS4/WS5 | 2 | E2.5, E2.6 |
| Cartesian & polar systems, plotting, area shading | WS4 | 2 | E2.4 |
| Graphs, trees, networks, flowcharts | WS5 | 3 | E3.5 |
| Snapping, alignment, constraints | WS5 | 2 | E2.7, E2.9 |
| Dashes, dots, thickness, colour, transparency, patterns, hatching, layers, transformations | WS5/WS2 | 1–2 | P1-10, E2.11 |
| Ruler, compass, protractor, set squares, templates | WS5/WS1 | 2 | E2.10 |
| Step-by-step construction animation | WS5 | 3 | E3.6 |
| Free / suggest / auto-correct; originals recoverable | WS5/WS3 | 2–3 | ADR-0003, E2.8, E3.3 |

## 3. Intelligent assistance

| Requirement | WS | Phase | Backlog |
|---|---|---|---|
| Works entirely offline | WS6 | 5 | E5.1, ADR-0005 |
| Understand notebook context | WS6 | 5 | E5.2, E5.5 |
| Interpret handwritten notes, maths, diagrams | WS6/WS3/WS5 | 3–5 | E3.1, E3.5, E5.2 |
| Explain concepts, help solve problems | WS6/WS4 | 5 | E5.3, E5.4 |
| Suggest corrections to constructions | WS6/WS5 | 5 | E5.3 (`geo.check`) |
| Summarise notes and documents | WS6 | 5 | E5.4 |
| Generate examples, exercises, diagrams | WS6 | 5 | E5.6 |
| Help organise research and study materials | WS6 | 5 | E5.7 |
| Deterministic methods for calculations | WS4/WS5/WS6 | 2–5 | ADR-0005, E5.3 |

## 4. Synchronisation and collaboration

| Requirement | WS | Phase | Backlog |
|---|---|---|---|
| Personal sync across devices | WS7 | 4 | E4.2, E4.3 |
| Simultaneous multi-user editing | WS7 | 4 | E4.4 |
| Multiple users, cursors, identities | WS7 | 4 | E4.1, E4.6 |
| Owner / editor / commenter / viewer | WS7 | 4 | E4.5 |
| Offline edits synced on reconnection | WS7 | 4 | ADR-0001, E4.4 |
| Detailed revision history | WS7 | 4 | E4.8 |
| Restore versions or individual changes | WS7 | 1 (local), 4 | P1-28, E4.8 |
| Comments, annotations, recoverable deletions | WS7/WS2 | 1, 4 | P1-04, E4.7 |
| Google infrastructure; Drive backup | WS7 | 4 | E4.1, E4.2, E4.4, E4.9 |

## 5. Privacy and independence

| Requirement | WS | Phase | Backlog |
|---|---|---|---|
| Fully functional offline, no account | All | 1 | P1-37, conventions §7 |
| Essential tools local | All | 1–3 | ADR-0001, ADR-0006 |
| No mandatory cloud, subscriptions, external AI APIs | All | — | ADR-0005, ADR-0006 |
| Local storage, autosave, backup | WS2 | 1 | E1.7 |
| Optional Google integration | WS7 | 4 | E4.1 |
| User-owned portable data, standard formats | WS2 | 1–2 | P0-13, P1-35, E2.12 |
| No provider dependence for existing work | WS2/WS7 | 1, 4 | `.kahie` + JSON, E4.3 |

## 6. User experience

| Requirement | WS | Phase | Backlog |
|---|---|---|---|
| Open and write immediately | WS8/WS1 | 1 | P1-37 |
| Familiar, intuitive, minimal UI | WS8 | 1+ | P0-14, P1-38, P1-39 |
| Simple defaults, no configuration | WS8 | 1+ | P1-37, P1-38 |
| Progressive disclosure | WS8 | 1–3 | P1-41, E3.7 |
| Instructions and optional tutorials | WS8 | 1 | P1-40 |
| Autosave, easy undo, recovery | WS2 | 1 | E1.7 |
| Consistent experience across devices | WS8/WS1 | 1+ | P1-42, E6.1, E6.2 |
| Simple and Advanced modes, same capabilities | WS8 | 1–3 | P1-41, E3.7 |
