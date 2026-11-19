// LaTeX → SVG via the vendored MathJax (Apache-2.0), cached as images so that
// maths can be drawn on the canvas and embedded in exports.

const cache = new Map(); // key -> { status, img, svg, w, h }
let listeners = new Set();
let ready = null;

function load() {
  if (ready) return ready;
  ready = new Promise((resolve, reject) => {
    window.MathJax = {
      tex: { packages: { '[+]': ['ams'] } },
      svg: { fontCache: 'none' },
      startup: { typeset: false, ready: () => { MathJax.startup.defaultReady(); MathJax.startup.promise.then(resolve); } },
    };
    const s = document.createElement('script');
    s.src = new URL('../vendor/mathjax/tex-svg-full.js', import.meta.url).href;
    s.async = true;
    s.onerror = () => reject(new Error('MathJax failed to load'));
    document.head.appendChild(s);
  });
  return ready;
}

const keyOf = (latex, size, color) => `${size}|${color}|${latex}`;

// Returns { svg, w, h } with px dimensions for a LaTeX string.
export async function texToSvg(latex, size = 28, color = '#1d1d1f') {
  await load();
  const node = MathJax.tex2svg(latex || '\\square', { display: true });
  const svgEl = node.querySelector('svg');
  const ex = size * 0.45;
  const w = parseFloat(svgEl.getAttribute('width')) * ex;
  const h = parseFloat(svgEl.getAttribute('height')) * ex;
  svgEl.setAttribute('width', w.toFixed(2));
  svgEl.setAttribute('height', h.toFixed(2));
  svgEl.removeAttribute('style');
  svgEl.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
  let svg = svgEl.outerHTML.replace(/currentColor/g, color);
  const error = !!svgEl.querySelector('[data-mml-node="merror"]');
  return { svg, w, h, error };
}

// Synchronous accessor for renderers: returns the cached entry or null and
// starts rendering in the background (onReady listeners fire when done).
export function mathImage(item) {
  const key = keyOf(item.latex, item.size, item.color);
  const hit = cache.get(key);
  if (hit) return hit.status === 'ok' ? hit : null;
  const entry = { status: 'loading' };
  cache.set(key, entry);
  texToSvg(item.latex, item.size, item.color).then(({ svg, w, h }) => {
    const img = new Image();
    img.onload = () => {
      Object.assign(entry, { status: 'ok', img, svg, w, h });
      listeners.forEach((fn) => fn());
    };
    img.onerror = () => { entry.status = 'error'; listeners.forEach((fn) => fn()); };
    img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg);
  }).catch((e) => { entry.status = 'error'; console.warn(e); listeners.forEach((fn) => fn()); });
  return null;
}

export async function ensureMath(items) {
  await Promise.all(items.filter((i) => i.type === 'math').map((item) => new Promise((resolve) => {
    if (mathImage(item)) return resolve();
    const key = keyOf(item.latex, item.size, item.color);
    const check = () => {
      const e = cache.get(key);
      if (!e || e.status !== 'loading') { listeners.delete(check); resolve(); }
    };
    listeners.add(check);
    setTimeout(() => { listeners.delete(check); resolve(); }, 8000);
  })));
}

export function onMathReady(fn) { listeners.add(fn); return () => listeners.delete(fn); }
export const preloadMath = () => load().catch(() => {});
