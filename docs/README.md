# Documentation index

Start with **[PLAN.md](PLAN.md)**.

| Document | Purpose |
|---|---|
| [PLAN.md](PLAN.md) | Goals, principles, scope, phases, milestones, team |
| [product/app-description.md](product/app-description.md) | Original product description (source requirements) |
| [architecture/overview.md](architecture/overview.md) | Layers, repository layout, interfaces, performance budgets |
| [architecture/data-model.md](architecture/data-model.md) | Document model, CRDT mapping, interpretations, file format |
| [adr/](adr/README.md) | Architecture Decision Records |
| [workstreams/](workstreams/) | One technical specification per workstream (WS1–WS8) |
| [backlog/phase-0.md](backlog/phase-0.md) | Foundations and de-risking spikes (ticket level) |
| [backlog/phase-1.md](backlog/phase-1.md) | Notebook MVP (ticket level) |
| [backlog/phases-2-6.md](backlog/phases-2-6.md) | Later phases (epic level) |
| [decisions-open.md](decisions-open.md) | Product/technical decisions still pending |
| [risks.md](risks.md) | Risk register |
| [engineering/conventions.md](engineering/conventions.md) | Repo conventions, Definition of Ready/Done, testing, privacy rules |
| [requirements-traceability.md](requirements-traceability.md) | Every requirement in the description → workstream, phase, ticket/epic |

## Conventions for these documents

- Ticket IDs: `P<phase>-<nn>` (e.g. `P1-07`). Epic IDs: `E<phase>.<n>` (e.g. `E2.3`).
- Decision IDs: `D<n>`. Risk IDs: `R<n>`. ADR IDs: `ADR-<nnnn>`.
- Sizes: **S** ≤ 3 days, **M** ≤ 2 weeks, **L** ≤ 5 weeks, **XL** = must be split before starting.
- Documents are living: update them in the same PR as the change they describe.
