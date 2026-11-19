// Canvas rendering of pages: paper, templates, ink, shapes, text, maths, plots, images.

import { outline, radiusAt } from './ink.js';
import { PAGE_SIZES, interpPoints } from './model.js';
import { compile, sample, tickStep } from './expr.js';
import { mathImage } from './mathtex.js';

export const TEXT_FONT = '"Atkinson Hyperlegible", system-ui, sans-serif';

export function pageSize(page) {
  if (page.size === 'custom') return { w: page.w, h: page.h };
  return PAGE_SIZES[page.size] || null;
}

const pathCache = new WeakMap();
const imgCache = new Map();
let onImageLoad = () => {};
export function setImageLoadHandler(fn) { onImageLoad = fn; }

export function getImage(src) {
  let img = imgCache.get(src);
  if (!img) {
    img = new Image();
    img.onload = () => onImageLoad();
    img.src = src;
    imgCache.set(src, img);
  }
  return img.complete && img.naturalWidth ? img : null;
}

function strokePath(item) {
  let p = pathCache.get(item);
  if (p) return p;
  p = new Path2D();
  if (item.interp?.show) {
    const pts = interpPoints(item.interp);
    pts.forEach((q, i) => (i ? p.lineTo(q[0], q[1]) : p.moveTo(q[0], q[1])));
    if (item.interp.kind === 'polygon' && item.interp.closed) p.closePath();
  } else {
    const poly = outline(item.points, item.width, item.tool !== 'highlighter');
    poly.forEach((q, i) => (i ? p.lineTo(q[0], q[1]) : p.moveTo(q[0], q[1])));
    p.closePath();
  }
  pathCache.set(item, p);
  return p;
}

export function drawStroke(ctx, item) {
  const path = strokePath(item);
  ctx.save();
  if (item.tool === 'highlighter') {
    ctx.globalAlpha = 0.35;
    ctx.globalCompositeOperation = 'multiply';
  }
  if (item.interp?.show) {
    ctx.strokeStyle = item.color;
    ctx.lineWidth = item.width * 0.9;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    ctx.stroke(path);
  } else {
    ctx.fillStyle = item.color;
    ctx.fill(path);
  }
  ctx.restore();
}

// Live (wet) segment drawing while the pen is down.
export function drawWetSegment(ctx, a, b, width, color, highlighter) {
  ctx.save();
  ctx.strokeStyle = color;
  ctx.lineCap = 'round';
  ctx.lineJoin = 'round';
  if (highlighter) ctx.globalAlpha = 0.35;
  ctx.lineWidth = highlighter ? width : (radiusAt(width, a[2]) + radiusAt(width, b[2]));
  ctx.beginPath();
  ctx.moveTo(a[0], a[1]);
  ctx.lineTo(b[0], b[1]);
  ctx.stroke();
  ctx.restore();
}

export function layoutText(ctx, item) {
  ctx.font = `${item.size}px ${TEXT_FONT}`;
  const lines = [];
  for (const para of String(item.text).split('\n')) {
    let line = '';
    for (const word of para.split(/(\s+)/)) {
      const next = line + word;
      if (line && ctx.measureText(next).width > item.w && word.trim()) {
        lines.push(line.trimEnd());
        line = word.trimStart();
      } else line = next;
    }
    lines.push(line);
  }
  return lines;
}

export function textHeight(ctx, item) {
  return layoutText(ctx, item).length * item.size * 1.35;
}

function drawText(ctx, item) {
  const lines = layoutText(ctx, item);
  ctx.fillStyle = item.color;
  ctx.textBaseline = 'top';
  lines.forEach((ln, i) => ctx.fillText(ln, item.x, item.y + i * item.size * 1.35 + item.size * 0.12));
}

function drawMath(ctx, item) {
  const m = mathImage(item);
  if (m) {
    ctx.drawImage(m.img, item.x, item.y, item.w, item.h);
  } else {
    ctx.fillStyle = '#8a8f98';
    ctx.font = `${item.size * 0.6}px ui-monospace, monospace`;
    ctx.textBaseline = 'top';
    ctx.fillText(item.latex, item.x, item.y);
  }
}

export const PLOT_COLORS = ['#2b59c3', '#d1495b', '#2a9d8f', '#e09f3e', '#7b2cbf'];

