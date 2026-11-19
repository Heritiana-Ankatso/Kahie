// Kahie app shell: library, notebook view, routing, settings, import/export.

import * as store from './store.js';
import { t, setLang, getLang, LANGS } from './i18n.js';
import { h, modal, promptText, confirmBox, toast, menu, pickFile, timeAgo } from './ui.js';
import { icon } from './icons.js';
import { newNotebook, newPage, uid, normaliseImport, TEMPLATES, PAGE_SIZES } from './model.js';
import { search } from './search.js';
import { drawPage, pageSize } from './render.js';
import { onMathReady, preloadMath } from './mathtex.js';
import { exportRect, notebookFile, download, safeName, printPages } from './export.js';
import { Editor } from './editor.js';

const COVERS = ['#3b6fd8', '#d1495b', '#2a9d8f', '#e09f3e', '#7b2cbf', '#3d405b', '#118ab2', '#8d6e63'];
const app = document.getElementById('app');
let editor = null;
let routeSeq = 0;
let settings = loadSettings();

function loadSettings() {
  let s = {};
  try { s = JSON.parse(localStorage.getItem('kahie.settings') || '{}'); } catch {}
  return { mode: 'simple', fingerDraw: true, penSeen: false, autoShapes: false, tool: 'pen', ...s };
}
function saveSettings() {
  try { localStorage.setItem('kahie.settings', JSON.stringify(settings)); } catch {}
}

// ---------- routing ----------
async function route() {
  if (editor) { editor.destroy(); editor = null; }
  document.querySelectorAll('.menu, .modal-back').forEach((m) => m.remove());
  const my = ++routeSeq;
  const hash = location.hash.slice(1);
  const [, kind, id] = hash.split('/');
  app.className = '';
  app.replaceChildren();
  try {
    if (kind === 'nb' && id) await showNotebook(id, my);
    else if (kind === 'page' && id) await showPage(id, my);
    else await showLibrary(my);
  } catch (e) {
    console.error(e);
    location.hash = '';
  }
}

const go = (hash) => { if (location.hash === hash) route(); else location.hash = hash; };

// ---------- library ----------
async function showLibrary(my) {
  app.className = 'view-library';
  const notebooks = await store.listNotebooks();
  const pages = await store.allPages();
  const counts = new Map();
  pages.forEach((p) => counts.set(p.notebookId, (counts.get(p.notebookId) || 0) + 1));

  const results = h('div', { class: 'results' });
  const grid = h('div', { class: 'nb-grid' });
  const q = h('input', { class: 'search', type: 'search', placeholder: t('search'), 'aria-label': t('search') });
  q.addEventListener('input', () => {
    const res = search(notebooks, pages, q.value);
    grid.hidden = !!q.value.trim();
    results.replaceChildren(...(q.value.trim() ? (res.length ? res.map(resultRow) : [h('p', { class: 'muted' }, t('noResults'))]) : []));
  });

  const resultRow = (r) => h('button', { class: 'result', onclick: () => go(r.kind === 'page' ? `#/page/${r.page.id}` : `#/nb/${r.notebook.id}`) },
    h('span', { class: 'result-dot', style: { background: r.notebook.color } }),
    h('span', {}, h('strong', {}, r.kind === 'page' ? (r.page.title || t('untitled')) : r.notebook.title),
      r.kind === 'page' ? h('span', { class: 'muted' }, ' — ' + r.notebook.title) : null,
      r.text && r.kind === 'page' ? h('span', { class: 'snippet' }, r.text) : null));

  for (const nb of notebooks) {
    const card = h('div', { class: 'nb-card' },
      h('button', { class: 'nb-cover', style: { '--c': nb.color }, onclick: () => go(`#/nb/${nb.id}`), 'aria-label': t('open') + ' ' + nb.title },
        h('span', { class: 'nb-cover-title' }, nb.title)),
      h('div', { class: 'nb-meta' },
        h('div', {}, h('div', { class: 'nb-title' }, nb.title), h('div', { class: 'muted small' }, `${counts.get(nb.id) || 0} ${t((counts.get(nb.id) || 0) === 1 ? 'pageOne' : 'pages')} · ${timeAgo(nb.updated)}`)),
        h('button', { class: 'icon-btn', 'aria-label': t('more'), title: t('more'), html: icon('more'), onclick: (e) => notebookMenu(e.currentTarget, nb) })));
    grid.appendChild(card);
  }
  grid.appendChild(h('button', { class: 'nb-card new', onclick: createNotebook },
    h('span', { class: 'nb-cover add', html: icon('plus', 34) }), h('div', { class: 'nb-title' }, t('newNotebook'))));

  const top = h('header', { class: 'lib-top' },
    h('div', { class: 'brand' }, h('img', { src: 'icons/icon.svg', alt: '', width: 32, height: 32 }), h('span', {}, 'Kahie')),
    h('div', { class: 'search-wrap', html: icon('search', 18) }, q),
    h('div', { class: 'lib-actions' },
      h('button', { class: 'btn primary', onclick: createNotebook, html: icon('plus', 18) + `<span>${t('newNotebook')}</span>` }),
      h('button', { class: 'icon-btn', title: t('more'), 'aria-label': t('more'), html: icon('more'), onclick: (e) => libraryMenu(e.currentTarget, notebooks, pages) })));
  if (my !== routeSeq) return;
  const warn = store.persistent() ? null : h('p', { class: 'warn' }, t('notSaved'));
  app.append(top, h('main', { class: 'lib-main' }, warn, results, grid));
  q.focus({ preventScroll: true });
}

