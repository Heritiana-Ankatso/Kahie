// Document model: notebooks, pages, items, and reversible operations.
// Pure module (no DOM) so it can be unit-tested under Node.

export const SCHEMA_VERSION = 1;

export function uid() {
  if (globalThis.crypto?.randomUUID) return crypto.randomUUID();
  return 'id-' + Math.random().toString(36).slice(2) + Date.now().toString(36);
}

export const PAGE_SIZES = {
  infinite: null,
  A4: { w: 794, h: 1123 },
  A5: { w: 559, h: 794 },
  'A4-landscape': { w: 1123, h: 794 },
  letter: { w: 816, h: 1056 },
};

export const TEMPLATES = ['blank', 'lined', 'grid', 'dots', 'axes'];

export function newNotebook(title, color = '#3b6fd8') {
  const now = Date.now();
  return { id: uid(), title, color, created: now, updated: now, v: SCHEMA_VERSION };
}

export function newPage(notebookId, { title = '', template = 'blank', size = 'A4', order = 0 } = {}) {
  const now = Date.now();
  return {
    id: uid(), notebookId, title, template, size, order,
    items: [], background: null, created: now, updated: now, v: SCHEMA_VERSION,
  };
}

const clone = (x) => (typeof structuredClone === 'function' ? structuredClone(x) : JSON.parse(JSON.stringify(x)));

// Operations. Every op is invertible and applied by applyOp.
//   { t: 'add',    entries: [{ index, item }] }
//   { t: 'remove', entries: [{ index, item }] }
//   { t: 'update', before: [item], after: [item] }
export function opAdd(page, items) {
  let index = page.items.length;
  return { t: 'add', entries: items.map((item) => ({ index: index++, item: clone(item) })) };
}

export function opRemove(page, ids) {
  const set = new Set(ids);
  const entries = [];
  page.items.forEach((item, index) => { if (set.has(item.id)) entries.push({ index, item: clone(item) }); });
  return { t: 'remove', entries };
}

export function opUpdate(page, afterItems) {
  const byId = new Map(page.items.map((it) => [it.id, it]));
  const before = [], after = [];
  for (const a of afterItems) {
    const b = byId.get(a.id);
    if (!b) continue;
    before.push(clone(b));
    after.push(clone(a));
  }
  return { t: 'update', before, after };
}

export function invert(op) {
  switch (op.t) {
    case 'add': return { t: 'remove', entries: op.entries };
    case 'remove': return { t: 'add', entries: op.entries };
    case 'update': return { t: 'update', before: op.after, after: op.before };
    default: throw new Error('unknown op ' + op.t);
  }
}

export function applyOp(page, op) {
  if (op.t === 'add') {
    const sorted = [...op.entries].sort((a, b) => a.index - b.index);
    for (const { index, item } of sorted) page.items.splice(Math.min(index, page.items.length), 0, clone(item));
  } else if (op.t === 'remove') {
    const ids = new Set(op.entries.map((e) => e.item.id));
    page.items = page.items.filter((it) => !ids.has(it.id));
  } else if (op.t === 'update') {
    const after = new Map(op.after.map((it) => [it.id, it]));
    page.items = page.items.map((it) => (after.has(it.id) ? clone(after.get(it.id)) : it));
  } else {
    throw new Error('unknown op ' + op.t);
  }
  page.updated = Date.now();
  return page;
}

export function isEmptyOp(op) {
  return op.t === 'update' ? op.after.length === 0 : op.entries.length === 0;
}

// Linear undo/redo history for one page.
export class History {
  constructor(page, limit = 500) {
    this.page = page;
    this.limit = limit;
    this.undoStack = [];
    this.redoStack = [];
  }
  do(op) {
    if (isEmptyOp(op)) return false;
    applyOp(this.page, op);
    this.undoStack.push(op);
    if (this.undoStack.length > this.limit) this.undoStack.shift();
    this.redoStack = [];
    return true;
  }
  undo() {
    const op = this.undoStack.pop();
    if (!op) return false;
    applyOp(this.page, invert(op));
    this.redoStack.push(op);
    return true;
  }
  redo() {
    const op = this.redoStack.pop();
    if (!op) return false;
    applyOp(this.page, op);
    this.undoStack.push(op);
    return true;
  }
  get canUndo() { return this.undoStack.length > 0; }
  get canRedo() { return this.redoStack.length > 0; }
}

