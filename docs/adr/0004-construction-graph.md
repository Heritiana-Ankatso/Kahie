# ADR-0004: Geometry stored as a construction dependency graph

- **Status:** Proposed
- **Date:** 2026-11-18
- **Related:** WS5, [data-model.md §7](../architecture/data-model.md#7-construction-graph-adr-0004)

## Context

Requirements: construct geometric objects and relationships; tangents, normals, intersections; constraints and snapping; step-by-step animation of constructions; AI that suggests corrections to constructions.

## Options considered

1. **Store final shapes only** (vector drawing model). Cannot express dependencies; no dynamic geometry; animation must be authored manually.
2. **Construction graph** (dynamic geometry model, as in established geometry software): free objects + derived objects defined by operations on inputs + explicit constraints for free objects.

## Decision

Option 2. Geometry objects are nodes of a directed acyclic graph with definitions (`Free` or `Derived{op, inputs}`), plus a set of constraints solved by `nb-solve` for free objects. Each node has a construction step index and optional caption.

## Consequences

- Positive: dynamic geometry (drag a point, dependent objects update); step-by-step animation is a replay; the graph is a precise, text-serialisable description usable by the AI assistant and for TikZ export.
- Negative: more complex than a shape list; requires robust handling of degenerate/undefined states; interaction design must hide graph complexity in Simple mode.

## Validation

Curriculum construction suite (≥ 100 standard constructions: perpendicular bisector, incircle, tangent from external point…) reproducible and animatable.