export function plotGeometry(item) {
  const { x, y, w, h, x0, x1, y0, y1 } = item;
  const sx = (v) => x + ((v - x0) / (x1 - x0)) * w;
  const sy = (v) => y + h - ((v - y0) / (y1 - y0)) * h;
  return { sx, sy };
}

// Returns drawing primitives so both canvas and SVG exports share the logic.
export function plotPrimitives(item) {
  const { x0, x1, y0, y1 } = item;
  const { sx, sy } = plotGeometry(item);
  const prim = { grid: [], axes: [], ticks: [], curves: [], errors: [] };
  const sxStep = tickStep(x1 - x0), syStep = tickStep(y1 - y0);
  for (let v = Math.ceil(x0 / sxStep) * sxStep; v <= x1 + 1e-9; v += sxStep) {
    prim.grid.push([[sx(v), sy(y0)], [sx(v), sy(y1)]]);
    if (Math.abs(v) > sxStep / 2) prim.ticks.push({ x: sx(v), y: sy(Math.min(Math.max(0, y0), y1)) + 4, text: fmt(v), align: 'center' });
  }
  for (let v = Math.ceil(y0 / syStep) * syStep; v <= y1 + 1e-9; v += syStep) {
    prim.grid.push([[sx(x0), sy(v)], [sx(x1), sy(v)]]);
    if (Math.abs(v) > syStep / 2) prim.ticks.push({ x: sx(Math.min(Math.max(0, x0), x1)) - 4, y: sy(v) - 6, text: fmt(v), align: 'right' });
  }
  if (y0 <= 0 && y1 >= 0) prim.axes.push([[sx(x0), sy(0)], [sx(x1), sy(0)]]);
  if (x0 <= 0 && x1 >= 0) prim.axes.push([[sx(0), sy(y0)], [sx(0), sy(y1)]]);
  (item.exprs || []).forEach((src, i) => {
    let f;
    try { f = compile(src); } catch (e) { prim.errors.push(src + ' — ' + e.message); return; }
    const color = PLOT_COLORS[i % PLOT_COLORS.length];
    for (const line of sample(f, x0, x1, y0, y1, Math.max(200, Math.round(item.w)))) {
      prim.curves.push({ color, pts: line.map(([a, b]) => [sx(a), sy(Math.max(y0 - (y1 - y0), Math.min(y1 + (y1 - y0), b)))]) });
    }
  });
  return prim;
}

function fmt(v) {
  const r = Math.round(v * 1e6) / 1e6;
  return String(r).replace('-', '−');
}

function drawPlot(ctx, item) {
  const prim = plotPrimitives(item);
  ctx.save();
  ctx.fillStyle = '#ffffff';
  ctx.fillRect(item.x, item.y, item.w, item.h);
  ctx.beginPath();
  ctx.rect(item.x, item.y, item.w, item.h);
  ctx.clip();
  const lines = (segs, color, width) => {
    ctx.strokeStyle = color; ctx.lineWidth = width; ctx.beginPath();
    for (const s of segs) s.forEach((p, i) => (i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1])));
    ctx.stroke();
  };
  lines(prim.grid, '#e3e6ec', 1);
  lines(prim.axes, '#5c6370', 1.4);
  ctx.fillStyle = '#5c6370';
  ctx.font = `11px ${TEXT_FONT}`;
  ctx.textBaseline = 'top';
  for (const tk of prim.ticks) { ctx.textAlign = tk.align; ctx.fillText(tk.text, tk.x, tk.y); }
  ctx.textAlign = 'left';
  ctx.lineJoin = 'round';
  for (const c of prim.curves) lines([c.pts], c.color, 2.2);
  ctx.font = `13px ${TEXT_FONT}`;
  (item.exprs || []).forEach((src, i) => {
    ctx.fillStyle = PLOT_COLORS[i % PLOT_COLORS.length];
    ctx.fillText(src, item.x + 8, item.y + 6 + i * 17);
  });
  ctx.fillStyle = '#d1495b';
  prim.errors.forEach((e, i) => ctx.fillText(e, item.x + 8, item.y + item.h - 20 - i * 17));
  ctx.restore();
  ctx.strokeStyle = '#c9ced8';
  ctx.lineWidth = 1;
  ctx.strokeRect(item.x, item.y, item.w, item.h);
}

