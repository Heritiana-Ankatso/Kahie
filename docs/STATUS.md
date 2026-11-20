# Project status and handoff

Living file: updated at the end of every work session so the next one can resume without context.
Working branch: `ccr-121ac074-e1jgb6` (not yet merged to `main`; squash-merge recommended because commit `a7ad8c9` accidentally contained ~300 MiB of build output, removed in `cc09da9`).

**Last updated:** 2026-11-20

## Decisions (all in `docs/decisions-open.md`)

GPL-3.0-or-later · Android only (12+, 6 GB) · FR/EN/MG for UI and handwriting · Loro · Giac · opt-in crash reports · self-hostable relay · `.kahie` · D2 (recognition engine) waits for P0-06.

## Phase 0 progress

| Item | State | Evidence |
|---|---|---|
| P0-01 Repo, build, CI | Done except Android unit tests | `.github/workflows/ci.yml` (web, core, cargo-deny, offline tests, schema), `android.yml` (APK) |
| P0-02 Ink latency spike | Prototype only | Jetpack Ink wet layer in `app/android/.../JetpackWetInk.kt`; needs devices (P0-15) |
| P0-03 CRDT validation | Done on desktop | `docs/spikes/p0-03-loro.md`; device run pending |
| P0-04 FFI spike | Done on desktop | `docs/spikes/p0-04-ffi.md`; JNA overhead on device pending |
| P0-05 Rendering pipeline | Core done on desktop | `docs/spikes/p0-05-render.md`; Android tile cache + device fps pending |
| P0-06 HWR benchmark | Blocked on corpus | Protocol + tool ready (P0-16) |
| P0-07 CAS (Giac) | Done on desktop | `docs/spikes/p0-07-giac.md`: 88/88 tasks, < 1 ms; Android build + size pending |
| P0-08 On-device LLM | Not started | Needs devices |
| P0-09 Typesetting | Done on desktop | `docs/spikes/p0-09-typesetting.md`: Typst + MiTeX, 99/99 formulas; APK size reduction pending |
| P0-10 Relay | Done on desktop | `docs/spikes/p0-10-relay.md`, `core/crates/nb-relay` |
| P0-11 Document model v0 | Done (page level) | `nb_model::crdt`, 15 tests incl. 3-replica convergence |
| P0-12 Storage v0 | Done | `nb-store`, 1 000-kill test, 2 000-stroke load 14 ms |
| P0-13 `.kahie` spec | Draft done | `docs/format/` |
| P0-14 UX research | Not started | Needs users |
| P0-15 Device lab | Not started | Needs hardware purchase (owner) |
| P0-16 Corpus protocol | Draft done | `docs/spikes/p0-16-corpus-protocol.md`, `app/web/collect.html` |

## Android app (what works)

- Pen/highlighter/eraser on the Rust core, Jetpack Ink wet layer, shape snapping, undo/redo, paper templates.
- Every edit saved immediately (SQLite per page); legacy JSON pages migrate automatically.
- Several notebooks (Notebooks dialog: switch, create, rename), pages outline (Pages dialog: open, add, delete to trash).
- `.kahie` export/import through the system file picker.

CI: `android.yml` builds the APK (artifact `kahie-debug-apk`) and runs an emulator smoke test (draws strokes, checks ink pixels and crashes; screenshots in artifact `smoke-screenshots`). `giac-android.yml` cross-compiles the CAS for arm64 and reports its size. Lessons: UniFFI bindings must be Kotlin source dirs (AGP 9); error fields must not be named `message`; Rust is pinned in `core/rust-toolchain.toml`; check bindings locally with kotlinc (conventions §6).

## Needs the owner (cannot be done by the agent)

- Buy/borrow reference devices (P0-15); run device measurements for P0-02/03/04/08.
- Approve the corpus protocol and consent text; organise collection sessions; native-speaker review of Malagasy prompts.
- Trademark check for "Kahie" (D10).
- Merge the working branch into `main` (squash).

## Next steps for the agent (in order)

1. Keep `android` (APK + smoke) green; fix `giac-android` until it reports the CAS size, then record it in p0-07-giac.md.
2. Sync client done (SyncSession for pages + outline; Share/Join/Settings in the app). Next: sync status indicator, sync all pages in background (not only the open one), relay compaction.
3. P0-05 Android side: tile cache executing nb-render display lists via FFI.
4. Section UI in the Pages dialog; trash view with restore.
5. `CasBackend` in nb-math on top of the Android Giac build; maths blocks in the app (Typst+MiTeX rendering after size reduction).
6. Relay compaction and per-document access control.
