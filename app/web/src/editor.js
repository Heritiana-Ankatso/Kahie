// Page editor: canvas view, stylus/touch/mouse input, tools, selection, inline editors.

import { t } from './i18n.js';
import { h, modal, menu, toast, pickFile, readAsDataURL, confirmBox } from './ui.js';
import { icon } from './icons.js';
import { History, opAdd, opRemove, opUpdate, uid, itemBounds, interpPoints, translateItem, rectsIntersect, unionBounds, TEMPLATES, PAGE_SIZES } from './model.js';
import { finishPoints, hitPolyline, segmentHitsPolyline } from './ink.js';
import { recognize } from './shapes.js';
import { drawPage, drawItem, drawWetSegment, drawStroke, pageSize, textHeight, setImageLoadHandler } from './render.js';
import { onMathReady, texToSvg } from './mathtex.js';
import { pageToPngBlob, pageToSvg, download, safeName, printPages } from './export.js';

export const COLORS = ['#1d1d1f', '#2b59c3', '#d1495b', '#2a9d8f', '#e09f3e'];
const HL_COLORS = ['#ffe066', '#9be564', '#7cc6fe', '#ff99c8', '#ffb86b'];
const WIDTHS = { pen: [1.6, 3, 6], highlighter: [12, 20, 32], eraser: [8, 18, 40] };
const MATH_SNIPPETS = ['\\frac{a}{b}', '\\sqrt{x}', 'x^{2}', 'x_{n}', '\\int_{a}^{b}', '\\sum_{k=1}^{n}', '\\lim_{x\\to 0}', '\\pi', '\\le', '\\ge', '\\ne', '\\infty', '\\to', '\\vec{u}', '\\in', '\\mathbb{R}'];

export class Editor {
  constructor(root, { page, notebook, pageIds, settings, saveSettings, save, back, goto, addPage }) {
    Object.assign(this, { root, page, notebook, pageIds, settings, saveSettings, saveFn: save, back, goto, addPage });
    this.history = new History(page);
    this.view = { s: 1, ox: 0, oy: 0 };
    this.tool = settings.tool || 'pen';
    this.sel = new Set();
    this.action = null;
    this.pointers = new Map();
    this.hidden = new Set();
    this.inline = null;
    this.dirty = false;
    this.build();
    this.cleanup = [];
    this.cleanup.push(onMathReady(() => this.requestRender()));
    setImageLoadHandler(() => this.requestRender());
    const ro = new ResizeObserver(() => this.resize());
    ro.observe(this.stage);
    this.cleanup.push(() => ro.disconnect());
    const onKey = (e) => this.onKey(e);
    const onKeyUp = (e) => { if (e.code === 'Space') this.spaceDown = false; };
    document.addEventListener('keydown', onKey);
    document.addEventListener('keyup', onKeyUp);
    this.cleanup.push(() => { document.removeEventListener('keydown', onKey); document.removeEventListener('keyup', onKeyUp); });
    const onHide = () => { if (document.visibilityState === 'hidden') this.flush(); };
    document.addEventListener('visibilitychange', onHide);
    window.addEventListener('pagehide', onHide);
    this.cleanup.push(() => { document.removeEventListener('visibilitychange', onHide); window.removeEventListener('pagehide', onHide); });
    this.resize(true);
  }

  destroy() {
    this.commitInline();
    this.flush();
    this.cleanup.forEach((f) => f());
    setImageLoadHandler(() => {});
  }

  // ---------- layout ----------
  build() {
    this.titleInput = h('input', {
      class: 'page-title', value: this.page.title, placeholder: t('untitled'), 'aria-label': t('page'),
      onchange: () => { this.page.title = this.titleInput.value.trim(); this.markDirty(); },
      onkeydown: (e) => { if (e.key === 'Enter') e.target.blur(); },
    });
    this.toolbar = h('div', { class: 'toolbar', role: 'toolbar' });
    this.status = h('span', { class: 'save-status', 'aria-live': 'polite' });
    this.pageNav = h('div', { class: 'page-nav' });
    const top = h('header', { class: 'editor-top' },
      h('button', { class: 'icon-btn', title: t('back'), 'aria-label': t('back'), html: icon('back'), onclick: () => this.back() }),
      h('div', { class: 'title-wrap' }, h('span', { class: 'nb-name' }, this.notebook.title), this.titleInput),
      this.status, this.pageNav);
    this.wet = h('canvas', { class: 'wet' });
    this.main = h('canvas', { class: 'main' });
    this.overlay = h('div', { class: 'overlay' });
    this.stage = h('div', { class: 'stage', tabindex: '0', 'aria-label': t('page') }, this.main, this.wet, this.overlay);
    this.root.append(top, this.toolbar, this.stage);
    this.ctx = this.main.getContext('2d');
    this.wctx = this.wet.getContext('2d', { desynchronized: true }) || this.wet.getContext('2d');
    for (const ev of ['pointerdown', 'pointermove', 'pointerup', 'pointercancel']) this.stage.addEventListener(ev, (e) => this['on' + ev](e));
    this.stage.addEventListener('wheel', (e) => this.onWheel(e), { passive: false });
    this.stage.addEventListener('contextmenu', (e) => e.preventDefault());
    this.stage.addEventListener('dblclick', (e) => this.onDouble(e));
    this.renderToolbar();
    this.renderPageNav();
  }

