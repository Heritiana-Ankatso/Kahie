# ADR-0003: Immutable raw ink; recognition as interpretations

- **Status:** Proposed
- **Date:** 2026-11-18
- **Related:** WS3, WS5, [data-model.md §6](../architecture/data-model.md#6-interpretations-adr-0003)

## Context

"The user can draw freely, receive suggestions or activate automatic correction. Original strokes should always remain recoverable." Recognition (text, maths, shapes, constructions, functions, diagrams) will sometimes be wrong. Models improve over time; re-recognition should be possible.

## Options considered

1. **Replace strokes by recognised objects** (most apps). Simple; destroys information; mistakes are costly.
2. **Keep strokes, store interpretations that reference them.** Slightly more complex rendering and data; fully reversible.

## Decision

Option 2. Committed `InkStroke` objects are immutable. Recognition creates `Interpretation` records referencing stroke IDs, with n-best alternatives, status and engine version. An applied interpretation hides (does not delete) its source strokes. Users can show originals, switch alternatives or revert at any time. Erasing ink is an explicit user action (tombstone), never a side effect of recognition.

## Consequences

- Positive: guarantees the requirement by construction; enables re-recognition with newer models; enables the "suggest / auto-correct / free" modes uniformly.
- Negative: storage overhead (strokes kept); rendering must resolve visibility; editing a recognised result (e.g. typed correction to a `MathBlock`) detaches it from the strokes (status `Applied (edited)`).

## Validation

Property test: for any sequence of recognition apply/revert operations, the set of original strokes is unchanged and fully restorable.
