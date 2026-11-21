# P0-05 — Rendering pipeline (core side)

- **Status:** Partial — core display lists, LOD and SVG writer done and measured on desktop; Android execution on Skia (fps on a mid-range tablet) pending P0-15
- **Date:** 2026-11-20
- **Code:** `core/crates/nb-render` (4 tests) · benchmark `cargo run --release -p nb-render --example bench_50k`

## Design

- **Display list per tile.** The core returns `DrawOp::Fill` (pressure outline) / `DrawOp::Stroke` (polyline) in page coordinates; the platform only executes them. The same list feeds `to_svg` (export, golden tests); a PDF writer can consume the same ops.
- **Uniform grid index** (256-unit cells): a tile query visits only nearby strokes; strokes spanning several cells are de-duplicated and kept in z-order.
- **Levels of detail:** level *l* simplifies with tolerance 0.25 px × 2^l on screen, so geometry never deviates by more than ¼ px from the full-detail outline. Each level is computed once per stroke and cached.
- **Small strokes:** below 8 px on screen a stroke is drawn as its simplified centre line; below ~1 px as a dot.
- **Tile cache (platform side, next step):** the Android layer rasterises each tile once into a bitmap and redraws bitmaps while panning; only tiles whose content or zoom level changed are rebuilt.

## Measurements (desktop container, Intel Xeon 2.3 GHz, 1 thread)


50000 strokes, 5975401 points, world 20000×20000 page units

Index build: 58 ms

| Zoom (px/unit) | Tile (page units) | Strokes | Vertices | Cold (ms) | Warm (ms) |
|---|---|---|---|---|---|
| 4 | 128 | 4 | 350 | 0.11 | 0.00 |
| 1 | 512 | 48 | 3682 | 0.68 | 0.01 |
| 0.25 | 2048 | 584 | 23284 | 5.49 | 0.22 |
| 0.05 | 10240 | 13532 | 175435 | 5.92 | 2.65 |
| 0.01 | 51200 | 45411 | 141160 | 13.61 | 11.08 |

"Cold" includes computing the LOD geometry for strokes seen for the first time; "warm" is a repeated query (cached LOD). Each row is the median of 20 tiles of 512 × 512 px.

## Assessment

| Criterion | Result |
|---|---|
| 50 000 strokes indexed | 58 ms |
| Tile build at normal zoom (1×) | 0.7 ms cold, 0.01 ms warm → far inside an 18 ms frame |
| Whole 20 000² canvas in one tile (0.01×) | 141 k vertices, 11–14 ms: acceptable only because tiles are cached as bitmaps and rebuilt rarely; a coarser "overview" level (pre-rendered thumbnails) is the fallback if devices are too slow |
| SVG export from the same display list | Done (stable output, golden-tested) |
| ≥ 55 fps pan/zoom on mid-range device | Not measured — needs P0-15 devices and the Android tile cache |

## Recommendation

Keep the core-generated display list with grid index and cached LOD. Next: Android tile cache executing these ops on `Canvas`, then measure fps on the reference tablets. If the zoomed-out case is too slow on device, add a per-notebook overview raster.
