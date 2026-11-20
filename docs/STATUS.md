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
| P0-07 CAS (Giac) | Not started | Needs Giac Android build |
| P0-08 On-device LLM | Not started | Needs devices |
| P0-09 Typesetting | Done on desktop | `docs/spikes/p0-09-typesetting.md`: Typst + MiTeX, 99/99 formulas; APK size reduction pending |
| P0-10 Relay cost/latency | Not started | D9 now self-hostable relay |
| P0-11 Document model v0 | Done (page level) | `nb_model::crdt`, 15 tests incl. 3-replica convergence |
| P0-12 Storage v0 | Done | `nb-store`, 1 000-kill test, 2 000-stroke load 14 ms |
| P0-13 `.kahie` spec | Draft done | `docs/format/` |
| P0-14 UX research | Not started | Needs users |
| P0-15 Device lab | Not started | Needs hardware purchase (owner) |
| P0-16 Corpus protocol | Draft done | `docs/spikes/p0-16-corpus-protocol.md`, `app/web/collect.html` |

## Android build

CI job `android` builds the APK and uploads `kahie-debug-apk`. History of fixes: UniFFI bindings must be registered as Kotlin sources (AGP 9); error fields must not be named `message`. Check generated bindings locally with kotlinc (see `docs/engineering/conventions.md` §6).

## Needs the owner (cannot be done by the agent)

- Buy/borrow reference devices (P0-15); run device measurements for P0-02/03/04/08.
- Approve the corpus protocol and consent text; organise collection sessions; native-speaker review of Malagasy prompts.
- Trademark check for "Kahie" (D10).
- Merge the working branch into `main` (squash).

## Next steps for the agent (in order)

1. Get the `android` CI job green; download and inspect the APK artifact.
2. P0-05 Android side: tile cache executing nb-render display lists via FFI.
3. Notebook-level document: section/page movable tree (Loro) + library screen in the Android app (P1-01…P1-03).
4. P0-07: cross-compile Giac for Android arm64 in CI; `CasBackend` trait.
5. P0-09 typesetting evaluation; P0-10 relay design.
