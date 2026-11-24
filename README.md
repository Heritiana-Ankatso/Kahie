# Kahie

An offline-first, stylus-first digital notebook and mathematical workspace — Android tablets first, also phones and computers.

> A beginner should be able to use it without instructions, while an expert should never feel constrained by it.

## Project status

**Kahie Web v0.1** is the first usable release: an offline, installable notebook with ink, shapes, maths, plots, PDF annotation, search and export. See [`app/web/`](app/web/README.md) and [ADR-0007](docs/adr/0007-web-pwa-first-release.md). The native Android app (Rust core + Kotlin, `core/` and `app/android/`) is in progress.

```bash
python3 app/web/tools/serve.py 5178   # then open http://127.0.0.1:5178/
```

## Licence

Kahie is free software under the [GNU General Public License v3.0 or later](LICENSE) (decision D1).

## Where to start

- **[Kahie Web](app/web/README.md)**: run, test and install the app.
- **[Delivery plan](docs/PLAN.md)** — goals, principles, phases, milestones, team.
- **[Project status and handoff](docs/STATUS.md)** — what is done, what is next.
- **[Hand-off](docs/HANDOFF.md)** — current state, branch and merge rules, how to work on the code, next steps.
- [Quick guide (Android)](docs/user-guide.md) — how to use the app.
- [Running a sync relay](docs/relay.md) — self-hosted sync between devices.
- [Documentation index](docs/README.md) — architecture, ADRs, workstreams, backlog, decisions, risks.
- [Phase 0 backlog](docs/backlog/phase-0.md) — the first tickets to pick up.
- [Product description](docs/product/app-description.md) — source requirements.
