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
| Undo / redo | Arrows in the toolbar |
| Navigate | With a stylus, fingers scroll and zoom (two fingers to zoom); *Fit* shows the whole page |

## Pages and notebooks

- **Pages**: list of pages; tap to open, *Add a page* inserts after the current one, *More → Delete page* moves it to the trash, *More → Trash* restores it.
- **Notebooks**: switch notebooks, create one, *More → Rename*.
- **Paper**: cycles lined, squared, dotted, millimetre graph, Cornell notes, music staves and blank.

## Sharing and backup

- **Notebooks → More → Export (.kahie)** saves the notebook as a `.kahie` file (open format: a ZIP with a readable `document.json`, see `docs/format/kahie-v0.md`). **Import** opens such a file as a new notebook.
- **Sync between your devices** needs a relay server (see `docs/relay.md`):
  1. *Notebooks → More → Sync settings*: server address (`wss://…`) and access token.
  2. *Share with my devices* shows a code for the current notebook.
  3. On the other tablet: *Join a shared notebook*, enter the code.
  The open page and the page list sync live; other pages sync when opened.

## Privacy

No account, no analytics. The network is used only for sync, and only with the server you configure.
