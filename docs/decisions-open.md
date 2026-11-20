# Open decisions

Each decision has an owner, a deadline and the work it blocks. When resolved: record the outcome and date here, and create/update the relevant ADR if architectural.

| ID | Decision | Owner | Deadline | Blocks | Status |
|---|---|---|---|---|---|
| D1 | Licence & business model | Product owner | Week 4 | P0-07, D7 | Resolved 2026-11-20 — GPL-3 |
| D2 | Handwriting recognition: buy or build | Product owner + tech lead | M0 | P0-06 scope, E1.8, E3.1, staffing | Deferred to P0-06 (constrained by D1) |
| D3 | Platforms beyond Android | Product owner | M2 | E6.1, UI architecture checks | Resolved 2026-11-20 — Android only |
| D4 | Minimum supported device | Product owner + tech lead | Week 3 | P0-15, P0-08, budgets | Resolved 2026-11-20 — Android 12+, 6 GB |
| D5 | Initial languages (UI, handwriting, AI) | Product owner | Week 4 | P0-06, P0-08, P1-29 | Resolved 2026-11-20 — FR, EN, MG (UI + HWR) |
| D6 | CRDT library | Tech lead | M0 | P0-11 | Resolved 2026-11-20 — Loro |
| D7 | CAS backend | Tech lead | M0 | E2.3 | Resolved 2026-11-20 — Giac |
| D8 | Telemetry and crash reporting | Product owner | M1α | P1-45 | Resolved 2026-11-20 — opt-in crash reports |
| D9 | Collaboration funding model | Product owner | M3 | E4.4 | Resolved 2026-11-20 — self-hostable relay |
| D10 | Product name & file extension | Product owner | M1α | `.nbk` naming, store listing | Resolved 2026-11-20 — Kahie, `.kahie` |

---

### D1 — Licence and business model
- **Options:** (a) Open source under GPL-3 — enables Giac CAS, community contributions, strong "independence" story; (b) proprietary/freemium — requires permissive dependencies only (SymEngine, own recognition) or commercial licences; (c) open-core.
- **Recommendation:** decide early; it constrains D2 and D7. If the independence principle is central to the product identity, (a) or (c) is coherent with it.
- **Outcome (2026-11-20):** (a) open source under **GPL-3**. Consequences: Giac is allowed (D7); MyScript is ruled out for D2; any closed-source SDK (e.g. ML Kit) needs a GPL linking exception added by the copyright holder; MuPDF (AGPL) becomes possible but PDFium stays the default.

### D2 — Handwriting recognition: buy or build
- **Options:** (a) MyScript iink SDK (text + maths + shapes, offline, excellent; per-device/user licence cost; proprietary); (b) ML Kit for text + in-house maths model; (c) fully in-house.
- **Trade-off:** (a) saves ≈ 12 engineer-months and reduces risk R3; costs licence fees and vendor dependence, incompatible with GPL (D1-a).
- **Recommendation:** decide after P0-06 data; meanwhile keep `TextHwr`/`MathHwr` traits engine-agnostic.
- **Outcome (2026-11-20):** deferred to P0-06 data as recommended. Under D1 (GPL-3), the remaining options are (b) ML Kit text + in-house maths (needs a linking exception) and (c) fully in-house. P0-06 must include **Malagasy** text (D5).

### D3 — Platforms beyond Android
- iPads are widespread in education; Compose Multiplatform + Rust core allow iOS later. Desktop preview is already in Phase 1.
- **Recommendation:** commit to iOS feasibility review at M2.
- **Outcome (2026-11-20):** **Android only.** The desktop preview (E1.11, P1-42) and iOS (E6.1) are descoped; Kahie Web remains available for other devices. Compose Multiplatform is no longer required (ADR-0002 to revisit).

### D4 — Minimum supported device
- Determines performance budgets, AI tiers and test matrix.
- **Recommendation:** Android 10+ (API 29), ≥ 4 GB RAM for the app, stylus support recommended but not required; AI Standard tier ≥ 8 GB RAM (to be confirmed by P0-08).
- **Outcome (2026-11-20):** **Android 12+ (API 31), ≥ 6 GB RAM.** `minSdk` and the NDK platform level are 31. On-device AI tier still needs ≥ 8 GB (P0-08).

### D5 — Initial languages
- Affects HWR models, AI model choice, UI translation, templates.
- **Recommendation:** start with French and English for UI and handwriting; confirm whether other languages are required for the target market.
- **Outcome (2026-11-20):** **French, English and Malagasy**, for the UI **and** handwriting recognition at launch. Malagasy HWR needs data collection and evaluation in P0-06.

### D6 — CRDT library
- Candidates: Loro (preferred), Automerge, Yrs. Resolved by P0-03 measurements.
- **Outcome (2026-11-20):** **Loro.** P0-03 now validates Loro against the performance budgets instead of comparing candidates.

### D7 — CAS backend
- Giac (GPL, complete) vs SymEngine (MIT, needs supplementing). Resolved by P0-07 and D1.
- **Outcome (2026-11-20):** **Giac** (enabled by D1). P0-07 now covers the Giac Android build, binary size and the curriculum suite.

### D8 — Telemetry and crash reporting
- **Recommendation:** off by default; opt-in crash reports with no user content; local diagnostic export for support; no analytics SDKs.
- **Outcome (2026-11-20):** as recommended — off by default, opt-in crash reports with no user content, local diagnostic export, no analytics SDKs.

### D9 — Collaboration funding model
- Firebase costs scale with active collaboration. Options: free within quotas, paid tier for heavy collaboration, user-provided Firebase project, self-hosted relay. Input: P0-10.
- **Outcome (2026-11-20):** **self-hostable open-source relay** (fits GPL-3). A public instance may be offered; Firebase is not required.

### D10 — Product name & file extension
- Working name "Kahie"; file extension placeholder `.nbk`. Check trademark and extension collisions.
- **Outcome (2026-11-20):** name **Kahie**, file extension **`.kahie`** (replaces the `.nbk` placeholder; the web backup format already uses it). Trademark check still to do.
