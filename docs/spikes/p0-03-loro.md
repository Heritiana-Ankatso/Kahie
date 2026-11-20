# P0-03 — CRDT library validation: Loro

- **Status:** Partial — desktop measurements done; reference-device run pending (needs P0-15 devices)
- **Date:** 2026-11-20
- **Decision context:** D6 chose Loro; this spike checks it against the budgets instead of comparing candidates.
- **Code:** [`spikes/p0-03/`](../../spikes/p0-03) — `cargo run --release` reproduces every number below (deterministic PRNG).

## Method

- Loro 1.18.0, release build, single thread.
- Machine: cloud container, Intel Xeon @ 2.30 GHz, 4 vCPU, Linux. **Not** the mid-range reference tablet; expect roughly 2–4× slower on a mid-range ARM tablet (estimate, to be measured).
- Stroke model: one entry per stroke in a `LoroMap` (key = stroke id, value = packed binary) plus a `LoroMovableList` for z-order. Points use the candidate packed encoding: quantised to 1/8 page unit and 1/255 pressure, delta-coded zig-zag varints.
- Synthetic strokes: smooth random walks, 150 points each, 1–2 page units per sample (handwriting-like density).

## Results
### S1 — 100000 strokes × 150 points

| Measure | Value |
|---|---|
| Points | 15000000 |
| Ink encoding (packed) | 3.05 bytes/point (target ≤ 6) |
| Raw f32 (x,y,p) size | 171.7 MiB |
| Build time (insert + commits) | 689 ms |
| RSS growth while building | 118 MiB |
| Snapshot with history | 89.9 MiB (export 610 ms) |
| State-only snapshot (no history) | 45.4 MiB |
| Load snapshot (import) | 193 ms |
| First state read after load | 240 ms |
| Load state-only snapshot + read | 310 ms |
| RSS growth after load | 183 MiB |

### S2 — Offline divergence: 5 000 ops per replica

| Measure | Value |
|---|---|
| Update size A→B / B→A | 1.6 / 1.6 MiB |
| Merge time into A / into B | 335 / 364 ms |
| Converged (deep value equal) | true |

### S3 — Movable tree: concurrent moves

| Measure | Value |
|---|---|
| Merge time (both directions) | 4.2 ms |
| Converged | true |
| No cycle after concurrent X→Y / Y→X | true |
| Pages still reachable (should be 200 + 1 section) | 201 |

### S4 — Rich text: concurrent edits

| Measure | Value |
|---|---|
| Merge time | 5.0 ms |
| Same text and marks on both replicas | true |
| Final length (chars) | 2056 |

### S5 — History, revert and attribution

| Measure | Value |
|---|---|
| Checkout version 500 of 1 000 | 0.4 ms (500 entries) |
| Per-user undo removes own change | true |
| …and keeps the other user's change | true |
| Author of remote change (expected peer 8) | Some(8) |

## Assessment against the acceptance criteria

| Criterion | Result |
|---|---|
| 100 000-stroke document: load, memory, size | Load ≈ 0.4 s total (import + first read) on desktop; snapshot 90 MiB with full history, 45 MiB without. Acceptable; reference-device confirmation pending |
| Offline divergence merge (5 000 ops each) | ≈ 0.35 s per direction; converges |
| Movable tree, concurrent moves | Converges, no cycles, no lost pages |
| Rich text concurrent edits | Converges, marks preserved |
| History: checkout, per-user revert, attribution | All supported (`checkout`, `UndoManager`, change peer id) |
| Ink encoding ≤ 6 bytes/point (P0-11) | **3.05 bytes/point** |
| Licence | MIT — compatible with GPL-3 (D1); passes `cargo-deny` |
| Binary size on Android | Not measured: the NDK linker is unavailable in this environment. To measure in the Android CI job |

## Findings

1. **History roughly doubles file size** because every inserted stroke blob is kept in the history. Mitigation: store page *state-only* snapshots for fast open, and keep full history in the update log (P0-12), compacted with shallow snapshots after a retention window.
2. **One map entry per stroke scales well** (100 000 inserts in 0.7 s). Stroke points stay immutable blobs (ADR-0003); edits are new strokes or `Interpretation`s, never in-place point edits.
3. **Pages should be separate Loro documents** (or at least lazily loaded containers): a 100 000-stroke notebook is an extreme case, but per-page documents bound open time to the page being viewed.

## Recommendation

Keep **Loro** (D6). Move ADR-0001 to *Accepted* once the reference-device run confirms load ≤ 2 s for the 100 000-stroke case. Proceed with P0-11 (document model v0) on Loro now, with one Loro document per page and a library-level document holding the movable tree.
