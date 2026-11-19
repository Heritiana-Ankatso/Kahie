# Kahie Web

An offline, installable notebook and maths workspace for stylus, touch and keyboard. This is the first Kahie release; see [ADR-0007](../../docs/adr/0007-web-pwa-first-release.md) for why it ships as a web app first.

## Features (v0.1)

| Area | What works |
|---|---|
| Library | Notebooks with coloured covers, pages with thumbnails, rename, reorder, duplicate, delete |
| Pages | A4, A5, A4 landscape, Letter and infinite canvas; blank, lined, squared, dotted and axes paper |
| Ink | Pressure-sensitive pen, highlighter, stroke eraser, undo/redo, smoothing |
| Palm rejection | After the first stylus touch, fingers only pan and zoom (can be turned back on with *Draw with finger*) |
| Shapes | Hold the pen still at the end of a stroke to turn it into a clean line, circle, ellipse, rectangle, triangle or polygon. The raw ink is kept, and *Show my ink* switches back. In Advanced mode, *Tidy shapes* does this automatically |
| Text | Typed text boxes, wrapped and editable |
| Maths | LaTeX blocks rendered by MathJax, with a live preview and a snippet bar |
| Plots | Functions of `x` (`y = x^2 - 2`, `2sin(x)`, `1/x`, …) over a chosen window. The expression parser is safe (no `eval`) |
| Images and PDF | Insert photos (downscaled); import a PDF as pages to annotate |
| Selection | Tap or drag a box to select; move, duplicate, recolour, delete; resize images, plots, maths and text |
| Search | Across notebook titles, page titles, text and LaTeX; accent-insensitive |
| Export | PNG, SVG, print/PDF for a page or a whole notebook; `.kahie` backup and import |
| Modes | Simple (the essential tools) and Advanced (maths, plots, images, shapes, pan tool, custom colours) |
| Languages | French and English (follows the device, switchable in the library menu) |
| Offline | A service worker caches the whole app; data stays in IndexedDB on the device |

Shortcuts: `P` pen, `H` highlighter, `E` eraser, `V` select, `T` text, `M` maths, `G` plot (the last two in Advanced mode), `Ctrl+Z` / `Ctrl+Shift+Z` undo/redo, `Delete` delete the selection, `Ctrl+D` duplicate, `+`/`-`/`0` zoom, hold `Space` to pan with the mouse.

## Run locally

```bash
python3 app/web/tools/serve.py 5178
```

Then open <http://127.0.0.1:5178/>. Add `?nosw` while developing so the service worker doesn't serve cached files, and `?debug` to expose the editor as `window.kahieEditor`.

## Tests

```bash
cd app/web && node --test tests/*.test.mjs
```

## Install on an Android tablet

The app must be served over HTTPS (or `localhost`) to be installable. Open it in Chrome, then use the menu: *Install app* / *Add to Home screen*. After the first visit, it works with no network.

## Layout

```
app/web/
├── index.html, styles.css, sw.js, manifest.webmanifest, icons/
├── src/
│   ├── app.js       library, notebook view, routing, PDF import
│   ├── editor.js    page editor: input, tools, selection, inline editors
│   ├── model.js     notebooks, pages, items, reversible ops, undo history
│   ├── ink.js       smoothing, simplification, outlines, hit tests
│   ├── shapes.js    shape recognition (interpretations, ADR-0003)
│   ├── expr.js      safe expression parser and sampler for plots
│   ├── render.js    canvas drawing of pages and items
│   ├── export.js    PNG / SVG / print / .kahie
│   ├── mathtex.js   LaTeX → SVG via MathJax
│   ├── search.js, store.js (IndexedDB), i18n.js, ui.js, icons.js
├── vendor/          MathJax 3.2.2 and pdf.js 4.10.38 (Apache-2.0)
├── tests/           node:test unit tests
└── tools/serve.py   no-cache dev server
```
