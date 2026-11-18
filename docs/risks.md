# Risk register

Likelihood (L) and Impact (I): 1 (low) – 5 (high). Score = L × I. Reviewed at every milestone; owners update mitigation status.

| ID | Risk | L | I | Score | Mitigation | Owner | Early warning |
|---|---|---|---|---|---|---|---|
| R1 | Stylus latency not "paper-like" on mid/low-end devices | 3 | 5 | 15 | P0-02 spike first; native front-buffered layer; device minimum (D4) | WS1 lead | P0-02 report misses budget |
| R2 | Scope too large for team/timeline | 4 | 5 | 20 | Strict phasing; milestone go/no-go; non-goals list; buy where sensible (D2) | Tech lead | Velocity < 70 % of plan two iterations in a row |
| R3 | Maths HWR accuracy insufficient | 3 | 4 | 12 | Benchmark early (P0-06); MyScript fallback; n-best UI; typed maths always available | WS3 lead | Exact match < 50 % at mid-Phase 3 |
| R4 | Beginners overwhelmed by features | 3 | 5 | 15 | Simple mode default; cold-start test at every milestone; progressive disclosure | WS8 lead | Cold-start test failures |
| R5 | On-device LLM too slow/weak on target devices | 4 | 3 | 12 | ADR-0005 tools; device tiers; LAN companion; AI optional | WS6 lead | P0-08 results below thresholds |
| R6 | CRDT document growth / performance with heavy ink | 3 | 4 | 12 | Per-stroke ops; packed encoding; compaction; P0-03 benchmark | WS2 lead | Load time > budget on 100 k strokes |
| R7 | Relationship inference produces annoying false suggestions | 3 | 3 | 9 | High threshold; Suggest mode default only in Advanced; on-device learning; easy disable | WS5 lead | Acceptance < 40 % in pilot |
| R8 | Licence incompatibilities (GPL, commercial SDKs, model licences, datasets) | 3 | 4 | 12 | Resolve D1 early; `cargo-deny` licence checks; model/dataset licence review in spikes | Tech lead | Unknown licence in dependency report |
| R9 | Firebase cost or quota limits for collaboration | 2 | 3 | 6 | P0-10 simulation; batching; relay abstraction; D9 | WS7 lead | Cost/user above model |
| R10 | Data loss (corruption, sync bug) destroys trust | 2 | 5 | 10 | Append-only log; kill tests; convergence property tests; backups; soak tests | WS2/WS7 leads | Any corruption report |
| R11 | Fragmented stylus support across Android vendors | 3 | 3 | 9 | Device lab; vendor quirks registry; fallbacks | WS1 lead | Device-specific bug cluster |
| R12 | Key-person dependency (geometry/ML specialists) | 3 | 4 | 12 | Documentation, pairing, ADRs, code review across workstreams | Tech lead | Single reviewer for a crate |
| R13 | CAS gaps (SymEngine path) delay Phase 2 | 3 | 3 | 9 | P0-07 gap analysis; prioritise curriculum suite; Giac if D1 allows | WS4 lead | Suite pass rate < 80 % at mid-Phase 2 |
| R14 | Privacy expectations violated (accidental network use, telemetry) | 2 | 5 | 10 | Offline CI job; no network permission until adapter enabled; privacy review per release | Tech lead | Network call detected in offline CI |