function libraryMenu(anchor, notebooks, pages) {
  menu(anchor, [
    { label: t('import'), onClick: importFile },
    { label: t('backup'), onClick: () => download(notebookFile(notebooks, pages), `kahie-backup-${new Date().toISOString().slice(0, 10)}.kahie`) },
    { sep: true }, { header: t('mode') },
    { label: t('simple'), checked: settings.mode !== 'advanced', onClick: () => { settings.mode = 'simple'; saveSettings(); } },
    { label: t('advanced'), checked: settings.mode === 'advanced', onClick: () => { settings.mode = 'advanced'; saveSettings(); } },
    { sep: true }, { header: t('language') },
    ...Object.entries(LANGS).map(([k, v]) => ({ label: v, checked: getLang() === k, onClick: () => { setLang(k); route(); } })),
    ...(deferredInstall ? [{ sep: true }, { label: t('install'), onClick: () => deferredInstall.prompt() }] : []),
    { sep: true }, { label: t('help'), onClick: () => modal(t('help'), h('p', {}, t('helpText'))) },
  ]);
}

async function createNotebook() {
  const title = await promptText(t('newNotebook'), '', t('notebook'));
  if (title === null) return;
  const nb = newNotebook(title || t('notebook'), COVERS[Math.floor(Math.random() * COVERS.length)]);
  const page = newPage(nb.id, { template: 'lined' });
  await store.putMany([nb], [page]);
  go(`#/page/${page.id}`);
}

function notebookMenu(anchor, nb) {
  menu(anchor, [
    { label: t('open'), onClick: () => go(`#/nb/${nb.id}`) },
    { label: t('rename'), onClick: async () => { const v = await promptText(t('rename'), nb.title); if (v) { nb.title = v; nb.updated = Date.now(); await store.putNotebook(nb); route(); } } },
    { header: t('color') },
    ...COVERS.map((c) => ({ label: '●', checked: nb.color === c, onClick: async () => { nb.color = c; await store.putNotebook(nb); route(); } })),
    { sep: true },
    { label: t('exportNotebook'), onClick: async () => download(notebookFile([nb], await store.listPages(nb.id)), safeName(nb.title) + '.kahie') },
    { label: t('printNotebook'), onClick: async () => printPages(await store.listPages(nb.id)) },
    { sep: true },
    { label: t('delete'), onClick: async () => { if (await confirmBox(t('confirmDeleteNotebook', nb.title))) { await store.deleteNotebook(nb.id); route(); } } },
  ]);
  // Colour items show their colour.
  document.querySelectorAll('.menu .menu-item').forEach((b) => {
    if (b.textContent.endsWith('●')) {
      const i = [...document.querySelectorAll('.menu .menu-item')].filter((x) => x.textContent.endsWith('●')).indexOf(b);
      b.style.color = COVERS[i];
    }
  });
}

