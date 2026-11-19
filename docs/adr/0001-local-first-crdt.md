# ADR-0001: Local-first architecture on a CRDT document model

- **Status:** Proposed
- **Date:** 2026-11-18
- **Related:** P0-03, P0-11, WS7, [data-model.md](../architecture/data-model.md)

## Context

Requirements: fully functional offline without an account; multi-device sync; real-time multi-user collaboration; offline edits merged on reconnection; detailed revision history (who/what/when); restore versions or individual changes; recoverable deletions; no dependence on a cloud provider.

## Options considered

1. **Local database + server-authoritative sync (last-write-wins / operational transform on a server).** Requires an always-available server for merging; weak offline merging; conflicts lose data.
2. **Local database + custom diff/merge.** High risk; reinvents CRDTs badly.
3. **CRDT document model as the single source of truth (local-first).** Merging is deterministic on any device, no server logic needed; history built in. Costs: metadata overhead, learning curve, library dependency.

## Decision

Every notebook is a CRDT document. All edits are CRDT operations produced by core commands. The device stores the CRDT update log as the source of truth. Sync transports only move opaque update blobs; servers never need to understand the document.

Candidate libraries: **Loro** (preferred: movable tree, rich text, time travel), **Automerge**, **Yrs**. Final choice by spike P0-03.

## Consequences

- Positive: offline, sync, collaboration, history and restore share one mechanism; any dumb storage (Drive, folder, WebDAV) can serve as a sync medium.
- Negative: document size grows with history → compaction policy needed; ink must be committed per stroke; schema migrations must be CRDT-compatible.
- Follow-up: history compaction policy (keep full history for N days, then snapshot) — WS7.

## Validation

P0-03 benchmark targets: 100 000 strokes document loads ≤ 1.5 s on mid-range device; merge of two 1-week divergent offline histories ≤ 2 s; file overhead ≤ 30 % over raw stroke data.
