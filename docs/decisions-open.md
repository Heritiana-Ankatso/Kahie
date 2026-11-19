# Open decisions

Each decision has an owner, a deadline and the work it blocks. When resolved: record the outcome and date here, and create/update the relevant ADR if architectural.

| ID | Decision | Owner | Deadline | Blocks | Status |
|---|---|---|---|---|---|
| D1 | Licence & business model | Product owner | Week 4 | P0-07, D7 | Open |
| D2 | Handwriting recognition: buy or build | Product owner + tech lead | M0 | P0-06 scope, E1.8, E3.1, staffing | Open |
| D3 | Platforms beyond Android | Product owner | M2 | E6.1, UI architecture checks | Open |
| D4 | Minimum supported device | Product owner + tech lead | Week 3 | P0-15, P0-08, budgets | Open |
| D5 | Initial languages (UI, handwriting, AI) | Product owner | Week 4 | P0-06, P0-08, P1-29 | Open |
| D6 | CRDT library | Tech lead | M0 | P0-11 | Open — resolved by P0-03 |
| D7 | CAS backend | Tech lead | M0 | E2.3 | Open — resolved by P0-07 + D1 |
| D8 | Telemetry and crash reporting | Product owner | M1α | P1-45 | Open |
| D9 | Collaboration funding model | Product owner | M3 | E4.4 | Open |
| D10 | Product name & file extension | Product owner | M1α | `.nbk` naming, store listing | Open |

---

### D1 — Licence and business model
- **Options:** (a) Open source under GPL-3 — enables Giac CAS, community contributions, strong "independence" story; (b) proprietary/freemium — requires permissive dependencies only (SymEngine, own recognition) or commercial licences; (c) open-core.
- **Recommendation:** decide early; it constrains D2 and D7. If the independence principle is central to the product identity, (a) or (c) is coherent with it.

### D2 — Handwriting recognition: buy or build
- **Options:** (a) MyScript iink SDK (text + maths + shapes, offline, excellent; per-device/user licence cost; proprietary); (b) ML Kit for text + in-house maths model; (c) fully in-house.
- **Trade-off:** (a) saves ≈ 12 engineer-months and reduces risk R3; costs licence fees and vendor dependence, incompatible with GPL (D1-a).
- **Recommendation:** decide after P0-06 data; meanwhile keep `TextHwr`/`MathHwr` traits engine-agnostic.

### D3 — Platforms beyond Android
- iPads are widespread in education; Compose Multiplatform + Rust core allow iOS later. Desktop preview is already in Phase 1.
- **Recommendation:** commit to iOS feasibility review at M2.

### D4 — Minimum supported device
- Determines performance budgets, AI tiers and test matrix.
- **Recommendation:** Android 10+ (API 29), ≥ 4 GB RAM for the app, stylus support recommended but not required; AI Standard tier ≥ 8 GB RAM (to be confirmed by P0-08).

### D5 — Initial languages
- Affects HWR models, AI model choice, UI translation, templates.
- **Recommendation:** start with French and English for UI and handwriting; confirm whether other languages are required for the target market.

### D6 — CRDT library
- Candidates: Loro (preferred), Automerge, Yrs. Resolved by P0-03 measurements.

### D7 — CAS backend
- Giac (GPL, complete) vs SymEngine (MIT, needs supplementing). Resolved by P0-07 and D1.

### D8 — Telemetry and crash reporting
- **Recommendation:** off by default; opt-in crash reports with no user content; local diagnostic export for support; no analytics SDKs.

### D9 — Collaboration funding model
- Firebase costs scale with active collaboration. Options: free within quotas, paid tier for heavy collaboration, user-provided Firebase project, self-hosted relay. Input: P0-10.

### D10 — Product name & file extension
- Working name "Kahie"; file extension placeholder `.nbk`. Check trademark and extension collisions.