async function importFile() {
  const file = await pickFile('.kahie,.json,application/json');
  if (!file) return;
  try {
    const data = normaliseImport(JSON.parse(await file.text()));
    await store.putMany(data.notebooks, data.pages);
    toast(t('importDone', data.notebooks.length));
    route();
  } catch (e) {
    console.error(e);
    toast(t('importFail'));
  }
}

// ---------- notebook ----------
async function showNotebook(id, my) {
  const nb = (await store.listNotebooks()).find((n) => n.id === id);
  if (!nb) { go(''); return; }
  const pages = await store.listPages(id);
  if (my !== routeSeq) return;
  app.className = 'view-notebook';
  const grid = h('div', { class: 'page-grid' });
  const thumbs = [];
  pages.forEach((p, i) => {
    const c = h('canvas', { class: 'thumb' });
    thumbs.push([c, p]);
    grid.appendChild(h('div', { class: 'page-card' },
      h('button', { class: 'thumb-btn', onclick: () => go(`#/page/${p.id}`), 'aria-label': `${t('page')} ${i + 1}` }, c),
      h('div', { class: 'page-meta' },
        h('span', { class: 'small' }, `${i + 1}. ${p.title || ''}`),
        h('button', { class: 'icon-btn', 'aria-label': t('more'), title: t('more'), html: icon('more', 18), onclick: (e) => pageMenu(e.currentTarget, nb, pages, i) }))));
  });
  grid.appendChild(h('button', { class: 'page-card new', onclick: () => newPageDialog(nb, pages) }, h('span', { html: icon('plus', 34) }), h('span', {}, t('newPage'))));

  const title = h('h1', { class: 'nb-heading', tabindex: '0', title: t('rename'), onclick: async () => {
    const v = await promptText(t('rename'), nb.title);
    if (v) { nb.title = v; nb.updated = Date.now(); await store.putNotebook(nb); route(); }
  } }, nb.title);
  app.append(
    h('header', { class: 'lib-top' },
      h('button', { class: 'icon-btn', title: t('library'), 'aria-label': t('library'), html: icon('back'), onclick: () => go('') }),
      h('span', { class: 'nb-swatch', style: { background: nb.color } }), title,
      h('div', { class: 'lib-actions' },
        h('button', { class: 'btn', onclick: () => importPdf(nb, pages), html: icon('upload', 18) + `<span>PDF</span>`, title: t('importPdf') }),
        h('button', { class: 'btn primary', onclick: () => newPageDialog(nb, pages), html: icon('plus', 18) + `<span>${t('newPage')}</span>` }),
        h('button', { class: 'icon-btn', title: t('more'), 'aria-label': t('more'), html: icon('more'), onclick: (e) => notebookMenu(e.currentTarget, nb) }))),
    h('main', { class: 'lib-main' }, grid));
  const draw = () => thumbs.forEach(([c, p]) => drawThumb(c, p));
  draw();
  const off = onMathReady(draw);
  window.addEventListener('hashchange', off, { once: true });
}

