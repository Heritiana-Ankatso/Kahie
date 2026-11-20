# Kahie for Android — quick guide

Kahie is a notebook for stylus, finger and keyboard. Everything is saved on the tablet as you write; nothing leaves it unless you turn on sync with your own server.

## Writing

| Tool | How |
|---|---|
| Pen, highlighter | Choose in the toolbar; pressure changes the width. Pick a colour and a width next to them |
| Eraser | Eraser tool, the stylus eraser end, or the stylus button while drawing; erases whole strokes |
| Tidy shapes | Hold the pen still at the end of a stroke: it becomes a clean line, circle, rectangle… Turn on *Shapes* to do it automatically |
| Text | *Text*, then tap the page; tap a text to edit it (empty it to delete it) |
| Compute | In the text dialog, *Compute* evaluates the last line (e.g. `factor(x^2-5*x+6)`, `solve(x^2=2,x)`) — in builds that include the maths engine |
| Undo / redo | Arrows in the toolbar, or tap with two fingers (undo) / three fingers (redo) |
| Select | *Select*, drag a rectangle; drag the selection to move it; *Selection* → duplicate, delete |
| Maths | *Maths*, tap the page and type LaTeX (e.g. `\frac{a}{b}`); tap a formula to edit it |
| Navigate | With a stylus, fingers scroll and zoom (two fingers to zoom); *Fit* shows the whole page |

## Pages and notebooks

- **Pages**: list of pages and sections; tap a page to open it, *Add a page* inserts after the current one. *More* has: rename the page, move it up or down, move it into a section, *New section*, *Delete page* (to the trash), *Trash* (restore). Tap a section to rename or delete it.
- **Notebooks**: switch notebooks, create one, *More → Rename*.
- **Paper**: cycles lined, squared, dotted, millimetre graph, Cornell notes, music staves and blank.
- **Language**: *Notebooks → More → Language* (English, Français, Malagasy, or the system's language).

## Sharing and backup

- **Notebooks → More → Import a PDF** adds one page per PDF page, with the PDF as the page background: write on worksheets and exam papers, then *Export PDF* to hand them back.
- **Image** inserts a photo (resized automatically); images sit under the ink so you can annotate them.
- **Plot** draws functions of x (one per line, e.g. `y = x^2 - 2`, `2sin(x)`) in a window you choose; tap a plot to edit it.
- **Pages → More → Export this page (PNG)** for sharing a single page; *Send this page (PNG)…* opens the share sheet (email, messaging, Drive…).
- **Notebooks → More → Send as PDF…** shares the whole notebook as a PDF through the share sheet.
- **Open with Kahie**: opening or sharing a PDF to Kahie creates a new notebook from it; opening a `.kahie` file imports it.
- **Notebooks → More → Export PDF** writes every page as a vector A4 PDF (ink, text and formulas stay sharp).
- **Notebooks → More → Export (.kahie)** saves the notebook as a `.kahie` file (open format: a ZIP with a readable `document.json`, see `docs/format/kahie-v0.md`). **Import** opens such a file as a new notebook.
- **Sync between your devices** needs a relay server (see `docs/relay.md`):
  1. *Notebooks → More → Sync settings*: server address (`wss://…`) and access token.
  2. *Share with my devices* shows two codes for the current notebook: an **editor code** (your other devices, co-teachers) and a **read-only code** (students: they see the notebook and every update, but cannot change it).
  3. On the other tablet: *Join a shared notebook*, enter a code.
  The open page and the page list sync live; other pages sync in the background every few minutes.
- **Automatic backup**: *Notebooks → More → Automatic backup* asks for a folder (on the tablet, an SD card or a synced cloud folder). Every day Kahie copies each notebook there as a `.kahie` file, checking each copy before it replaces the previous one.

## Privacy

No account, no analytics. The network is used only for sync, and only with the server you configure.