  renderPageNav() {
    const i = this.pageIds.indexOf(this.page.id);
    this.pageNav.replaceChildren(
      h('button', { class: 'icon-btn', title: t('prevPage'), 'aria-label': t('prevPage'), html: icon('prev'), disabled: i <= 0, onclick: () => this.goto(this.pageIds[i - 1]) }),
      h('span', { class: 'page-count' }, `${i + 1} / ${this.pageIds.length}`),
      h('button', { class: 'icon-btn', title: t('nextPage'), 'aria-label': t('nextPage'), html: icon('next'), disabled: i >= this.pageIds.length - 1, onclick: () => this.goto(this.pageIds[i + 1]) }),
      h('button', { class: 'icon-btn', title: t('addPageAfter'), 'aria-label': t('addPageAfter'), html: icon('plus'), onclick: () => this.addPage(this.page) }),
    );
  }

  renderToolbar() {
    const adv = this.settings.mode === 'advanced';
    const toolBtn = (name) => h('button', {
      class: 'tool' + (this.tool === name ? ' active' : ''), title: t(name), 'aria-label': t(name), 'aria-pressed': String(this.tool === name),
      html: icon(name), onclick: () => this.setTool(name),
    });
    const tools = ['pen', 'highlighter', 'eraser', 'select', 'text'];
    if (adv) tools.push('math', 'plot', 'hand');
    const group1 = h('div', { class: 'tb-group' }, tools.map(toolBtn),
      adv ? h('button', { class: 'tool', title: t('image'), 'aria-label': t('image'), html: icon('image'), onclick: () => this.insertImage() }) : null,
      adv ? h('button', {
        class: 'tool toggle' + (this.settings.autoShapes ? ' active' : ''), title: t('shapes'), 'aria-label': t('shapes'), 'aria-pressed': String(!!this.settings.autoShapes),
        html: icon('wand'), onclick: () => { this.settings.autoShapes = !this.settings.autoShapes; this.saveSettings(); this.renderToolbar(); toast(t('shapes') + (this.settings.autoShapes ? ' ✓' : ' ✗')); },
      }) : null);

    const isHl = this.tool === 'highlighter';
    const palette = isHl ? HL_COLORS : COLORS;
    const cur = this.currentColor();
    const colors = h('div', { class: 'tb-group colors' }, palette.map((c) => h('button', {
      class: 'swatch' + (c === cur ? ' active' : ''), style: { '--c': c }, title: c, 'aria-label': t('color') + ' ' + c,
      onclick: () => this.setColor(c),
    })), adv ? h('label', { class: 'swatch custom', title: t('customColor') },
      h('input', { type: 'color', value: cur, 'aria-label': t('customColor'), oninput: (e) => this.setColor(e.target.value) })) : null);

    const wt = WIDTHS[this.tool] || WIDTHS.pen;
    const curW = this.currentWidth();
    const widths = h('div', { class: 'tb-group widths' }, wt.map((w, i) => h('button', {
      class: 'width-btn' + (Math.abs(w - curW) < 1e-6 ? ' active' : ''), title: t('width') + ' ' + (i + 1), 'aria-label': t('width') + ' ' + (i + 1),
      onclick: () => this.setWidth(w),
    }, h('span', { class: 'dot', style: { width: 4 + i * 5 + 'px', height: 4 + i * 5 + 'px' } }))));

    const hist = h('div', { class: 'tb-group' },
      this.undoBtn = h('button', { class: 'tool', title: t('undo') + ' (Ctrl+Z)', 'aria-label': t('undo'), html: icon('undo'), onclick: () => this.undo() }),
      this.redoBtn = h('button', { class: 'tool', title: t('redo') + ' (Ctrl+Shift+Z)', 'aria-label': t('redo'), html: icon('redo'), onclick: () => this.redo() }));
    const more = h('button', { class: 'tool', title: t('more'), 'aria-label': t('more'), html: icon('more'), onclick: (e) => this.openMenu(e.currentTarget) });
    const fit = h('button', { class: 'tool', title: t('fitPage'), 'aria-label': t('fitPage'), html: icon('fit'), onclick: () => { this.fit(true); } });
    const modeBtn = h('button', { class: 'mode-btn', onclick: () => this.toggleMode() }, adv ? t('simple') : t('advanced'));
    this.toolbar.replaceChildren(group1, colors, widths, hist, h('div', { class: 'tb-group' }, fit, more), modeBtn);
    this.updateHistoryButtons();
  }

  toggleMode() {
    this.settings.mode = this.settings.mode === 'advanced' ? 'simple' : 'advanced';
    if (this.settings.mode === 'simple' && ['math', 'plot', 'hand'].includes(this.tool)) this.tool = 'pen';
    this.saveSettings();
    this.renderToolbar();
  }

  updateHistoryButtons() {
    if (this.undoBtn) this.undoBtn.disabled = !this.history.canUndo;
    if (this.redoBtn) this.redoBtn.disabled = !this.history.canRedo;
  }

  currentColor() {
    const key = this.tool === 'highlighter' ? 'hlColor' : 'color';
    return this.settings[key] || (this.tool === 'highlighter' ? HL_COLORS[0] : COLORS[0]);
  }
  currentWidth() {
    const tool = WIDTHS[this.tool] ? this.tool : 'pen';
    return this.settings['w_' + tool] ?? WIDTHS[tool][1];
  }

  setTool(name) {
    this.commitInline();
    this.tool = name;
    if (name !== 'select') this.clearSelection();
    this.settings.tool = ['pen', 'highlighter', 'eraser', 'select', 'text'].includes(name) ? name : this.settings.tool;
    this.saveSettings();
    this.renderToolbar();
    this.stage.dataset.tool = name;
  }

  setColor(c) {
    if (this.tool === 'select' && this.sel.size) {
      const items = this.selectedItems().filter((it) => 'color' in it).map((it) => ({ ...it, color: c }));
      this.apply(opUpdate(this.page, items));
      this.settings.color = c;
    } else {
      this.settings[this.tool === 'highlighter' ? 'hlColor' : 'color'] = c;
      if (!['pen', 'highlighter', 'text', 'math'].includes(this.tool)) this.setTool('pen');
    }
    this.saveSettings();
    this.renderToolbar();
  }

