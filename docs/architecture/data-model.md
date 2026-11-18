# Data model and file format

**Status:** Proposed — v0 implemented in [P0-11](../backlog/phase-0.md#p0-11--document-model-v0); CRDT library chosen in [P0-03](../backlog/phase-0.md#p0-03--crdt-library-evaluation).

---

## 1. Hierarchy

```
Library (per device / per account)
└── Folder (nestable)
    └── Notebook                 ← unit of storage, sync and sharing
        └── Section (nestable, optional)
            └── Page
                ├── Layer[]      (ordered; visibility, lock, opacity)
                │   └── Object[] (z-ordered by fractional index)
                └── Interpretation[]
```

- **Notebook** is the unit of persistence, sync and sharing. One CRDT document per notebook.
- The library/folder tree is a separate small CRDT document (per user), so moving notebooks does not touch notebook data.
- Large binary assets (images, PDFs, fonts) are **content-addressed** (SHA-256) and stored outside the CRDT; the CRDT holds only the hash.

## 2. Page

| Field | Type | Notes |
|---|---|---|
| `id` | ObjectId | Stable unique ID |
| `kind` | `Fixed { size, orientation }` \| `Infinite` | Fixed sizes: A3/A4/A5/Letter/custom |
| `background` | TemplateRef | Plain, ruled, squared, dotted, graph, music, custom |
| `coordinate_systems` | CoordSystem[] | Optional Cartesian/polar frames placed on the page |
| `layers` | Layer[] | Default single layer "Main" (Simple mode hides layers) |
| `pdf_source` | `Option<{asset, page_index}>` | For annotated PDF pages |

## 3. Objects

Common fields for every object:

| Field | Type | Notes |
|---|---|---|
| `id` | ObjectId | CRDT-unique |
| `type` | enum | See table below |
| `layer` | LayerId | |
| `z` | FractionalIndex | Concurrent reordering without conflicts |
| `transform` | Affine2D | Translation/rotation/scale/skew |
| `style` | Style | See §5 |
| `created_by`, `created_at` | ActorId, timestamp | Also derivable from CRDT history |
| `deleted` | bool (tombstone) | Recoverable deletion (trash) |

| Type | Payload |
|---|---|
| `InkStroke` | Tool (pen/pencil/highlighter/brush), packed points (see §4), base width, pressure curve. **Immutable after commit.** |
| `TextBlock` | Rich text (CRDT text with marks), box geometry, auto/fixed width |
| `MathBlock` | Expression tree (canonical), LaTeX source, display mode, optional link to source strokes |
| `Image` | Asset hash, crop rect, natural size |
| `Shape` | Primitive geometry (line, polyline, rectangle, ellipse, arc, Bézier path) — "dumb" shapes |
| `GeoObject` | Node in the construction graph (§6) |
| `Plot` | Function definition(s), domain, coordinate system ref, shading regions |
| `Diagram` | Graph structure: nodes (object refs), edges (connector objects), layout algorithm + params |
| `Group` | Child object IDs |
| `Comment` | Anchor (object or region), thread of messages, resolved flag |
| `Instrument` | Persisted instrument placement (optional; usually transient) |

## 4. Ink encoding

Each point: `x, y` (float32, page units), `pressure` (u16), `tilt_x, tilt_y` (i8), `t` (ms delta, u16). Stored as **delta-encoded, varint-packed** arrays per channel (columnar), then compressed. Target ≤ 6 bytes/point average.

Strokes are committed as **one CRDT operation per stroke** (never per point) to keep history and sync volume manageable.

## 5. Style

```
Style {
  stroke: { colour: RGBA, width, dash: Solid | Dashed(pattern) | Dotted, cap, join } ?
  fill:   { colour: RGBA | Pattern(Hatch{angle, spacing} | Dots | Grid) } ?
  opacity: f32
  arrow_start / arrow_end: ArrowStyle ?
  font (for text/labels)
}
```

Named styles (theme presets) are stored per notebook; objects reference a preset and may override fields.

## 6. Interpretations (ADR-0003)

Recognition never mutates strokes. It creates an `Interpretation`:

| Field | Notes |
|---|---|
| `id` | |
| `sources` | Stroke IDs interpreted |
| `kind` | `Text`, `Math`, `Shape`, `GeoConstruction`, `Function`, `Diagram` |
| `result` | Object(s) to display when applied (e.g. a `MathBlock`, a `GeoObject` subgraph) |
| `alternatives` | n-best list with confidence |
| `status` | `Suggested` → `Accepted`/`Rejected` → `Applied`/`Reverted` |
| `engine` | Engine name + model version (for reproducibility and re-recognition) |

**Display rule:** when `Applied`, the result is rendered and source strokes are hidden (not deleted). The user can toggle "show original ink" at page or object level, or revert.

## 7. Construction graph (ADR-0004)

```
GeoObject {
  id
  kind: Point | Line | Segment | Ray | Circle | Arc | Conic | Polygon | Vector | Angle | Locus | FunctionGraph | Label ...
  definition:
      Free { params }                               // e.g. point at (x, y), draggable
    | Derived { op: GeoOp, inputs: [GeoObjectId] }  // e.g. Midpoint(A,B), Intersect(c1,l2,#1)
  constraints: [ConstraintId]                       // for free objects driven by the solver
  step: u32                                         // construction order (animation)
  caption: Option<RichText>                         // step description for animation/teaching
  visibility: Shown | Hidden | Auxiliary
}

Constraint { id, kind: Parallel|Perpendicular|EqualLength|PointOn|Tangent|Coincident|Angle(v)|Distance(v)|Fixed..., refs }
```

- Evaluation: topological order; derived objects recomputed on any input change.
- Cycles are rejected at insertion time.
- Undefined results (e.g. intersection of parallel lines) are represented explicitly (`Undefined`), not as errors.
- Animation = replay by `step`.

## 8. CRDT mapping

| Model element | CRDT structure (indicative; finalised after P0-03) |
|---|---|
| Notebook tree (sections/pages) | Movable tree |
| Objects of a page | Map `ObjectId → Map(fields)` |
| Z-order | Fractional index field |
| Text content | CRDT rich text |
| Ink points | Opaque immutable binary value (set once) |
| Construction graph | Map of `GeoObject` maps; constraints map |
| Comments | List per anchor |

History: every CRDT change carries `actor` (device/user ID mapped to identity) and timestamp → revision history and attribution come for free.

## 9. Storage on device

One SQLite database per notebook (`<notebook-id>.db`), WAL mode:

| Table | Content |
|---|---|
| `updates` | Append-only CRDT update log (journal; the source of truth) |
| `snapshots` | Periodic compacted snapshots (fast load) |
| `assets` | Hash → file path, size, MIME, refcount |
| `index_*` | Search index (derived; can be rebuilt) |
| `thumbnails` | Derived; can be rebuilt |
| `meta` | Format version, notebook metadata |

Crash safety: an edit is durable once its update row is committed. Snapshots are an optimisation only.

## 10. Portable file format — `.nbk`

A documented, versioned **ZIP container**:

```
notebook.nbk
├── manifest.json        format version, notebook metadata, spec URL
├── document.crdt        compacted CRDT snapshot (full history optional)
├── document.json        human-readable JSON export of current state (for independence)
├── assets/<sha256>      images, PDFs, fonts
├── preview.pdf          optional rendered PDF of all pages
└── thumbnails/
```

- `document.json` ensures data remains readable **without our app** (schema published in `docs/format/`, written in P1).
- The format spec is versioned with semantic versioning; readers must accept older versions (migration code in `nb-store`).
