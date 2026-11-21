# Kahie for Android — quick guide

Kahie is a notebook for stylus, finger and keyboard. Everything is saved on the tablet as you write; nothing leaves it unless you turn on sync with your own server.

## Writing

| Tool | How |
|---|---|
| Pen, highlighter | Choose in the toolbar; pressure changes the width. Pick a colour and a width next to them |
| Eraser | Eraser tool, the stylus eraser end, or the stylus button while drawing; erases whole strokes |
| Tidy shapes | Hold the pen still at the end of a stroke: it becomes a clean line, circle, rectangle… Turn on *Shapes* to do it automatically |
| Text | *Text*, then tap the page; tap a text to edit it (empty it to delete it) |
| Compute | In the text dialog, *Compute* evaluates the last line (e.g. `factor(x^2-5*x+6)`, `solve(x^2=2,x)`). The buttons under the text apply an operation to the last line without typing the command: *Solve* (`x^2 = 2`, or an expression = 0), *Factor*, *Expand*, *Simplify*, *d/dx*, *∫ dx*, *≈* (decimal value). Results are exact unless you choose ≈. In builds that include the maths engine |
| Undo / redo | Arrows in the toolbar, or tap with two fingers (undo) / three fingers (redo) |
| Select | *Select*, drag a rectangle; drag the selection to move it; *Selection* → duplicate, delete |
| Maths | *Maths*, tap the page and type LaTeX (e.g. `\frac{a}{b}`); tap a formula to edit it |
| Navigate | With a stylus, fingers scroll and zoom (two fingers to zoom); *Fit* shows the whole page |

## Pages and notebooks

- **Pages**: list of pages and sections; tap a page to open it, *Add a page* inserts after the current one. *More* has: rename the page, move it up or down, move it into a section, *New section*, *Delete page* (to the trash), *Trash* (restore). Tap a section to rename or delete it. *More → Earlier versions of this page* restores the page as it was (Kahie keeps a version each time you open a page and every 10 minutes of work; Undo reverses a restore).
- **Notebooks**: switch notebooks, create one, *More → Rename*.
- **Paper**: cycles lined, squared, dotted, millimetre graph, Cornell notes, music staves and blank.
- **Templates**: *Pages → More → New page from a template…* adds a page from a built-in model (lesson plan, lecture notes, exercise sheet, research canvas, journal, weekly planner) or one of yours; *Save this page as a template* keeps any page as a model (marked ★ in the list).
- **Notebook templates**: *Notebooks → More → New notebook from a template* creates a whole notebook from a built-in model (course notebook with Lessons / Exercises / Assessments sections, monthly planner, exercise book) or one of yours; *Save this notebook as a template* keeps the current notebook's sections, page titles and pages as a model.
- **Page scrolling**: *Notebooks → More → Page scrolling*. *Vertical* (default): move freely on the page, the arrows turn pages. *Horizontal*: the whole page fits the screen; drag sideways past the edge of the page (two fingers, or one finger when you write with a stylus) to turn to the next or previous page.
- **Paper size**: *Pages → More → Paper size* (A4, A5 or Letter, portrait or landscape, or *Custom size…* in millimetres, 50 to 1000 mm per side) for the open page. Landscape PDF pages are imported as landscape pages; PDF export keeps each page's size.
- **Language**: *Notebooks → More → Language* (English, Français, Malagasy, or the system's language).

## Sharing and backup

- **Notebooks → More → Import a PDF** adds one page per PDF page, with the PDF as the page background: write on worksheets and exam papers, then *Export PDF* to hand them back.
- **Image** inserts a photo (resized automatically); images sit under the ink so you can annotate them.
- **Selection** (after *Select*): copy (paste on any page or notebook from *Selection → Paste*), duplicate, delete, bring to front, send to back.
- **Plot** draws, one per line, in a window you choose (tap a plot to edit it):
  - functions of x: `y = x^2 - 2`, `f(x) = 2sin(x)` (functions are referred to by name, `f`, or by order, `1`, `2`…);
  - parametric curves `(cos t, sin t)`, polar curves `r = 1 + cos θ` (optional range: `t=0..4pi`), points `(2, 1)`;
  - analysis (button *Analyse…*): `roots f`, `extrema f`, `inflections f`, `asymptotes f`, `intersections f g`, `tangent f at 2`, `normal f at 2`, `area f from 0 to 2` (signed integral) and `area f g from 0 to 2` (area between two curves). French keywords work too: `racines f`, `extremums f`, `tangente f en 2`, `aire f de 0 à 2`. Values are numerical (3 decimals).
  - geometry constructions (dynamic: with the *Plot* tool, drag a free point and everything built on it follows; points snap to half units): `A = (0, 0)` (free point), `M = midpoint(A, B)`, `d = line(A, B)`, `segment(A, B)`, `ray(A, B)`, `c = circle(A, B)` or `circle(A, 2)`, `perpendicular(d, A)`, `parallel(d, A)`, `bisector(A, B)` (perpendicular bisector), `anglebisector(A, B, C)`, `P = intersect(c, d, 2)`, `on(c, 0.25)`, `reflect(A, d)`, `polygon(A, B, C)`, `distance(A, B)`, `angle(B, A, C)`; `axes off` hides the axes. French names work too (`milieu`, `droite`, `cercle`, `mediatrice`, `bissectrice`, `intersection`, `symetrique`, `polygone`, `sans axes`). An impossible construction (e.g. parallel lines meeting) is simply not drawn until the figure changes.
- **Pages → More → Export this page (SVG)** writes the page as a vector SVG at its real paper size (for editing in Inkscape or printing at any scale).
- **Pages → More → Export this page (PNG)** for sharing a single page; *Send this page (PNG)…* opens the share sheet (email, messaging, Drive…).
- **Notebooks → More → Send as PDF…** shares the whole notebook as a PDF through the share sheet.
- **Open with Kahie**: opening or sharing a PDF to Kahie creates a new notebook from it; opening a `.kahie` file imports it.
- **Notebooks → More → Export PDF** writes every page as a vector A4 PDF (ink, text and formulas stay sharp).
- **Notebooks → More → Export as Markdown / LaTeX (ZIP)**: the notebook as `notebook.md` or `notebook.tex` with an `assets/` folder. Typed text, formulas and headings (sections, page titles) are exported as text; plots become pgfplots figures in LaTeX (they stay exact and vector) and pictures in Markdown; handwriting is exported as one picture per page. The LaTeX file compiles with `pdflatex` (packages: amsmath, graphicx, pgfplots).
- **Notebooks → More → Export (.kahie)** saves the notebook as a `.kahie` file (open format: a ZIP with a readable `document.json`, see `docs/format/kahie-v0.md`). **Import** opens such a file as a new notebook.
- **Sync between your devices** needs a relay server (see `docs/relay.md`):
  1. *Notebooks → More → Sync settings*: server address (`wss://…`) and access token.
  2. *Share with my devices* first asks whether to **encrypt** the notebook (recommended: then only devices with a code can read it, not whoever runs the relay; a lost code cannot be recovered from the relay). It then shows two codes for the current notebook: an **editor code** (your other devices, co-teachers) and a **read-only code** (students: they see the notebook and every update, but cannot change it).
  3. On the other tablet: *Join a shared notebook*, enter a code.
  The open page and the page list sync live; other pages sync in the background every few minutes.
- **Automatic backup**: *Notebooks → More → Automatic backup* asks for a folder (on the tablet, an SD card or a synced cloud folder). Every day Kahie copies each notebook there as a `.kahie` file, checking each copy before it replaces the previous one.

## Privacy

No account, no analytics. The network is used only for sync, and only with the server you configure.
