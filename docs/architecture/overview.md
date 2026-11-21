# Architecture overview

**Status:** Proposed (to be confirmed by Phase 0 spikes). Decisions referenced here are recorded in [`../adr/`](../adr/README.md).

---

## 1. Layered architecture

```
┌──────────────────────────────────────────────────────────────────────┐
│ UI SHELL  — Kotlin, Compose Multiplatform                            │
│  Library · Page view · Toolbars (Simple / Advanced) · Instruments    │
│  Wet-ink layer (Android native, front-buffered, low latency)         │
├──────────────────────────────────────────────────────────────────────┤
│ INTELLIGENCE  — pluggable engines (Rust traits, native/ML backends)  │
│  TextHwr · MathHwr · ShapeRecogniser · RelationSuggester             │
│  CurveFitter · DiagramRecogniser · LlmRuntime + ToolRouter           │
├──────────────────────────────────────────────────────────────────────┤
│ CORE  — Rust workspace (deterministic, platform-independent)         │
│  Document model (CRDT) · Scene graph · Undo/history                  │
│  Geometry (construction graph) · Constraint solver                   │
│  Maths (expression tree, CAS bridge, robust plotter) · Typesetting   │
│  Display-list renderer · Import/export · Search index                │
├──────────────────────────────────────────────────────────────────────┤
│ PERSISTENCE  — SQLite per notebook + content-addressed asset store   │
├──────────────────────────────────────────────────────────────────────┤
│ ADAPTERS (optional)  — SyncTransport · AuthProvider · BlobStore      │
│  Google Drive · Firebase relay · Local folder / WebDAV · (future) own│
└──────────────────────────────────────────────────────────────────────┘
```

**Rules**

- The UI shell holds **no business logic**: it renders display lists, forwards input and invokes core commands.
- The core never performs network I/O. Only adapters do, and only when the user enabled them.
- Everything in the Intelligence layer is optional at runtime; the app must degrade gracefully when an engine is absent.

---

## 2. Repository layout (target)

```
/
├── core/                      Rust workspace
│   ├── crates/
│   │   ├── nb-model/          Document model, CRDT wrapper, commands, undo
│   │   ├── nb-store/          SQLite persistence, journaling, .kahie import/export
│   │   ├── nb-scene/          Scene graph, spatial index (R-tree), hit testing
│   │   ├── nb-ink/            Stroke processing: smoothing, simplification, outlines
│   │   ├── nb-render/         Display-list generation, tiling, LOD
│   │   ├── nb-geo/            Geometric primitives, construction graph, evaluation
│   │   ├── nb-solve/          Geometric constraint solver
│   │   ├── nb-math/           Expression tree, parser, CAS bridge, plotter
│   │   ├── nb-typeset/        Maths/text typesetting bridge
│   │   ├── nb-recog/          Shape fitting, relation hypotheses, curve fitting, ML runtime glue
│   │   ├── nb-io/             PDF/SVG/PNG/Markdown/LaTeX/TikZ export, PDF import
│   │   ├── nb-search/         Full-text and maths indexing, retrieval
│   │   ├── nb-sync/           Sync protocol, adapter traits, presence
│   │   ├── nb-ai/             Tool registry, context builder, LLM runtime trait
│   │   └── nb-ffi/            UniFFI bindings (Kotlin; later Swift)
│   └── benches/
├── app/
│   ├── shared-ui/             Compose Multiplatform UI
│   ├── android/               Android app module, wet-ink layer, platform adapters
│   └── desktop/               Desktop (JVM) app module
├── adapters/
│   ├── google-drive/
│   └── firebase/
├── ml/                        Training & evaluation pipelines (Python), model cards
├── tools/                     Latency harness, corpus tools, benchmarks
├── testdata/                  Golden images, handwriting corpus (with consent records)
└── docs/
```

---

## 3. Core interfaces (swappable components)

Every component that is (a) external, (b) a buy-or-build candidate, or (c) platform-specific is behind a trait. Sketch:

