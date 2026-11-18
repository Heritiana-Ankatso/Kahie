# WS7 — Sync & collaboration

**Lead role:** Backend/sync engineer · **Phases:** 0 (spike), 4 · **Related ADRs:** 0001, 0006

## Mission

Optional online features that never compromise offline use: personal multi-device sync, real-time collaboration, permissions, revision history and restore.

## Scope

- Personal sync across the user's devices.
- Shared notebooks with roles: **owner, editor, commenter, viewer**.
- Real-time presence: cursors, selections, identities (name, colour, avatar).
- Offline edits merged on reconnection.
- Revision history: who changed what, when, how; browse, compare, restore version, revert individual change.
- Comments and annotations; recoverable deletions (trash).
- Backup to Google Drive; local folder/WebDAV adapter for independence.

## Technical approach

| Topic | Approach |
|---|---|
| Identity | `AuthProvider`: Firebase Authentication with Google Sign-In; local anonymous actor ID before sign-in, mapped to account at sign-in |
| Personal sync | `SyncTransport` over **Google Drive appDataFolder**: each device appends compressed CRDT update files; periodic compaction into snapshot by any device (deterministic merge makes this safe) |
| Real-time collaboration | Firebase **Realtime Database** (low latency, presence built in) or **Firestore** as relay of CRDT update blobs per notebook; choose after P0-10 |
| Assets | `BlobStore`: Firebase Storage (shared notebooks) / Drive (personal); content-addressed, uploaded once |
| Permissions | ACL document per shared notebook; Firebase Security Rules enforce: viewers read-only, commenters write only to comments path, editors write updates, owner manages ACL |
| Viewer/commenter enforcement in CRDT | Clients reject applying non-comment updates from actors without editor role (defence in depth); server rules are the authority |
| Presence | Ephemeral channel (RTDB `onDisconnect`), not stored in CRDT |
| History UI | Timeline grouped by session/user; per-object history; diff overlay (added/removed highlighting); restore = checkout old state + commit as new change; revert change = apply inverse ops |
| Compaction | Keep full history for N days (configurable), then snapshot; named versions kept forever |
| Independence | Local folder / WebDAV `SyncTransport` shipped in Phase 4 |

## Known limitations (communicate to users)

- Revoking access cannot remove data already synced to a former collaborator's device.
- Firebase free-tier quotas; cost model from P0-10 decides whether collaboration needs a paid plan.

## Validation

- Convergence property tests: random concurrent edit sequences across N simulated replicas always converge (CI, thousands of seeds).
- Soak test: 3 devices × 5 users × 72 h with random network partitions → no divergence, no data loss.
- Collaboration latency: remote stroke visible ≤ 500 ms p95 on 4G.

## Deliverables by phase

| Phase | Deliverable |
|---|---|
| 0 | P0-10 Firebase cost/latency simulation |
| 4 | All of the above |
