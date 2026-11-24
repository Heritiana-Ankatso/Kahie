# Kahie for Android — quick guide

Kahie is a notebook for stylus, finger and keyboard. Everything is saved on the tablet as you write; nothing leaves it unless you turn on sync with your own server.

## First steps

Kahie opens on a blank page with the pen selected: just write. Nothing to sign up for, nothing to allow.

- **The toolbar** (Simple mode, the default) has seven buttons: pen, highlighter, eraser, select, undo, redo and *More* (⋯). Next to them: the page arrows, the page number and **+** (add a page after this one).
- **Colour and thickness**: tap the pen (or highlighter) again when it is already selected.
- **More** holds everything else: Text, Maths, Plot, Image, Selection, Paper, Pages, Notebooks, Fit, Clear page, *PDF text* (on a PDF page), *Tutorials* and *Settings*.
- **Tips** appear once, at the bottom of the page, the first time they are useful; *Got it* closes them (they also go away by themselves). *Settings → Show the tips again* brings them back.
- **Tutorials** (*More → Tutorials*), done on your real page, one step at a time: *Write and erase*, *Organise pages and notebooks*, *Annotate a PDF* (with a sample worksheet if you have no PDF at hand). *Stop* ends a tutorial at any time.

## Settings (*More → Settings*)

| Setting | What it does |
|---|---|
| Mode | *Simple* (essential tools) or *Advanced*: every tool on the bar, colour swatches and widths, and *Style…* (any pen colour as `#RRGGBB`, exact pen and highlighter thickness, eraser size, shape tidying). Switching never changes or hides what is on your pages |
| Toolbar | At the top, or on the side |
| Left-handed | The side toolbar goes to the right, and the top toolbar's order is mirrored, so the tools sit under your free hand |
| Theme | Like the system, light, or dark. In dark mode the toolbars and the desk turn dark; the paper stays white so ink keeps its colours |
| Crash reports | Off unless you turn it on. When on, Kahie keeps a log of crashes **on the tablet** with technical data only (app and Android versions, device model, where in the code it failed: never your notes). *Copy log* / *Share log* to send it to whoever helps you; *Delete* (or turning the option off) removes it |

Kahie follows the system font size and works with TalkBack: every button is announced by name, colours by their name, and the page number when it changes.

## Writing

| Tool | How |
|---|---|
| Pen, highlighter | Choose in the toolbar; pressure changes the width. Tap the selected tool again for its colours and widths (Advanced mode: swatches on the bar and *Style…*); *Tools* (in *More* in Simple mode) offers pencil, brush and all colours |
| Tools | *Tools* chooses pen, pencil (lighter; tilting the stylus gives a broader, shading line), brush (very sensitive to pressure, wider when tilted) or highlighter, one of 12 colours or a custom colour (*Choose a colour… → Custom colour…*, e.g. `#2B59C3`), and one of 5 widths |
| Eraser | Eraser tool, the stylus eraser end, or the stylus side button while drawing. By default it erases whole strokes; in *Tools* turn on *Partial eraser* to erase only the part you touch (the rest of the stroke stays; Undo brings the whole stroke back). *Tools* also sets what the eraser end and the side button do |
| Tidy shapes | Hold the pen still at the end of a stroke: it becomes a clean line, circle, rectangle… Turn on *Shapes* to do it automatically |
| Text | *Text*, then tap the page; tap a text to edit it (empty it to delete it) |
| Compute | In the text dialog, *Compute* evaluates the last line (e.g. `factor(x^2-5*x+6)`, `solve(x^2=2,x)`). The buttons under the text apply an operation to the last line without typing the command: *Solve* (`x^2 = 2`, or an expression = 0), *Factor*, *Expand*, *Simplify*, *d/dx*, *∫ dx*, *≈* (decimal value). Results are exact unless you choose ≈. In builds that include the maths engine |
| Undo / redo | Arrows in the toolbar, or tap with two fingers (undo) / three fingers (redo) |
| Select | *Select* drags a rectangle, *Lasso* draws around what you want (ink, text, images, formulas…). Drag the selection to move it, the square handle at its corner to resize it, the round handle above it to rotate it (it snaps every 15°); each is one Undo step. Text, images and formulas stay upright when rotated |
| Selection menu | *Selection* → copy (also to other apps: text as text, a single image as an image), duplicate, delete, bring to front / send to back, *Group* (grouped items are then always selected together) / *Ungroup*, *Crop / rotate image* for a single image. With nothing selected it offers *Paste*: what you copied last, in Kahie or in another app |
| Rich text | In the text dialog, select words and use **B**, *I*, U, H1–H3 (headings), • and 1. (lists), A (colour) and ⌫ (clear formatting) |
| Insert | *Insert* → a photo from the gallery or the camera, text or an image from the clipboard, or any other file as an attachment. An attachment is a labelled box on the page; tap it with *Select* (or *Selection → Open file*) to open it in another app. Files up to 25 MB; they sync and export with the page |
| Maths | *Maths*, tap the page and type LaTeX (e.g. `\frac{a}{b}`), or use the buttons under the field (fraction, root, power, index, integral, sum, limit, vector, cases, matrix, symbols and Greek letters): the cursor lands in the first empty slot. The formula is previewed as you type. Tap a formula to edit it |
| Navigate | With a stylus, fingers scroll and zoom (two fingers to zoom); *Fit* shows the whole page |
| Infinite canvas | *Pages → More → Paper size → Infinite canvas*: a page without edges; pan anywhere and zoom from 5 % to 3 200 %. *Fit* zooms to show everything on it. PDF, PNG, SVG and LaTeX/document exports of an infinite canvas are cropped to its content plus a small margin. |