  setWidth(w) {
    const tool = WIDTHS[this.tool] ? this.tool : 'pen';
    this.settings['w_' + tool] = w;
    this.saveSettings();
    this.renderToolbar();
  }

  openMenu(anchor) {
    const p = this.page;
    const items = [{ header: t('template') }];
    for (const tp of TEMPLATES) items.push({ label: t(tp), checked: p.template === tp, onClick: () => { p.template = tp; this.markDirty(); this.requestRender(); } });
    if (p.size !== 'custom') {
      items.push({ sep: true }, { header: t('size') });
      for (const sz of Object.keys(PAGE_SIZES)) items.push({ label: sz === 'infinite' ? t('infinite') : sz, checked: p.size === sz, onClick: () => { p.size = sz; this.markDirty(); this.fit(); } });
    }
    items.push({ sep: true }, { header: t('export') },
      { label: t('exportPng'), onClick: async () => download(await pageToPngBlob(p), safeName(p.title || this.notebook.title) + '.png') },
      { label: t('exportSvg'), onClick: async () => download(new Blob([await pageToSvg(p)], { type: 'image/svg+xml' }), safeName(p.title || this.notebook.title) + '.svg') },
      { label: t('print'), onClick: () => printPages([p]) },
      { sep: true },
      { label: t('fingerDraw'), checked: !!this.settings.fingerDraw, onClick: () => { this.settings.fingerDraw = !this.settings.fingerDraw; this.saveSettings(); } },
      { label: t('clear'), onClick: async () => { if (await confirmBox(t('confirmClear'))) this.apply(opRemove(p, p.items.map((i) => i.id))); } },
      { label: t('help'), onClick: () => modal(t('help'), h('p', {}, t('helpText'))) });
    menu(anchor, items);
  }

  // ---------- view ----------
  resize(initial = false) {
    const r = this.stage.getBoundingClientRect();
    if (!r.width || !r.height) return;
    this.dpr = window.devicePixelRatio || 1;
    this.W = r.width; this.H = r.height;
    for (const c of [this.main, this.wet]) {
      c.width = Math.round(r.width * this.dpr);
      c.height = Math.round(r.height * this.dpr);
      c.style.width = r.width + 'px';
      c.style.height = r.height + 'px';
    }
    if (initial || !this.userMoved) this.fit();
    this.render();
  }

  // Fit width (default) or the whole page (whole = true).
  fit(whole = false) {
    const paper = pageSize(this.page);
    const W = this.W, H = this.H;
    if (!W) return;
    if (paper) {
      const sw = (W - 32) / paper.w, sh = (H - 32) / paper.h;
      const s = whole ? Math.min(sw, sh) : Math.min(sw, 1.4);
      this.view = { s, ox: (W - paper.w * s) / 2, oy: whole ? (H - paper.h * s) / 2 : 16 };
    } else {
      const b = unionBounds(this.page.items);
      if (whole && b) {
        const s = Math.min((W - 64) / Math.max(b.w, 1), (H - 64) / Math.max(b.h, 1), 2);
        this.view = { s, ox: W / 2 - (b.x + b.w / 2) * s, oy: H / 2 - (b.y + b.h / 2) * s };
      } else this.view = { s: 1, ox: 24, oy: 24 };
    }
    this.requestRender();
  }

  toWorld(sx, sy) { return [(sx - this.view.ox) / this.view.s, (sy - this.view.oy) / this.view.s]; }
  toScreen(wx, wy) { return [wx * this.view.s + this.view.ox, wy * this.view.s + this.view.oy]; }
  localXY(e) { const r = this.stage.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; }

  zoomAt(sx, sy, factor) {
    this.userMoved = true;
    const s = Math.max(0.1, Math.min(8, this.view.s * factor));
    const [wx, wy] = this.toWorld(sx, sy);
    this.view = { s, ox: sx - wx * s, oy: sy - wy * s };
    this.requestRender();
  }

  requestRender() {
    if (this.raf) return;
    this.raf = requestAnimationFrame(() => { this.raf = 0; this.render(); });
  }

  render() {
    const { ctx, dpr, view } = this;
    if (!ctx || !this.W) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = getComputedStyle(this.stage).getPropertyValue('--desk').trim() || '#e8ebf0';
    ctx.fillRect(0, 0, this.main.width, this.main.height);
    ctx.setTransform(dpr * view.s, 0, 0, dpr * view.s, dpr * view.ox, dpr * view.oy);
    const [x0, y0] = this.toWorld(0, 0), [x1, y1] = this.toWorld(this.W, this.H);
    const hidden = new Set(this.hidden);
    const moving = this.action && (this.action.kind === 'move' || this.action.kind === 'resize');
    if (moving) this.sel.forEach((id) => hidden.add(id));
    drawPage(ctx, this.page, { visible: { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }, scale: view.s, hidden });
    if (moving) {
      for (const it of this.previewItems()) drawItem(ctx, it);
    }
    // Selection outline
    if (this.sel.size) {
      const items = moving ? this.previewItems() : this.selectedItems();
      const b = unionBounds(items);
      if (b) {
        ctx.save();
        ctx.strokeStyle = '#2b59c3';
        ctx.lineWidth = 1.5 / view.s;
        ctx.setLineDash([6 / view.s, 4 / view.s]);
        ctx.strokeRect(b.x - 4 / view.s, b.y - 4 / view.s, b.w + 8 / view.s, b.h + 8 / view.s);
        ctx.restore();
        const single = items.length === 1 && ['image', 'plot', 'text', 'math'].includes(items[0].type);
        if (single) {
          ctx.fillStyle = '#2b59c3';
          const r = 7 / view.s;
          ctx.beginPath(); ctx.arc(b.x + b.w + 4 / view.s, b.y + b.h + 4 / view.s, r, 0, Math.PI * 2); ctx.fill();
        }
      }
    }
    if (this.action?.kind === 'marquee') {
      const a = this.action;
      ctx.save();
      ctx.fillStyle = 'rgba(43,89,195,0.08)';
      ctx.strokeStyle = '#2b59c3';
      ctx.lineWidth = 1 / view.s;
      const r = normRect(a.start, a.cur);
      ctx.fillRect(r.x, r.y, r.w, r.h);
      ctx.strokeRect(r.x, r.y, r.w, r.h);
      ctx.restore();
    }
    this.positionSelectionBar();
  }