// Bounding box of an item in page coordinates: { x, y, w, h }.
export function itemBounds(item) {
  if (item.type === 'stroke') {
    const pts = item.interp?.show ? interpPoints(item.interp) : item.points;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const p of pts) {
      if (p[0] < x0) x0 = p[0]; if (p[0] > x1) x1 = p[0];
      if (p[1] < y0) y0 = p[1]; if (p[1] > y1) y1 = p[1];
    }
    const r = (item.width || 2) / 2 + 1;
    if (!isFinite(x0)) return { x: 0, y: 0, w: 0, h: 0 };
    return { x: x0 - r, y: y0 - r, w: x1 - x0 + 2 * r, h: y1 - y0 + 2 * r };
  }
  return { x: item.x, y: item.y, w: item.w || 0, h: item.h || 0 };
}

// Points describing a recognised shape (used for bounds and hit tests).
export function interpPoints(interp) {
  if (interp.kind === 'line') return [interp.a, interp.b];
  if (interp.kind === 'polygon') return interp.closed ? [...interp.points, interp.points[0]] : interp.points;
  if (interp.kind === 'ellipse') {
    const out = [];
    for (let i = 0; i <= 64; i++) {
      const t = (i / 64) * Math.PI * 2;
      const c = Math.cos(interp.angle || 0), s = Math.sin(interp.angle || 0);
      const ex = interp.rx * Math.cos(t), ey = interp.ry * Math.sin(t);
      out.push([interp.cx + ex * c - ey * s, interp.cy + ex * s + ey * c]);
    }
    return out;
  }
  return [];
}

export function translateItem(item, dx, dy) {
  const it = clone(item);
  if (it.type === 'stroke') {
    it.points = it.points.map((p) => [p[0] + dx, p[1] + dy, ...p.slice(2)]);
    if (it.interp) {
      const m = (q) => [q[0] + dx, q[1] + dy];
      const ip = it.interp;
      if (ip.a) ip.a = m(ip.a);
      if (ip.b) ip.b = m(ip.b);
      if (ip.points) ip.points = ip.points.map(m);
      if (ip.cx !== undefined) { ip.cx += dx; ip.cy += dy; }
    }
  } else {
    it.x += dx; it.y += dy;
  }
  return it;
}

export function rectsIntersect(a, b) {
  return a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h;
}

export function unionBounds(items) {
  if (!items.length) return null;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const it of items) {
    const b = itemBounds(it);
    x0 = Math.min(x0, b.x); y0 = Math.min(y0, b.y);
    x1 = Math.max(x1, b.x + b.w); y1 = Math.max(y1, b.y + b.h);
  }
  return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
}

// Validate and normalise imported data ({ notebooks, pages }).
export function normaliseImport(data) {
  if (!data || !Array.isArray(data.notebooks) || !Array.isArray(data.pages)) {
    throw new Error('not a Kahie export');
  }
  const idMap = new Map();
  const notebooks = data.notebooks.map((nb) => {
    const id = uid();
    idMap.set(nb.id, id);
    return { ...newNotebook(String(nb.title || 'Notebook'), nb.color || '#3b6fd8'), id, created: nb.created || Date.now() };
  });
  const pages = data.pages
    .filter((p) => idMap.has(p.notebookId))
    .map((p) => ({
      ...newPage(idMap.get(p.notebookId), { title: p.title || '', template: TEMPLATES.includes(p.template) ? p.template : 'blank', size: p.size in PAGE_SIZES ? p.size : 'A4', order: p.order || 0 }),
      items: Array.isArray(p.items) ? p.items : [],
      background: p.background || null,
    }));
  return { notebooks, pages };
}