## Pages and notebooks

- **Pages**: the open notebook's sections and pages, with a thumbnail of each page. Tap a page to open it; *Add a page* inserts after the current one. **Reorder** by touching and holding a page or a section, then dragging it: drop on the top half of a row to put it before, the bottom half to put it after, the middle of a section to put it inside. *More* has: rename the page, move it up or down, move it into a section, *New section*, *Delete page* (to the trash), *Trash* (restore, or *Empty trash* after a confirmation). Pages stay in the trash for 30 days, then are deleted for good. Tap a section to rename or delete it. *More → Earlier versions of this page* restores the page as it was (Kahie keeps a version each time you open a page and every 10 minutes of work; Undo reverses a restore).
- **Notebooks** (the library): every notebook with its cover (its first page), most recently opened first, as a grid or a list (*Grid* / *List*). Tap a notebook to open it. *New folder* groups notebooks (folders can hold folders); tap a folder to go in, *Up* or Back to come out. Touch and hold a notebook or folder to rename it, *Move to folder*, or *Delete* it. Deleted notebooks and folders go to the library's *Trash*, where you can restore them for 30 days; after that, or with *Empty trash* (asks first), they are deleted for good. *More* has the notebook actions (rename, export, sync, templates…).
- **Search**: *Search* in the toolbar (also in the library and the page list) finds words in notebook, section and page titles, typed text, maths (LaTeX) and plot expressions, in *This notebook* or *All notebooks*. Accents and capitals do not matter (`equation` finds “Équation”). Tap a result to open its page; the block found is centred and highlighted for a few seconds. Formulas can also be found by structure: type `x^2 + ?` (where `?` stands for any term, or anything inside brackets: `\frac{?}{2}`, `sin(?)`) to find maths blocks and plots containing that pattern; `2x` and `2·x` are the same, but terms are not reordered (`x^2 + ?` does not find `3x + x^2`). Handwriting is not searched yet.
- **Paper**: cycles lined, squared, dotted, millimetre graph, Cornell notes, music staves and blank.
- **Templates**: *Pages → More → New page from a template…* adds a page from a built-in model (lesson plan, lecture notes, exercise sheet, research canvas, journal, weekly planner) or one of yours; *Save this page as a template* keeps any page as a model (marked ★ in the list).
- **Notebook templates**: *Notebooks → More → New notebook from a template* creates a whole notebook from a built-in model (course notebook with Lessons / Exercises / Assessments sections, monthly planner, exercise book) or one of yours; *Save this notebook as a template* keeps the current notebook's sections, page titles and pages as a model.
- **Page scrolling**: *Notebooks → More → Page scrolling*. *Vertical* (default): move freely on the page, the arrows turn pages. *Horizontal*: the whole page fits the screen; drag sideways past the edge of the page (two fingers, or one finger when you write with a stylus) to turn to the next or previous page.
- **Paper size**: *Pages → More → Paper size* (A3, A4, A5 or Letter, portrait or landscape, or *Custom size…* in millimetres, 50 to 1000 mm per side) for the open page. Landscape PDF pages are imported as landscape pages; PDF export keeps each page's size.
- **Language**: *Notebooks → More → Language* (English, Français, Malagasy, or the system's language).

## Sharing and backup

- **Notebooks → More → Import a PDF** adds one page per PDF page, each the size of its PDF page, with the PDF page under your writing: annotate worksheets and exam papers, then *Export PDF* to hand them back. The PDF is kept once in the notebook (not copied into every page) and drawn sharp at any zoom. **+** inserts a blank page after the current one, so you can add room between PDF pages; *Pages → More → New page from a template* inserts a lined, squared or other page there. **More → PDF text** shows the text of the PDF page: select any part to copy it, or *Copy all*. With *Select*, drawing a rectangle over the PDF where nothing is written selects the text under it and offers *Copy*. PDF text is also found by *Search*. (Scanned PDFs have no text.) Shared notebooks send the PDF to the other devices once; PDFs larger than about 15 MB cannot go through the relay yet.
- **Image** inserts a photo (resized automatically); images sit under the ink so you can annotate them.
- **Selection** (after *Select*): copy (paste on any page or notebook from *Selection → Paste*), duplicate, delete, bring to front, send to back.
- **Plot** draws, one per line, in a window you choose (tap a plot to edit it):
  - functions of x: `y = x^2 - 2`, `f(x) = 2sin(x)` (functions are referred to by name, `f`, or by order, `1`, `2`…);
  - parametric curves `(cos t, sin t)`, polar curves `r = 1 + cos θ` (optional range: `t=0..4pi`), points `(2, 1)`;
  - analysis (button *Analyse…*): `roots f`, `extrema f`, `inflections f`, `asymptotes f`, `intersections f g`, `tangent f at 2`, `normal f at 2`, `area f from 0 to 2` (signed integral) and `area f g from 0 to 2` (area between two curves). French keywords work too: `racines f`, `extremums f`, `tangente f en 2`, `aire f de 0 à 2`. Values are numerical (3 decimals).
  - geometry constructions (dynamic: with the *Plot* tool, drag a free point and everything built on it follows; points snap to half units): `A = (0, 0)` (free point), `M = midpoint(A, B)`, `d = line(A, B)`, `segment(A, B)`, `ray(A, B)`, `c = circle(A, B)` or `circle(A, 2)`, `perpendicular(d, A)`, `parallel(d, A)`, `bisector(A, B)` (perpendicular bisector), `anglebisector(A, B, C)`, `P = intersect(c, d, 2)`, `on(c, 0.25)`, `reflect(A, d)`, `polygon(A, B, C)`, `distance(A, B)`, `angle(B, A, C)`; `axes off` hides the axes. French names work too (`milieu`, `droite`, `cercle`, `mediatrice`, `bissectrice`, `intersection`, `symetrique`, `polygone`, `sans axes`). An impossible construction (e.g. parallel lines meeting) is simply not drawn until the figure changes.
  - constraints on free points: `constrain distance(A, B) = 5`, `constrain angle(B, A, C) = 60`, `constrain horizontal(A, B)`, `vertical(A, B)`, `parallel(A, B, C, D)`, `perpendicular(A, B, C, D)`, `equal(A, B, C, D)` (same length). The points move as little as possible to satisfy them (French: `contrainte distance(A, B) = 5`); constraints that cannot all hold are marked as errors.
  - transformations of any point, line, circle or polygon: `rotate(T, O, 90)` (degrees, anticlockwise), `dilate(c, O, 2)` (homothety; FR `homothetie`), `translate(T, A, B)` (by vector AB), `reflect(T, d)`;
  - styles at the end of any line: `[red, thick, dashed]` (colours: red, blue, green, black, orange, purple, grey, brown; also `thin`); `[hidden]` keeps a helper object usable but not drawn. French: `[rouge, epais, pointille]`, `[cache]`.
- **Pages → More → Export this page (SVG)** writes the page as a vector SVG at its real paper size (for editing in Inkscape or printing at any scale).
- **Pages → More → Export this page (PNG)** for sharing a single page; *Send this page (PNG)…* opens the share sheet (email, messaging, Drive…).
- **Notebooks → More → Send as PDF…** shares the whole notebook as a PDF through the share sheet.
- **Open with Kahie**: opening or sharing a PDF to Kahie creates a new notebook from it; opening a `.kahie` file imports it.
- **Notebooks → More → Export PDF** writes a vector PDF, each page at its size (ink, text and formulas stay sharp). Choose *All pages*, *This page*, or *Pages…* and type a range such as `1-3, 5` or `4-` (page 4 to the end).
- **Notebooks → More → Export as Markdown / LaTeX (ZIP)**: the notebook as `notebook.md` or `notebook.tex` with an `assets/` folder. Typed text, formulas and headings (sections, page titles) are exported as text; plots become pgfplots figures in LaTeX (they stay exact and vector) and pictures in Markdown; handwriting is exported as one picture per page. The LaTeX file compiles with `pdflatex` (packages: amsmath, graphicx, pgfplots).
- **Notebooks → More → Export (.kahie)** saves the notebook as a `.kahie` file (open format: a ZIP with a readable `document.json`, see `docs/format/kahie-v0.md`). **Import** opens such a file as a new notebook.
- **Sync between your devices** needs a relay server (see `docs/relay.md`):
  1. *Notebooks → More → Sync settings*: server address (`wss://…`) and access token.
  2. *Share with my devices* first asks whether to **encrypt** the notebook (recommended: then only devices with a code can read it, not whoever runs the relay; a lost code cannot be recovered from the relay). It then shows two codes for the current notebook: an **editor code** (your other devices, co-teachers) and a **read-only code** (students: they see the notebook and every update, but cannot change it).
  3. On the other tablet: *Join a shared notebook*, enter a code.
  The open page and the page list sync live; other pages sync in the background every few minutes.
- **Automatic backup**: *Notebooks → More → Automatic backup* asks for a folder (on the tablet, an SD card or a synced cloud folder). Every day Kahie copies each notebook there as a `.kahie` file, checking each copy before it replaces the previous one. To get a notebook back, open *Automatic backup → Restore*, pick a copy (newest first) and confirm: it is added as a new notebook, your current notebooks are not changed.

## Privacy

No account, no analytics. The network is used only for sync, and only with the server you configure.
