// Small DOM helpers: element builder, modal dialogs, toasts, menus.

import { t } from './i18n.js';

export function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === undefined || v === null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === 'html') el.innerHTML = v;
    else if (k === 'style' && typeof v === 'object') for (const [p, val] of Object.entries(v)) p.startsWith('--') ? el.style.setProperty(p, val) : (el.style[p] = val);
    else if (v === true) el.setAttribute(k, '');
    else el.setAttribute(k, v);
  }
  for (const c of children.flat()) {
    if (c === null || c === undefined || c === false) continue;
    el.append(c instanceof Node ? c : document.createTextNode(String(c)));
  }
  return el;
}

// Generic modal. `body` is a node; buttons: [{ label, value, primary, danger }].
export function modal(title, body, buttons = [{ label: t('ok'), value: true, primary: true }]) {
  return new Promise((resolve) => {
    const prev = document.activeElement;
    const close = (v) => { back.remove(); document.removeEventListener('keydown', onKey, true); prev?.focus?.(); resolve(v); };
    const onKey = (e) => {
      if (e.key === 'Escape') { e.stopPropagation(); close(null); }
      if (e.key === 'Enter' && !(e.target instanceof HTMLTextAreaElement)) {
        const p = buttons.find((b) => b.primary);
        if (p) { e.preventDefault(); e.stopPropagation(); close(typeof p.value === 'function' ? p.value() : p.value); }
      }
    };
    const box = h('div', { class: 'modal', role: 'dialog', 'aria-modal': 'true', 'aria-label': title },
      title ? h('h2', {}, title) : null,
      body,
      h('div', { class: 'modal-actions' }, buttons.map((b) => h('button', {
        class: 'btn' + (b.primary ? ' primary' : '') + (b.danger ? ' danger' : ''),
        onclick: () => close(typeof b.value === 'function' ? b.value() : b.value),
      }, b.label))));
    const back = h('div', { class: 'modal-back', onpointerdown: (e) => { if (e.target === back) close(null); } }, box);
    document.body.appendChild(back);
    document.addEventListener('keydown', onKey, true);
    setTimeout(() => (box.querySelector('input,textarea,select') || box.querySelector('.btn.primary'))?.focus(), 20);
  });
}

export async function promptText(title, value = '', placeholder = '') {
  const input = h('input', { class: 'field', value, placeholder, 'aria-label': title });
  const v = await modal(title, input, [
    { label: t('cancel'), value: null },
    { label: t('ok'), value: () => input.value, primary: true },
  ]);
  return v === null ? null : v.trim();
}

export function confirmBox(text, danger = true) {
  return modal('', h('p', {}, text), [
    { label: t('cancel'), value: false },
    { label: danger ? t('delete') : t('ok'), value: true, primary: true, danger },
  ]);
}

let toastTimer;
export function toast(text, ms = 2600) {
  let el = document.getElementById('toast');
  if (!el) { el = h('div', { id: 'toast', role: 'status', 'aria-live': 'polite' }); document.body.appendChild(el); }
  el.textContent = text;
  el.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => el.classList.remove('show'), ms);
}

// Popover menu anchored to a button. items: [{ label, onClick, checked, sep, header }]
export function menu(anchor, items) {
  document.querySelectorAll('.menu').forEach((m) => m.remove());
  const el = h('div', { class: 'menu', role: 'menu' }, items.map((it) => {
    if (it.sep) return h('div', { class: 'menu-sep' });
    if (it.header) return h('div', { class: 'menu-header' }, it.header);
    return h('button', {
      class: 'menu-item' + (it.checked ? ' checked' : ''), role: 'menuitem',
      onclick: () => { el.remove(); it.onClick(); },
    }, h('span', { class: 'menu-check' }, it.checked ? '✓' : ''), it.label);
  }));
  document.body.appendChild(el);
  const r = anchor.getBoundingClientRect();
  const w = el.offsetWidth, hh = el.offsetHeight;
  el.style.left = Math.max(8, Math.min(window.innerWidth - w - 8, r.right - w)) + 'px';
  el.style.top = Math.min(window.innerHeight - hh - 8, r.bottom + 6) + 'px';
  const off = (e) => { if (!el.contains(e.target)) { el.remove(); document.removeEventListener('pointerdown', off, true); } };
  setTimeout(() => document.addEventListener('pointerdown', off, true), 0);
  el.querySelector('button')?.focus();
  return el;
}

export function pickFile(accept, multiple = false) {
  return new Promise((resolve) => {
    const input = h('input', { type: 'file', accept, style: { display: 'none' } });
    if (multiple) input.multiple = true;
    input.addEventListener('change', () => { resolve(multiple ? [...input.files] : input.files[0] || null); input.remove(); });
    document.body.appendChild(input);
    input.click();
  });
}

export function readAsDataURL(file) {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result);
    r.onerror = () => reject(r.error);
    r.readAsDataURL(file);
  });
}

export function timeAgo(ts) {
  const d = new Date(ts);
  return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' }) + ' ' + d.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}
