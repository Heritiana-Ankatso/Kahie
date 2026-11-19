// Export: PNG, SVG, print/PDF, and .kahie notebook files.

import { drawPage, pageSize, templateLines, plotPrimitives, layoutText, PLOT_COLORS, TEXT_FONT } from './render.js';
import { outline, polyToSvgPath } from './ink.js';
import { interpPoints, unionBounds } from './model.js';
import { ensureMath, texToSvg } from './mathtex.js';

const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

// Area exported for a page: the paper, or the content bounds (+ margin) for infinite canvases.
export function exportRect(page) {
  const paper = pageSize(page);
  if (paper) return { x: 0, y: 0, w: paper.w, h: paper.h };
  const b = unionBounds(page.items) || { x: 0, y: 0, w: 800, h: 600 };
  const m = 40;
  return { x: Math.floor(b.x - m), y: Math.floor(b.y - m), w: Math.ceil(b.w + 2 * m), h: Math.ceil(b.h + 2 * m) };
}

export async function pageToCanvas(page, scale = 2) {
  await ensureMath(page.items);
  if (page.background) await loadImage(page.background.src);
  await Promise.all(page.items.filter((i) => i.type === 'image').map((i) => loadImage(i.src)));
  const r = exportRect(page);
  const c = document.createElement('canvas');
  c.width = Math.max(1, Math.round(r.w * scale));
  c.height = Math.max(1, Math.round(r.h * scale));
  const ctx = c.getContext('2d');
  ctx.setTransform(scale, 0, 0, scale, -r.x * scale, -r.y * scale);
  drawPage(ctx, page, { visible: r, scale: 1, paperShadow: false });
  return c;
}

function loadImage(src) {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = img.onerror = () => resolve(img);
    img.src = src;
  });
}

export async function pageToPngBlob(page, scale = 2) {
  const c = await pageToCanvas(page, scale);
  return new Promise((resolve) => c.toBlob(resolve, 'image/png'));
}

export async function pageToSvg(page) {
  const r = exportRect(page);
  const paper = pageSize(page);
  const out = [];
  out.push(`<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="${r.w}" height="${r.h}" viewBox="${r.x} ${r.y} ${r.w} ${r.h}">`);
  out.push(`<rect x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" fill="#fff"/>`);
  if (page.background) out.push(`<image x="0" y="0" width="${paper.w}" height="${paper.h}" href="${esc(page.background.src)}"/>`);
  const { lines, dots } = templateLines(page.template, r, paper);
  for (const l of lines) out.push(`<line x1="${l.a[0]}" y1="${l.a[1]}" x2="${l.b[0]}" y2="${l.b[1]}" stroke="${l.c}" stroke-width="${l.wd}"/>`);
  if (dots.length < 40000) for (const d of dots) out.push(`<circle cx="${d[0]}" cy="${d[1]}" r="1.3" fill="#b8c0cc"/>`);

  const measure = document.createElement('canvas').getContext('2d');
  for (const it of page.items) {
    if (it.type === 'stroke') {
      const hl = it.tool === 'highlighter' ? ' fill-opacity="0.35" stroke-opacity="0.35" style="mix-blend-mode:multiply"' : '';
      if (it.interp?.show) {
        const pts = interpPoints(it.interp);
        const closed = it.interp.kind !== 'line' && it.interp.closed !== false;
        out.push(`<path d="${polyToSvgPath(pts, closed)}" fill="none" stroke="${esc(it.color)}" stroke-width="${it.width * 0.9}" stroke-linejoin="round" stroke-linecap="round"${hl}/>`);
      } else {
        out.push(`<path d="${polyToSvgPath(outline(it.points, it.width, it.tool !== 'highlighter'))}" fill="${esc(it.color)}"${hl}/>`);
      }
    } else if (it.type === 'text') {
      const lns = layoutText(measure, it);
      out.push(`<text font-family='${TEXT_FONT.replace(/"/g, '')}' font-size="${it.size}" fill="${esc(it.color)}">` +
        lns.map((ln, i) => `<tspan x="${it.x}" y="${it.y + i * it.size * 1.35 + it.size * 0.95}">${esc(ln)}</tspan>`).join('') + '</text>');
    } else if (it.type === 'math') {
      const { svg } = await texToSvg(it.latex, it.size, it.color);
      out.push(svg.replace('<svg ', `<svg x="${it.x}" y="${it.y}" `).replace(/width="[^"]*"/, `width="${it.w}"`).replace(/height="[^"]*"/, `height="${it.h}"`));
    } else if (it.type === 'image') {
      out.push(`<image x="${it.x}" y="${it.y}" width="${it.w}" height="${it.h}" href="${esc(it.src)}" preserveAspectRatio="none"/>`);
    } else if (it.type === 'plot') {
      const p = plotPrimitives(it);
      const id = 'clip-' + it.id.replace(/[^a-z0-9]/gi, '');
      out.push(`<clipPath id="${id}"><rect x="${it.x}" y="${it.y}" width="${it.w}" height="${it.h}"/></clipPath>`);
      out.push(`<g clip-path="url(#${id})"><rect x="${it.x}" y="${it.y}" width="${it.w}" height="${it.h}" fill="#fff"/>`);
      const seg = (s, c, wd) => `<path d="${s.map((q) => polyToSvgPath(q, false)).join('')}" fill="none" stroke="${c}" stroke-width="${wd}"/>`;
      out.push(seg(p.grid, '#e3e6ec', 1), seg(p.axes, '#5c6370', 1.4));
      for (const tk of p.ticks) out.push(`<text x="${tk.x}" y="${tk.y + 10}" font-size="11" font-family="sans-serif" fill="#5c6370" text-anchor="${tk.align === 'center' ? 'middle' : 'end'}">${esc(tk.text)}</text>`);
      for (const c of p.curves) out.push(seg([c.pts], c.color, 2.2));
      (it.exprs || []).forEach((src, i) => out.push(`<text x="${it.x + 8}" y="${it.y + 18 + i * 17}" font-size="13" font-family="sans-serif" fill="${PLOT_COLORS[i % PLOT_COLORS.length]}">${esc(src)}</text>`));
      out.push(`</g><rect x="${it.x}" y="${it.y}" width="${it.w}" height="${it.h}" fill="none" stroke="#c9ced8"/>`);
    }
  }
  out.push('</svg>');
  return out.join('\n');
}

export function download(blob, filename) {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(blob);
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 4000);
}

export function safeName(s) {
  return (String(s || 'kahie').normalize('NFD').replace(/[̀-ͯ]/g, '').replace(/[^a-z0-9-_ ]/gi, '').trim().replace(/\s+/g, '-') || 'kahie').slice(0, 60);
}

export function notebookFile(notebooks, pages) {
  const data = { format: 'kahie', version: 1, exported: new Date().toISOString(), notebooks, pages };
  return new Blob([JSON.stringify(data)], { type: 'application/json' });
}

// Print one or more pages (browser "Save as PDF" gives a PDF).
export async function printPages(pages, onProgress = () => {}) {
  const root = document.getElementById('print-root');
  root.innerHTML = '';
  for (let i = 0; i < pages.length; i++) {
    onProgress(i + 1, pages.length);
    const c = await pageToCanvas(pages[i], 2);
    const img = document.createElement('img');
    img.src = c.toDataURL('image/png');
    img.className = 'print-page';
    const r = exportRect(pages[i]);
    img.style.aspectRatio = `${r.w} / ${r.h}`;
    root.appendChild(img);
  }
  await Promise.all([...root.querySelectorAll('img')].map((img) => img.decode().catch(() => {})));
  window.print();
}