```rust
/// Text handwriting recognition (ML Kit on Android, MyScript, or in-house).
pub trait TextHwr {
    fn languages(&self) -> Vec<LanguageTag>;
    fn recognise(&self, strokes: &[StrokeRef], hint: TextHint) -> Result<Vec<TextCandidate>>;
}

/// Handwritten maths → expression tree + LaTeX.
pub trait MathHwr {
    fn recognise(&self, strokes: &[StrokeRef]) -> Result<Vec<MathCandidate>>; // n-best
}

/// Computer algebra backend (SymEngine-based or Giac-based).
pub trait CasBackend {
    fn simplify(&self, e: &Expr) -> Result<Expr>;
    fn solve(&self, eqs: &[Expr], vars: &[Symbol]) -> Result<Solutions>;
    fn diff(&self, e: &Expr, x: &Symbol) -> Result<Expr>;
    fn limit(&self, e: &Expr, x: &Symbol, at: Point) -> Result<Expr>;
    fn integrate(&self, e: &Expr, x: &Symbol) -> Result<Option<Expr>>;
}

/// On-device language model runtime (llama.cpp, LiteRT-LM, remote-on-LAN).
pub trait LlmRuntime {
    fn capabilities(&self) -> LlmCaps;          // context size, tool-calling support
    fn generate(&self, req: LlmRequest, sink: &dyn TokenSink) -> Result<LlmResponse>;
}

/// Sync transport (Drive app folder, Firebase relay, local folder, WebDAV…).
pub trait SyncTransport {
    fn push(&self, doc: DocId, updates: &[UpdateBlob]) -> Result<()>;
    fn pull(&self, doc: DocId, since: VersionVector) -> Result<Vec<UpdateBlob>>;
    fn subscribe(&self, doc: DocId, cb: Box<dyn Fn(UpdateBlob) + Send>) -> Result<Subscription>;
}

pub trait AuthProvider { /* sign-in, identity, token refresh */ }
pub trait BlobStore    { /* content-addressed asset upload/download */ }
```

Interfaces are finalised in Phase 0 (P0-11) and versioned; breaking changes need an ADR.

---

## 4. Rendering pipeline

1. **Wet ink** (stroke in progress) is drawn by the platform layer using front-buffered rendering and motion prediction, independent of the core. Target: minimal latency.
2. On pen-up, the stroke is committed to the core (`nb-model` command).
3. The core produces a **display list** (paths, fills, text runs, images, clip/transform stack) per visible tile, with level-of-detail simplification.
4. Each platform executes the display list on Skia (Android `Canvas`, Skiko on desktop).
5. **Export uses the same display list** (PDF/SVG writers in `nb-io`) → what you see is what you export.

Infinite canvas: quadtree tiles cached as bitmaps at discrete zoom levels; invalidated per object bounding box.

---

## 5. Threading model

| Thread | Responsibility |
|---|---|
| UI thread | Input dispatch, Compose UI |
| Ink thread | Wet-ink rendering (front buffer) |
| Core worker | Commands, CRDT, scene updates (single writer per document) |
| Render workers | Tile rasterisation |
| Background pool | Recognition, indexing, export, sync, LLM inference |

The core is **single-writer per document**; readers get immutable snapshots. Long-running engines (recognition, LLM) must be cancellable.

---

## 6. Performance budgets

Measured on the **mid-range reference device** unless noted (see [P0-15](../backlog/phase-0.md#p0-15--reference-device-lab)).

| Metric | Budget |
|---|---|
| Stylus-to-pixel latency (wet ink) | ≤ 25 ms (high-end), ≤ 35 ms (mid-range) — high-speed camera measurement |
| Cold start → writable blank page | ≤ 2.0 s |
| Open page with 2 000 strokes | ≤ 400 ms to first full render |
| Pan/zoom, infinite canvas, 50 000 strokes | ≥ 55 fps sustained (p95 frame ≤ 18 ms) |
| Commit stroke to durable storage | ≤ 1 s after pen-up (autosave) |
| Text HWR on a 10-word line | ≤ 300 ms |
| Maths HWR on a 15-symbol expression | ≤ 500 ms |
| Shape recognition on pen-up | ≤ 50 ms |
| Constraint solve, 100 constraints | ≤ 16 ms (interactive drag) |
| PDF page render (A4, typical) | ≤ 250 ms |
| Search query across 100 notebooks | ≤ 300 ms |
| APK base size (no ML models) | ≤ 60 MB; ML models as on-demand downloads |

Budgets are enforced in CI benchmarks (see [`../engineering/conventions.md`](../engineering/conventions.md#5-performance-regression)).

---

## 7. Security and privacy

- No network permission is used unless an adapter is enabled by the user.
- Local data at rest: relies on Android file-based encryption; optional notebook-level encryption is a Phase 6 item.
- Telemetry/crash reporting: **off by default**, opt-in only, no content ever leaves the device ([D8](../decisions-open.md#d8--telemetry-and-crash-reporting)).
- Collaboration access control is enforced server-side (Firebase rules) *and* client-side (UI), never client-side only.
