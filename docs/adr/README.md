# Architecture Decision Records

An ADR records one significant architectural decision: context, decision, consequences.

## Process

1. Copy [`template.md`](template.md) to `NNNN-short-title.md` (next free number).
2. Open a PR with status **Proposed**.
3. Tech lead + at least one engineer of each affected workstream review.
4. Merge with status **Accepted**. Superseding requires a new ADR referencing the old one (old one → **Superseded by ADR-xxxx**).

## Index

| ADR | Title | Status | Confirmed by |
|---|---|---|---|
| [0001](0001-local-first-crdt.md) | Local-first architecture on a CRDT document model | Proposed | P0-03 |
| [0002](0002-rust-core-kotlin-ui.md) | Rust core with Kotlin / Compose Multiplatform UI | Proposed | P0-02, P0-04, P0-05 |
| [0003](0003-immutable-ink-interpretations.md) | Immutable raw ink; recognition as interpretations | Proposed | — |
| [0004](0004-construction-graph.md) | Geometry stored as a construction dependency graph | Proposed | — |
| [0005](0005-llm-as-tool-orchestrator.md) | Language model as tool orchestrator, never as calculator | Proposed | P0-08 |
| [0006](0006-pluggable-adapters.md) | All external services and buy/build engines behind interfaces | Proposed | — |
