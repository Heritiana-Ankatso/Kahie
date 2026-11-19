# ADR-0002: Rust core with Kotlin / Compose Multiplatform UI

- **Status:** Proposed
- **Date:** 2026-11-18
- **Related:** P0-02, P0-04, P0-05, [overview.md](../architecture/overview.md)

## Context

Primary platform: Android tablets with stylus. Secondary: phones and computers. Future: possibly iPad and web viewer. The product contains large amounts of deterministic logic (CRDT, geometry, solver, plotting, export) that must behave identically everywhere. Stylus latency is the first quality users perceive.

## Options considered

1. **Kotlin everywhere (Kotlin Multiplatform).** Good Android integration; numeric/geometry ecosystem weaker; no WASM path of comparable maturity for heavy logic.
2. **Flutter + Rust core.** Single UI codebase across all platforms; stylus low-latency path on Android needs native platform views anyway.
3. **Rust core + Kotlin/Compose Multiplatform UI.** Best Android stylus access (front-buffered rendering, motion prediction, palm rejection); Compose also targets desktop; Rust core portable to Android, desktop, iOS and WASM.
4. **C++ core.** Larger legacy library pool (CAS, PDFium), weaker safety and tooling.

## Decision

Option 3. Core logic in a Rust workspace, exposed through UniFFI-generated Kotlin bindings. UI in Compose Multiplatform (Android + desktop). Android-specific wet-ink layer implemented natively in Kotlin. C/C++ libraries (PDFium, CAS) are wrapped behind Rust traits.

## Consequences

- Positive: one implementation of all deterministic logic; testable in isolation; future iOS/web reuse the core.
- Negative: FFI boundary must be designed carefully (batching, no chatty calls); two languages in the team; Rust Android build tooling (cargo-ndk) in CI.
- Follow-up: FFI guidelines in [conventions](../engineering/conventions.md).

## Validation

- P0-02: wet-ink latency within budget with native front-buffered layer.
- P0-04: FFI round-trip for committing a 500-point stroke ≤ 1 ms; scene update notifications without UI jank.
- P0-05: display-list rendering of 50 000 strokes at ≥ 55 fps pan/zoom.

If P0-04/P0-05 fail, fallback: render in Rust directly to a GPU surface (Skia bindings), keeping Compose only for chrome.