  // ---------- items ----------
  selectedItems() { return this.page.items.filter((it) => this.sel.has(it.id)); }

  previewItems() {
    const a = this.action;
    const items = this.selectedItems();
    if (a?.kind === 'move') return items.map((it) => translateItem(it, a.dx, a.dy));
    if (a?.kind === 'resize') return items.map((it) => (it.id === a.item.id ? this.resized(it, a) : it));
    return items;
  }

  resized(it, a) {
    const minW = 40;
    let w = Math.max(minW, a.item.w + a.dx);
    let hh = Math.max(24, a.item.h + a.dy);
    if (it.type === 'image') hh = w * (a.item.h / a.item.w);
    if (it.type === 'math') { const k = w / a.item.w; return { ...it, w, h: a.item.h * k, size: Math.max(8, Math.round(a.item.size * k)) }; }
    if (it.type === 'text') { const n = { ...it, w }; n.h = textHeight(this.ctx, n); return n; }
    return { ...it, w, h: hh };
  }

  apply(op) {
    if (this.history.do(op)) {
      this.sel = new Set([...this.sel].filter((id) => this.page.items.some((it) => it.id === id)));
      this.markDirty();
    }
    this.updateHistoryButtons();
    this.requestRender();
  }

  undo() { this.commitInline(); if (this.history.undo()) { this.clearSelection(); this.markDirty(); } this.updateHistoryButtons(); this.requestRender(); }
  redo() { this.commitInline(); if (this.history.redo()) { this.clearSelection(); this.markDirty(); } this.updateHistoryButtons(); this.requestRender(); }