function drawImageItem(ctx, item) {
  const img = getImage(item.src);
  if (img) ctx.drawImage(img, item.x, item.y, item.w, item.h);
  else { ctx.fillStyle = '#eef0f4'; ctx.fillRect(item.x, item.y, item.w, item.h); }
}

export function drawItem(ctx, item) {
  switch (item.type) {
    case 'stroke': return drawStroke(ctx, item);
    case 'text': return drawText(ctx, item);
    case 'math': return drawMath(ctx, item);
    case 'plot': return drawPlot(ctx, item);
    case 'image': return drawImageItem(ctx, item);
  }
}

// Template lines for a world rectangle (paper rect or visible area).
export function templateLines(template, rect, paper) {
  const lines = [], dots = [];
  const { x, y, w, h } = rect;
  if (template === 'lined') {
    const step = 32, top = paper ? 96 : 0;
    for (let v = Math.ceil((y - top) / step) * step + top; v <= y + h; v += step) if (v >= y) lines.push({ a: [x, v], b: [x + w, v], c: '#c9d8f0', wd: 1 });
    if (paper) lines.push({ a: [80, y], b: [80, y + h], c: '#f0b4b4', wd: 1.2 });
  } else if (template === 'grid' || template === 'axes') {
    const step = 24;
    for (let v = Math.ceil(x / step) * step; v <= x + w; v += step) lines.push({ a: [v, y], b: [v, y + h], c: '#dde3ee', wd: 1 });
    for (let v = Math.ceil(y / step) * step; v <= y + h; v += step) lines.push({ a: [x, v], b: [x + w, v], c: '#dde3ee', wd: 1 });
    if (template === 'axes') {
      const ox = paper ? Math.round(paper.w / 2 / step) * step : 0;
      const oy = paper ? Math.round(paper.h / 2 / step) * step : 0;
      if (oy >= y && oy <= y + h) lines.push({ a: [x, oy], b: [x + w, oy], c: '#6b7280', wd: 1.5 });
      if (ox >= x && ox <= x + w) lines.push({ a: [ox, y], b: [ox, y + h], c: '#6b7280', wd: 1.5 });
    }
  } else if (template === 'dots') {
    const step = 24;
    for (let i = Math.ceil(x / step) * step; i <= x + w; i += step)
      for (let j = Math.ceil(y / step) * step; j <= y + h; j += step) dots.push([i, j]);
  }
  return { lines, dots };
}

export function drawTemplate(ctx, template, rect, paper, scale = 1) {
  const { lines, dots } = templateLines(template, rect, paper);
  if (dots.length > 40000) return;
  for (const l of lines) {
    ctx.strokeStyle = l.c;
    ctx.lineWidth = l.wd / Math.max(scale, 0.5);
    ctx.beginPath(); ctx.moveTo(l.a[0], l.a[1]); ctx.lineTo(l.b[0], l.b[1]); ctx.stroke();
  }
  ctx.fillStyle = '#b8c0cc';
  const r = 1.3 / Math.max(scale, 0.6);
  for (const d of dots) { ctx.beginPath(); ctx.arc(d[0], d[1], r, 0, Math.PI * 2); ctx.fill(); }
}

// Draw a whole page in world coordinates (ctx already transformed).
// visible: world rect to draw template in (for infinite pages).
export function drawPage(ctx, page, { visible, scale = 1, hidden = null, paperShadow = true } = {}) {
  const paper = pageSize(page);
  if (paper) {
    ctx.save();
    if (paperShadow) {
      ctx.shadowColor = 'rgba(15, 23, 42, 0.18)';
      ctx.shadowBlur = 18 * scale;
      ctx.shadowOffsetY = 3 * scale;
    }
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, paper.w, paper.h);
    ctx.restore();
    if (page.background) {
      const img = getImage(page.background.src);
      if (img) ctx.drawImage(img, 0, 0, paper.w, paper.h);
    }
    ctx.save();
    ctx.beginPath(); ctx.rect(0, 0, paper.w, paper.h); ctx.clip();
    drawTemplate(ctx, page.template, { x: 0, y: 0, w: paper.w, h: paper.h }, paper, scale);
    ctx.restore();
  } else if (visible) {
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(visible.x, visible.y, visible.w, visible.h);
    drawTemplate(ctx, page.template, visible, null, scale);
  }
  for (const item of page.items) {
    if (hidden && hidden.has(item.id)) continue;
    drawItem(ctx, item);
  }
}
