# P0-10 — Self-hostable sync relay (replaces the Firebase simulation)

- **Status:** Done on desktop (relay implemented, simulated, measured); production hardening listed below
- **Date:** 2026-11-20
- **Context:** decision D9 chose a self-hostable relay instead of Firebase, so this spike builds and measures that relay rather than comparing Firebase products.
- **Code:** `core/crates/nb-relay` (server `kahie-relay`, integration test), simulation `cargo run --release -p nb-relay --example classroom`

## Design

- **Opaque store-and-forward.** Clients send Loro updates; the relay appends each one to a per-document SQLite log (`nb-store`) and forwards it to the other clients of that document. It never parses content, so end-to-end encryption can be added later without changing the server.
- **Cursors.** Every stored update gets a sequence number; a client reconnects with `?after=<seq>` and receives what it missed, then live updates. A client too slow for the live stream is disconnected and catches up from storage.
- **Sync cursor in the model** (`CrdtPage::sync_outgoing/sync_incoming`): a client uploads exactly the changes the relay lacks — including offline edits from earlier sessions — and never echoes received changes back. (Using the persistence cursor for uploads made every client resend everyone's strokes: 3.6 kB per stroke instead of 0.7 kB.)
- **Protocol rule:** the creator of a document uploads its initial state first, and joining clients build their replica from relayed bytes (`CrdtPage::load`), never from a fresh local page.
- **Security v0:** one shared bearer token per server; the binary refuses to listen on a non-loopback address without it; document ids restricted to `[A-Za-z0-9_-]{1,64}`. TLS by a reverse proxy.

## Simulation

One teacher (a stroke every 3 s) and 30 students (a stroke every 10 s) on **one shared page**, 150-point strokes, Poisson arrivals, all on one 4-vCPU container together with the relay. The teacher and one student are full replicas (import everything, checked for convergence); the other 29 send real concurrent updates and receive every frame.

**Run A — 10 minutes at 10× density** (the default):

| Measure | Value |
|---|---|
| Strokes | 2 142 (214 per simulated minute) |
| Update size per stroke | 692 bytes (ink ≈ 450 + CRDT metadata) |
| Upload / download, all clients | 1.5 MB / 45 MB (fan-out × 30) |
| Relay storage | 5.9 MB |
| End-to-end latency p50 / p95 / p99 | **1.1 / 38 / 105 ms** |
| Clients dropped | 0 |
| Full replicas converged | yes |

**Run B — 60 minutes at 60× density** (stress): 10 717 strokes, 7.4 MB up, 214 MB down, 12.7 MB stored, latency p50 10 ms, p95 41 ms; the two full replicas could not keep up (the 31 client threads and the relay share 4 CPUs), so p99 grew to seconds. Not representative of a real lesson, where each device runs one client.

**Client cost:** importing one remote stroke into a 3 000-stroke page costs 0.72 ms (Loro 0.04 ms + view update 0.68 ms), i.e. ~1 400 strokes/s per device.

## Findings fixed during the spike

1. The page view was rebuilt from the whole document on every import (`get_deep_value`, ≈ 10 µs per object). It now follows Loro change events and re-reads only changed objects; unchanged items are moved, not cloned. Debug builds compare the cached view with a full rebuild after every change.
2. Upload cursor separated from the persistence cursor (see Design).

## Cost estimate for one real lesson (60 min, 31 devices)

Upload ≈ 7 MB, download ≈ 215 MB **per class per lesson** on one shared page (each stroke goes to 30 devices); storage ≈ 13 MB per shared page per lesson before compaction. A small VPS (1 vCPU, 1 GB RAM, ~€5/month) handles many concurrent classes; bandwidth is the main cost (≈ 0.2 GB per lesson per shared page). Personal sync (one user, several devices) is negligible in comparison. No per-operation fees, unlike Firebase.

## Next steps (production)

- ~~Server-side compaction~~ — done: automatic every 1 000 updates and `kahie-relay compact <doc>`; clients behind the snapshot receive it first.
- Per-document access control (teacher/student roles) instead of one server token.
- Optional end-to-end encryption of updates.
- ~~Android sync client~~ — done: `nb-ffi` `SyncSession` (pages and notebook outlines), cursor stored in each database; app: Share / Join / Sync settings.
