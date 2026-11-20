# ADR-0006: All external services and buy/build engines behind interfaces

- **Status:** Proposed
- **Date:** 2026-11-18
- **Related:** [overview.md §3](../architecture/overview.md#3-core-interfaces-swappable-components), D2, D7, WS7

## Context

Google services are proposed for authentication, sync, collaboration and backup, while the description also requires "no dependence on a particular cloud provider" and "no mandatory cloud services". Several engines are buy-or-build candidates (handwriting recognition, CAS, LLM runtime).

## Decision

Define Rust traits for `SyncTransport`, `AuthProvider`, `BlobStore`, `TextHwr`, `MathHwr`, `CasBackend`, `LlmRuntime`. Google Drive and Firebase are *implementations*, not dependencies of the core. A **local folder / WebDAV** sync adapter is shipped alongside the Google adapters (Phase 4) to prove independence. The portable `.kahie` format includes a human-readable JSON export.

## Consequences

- Positive: vendor independence; buy/build decisions reversible; testable with in-memory fakes.
- Negative: interface design effort; lowest-common-denominator risk (mitigated by optional capability flags).

## Validation

CI runs the full test suite with only in-memory/local adapters and networking disabled.
