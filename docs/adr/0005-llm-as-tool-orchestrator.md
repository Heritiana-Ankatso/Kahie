# ADR-0005: Language model as tool orchestrator, never as calculator

- **Status:** Proposed
- **Date:** 2026-11-18
- **Related:** WS6, P0-08

## Context

"AI should complement reliable mathematical algorithms, not replace them." The assistant must work fully offline, which limits us to small on-device models (≈ 1–4B parameters) whose multi-step mathematical reasoning is unreliable.

## Options considered

1. **LLM answers directly** (chat-style). Simple; frequent mathematical errors presented confidently.
2. **LLM + tools.** The model plans, calls deterministic tools (CAS, geometry checker, plotter, search) and phrases explanations around verified results.

## Decision

Option 2. The assistant receives a **structured context** (recognised text, LaTeX, construction graph in text form, selection) rather than raw pixels. Mathematical results shown to the user must come from a tool call; the UI visually distinguishes *verified* (tool-produced) content from *generated* prose. If the model attempts a computation without a tool, the tool router re-runs it through the CAS where possible, or flags it as unverified.

## Consequences

- Positive: reliable mathematics; smaller models suffice; explanations grounded in the user's notebook.
- Negative: tool schema design and model fine-tuning/prompting for tool use required; latency of multi-step tool loops.

## Validation

P0-08 and Phase 5 benchmark: ≥ 90 % correct final answers on a 200-task school/university maths set *with* tools, and 0 unflagged incorrect numeric results.
