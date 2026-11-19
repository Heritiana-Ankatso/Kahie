// Library search over notebook titles, page titles, typed text, maths and plots.

export function fold(s) {
  return String(s || '').normalize('NFD').replace(/[̀-ͯ]/g, '').toLowerCase();
}

function itemText(item) {
  if (item.type === 'text') return item.text;
  if (item.type === 'math') return item.latex;
  if (item.type === 'plot') return (item.exprs || []).join(' ');
  if (item.type === 'stroke' && item.interp?.label) return item.interp.label;
  return '';
}

function snippet(text, idx, len) {
  const start = Math.max(0, idx - 30);
  const end = Math.min(text.length, idx + len + 30);
  return (start > 0 ? '…' : '') + text.slice(start, end) + (end < text.length ? '…' : '');
}

export function search(notebooks, pages, query) {
  const q = fold(query).trim();
  if (!q) return [];
  const terms = q.split(/\s+/);
  const nbById = new Map(notebooks.map((n) => [n.id, n]));
  const results = [];
  for (const nb of notebooks) {
    if (terms.every((t) => fold(nb.title).includes(t))) results.push({ kind: 'notebook', notebook: nb, score: 3, text: nb.title });
  }
  for (const page of pages) {
    const nb = nbById.get(page.notebookId);
    if (!nb) continue;
    const parts = [page.title, ...page.items.map(itemText)].filter(Boolean);
    const all = fold(parts.join('\n'));
    if (!terms.every((t) => all.includes(t))) continue;
    let best = '';
    for (const part of parts) {
      const f = fold(part);
      const idx = f.indexOf(terms[0]);
      if (idx >= 0) { best = snippet(part, idx, terms[0].length); break; }
    }
    const score = terms.every((t) => fold(page.title).includes(t)) ? 2 : 1;
    results.push({ kind: 'page', notebook: nb, page, score, text: best });
  }
  return results.sort((a, b) => b.score - a.score || (b.page?.updated || 0) - (a.page?.updated || 0));
}