function drawThumb(canvas, page) {
  const r = exportRect(page);
  const W = 180, k = W / r.w, H = Math.min(260, Math.round(r.h * k));
  const dpr = window.devicePixelRatio || 1;
  canvas.width = W * dpr; canvas.height = H * dpr;
  canvas.style.width = W + 'px'; canvas.style.height = H + 'px';
  const ctx = canvas.getContext('2d');
  ctx.setTransform(k * dpr, 0, 0, k * dpr, -r.x * k * dpr, -r.y * k * dpr);
  drawPage(ctx, page, { visible: r, scale: k, paperShadow: false });
}

function pageMenu(anchor, nb, pages, i) {
  const p = pages[i];
  const reorder = async (j) => {
    if (j < 0 || j >= pages.length) return;
    [pages[i], pages[j]] = [pages[j], pages[i]];
    pages.forEach((pg, k) => (pg.order = k));
    await store.putMany([], pages);
    route();
  };
  menu(anchor, [
    { label: t('open'), onClick: () => go(`#/page/${p.id}`) },
    { label: t('rename'), onClick: async () => { const v = await promptText(t('rename'), p.title); if (v !== null) { p.title = v; await store.putPage(p); route(); } } },
    { label: t('duplicate'), onClick: async () => {
      const copy = { ...structuredClone(p), id: uid(), order: p.order + 0.5, created: Date.now(), updated: Date.now() };
      const all = [...pages, copy].sort((a, b) => a.order - b.order);
      all.forEach((pg, k) => (pg.order = k));
      await store.putMany([], all);
      route();
    } },
    { label: t('moveUp'), onClick: () => reorder(i - 1) },
    { label: t('moveDown'), onClick: () => reorder(i + 1) },
    { sep: true },
    { label: t('delete'), onClick: async () => { if (await confirmBox(t('confirmDeletePage'))) { await store.deletePage(p.id); route(); } } },
  ]);
}

async function newPageDialog(nb, pages, after = null) {
  let template = settings.lastTemplate || 'lined', size = settings.lastSize || 'A4';
  const opts = (list, cur, label, set) => h('div', { class: 'choice-row', role: 'radiogroup', 'aria-label': label },
    list.map((v) => h('button', {
      class: 'choice' + (v === cur ? ' active' : ''), role: 'radio', 'aria-checked': String(v === cur),
      onclick: (e) => { set(v); e.currentTarget.parentNode.querySelectorAll('.choice').forEach((b) => { b.classList.toggle('active', b === e.currentTarget); b.setAttribute('aria-checked', String(b === e.currentTarget)); }); },
    }, v === 'infinite' ? t('infinite') : TEMPLATES.includes(v) ? t(v) : v)));
  const ok = await modal(t('newPage'), h('div', {},
    h('h3', {}, t('template')), opts(TEMPLATES, template, t('template'), (v) => (template = v)),
    h('h3', {}, t('size')), opts(Object.keys(PAGE_SIZES), size, t('size'), (v) => (size = v))),
  [{ label: t('cancel'), value: false }, { label: t('ok'), value: true, primary: true }]);
  if (!ok) return null;
  settings.lastTemplate = template; settings.lastSize = size; saveSettings();
  return insertPage(nb, pages, after, { template, size });
}

async function insertPage(nb, pages, after, opts) {
  const idx = after ? pages.findIndex((p) => p.id === after.id) + 1 : pages.length;
  const page = newPage(nb.id, { ...opts, order: idx });
  const all = [...pages.slice(0, idx), page, ...pages.slice(idx)];
  all.forEach((p, k) => (p.order = k));
  nb.updated = Date.now();
  await store.putMany([nb], all);
  go(`#/page/${page.id}`);
  return page;
}

async function importPdf(nb, pages) {
  const file = await pickFile('application/pdf');
  if (!file) return;
  let created;
  try { created = await pdfToPages(nb, pages, await file.arrayBuffer(), file.name.replace(/\.pdf$/i, ''), (n, total) => toast(t('pdfLoading', n, total), 60000)); }
  catch (e) { console.error(e); toast(t('importFail')); return; }
  toast('✓', 1200);
  if (created[0]) go(`#/page/${created[0].id}`);
}