  markDirty() {
    this.dirty = true;
    this.page.updated = Date.now();
    this.status.textContent = t('saving');
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => this.flush(), 400);
  }

  async flush() {
    clearTimeout(this.saveTimer);
    if (!this.dirty) return;
    this.dirty = false;
    try {
      await this.saveFn(this.page);
      this.status.textContent = t('saved');
    } catch (e) {
      console.error(e);
      this.status.textContent = '⚠';
      this.dirty = true;
    }
  }

  hitTest(wx, wy) {
    const tol = Math.max(6 / this.view.s, 3);
    for (let i = this.page.items.length - 1; i >= 0; i--) {
      const it = this.page.items[i];
      if (it.type === 'stroke') {
        const pts = it.interp?.show ? interpPoints(it.interp) : it.points;
        if (hitPolyline(pts, [wx, wy], tol + it.width / 2)) return it;
      } else {
        const b = itemBounds(it);
        if (wx >= b.x && wx <= b.x + b.w && wy >= b.y && wy <= b.y + b.h) return it;
      }
    }
    return null;
  }

  clearSelection() {
    if (!this.sel.size) return;
    this.sel.clear();
    this.requestRender();
  }

  // ---------- input ----------
  onpointerdown(e) {
    if (this.inline) { this.commitInline(); return; }
    document.querySelectorAll('.menu').forEach((m) => m.remove());
    this.stage.focus({ preventScroll: true });
    try { this.stage.setPointerCapture(e.pointerId); } catch {}
    const [sx, sy] = this.localXY(e);
    this.pointers.set(e.pointerId, { x: sx, y: sy, type: e.pointerType });
    if (e.pointerType === 'pen' && !this.settings.penSeen) {
      this.settings.penSeen = true;
      this.settings.fingerDraw = false;
      this.saveSettings();
    }
    const touches = [...this.pointers.values()].filter((p) => p.type === 'touch');
    if (e.pointerType === 'touch') {
      const navOnly = this.settings.penSeen && !this.settings.fingerDraw;
      if (touches.length >= 2 || navOnly || this.tool === 'hand') {
        if (this.action && this.action.kind !== 'gesture' && this.action.pointerType === 'touch') this.cancelAction();
        if (!this.action || this.action.kind === 'gesture') this.startGesture();
        return;
      }
    }
    if (e.pointerType === 'mouse' && (e.button === 1 || e.button === 2 || this.spaceDown || this.tool === 'hand')) {
      this.action = { kind: 'pan', pointerId: e.pointerId, last: [sx, sy] };
      return;
    }
    if (e.pointerType === 'mouse' && e.button !== 0) return;
    if (this.action) return;
    this.startTool(e, sx, sy);
  }

  startGesture() {
    this.action = { kind: 'gesture', snap: this.gestureSnapshot() };
  }

  gestureSnapshot() {
    const ts = [...this.pointers.entries()].filter(([, p]) => p.type === 'touch' || this.tool === 'hand' || p.type === 'mouse');
    if (!ts.length) return null;
    const cx = ts.reduce((s, [, p]) => s + p.x, 0) / ts.length;
    const cy = ts.reduce((s, [, p]) => s + p.y, 0) / ts.length;
    const d = ts.length >= 2 ? Math.hypot(ts[0][1].x - ts[1][1].x, ts[0][1].y - ts[1][1].y) : 0;
    return { cx, cy, d, n: ts.length };
  }

  startTool(e, sx, sy) {
    const [wx, wy] = this.toWorld(sx, sy);
    const pressure = e.pointerType === 'pen' ? e.pressure || 0.5 : 0.5;
    const base = { pointerId: e.pointerId, pointerType: e.pointerType };
    switch (this.tool) {
      case 'pen':
      case 'highlighter': {
        this.action = { ...base, kind: 'ink', pts: [[wx, wy, pressure]], lastScreen: [sx, sy], interp: null };
        this.clearWet();
        this.armHold();
        break;
      }
      case 'eraser': {
        this.action = { ...base, kind: 'erase', last: [wx, wy], removed: new Set() };
        this.eraseSegment([wx, wy], [wx, wy]);
        break;
      }
      case 'select': {
        const b = unionBounds(this.selectedItems());
        const pad = 10 / this.view.s;
        const items = this.selectedItems();
        if (items.length === 1 && b && ['image', 'plot', 'text', 'math'].includes(items[0].type)
          && Math.hypot(wx - (b.x + b.w + 4 / this.view.s), wy - (b.y + b.h + 4 / this.view.s)) < 16 / this.view.s) {
          this.action = { ...base, kind: 'resize', start: [wx, wy], dx: 0, dy: 0, item: { ...items[0] } };
          break;
        }
        if (b && wx >= b.x - pad && wx <= b.x + b.w + pad && wy >= b.y - pad && wy <= b.y + b.h + pad) {
          this.action = { ...base, kind: 'move', start: [wx, wy], dx: 0, dy: 0 };
          break;
        }
        const hit = this.hitTest(wx, wy);
        if (hit) {
          this.sel = new Set([hit.id]);
          this.action = { ...base, kind: 'move', start: [wx, wy], dx: 0, dy: 0 };
        } else {
          this.sel.clear();
          this.action = { ...base, kind: 'marquee', start: [wx, wy], cur: [wx, wy] };
        }
        this.requestRender();
        break;
      }
      case 'text': {
        const hit = this.hitTest(wx, wy);
        if (hit?.type === 'text') this.editText(hit);
        else this.editText(null, wx, wy);
        break;
      }
      case 'math': {
        const hit = this.hitTest(wx, wy);
        this.editMath(hit?.type === 'math' ? hit : null, wx, wy);
        break;
      }
      case 'plot': {
        const hit = this.hitTest(wx, wy);
        this.editPlot(hit?.type === 'plot' ? hit : null, wx, wy);
        break;
      }
    }
  }

  onpointermove(e) {
    const p = this.pointers.get(e.pointerId);
    const [sx, sy] = this.localXY(e);
    if (p) { p.x = sx; p.y = sy; }
    const a = this.action;
    if (!a) return;
    if (a.kind === 'gesture') return this.moveGesture();
    if (a.pointerId !== e.pointerId) return;
    if (a.kind === 'pan') {
      this.userMoved = true;
      this.view.ox += sx - a.last[0];
      this.view.oy += sy - a.last[1];
      a.last = [sx, sy];
      return this.requestRender();
    }
    const events = e.getCoalescedEvents ? e.getCoalescedEvents() : [e];
    if (a.kind === 'ink') {
      const isHl = this.tool === 'highlighter';
      const color = this.currentColor(), width = this.currentWidth();
      this.prepWet();
      for (const ce of events.length ? events : [e]) {
        const [cx, cy] = this.localXY(ce);
        const [wx, wy] = this.toWorld(cx, cy);
        const last = a.pts[a.pts.length - 1];
        if (Math.hypot(wx - last[0], wy - last[1]) * this.view.s < 0.6) continue;
        const pr = e.pointerType === 'pen' ? (ce.pressure || 0.5) : 0.5;
        const pt = [wx, wy, last[2] * 0.6 + pr * 0.4];
        a.pts.push(pt);
        if (!a.interp) drawWetSegment(this.wctx, last, pt, width, color, isHl);
      }
      if (Math.hypot(sx - a.lastScreen[0], sy - a.lastScreen[1]) > 4) {
        a.lastScreen = [sx, sy];
        if (a.interp) { a.interp = null; this.redrawWetInk(); }
        this.armHold();
      }
      return;
    }
    const [wx, wy] = this.toWorld(sx, sy);
    if (a.kind === 'erase') {
      this.eraseSegment(a.last, [wx, wy]);
      a.last = [wx, wy];
      this.drawEraserCursor(sx, sy);
    } else if (a.kind === 'move' || a.kind === 'resize') {
      a.dx = wx - a.start[0];
      a.dy = wy - a.start[1];
      this.requestRender();
    } else if (a.kind === 'marquee') {
      a.cur = [wx, wy];
      this.requestRender();
    }
  }

  moveGesture() {
    const a = this.action;
    const now = this.gestureSnapshot();
    if (!now || !a.snap) { a.snap = now; return; }
    if (now.n !== a.snap.n) { a.snap = now; return; }
    this.userMoved = true;
    this.view.ox += now.cx - a.snap.cx;
    this.view.oy += now.cy - a.snap.cy;
    if (now.n >= 2 && a.snap.d > 0 && now.d > 0) this.zoomAt(now.cx, now.cy, now.d / a.snap.d);
    a.snap = now;
    this.requestRender();
  }

  onpointerup(e) {
    this.pointers.delete(e.pointerId);
    const a = this.action;
    if (!a) return;
    if (a.kind === 'gesture') {
      if (![...this.pointers.values()].some((p) => p.type === 'touch' || this.tool === 'hand')) this.action = null;
      else a.snap = this.gestureSnapshot();
      return;
    }
    if (a.pointerId !== e.pointerId) return;
    this.action = null;
    clearTimeout(this.holdTimer);
    if (a.kind === 'ink') this.finishInk(a);
    else if (a.kind === 'erase') {
      this.clearWet();
      const ids = [...a.removed];
      this.hidden.clear();
      if (ids.length) this.apply(opRemove(this.page, ids));
      else this.requestRender();
    } else if (a.kind === 'move') {
      if (Math.hypot(a.dx, a.dy) * this.view.s > 1) this.apply(opUpdate(this.page, this.selectedItems().map((it) => translateItem(it, a.dx, a.dy))));
      else this.requestRender();
    } else if (a.kind === 'resize') {
      this.apply(opUpdate(this.page, this.selectedItems().filter((it) => it.id === a.item.id).map((it) => this.resized(it, a))));
    } else if (a.kind === 'marquee') {
      const r = normRect(a.start, a.cur);
      this.sel = new Set(this.page.items.filter((it) => rectsIntersect(itemBounds(it), r)).map((it) => it.id));
      this.requestRender();
    }
  }

  onpointercancel(e) {
    this.pointers.delete(e.pointerId);
    if (this.action && (this.action.pointerId === e.pointerId || this.action.kind === 'gesture')) this.cancelAction();
  }

  cancelAction() {
    clearTimeout(this.holdTimer);
    this.action = null;
    this.hidden.clear();
    this.clearWet();
    this.requestRender();
  }

  onWheel(e) {
    e.preventDefault();
    const [sx, sy] = this.localXY(e);
    if (e.ctrlKey || e.metaKey) this.zoomAt(sx, sy, Math.exp(-e.deltaY * 0.01));
    else {
      this.userMoved = true;
      this.view.ox -= e.shiftKey ? e.deltaY : e.deltaX;
      this.view.oy -= e.shiftKey ? 0 : e.deltaY;
      this.requestRender();
    }
  }

  onDouble(e) {
    const [sx, sy] = this.localXY(e);
    const [wx, wy] = this.toWorld(sx, sy);
    const hit = this.hitTest(wx, wy);
    if (!hit) return;
    if (hit.type === 'text') this.editText(hit);
    else if (hit.type === 'math') this.editMath(hit);
    else if (hit.type === 'plot') this.editPlot(hit);
  }

  onKey(e) {
    const tag = e.target?.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || document.querySelector('.modal-back')) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'z') { e.preventDefault(); e.shiftKey ? this.redo() : this.undo(); return; }
    if (mod && e.key.toLowerCase() === 'y') { e.preventDefault(); this.redo(); return; }
    if (mod && e.key.toLowerCase() === 'd' && this.sel.size) { e.preventDefault(); this.duplicateSelection(); return; }
    if (mod) return;
    if ((e.key === 'Delete' || e.key === 'Backspace') && this.sel.size) { e.preventDefault(); this.apply(opRemove(this.page, [...this.sel])); return; }
    if (e.key === 'Escape') { this.clearSelection(); return; }
    if (e.code === 'Space') { this.spaceDown = true; e.preventDefault(); return; }
    const keys = { p: 'pen', h: 'highlighter', e: 'eraser', v: 'select', s: 'select', t: 'text' };
    if (this.settings.mode === 'advanced') Object.assign(keys, { m: 'math', g: 'plot' });
    if (keys[e.key]) { this.setTool(keys[e.key]); return; }
    if (e.key === '+' || e.key === '=') this.zoomAt(this.W / 2, this.H / 2, 1.2);
    if (e.key === '-') this.zoomAt(this.W / 2, this.H / 2, 1 / 1.2);
    if (e.key === '0') this.fit(true);
  }

  // ---------- ink ----------
  prepWet() {
    const { dpr, view } = this;
    this.wctx.setTransform(dpr * view.s, 0, 0, dpr * view.s, dpr * view.ox, dpr * view.oy);
  }
  clearWet() {
    this.wctx.setTransform(1, 0, 0, 1, 0, 0);
    this.wctx.clearRect(0, 0, this.wet.width, this.wet.height);
  }

  redrawWetInk() {
    const a = this.action;
    this.clearWet();
    this.prepWet();
    const color = this.currentColor(), width = this.currentWidth();
    if (a.interp) {
      drawStroke(this.wctx, { type: 'stroke', tool: this.tool, color, width, points: a.pts, interp: { ...a.interp, show: true } });
    } else {
      for (let i = 1; i < a.pts.length; i++) drawWetSegment(this.wctx, a.pts[i - 1], a.pts[i], width, color, this.tool === 'highlighter');
      if (a.pts.length === 1) drawWetSegment(this.wctx, a.pts[0], a.pts[0], width, color, this.tool === 'highlighter');
    }
  }

  // Hold the pen still for 550 ms at the end of a stroke to snap it into a shape.
  armHold() {
    clearTimeout(this.holdTimer);
    this.holdTimer = setTimeout(() => {
      const a = this.action;
      if (!a || a.kind !== 'ink' || a.pts.length < 4) return;
      const r = recognize(a.pts);
      if (r) { a.interp = r; this.redrawWetInk(); navigator.vibrate?.(8); }
    }, 550);
  }

  finishInk(a) {
    let interp = a.interp;
    if (!interp && this.settings.autoShapes && this.settings.mode === 'advanced' && a.pts.length >= 4) interp = recognize(a.pts);
    const item = {
      id: uid(), type: 'stroke', tool: this.tool, color: this.currentColor(), width: this.currentWidth(),
      points: finishPoints(a.pts, this.view.s),
    };
    if (interp) item.interp = { ...interp, show: true };
    this.apply(opAdd(this.page, [item]));
    // Clear the wet layer once the committed stroke is on the main canvas.
    requestAnimationFrame(() => { if (!this.action) this.clearWet(); });
  }

  eraseSegment(a, b) {
    const r = this.currentWidth() / 2 / this.view.s;
    const act = this.action;
    for (const it of this.page.items) {
      if (act.removed.has(it.id)) continue;
      let hit = false;
      if (it.type === 'stroke') {
        const pts = it.interp?.show ? interpPoints(it.interp) : it.points;
        hit = segmentHitsPolyline(pts, a, b, r + it.width / 2);
      } else {
        const bb = itemBounds(it);
        hit = b[0] >= bb.x && b[0] <= bb.x + bb.w && b[1] >= bb.y && b[1] <= bb.y + bb.h;
      }
      if (hit) { act.removed.add(it.id); this.hidden.add(it.id); }
    }
    this.requestRender();
  }

  drawEraserCursor(sx, sy) {
    this.clearWet();
    const c = this.wctx;
    c.setTransform(this.dpr, 0, 0, this.dpr, 0, 0);
    c.strokeStyle = 'rgba(30,30,30,0.5)';
    c.lineWidth = 1;
    c.beginPath();
    c.arc(sx, sy, this.currentWidth() / 2, 0, Math.PI * 2);
    c.stroke();
  }

  // ---------- selection bar ----------
  positionSelectionBar() {
    const items = this.action ? [] : this.selectedItems();
    if (!items.length || this.tool !== 'select') { this.selBar?.remove(); this.selBar = null; this.selKey = ''; return; }
    const key = items.map((i) => i.id + (i.interp?.show ? '1' : '0')).join(',');
    if (this.selKey !== key) {
      this.selBar?.remove();
      this.selKey = key;
      const btns = [
        h('button', { class: 'icon-btn', title: t('delete'), 'aria-label': t('delete'), html: icon('trash'), onclick: () => this.apply(opRemove(this.page, [...this.sel])) }),
        h('button', { class: 'icon-btn', title: t('duplicate'), 'aria-label': t('duplicate'), html: icon('copy'), onclick: () => this.duplicateSelection() }),
      ];
      const one = items.length === 1 ? items[0] : null;
      if (one && ['text', 'math', 'plot'].includes(one.type)) {
        btns.push(h('button', { class: 'icon-btn', title: t('edit'), 'aria-label': t('edit'), html: icon('edit'),
          onclick: () => (one.type === 'text' ? this.editText(one) : one.type === 'math' ? this.editMath(one) : this.editPlot(one)) }));
      }
      const strokes = items.filter((i) => i.type === 'stroke');
      if (strokes.length) {
        const anyShown = strokes.some((s) => s.interp?.show);
        btns.push(h('button', { class: 'chip', onclick: () => this.toggleShapes(strokes, !anyShown) }, anyShown ? t('showRaw') : t('showShape')));
      }
      this.selBar = h('div', { class: 'sel-bar' }, btns);
      this.overlay.appendChild(this.selBar);
    }
    const b = unionBounds(items);
    const [sx, sy] = this.toScreen(b.x + b.w / 2, b.y);
    const bw = this.selBar.offsetWidth || 120;
    this.selBar.style.left = Math.max(8, Math.min(this.W - bw - 8, sx - bw / 2)) + 'px';
    this.selBar.style.top = Math.max(8, sy - 56) + 'px';
  }

  toggleShapes(strokes, show) {
    const updated = [];
    for (const s of strokes) {
      if (show) {
        const interp = s.interp || recognize(s.points);
        if (interp) updated.push({ ...s, interp: { ...interp, show: true } });
      } else if (s.interp) updated.push({ ...s, interp: { ...s.interp, show: false } });
    }
    if (!updated.length) { toast('—'); return; }
    this.apply(opUpdate(this.page, updated));
  }

  duplicateSelection() {
    const copies = this.selectedItems().map((it) => ({ ...translateItem(it, 24, 24), id: uid() }));
    this.apply(opAdd(this.page, copies));
    this.sel = new Set(copies.map((c) => c.id));
    this.selKey = '';
    this.requestRender();
  }

  // ---------- inline editors ----------
  commitInline() {
    if (this.inline) { const f = this.inline.commit; this.inline = null; f(); }
  }

  editText(item, wx, wy) {
    this.commitInline();
    const paper = pageSize(this.page);
    const base = item || {
      id: uid(), type: 'text', x: wx, y: wy - 12, size: 20, color: this.settings.color || COLORS[0], text: '',
      w: paper ? Math.max(160, Math.min(520, paper.w - wx - 40)) : 420, h: 28,
    };
    if (item) { this.hidden.add(item.id); this.requestRender(); }
    const ta = h('textarea', { class: 'inline-text', placeholder: t('textPrompt'), 'aria-label': t('text') });
    ta.value = base.text;
    const place = () => {
      const [sx, sy] = this.toScreen(base.x, base.y);
      Object.assign(ta.style, {
        left: sx + 'px', top: sy + 'px', width: base.w * this.view.s + 'px',
        fontSize: base.size * this.view.s + 'px', color: base.color,
      });
      ta.style.height = 'auto';
      ta.style.height = Math.max(base.size * 1.5 * this.view.s, ta.scrollHeight) + 'px';
    };
    ta.addEventListener('input', place);
    ta.addEventListener('keydown', (e) => { if (e.key === 'Escape') { e.preventDefault(); this.commitInline(); } e.stopPropagation(); });
    ta.addEventListener('blur', () => setTimeout(() => { if (this.inline?.el === ta) this.commitInline(); }, 0));
    this.overlay.appendChild(ta);
    place();
    ta.focus();
    this.inline = {
      el: ta,
      commit: () => {
        ta.remove();
        this.hidden.delete(base.id);
        const text = ta.value.replace(/\s+$/, '');
        if (!text) { if (item) this.apply(opRemove(this.page, [item.id])); else this.requestRender(); return; }
        const next = { ...base, text };
        next.h = textHeight(this.ctx, next);
        this.apply(item ? opUpdate(this.page, [next]) : opAdd(this.page, [next]));
      },
    };
  }

  async editMath(item, wx, wy) {
    this.commitInline();
    const input = h('textarea', { class: 'field mono', rows: 3, placeholder: t('mathPrompt'), 'aria-label': t('math'), spellcheck: 'false' });
    input.value = item?.latex || '';
    const preview = h('div', { class: 'math-preview', 'aria-live': 'polite' });
    let seq = 0;
    const update = async () => {
      const my = ++seq;
      try {
        const { svg } = await texToSvg(input.value || '\\square', 30, '#1d1d1f');
        if (my === seq) preview.innerHTML = svg;
      } catch { preview.textContent = '…'; }
    };
    input.addEventListener('input', update);
    const snippets = h('div', { class: 'snippets' }, MATH_SNIPPETS.map((s) => h('button', {
      class: 'chip mono', onclick: () => {
        const a = input.selectionStart, b = input.selectionEnd;
        input.value = input.value.slice(0, a) + s + input.value.slice(b);
        input.focus();
        input.selectionStart = input.selectionEnd = a + s.length;
        update();
      },
    }, s)));
    update();
    const latex = await modal(t('math'), h('div', { class: 'math-editor' }, input, snippets, preview), [
      { label: t('cancel'), value: null },
      { label: t('ok'), value: () => input.value.trim(), primary: true },
    ]);
    if (latex === null) return;
    if (!latex) { if (item) this.apply(opRemove(this.page, [item.id])); return; }
    const size = item?.size || 28;
    const color = item?.color || this.settings.color || COLORS[0];
    const { w, h: hh } = await texToSvg(latex, size, color);
    const next = item ? { ...item, latex, w, h: hh } : { id: uid(), type: 'math', latex, size, color, x: wx, y: wy - hh / 2, w, h: hh };
    this.apply(item ? opUpdate(this.page, [next]) : opAdd(this.page, [next]));
  }

  async editPlot(item, wx, wy) {
    this.commitInline();
    const exprs = h('textarea', { class: 'field mono', rows: 3, placeholder: t('plotExprs'), 'aria-label': t('plotExprs'), spellcheck: 'false' });
    exprs.value = (item?.exprs || ['y = x^2 - 2']).join('\n');
    const num = (v, label) => h('label', { class: 'num' }, label, h('input', { class: 'field', type: 'number', step: 'any', value: v, 'aria-label': label }));
    const fx0 = num(item?.x0 ?? -5, 'x min'), fx1 = num(item?.x1 ?? 5, 'x max'), fy0 = num(item?.y0 ?? -4, 'y min'), fy1 = num(item?.y1 ?? 4, 'y max');
    const val = (l) => Number(l.querySelector('input').value);
    const res = await modal(t('plotTitle'), h('div', { class: 'plot-editor' }, exprs, h('div', { class: 'range' }, h('span', {}, t('range')), fx0, fx1, fy0, fy1)), [
      { label: t('cancel'), value: null },
      { label: t('ok'), value: () => ({ exprs: exprs.value.split('\n').map((s) => s.trim()).filter(Boolean), x0: val(fx0), x1: val(fx1), y0: val(fy0), y1: val(fy1) }), primary: true },
    ]);
    if (!res) return;
    if (!(res.x1 > res.x0)) [res.x0, res.x1] = [-5, 5];
    if (!(res.y1 > res.y0)) [res.y0, res.y1] = [-4, 4];
    if (!res.exprs.length) { if (item) this.apply(opRemove(this.page, [item.id])); return; }
    const next = item ? { ...item, ...res } : { id: uid(), type: 'plot', x: wx, y: wy, w: 420, h: 300, ...res };
    this.apply(item ? opUpdate(this.page, [next]) : opAdd(this.page, [next]));
  }

  async insertImage() {
    const file = await pickFile('image/*');
    if (!file) return;
    const src = await downscaleImage(file, 1600);
    const img = new Image();
    img.src = src;
    await img.decode().catch(() => {});
    const maxW = 480;
    const k = Math.min(1, maxW / (img.naturalWidth || maxW));
    const w = (img.naturalWidth || 400) * k, hh = (img.naturalHeight || 300) * k;
    const [cx, cy] = this.toWorld(this.W / 2, this.H / 2);
    const item = { id: uid(), type: 'image', src, x: cx - w / 2, y: cy - hh / 2, w, h: hh };
    this.apply(opAdd(this.page, [item]));
    this.setTool('select');
    this.sel = new Set([item.id]);
    this.requestRender();
  }
}

function normRect(a, b) {
  return { x: Math.min(a[0], b[0]), y: Math.min(a[1], b[1]), w: Math.abs(a[0] - b[0]), h: Math.abs(a[1] - b[1]) };
}

// Re-encode large photos so pages stay light in storage.
export async function downscaleImage(file, max) {
  const src = await readAsDataURL(file);
  if (file.type === 'image/svg+xml') return src;
  const img = new Image();
  img.src = src;
  try { await img.decode(); } catch { return src; }
  const k = Math.min(1, max / Math.max(img.naturalWidth, img.naturalHeight));
  if (k === 1 && file.size < 600_000) return src;
  const c = document.createElement('canvas');
  c.width = Math.round(img.naturalWidth * k);
  c.height = Math.round(img.naturalHeight * k);
  c.getContext('2d').drawImage(img, 0, 0, c.width, c.height);
  return c.toDataURL(file.type === 'image/png' ? 'image/png' : 'image/jpeg', 0.86);
}
