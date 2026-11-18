# WS6 — Offline AI assistant

**Lead role:** ML engineer · **Phases:** 0 (spike), 5 · **Related ADRs:** 0005, 0006 · **Blocked by decisions:** D4, D5

## Mission

An assistant that understands the user's notebook, explains, summarises and generates material — entirely on-device — while every mathematical fact it states is computed by deterministic tools.

## Scope

- Context understanding: current page/selection/notebook (structured, not pixels).
- Explanations of concepts; guided problem solving (hints, step-by-step, never only the final answer in "study" mode).
- Construction feedback: "your perpendicular bisector is not through the midpoint" (via geometry checker).
- Summaries of notes and PDFs.
- Generation: examples, exercises (with solutions verified by CAS), diagrams (emitted as construction-graph commands).
- Organisation help: tagging, linking, suggested sections.

## Technical approach

| Topic | Approach |
|---|---|
| Runtime | `LlmRuntime` trait; implementations: **llama.cpp** (GGUF, CPU/GPU) and **LiteRT-LM / MediaPipe LLM Inference**; optional "companion" runtime on a user-owned PC on the local network |
| Models | Small instruction-tuned models with permissive licences (≈ 1–4B parameters, 4-bit quantised); candidates evaluated in P0-08 by tool-calling accuracy, languages (D5), maths explanation quality, speed |
| Distribution | Optional in-app download (1–3 GB) after capability check; never bundled in the APK |
| Device tiers | **None** (feature hidden) / **Basic** (≈1–1.5B: summaries, rephrasing) / **Standard** (≈3–4B: explanations, tool use, exercises) — thresholds set by D4 + P0-08 |
| Context builder | Serialises selection/page into compact text: recognised text, LaTeX, construction graph DSL, PDF text excerpts; budgeted to model context |
| Retrieval | Local search index + small on-device embedding model for notebook-wide questions |
| Tools | `cas.*`, `plot.add`, `geo.describe`, `geo.check`, `geo.construct`, `notebook.search`, `notebook.insert` — JSON schemas, validated arguments |
| Verification | Tool router; output UI marks *verified* vs *generated* content (ADR-0005) |
| Fine-tuning | Optional LoRA fine-tune for tool use and our graph DSL, if base models under-perform in P0-08 |

## Safety and UX rules

- AI never edits the notebook silently: insertions appear as suggestions to accept.
- Clear "may be inaccurate" visual for unverified prose; no anthropomorphic UI.
- Runs only on user request (no background inference draining battery).

## Validation

- Tool-use benchmark (200 tasks): ≥ 90 % correct with tools; 0 unflagged incorrect numeric results.
- Latency: first token ≤ 2 s, ≥ 8 tokens/s on Standard-tier reference device.
- Pilot usefulness rating ≥ 4/5 from teachers for explanation and exercise generation.

## Deliverables by phase

| Phase | Deliverable |
|---|---|
| 0 | P0-08 runtime & model benchmark, tier proposal |
| 5 | Runtime integration, context builder, tool router, assistant UI, retrieval, exercise generator |