// Each PDF page becomes a Kahie page with the rendered PDF as its background.
export async function pdfToPages(nb, pages, data, name, onProgress = () => {}) {
  const pdfjs = await import('../vendor/pdfjs/pdf.min.mjs');
  pdfjs.GlobalWorkerOptions.workerSrc = new URL('../vendor/pdfjs/pdf.worker.min.mjs', import.meta.url).href;
  const doc = await pdfjs.getDocument({ data, isEvalSupported: false }).promise;
  const created = [];
  for (let n = 1; n <= doc.numPages; n++) {
    onProgress(n, doc.numPages);
    const pg = await doc.getPage(n);
    const vp1 = pg.getViewport({ scale: 1 });
    const w = 794, k = w / vp1.width, hh = Math.round(vp1.height * k);
    const vp = pg.getViewport({ scale: k * 2 });
    const c = document.createElement('canvas');
    c.width = Math.round(vp.width); c.height = Math.round(vp.height);
    const ctx = c.getContext('2d');
    ctx.fillStyle = '#fff'; ctx.fillRect(0, 0, c.width, c.height);
    // 'print' intent renders without requestAnimationFrame, so imports finish in background tabs.
    await pg.render({ canvasContext: ctx, viewport: vp, intent: 'print' }).promise;
    const page = newPage(nb.id, { title: `${name} — ${n}`, template: 'blank', size: 'custom', order: pages.length + n });
    page.w = w; page.h = hh;
    page.background = { src: c.toDataURL('image/jpeg', 0.85), w, h: hh };
    created.push(page);
  }
  const all = [...pages, ...created];
  all.forEach((p, k) => (p.order = k));
  nb.updated = Date.now();
  await store.putMany([nb], all);
  return created;
}

// ---------- page ----------
async function showPage(id, my) {
  const page = await store.getPage(id);
  if (!page) { go(''); return; }
  const nb = (await store.listNotebooks()).find((n) => n.id === page.notebookId);
  if (!nb) { go(''); return; }
  const pages = await store.listPages(nb.id);
  if (my !== routeSeq) return;
  app.className = 'view-editor';
  editor = new Editor(app, {
    page, notebook: nb, pageIds: pages.map((p) => p.id), settings,
    saveSettings,
    save: async (p) => { await store.putPage(p); nb.updated = Date.now(); await store.putNotebook(nb); },
    back: () => go(`#/nb/${nb.id}`),
    goto: (pid) => go(`#/page/${pid}`),
    addPage: (cur) => insertPage(nb, pages, cur, { template: cur.template, size: cur.size === 'custom' ? 'A4' : cur.size }),
  });
  editor.stage.dataset.tool = editor.tool;
  if (/[?&]debug/.test(location.search)) window.kahieEditor = editor;
}

// ---------- first run ----------
async function firstRun() {
  if ((await store.listNotebooks()).length) return false;
  const nb = newNotebook(t('myNotebook'), COVERS[0]);
  const page = newPage(nb.id, { template: 'lined', title: '' });
  await store.putMany([nb], [page]);
  location.hash = `#/page/${page.id}`;
  return true;
}

// ---------- PWA ----------
let deferredInstall = null;
window.addEventListener('beforeinstallprompt', (e) => { e.preventDefault(); deferredInstall = e; });
if ('serviceWorker' in navigator && location.protocol !== 'file:' && !/[?&]nosw/.test(location.search)) {
  navigator.serviceWorker.register('sw.js').catch((e) => console.warn('SW', e));
}

// ---------- boot ----------
(async function boot() {
  setLang(getLang());
  await store.open();
  window.addEventListener('hashchange', route);
  const fresh = await firstRun();
  if (!fresh) await route();
  if (fresh) toast(t('welcomeText'), 6000);
  setTimeout(preloadMath, 1500);
})();

