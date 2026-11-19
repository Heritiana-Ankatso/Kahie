# Intelligent Notebook — App description

> Source requirements document, as provided by the product owner. Do not edit the requirements here without product-owner approval; record interpretations in [`../PLAN.md`](../PLAN.md) or [`../requirements-traceability.md`](../requirements-traceability.md).

An offline-first, AI-powered digital notebook and mathematical workspace, designed primarily for Android tablets and stylus use, but also accessible from phones and computers.

Its central idea is simple: make writing, drawing, organising knowledge and working with mathematics as natural as using paper, while providing the capabilities of a sophisticated digital workspace.

## 1. Notebook and workspace

- Unlimited notebooks, folders, sections and pages.
- Traditional pages (A4, A5, etc.) and infinite canvases.
- Combine handwriting, typed text, mathematical expressions, drawings, diagrams and images on the same page.
- Convert handwriting into editable, beautifully typeset text and mathematics.
- Read and annotate PDFs, insert documents and images.
- Search across notebooks, handwriting, text and mathematical content.
- Templates for teaching, research, studying and personal notes.
- Export to PDF, SVG, PNG, Markdown, LaTeX and other standard formats.

## 2. Intelligent mathematical drawing

A sophisticated drawing engine that understands what the user is trying to construct, rather than merely recognising shapes.

- Recognise and construct geometric objects and their relationships.
- Recognise mathematical curves and suggest possible functions.
- Construct tangents, normals, intersections, asymptotes and other mathematical objects.
- Support Cartesian and polar coordinate systems, function plotting and area shading.
- Understand graphs, trees, networks, flowcharts and mathematical diagrams.
- Provide intelligent snapping, alignment and geometric constraints.
- Support professional drawing features: dashed and dotted lines, thickness, colours, transparency, patterns, hatching, layers and transformations.
- Include virtual instruments: ruler, compass, protractor, set squares and geometric templates.
- Allow mathematical constructions to be animated step by step.

The user can draw freely, receive suggestions or activate automatic correction. Original strokes should always remain recoverable.

## 3. Intelligent assistance

An integrated AI assistant that works entirely offline.

- Understand the current notebook and its context.
- Interpret handwritten notes, mathematical expressions and diagrams.
- Explain concepts and help solve mathematical problems.
- Suggest corrections and improvements to constructions.
- Summarise notes and documents.
- Generate examples, exercises and diagrams.
- Help organise research and study materials.

AI should complement reliable mathematical algorithms, not replace them. Mathematical calculations, geometric constraints and other deterministic operations should use appropriate computational methods.

## 4. Synchronisation and collaboration

Optional online capabilities, independent of ordinary offline use.

- Personal synchronisation: keep notebooks consistent across the user's devices.
- Collaboration: allow several people to work simultaneously on shared notebooks and canvases.
- Multiple users, cursors and identities.
- Sharing permissions: owner, editor, commenter and viewer.
- Offline edits synchronised when connectivity returns.
- Detailed revision history: who changed what, when and how.
- Restore previous versions or individual changes.
- Comments, annotations and recoverable deletions.

Google services could provide managed infrastructure for authentication, synchronisation and collaboration, avoiding the need to rent and administer a personal server. Google Drive could also provide backup and file storage.

## 5. Privacy and independence

- Fully functional offline, without an account.
- All essential notebooks and mathematical tools available locally.
- No mandatory cloud services, subscriptions or external AI APIs.
- Local storage, automatic saving and backup options.
- Optional Google account integration for online features.
- User-owned, portable data in standard formats.
- No dependence on a particular cloud provider for accessing existing work.

## 6. User experience — a fundamental requirement

**Non-negotiable design principle:** the application must be usable even by someone with very little technological experience.

- Open the application and start writing immediately.
- Familiar, intuitive interactions.
- Minimal and uncluttered interface.
- Simple defaults that work without configuration.
- Advanced features revealed progressively, only when needed.
- Clear instructions and optional guided tutorials.
- Automatic saving, easy undo and recovery.
- Consistent experience across devices.

Two levels of interaction should be available:

- **Simple mode:** a straightforward notebook experience with essential tools.
- **Advanced mode:** access to the full mathematical, drawing, research and collaboration capabilities.

Both modes should have the same underlying capabilities. The difference is how much control is exposed.

## The overall vision

A single application combining the best ideas of a digital notebook, mathematical whiteboard, geometry environment, research workspace and intelligent personal assistant.

It should be:

- Simple enough for someone who has never used a digital notebook.
- Powerful enough for a mathematics teacher, researcher or university student.
- Flexible enough for personal notes, teaching, research and collaboration.
- Independent enough to remain useful without internet access or cloud services.

**The governing principle:** a beginner should be able to use it without instructions, while an expert should never feel constrained by it.
