# ADR-0007: Ship a web (PWA) release first; native Rust/Kotlin stays the long-term track

- **Status:** Proposed
- **Date:** 2026-11-19
- **Deciders:** Product owner (pending review)
- **Related:** ADR-0002, ADR-0003, PLAN §4, decisions D1–D5

## Context

The product owner asked for a usable Kahie **within one week** (2026-11-19). The delivery plan (PLAN.md) assumes a team of 7–9 people over roughly 30 months, with a native stack (ADR-0002: Rust core + Kotlin/Compose UI). That stack needs an Android toolchain, reference devices and Phase 0 spikes before any user can write a single stroke. None of that fits in a week. The development machine also has no Android SDK set up yet (`ANDROID_HOME` unset, no Gradle/adb/cargo-ndk).

## Options considered

1. **Follow the plan as written:** start Phase 0 spikes. Nothing a user can actually use exists by the end of the week.
2. **Native Android app with a minimal Kotlin canvas:** needs SDK setup, device testing and a Play Store or APK channel. It fits a week only with a much smaller feature set, and the result is Android-only.
3. **Installable offline web app (PWA), no build step:** it runs on Android tablets (Chrome supports pen pressure through Pointer Events and can install the app to the home screen), on phones and on computers. It can be verified in a browser today, and the data stays on the device in IndexedDB.

## Decision

Option 3. Ship **Kahie Web v0.1** under `app/web/`: plain ES modules, no bundler, no runtime network access. It covers notebooks and pages; pen, highlighter, eraser, selection, text, LaTeX maths (vendored MathJax), function plots, shape tidying, templates, PDF import for annotation (vendored pdf.js), search, PNG/SVG/print-to-PDF export, `.kahie` backup files, French and English, and Simple/Advanced modes.

ADR-0002 is **not** superseded. The native stack remains the long-term architecture. The web app is the first shipping vehicle: it puts the product in teachers' hands now and makes the Phase 1 pilot possible early. It already honours the other binding principles:

- Local-first: there is no server at all.
- ADR-0003: a tidied shape is stored as `stroke.interp` next to the untouched raw points, and can be switched back.
- ADR-0006: maths and PDF engines sit behind small modules (`mathtex.js`, the PDF import function) and can be swapped.

Dependencies are permissive only (MathJax and pdf.js, both Apache-2.0), so licence decision D1 stays fully open.

## Consequences

- **Positive:** a usable product this week on every platform; real usage data for the Phase 1 pilot, which feeds D2, D4 and D5; the data model already matches the planned document model in spirit (items, interpretations, reversible operations).
- **Negative / costs:**
  - Stylus latency depends on the browser. There is no front-buffer rendering, so the plan's ≤ 25 ms latency target is not guaranteed.
  - No CRDT yet (ADR-0001), so there is no sync or collaboration.
  - Storage is per browser profile; `.kahie` backup files are the portability path.
  - PDF pages are stored as raster backgrounds.
- **Follow-up work:**
  - Measure pen latency on real tablets (P0-02 can use this app as its baseline).
  - Write a migration from the web `.kahie` backup format to the native `.kahie` format (same extension since D10; distinguish by format version) when the native model lands (P0-13).
  - Decide the hosting channel for the PWA.

## Validation

- Unit tests (`app/web/tests`) and the browser checks recorded in the PR.
- Review after two weeks of pilot use: if teachers find the stylus feel acceptable, the native track can be re-prioritised against Phases 2–3 features. If not, P0-02 and ADR-0002 move up.
