import test from 'node:test';
import assert from 'node:assert/strict';
import { newNotebook, newPage, History, opAdd, opRemove, opUpdate, translateItem, itemBounds, normaliseImport } from '../src/model.js';
import { outline, simplify, finishPoints, hitPolyline } from '../src/ink.js';
import { recognize } from '../src/shapes.js';
import { compile, parse, sample, ParseError, tickStep } from '../src/expr.js';
import { search, fold } from '../src/search.js';

const stroke = (id, points) => ({ id, type: 'stroke', tool: 'pen', color: '#000', width: 3, points });

test('history: add, remove, update round-trip through undo/redo', () => {
  const page = newPage('nb');
  const h = new History(page);
  h.do(opAdd(page, [stroke('a', [[0, 0, 0.5], [10, 0, 0.5]]), stroke('b', [[0, 5, 0.5]])]));
  assert.deepEqual(page.items.map((i) => i.id), ['a', 'b']);
  h.do(opRemove(page, ['a']));
  assert.deepEqual(page.items.map((i) => i.id), ['b']);
  h.undo();
  assert.deepEqual(page.items.map((i) => i.id), ['a', 'b'], 'removed item restored at its index');
  h.do(opUpdate(page, [translateItem(page.items[1], 5, 5)]));
  assert.equal(page.items[1].points[0][0], 5);
  h.undo();
  assert.equal(page.items[1].points[0][0], 0);
  h.redo();
  assert.equal(page.items[1].points[0][0], 5);
  assert.equal(h.canRedo, false);
  h.undo(); h.undo(); h.undo();
  assert.equal(page.items.length, 0);
  assert.equal(h.canUndo, false);
});

test('history: empty ops are ignored and a new op clears redo', () => {
  const page = newPage('nb');
  const h = new History(page);
  assert.equal(h.do(opRemove(page, ['missing'])), false);
  h.do(opAdd(page, [stroke('a', [[0, 0]])]));
  h.undo();
  assert.equal(h.canRedo, true);
  h.do(opAdd(page, [stroke('b', [[0, 0]])]));
  assert.equal(h.canRedo, false);
});

test('raw ink preserved when an interpretation is translated', () => {
  const s = { ...stroke('a', [[0, 0, 0.5], [100, 0, 0.5]]), interp: { kind: 'line', a: [0, 0], b: [100, 0], show: true } };
  const t = translateItem(s, 10, 20);
  assert.deepEqual(t.points[1], [110, 20, 0.5]);
  assert.deepEqual(t.interp.b, [110, 20]);
  assert.deepEqual(s.points[1], [100, 0, 0.5], 'original untouched');
  const b = itemBounds(t);
  assert.ok(b.x < 10 && b.x + b.w > 110);
});

test('ink: outline is closed polygon around the stroke, simplify keeps endpoints', () => {
  const pts = Array.from({ length: 50 }, (_, i) => [i * 2, Math.sin(i / 5) * 10, 0.5]);
  const poly = outline(pts, 4);
  assert.ok(poly.length > pts.length * 2);
  const s = simplify(pts, 0.5);
  assert.deepEqual(s[0], pts[0]);
  assert.deepEqual(s[s.length - 1], pts[pts.length - 1]);
  assert.ok(s.length < pts.length);
  assert.ok(finishPoints(pts).length <= pts.length);
  assert.ok(hitPolyline(pts, [20, Math.sin(2) * 10 + 1], 2));
  assert.ok(!hitPolyline(pts, [20, 50], 2));
});

function jitter(points, amp = 1.5, seed = 1) {
  let s = seed;
  const rnd = () => ((s = (s * 16807) % 2147483647) / 2147483647 - 0.5) * 2 * amp;
  return points.map(([x, y]) => [x + rnd(), y + rnd(), 0.5]);
}
function along(path, per = 20) {
  const out = [];
  for (let i = 1; i < path.length; i++) {
    for (let k = 0; k < per; k++) {
      const t = k / per;
      out.push([path[i - 1][0] + (path[i][0] - path[i - 1][0]) * t, path[i - 1][1] + (path[i][1] - path[i - 1][1]) * t]);
    }
  }
  out.push(path[path.length - 1]);
  return out;
}

test('shapes: line, snapped horizontal', () => {
  const r = recognize(jitter(along([[0, 0], [200, 4]])));
  assert.equal(r.kind, 'line');
  assert.ok(Math.abs(r.a[1] - r.b[1]) < 1e-9, 'snapped horizontal');
});

test('shapes: circle and ellipse', () => {
  const circle = jitter(Array.from({ length: 80 }, (_, i) => [100 + 60 * Math.cos(i / 79 * 2 * Math.PI), 100 + 60 * Math.sin(i / 79 * 2 * Math.PI)]));
  const c = recognize(circle);
  assert.equal(c.label, 'circle');
  assert.ok(Math.abs(c.rx - 60) < 6);
  const ell = jitter(Array.from({ length: 80 }, (_, i) => [100 + 100 * Math.cos(i / 79 * 2 * Math.PI), 100 + 40 * Math.sin(i / 79 * 2 * Math.PI)]));
  const e = recognize(ell);
  assert.equal(e.kind, 'ellipse');
  assert.ok(!e.label);
  assert.ok(Math.abs(e.rx - 100) < 12 && Math.abs(e.ry - 40) < 8);
});

test('shapes: rectangle and triangle', () => {
  const r = recognize(jitter(along([[0, 0], [200, 2], [198, 120], [-2, 118], [0, 3]])));
  assert.equal(r.label, 'rectangle');
  assert.equal(r.points.length, 4);
  assert.ok(Math.abs(r.points[0][1] - r.points[1][1]) < 1e-6, 'axis aligned');
  const t = recognize(jitter(along([[0, 0], [150, 0], [75, 120], [1, 2]])));
  assert.equal(t.label, 'triangle');
});

test('shapes: scribble is not recognised', () => {
  const pts = Array.from({ length: 120 }, (_, i) => [i * 3, 40 * Math.sin(i / 3) + (i % 7) * 5, 0.5]);
  assert.equal(recognize(pts), null);
});

test('expr: precedence, implicit multiplication, functions, constants', () => {
  const near = (a, b) => assert.ok(Math.abs(a - b) < 1e-12, `${a} != ${b}`);
  near(compile('2x^2 - 3x + 1')(2), 3);
  near(compile('-x^2')(3), -9);
  near(compile('y = sin(pi x)')(0.5), 1);
  near(compile('f(x) = 2^x^2')(2), 16, 'right associative');
  near(compile('sqrt(x)ln x')(Math.E), Math.sqrt(Math.E));
  near(compile('sin^2 x + cos^2 x')(1.3), 1);
  near(compile('x²+1')(3), 10);
  near(compile('1/(x-1)')(3), 0.5);
  near(compile('3e-2')(0), 0.03);
  near(compile('e^x')(1), Math.E);
  assert.throws(() => parse('2+'), ParseError);
  assert.throws(() => parse('foo(x)'), ParseError);
  assert.throws(() => parse('alert(1)'), ParseError);
});

test('expr: sampling splits at poles', () => {
  const lines = sample(compile('1/x'), -2, 2, -5, 5, 401);
  assert.ok(lines.length >= 2);
  assert.ok(tickStep(10) === 1 && tickStep(100) === 10 && tickStep(3) === 0.5);
});

test('search: accents folded, page text and maths found', () => {
  const nb = newNotebook('Géométrie');
  const p = newPage(nb.id, { title: 'Théorème de Pythagore' });
  p.items.push({ id: 't', type: 'text', text: 'Dans un triangle rectangle…', x: 0, y: 0, w: 100, h: 20 });
  p.items.push({ id: 'm', type: 'math', latex: 'a^2+b^2=c^2', x: 0, y: 0, w: 1, h: 1 });
  assert.equal(fold('Théorème'), 'theoreme');
  assert.equal(search([nb], [p], 'geometrie')[0].kind, 'notebook');
  assert.equal(search([nb], [p], 'pythagore')[0].page.id, p.id);
  assert.equal(search([nb], [p], 'triangle rectangle')[0].kind, 'page');
  assert.equal(search([nb], [p], 'b^2').length, 1);
  assert.equal(search([nb], [p], 'cosinus').length, 0);
});

test('import: ids are remapped, unknown notebooks dropped, bad input rejected', () => {
  const nb = newNotebook('A');
  const p = newPage(nb.id, { title: 'x', template: 'grid' });
  const orphan = newPage('nope');
  const out = normaliseImport({ notebooks: [nb], pages: [p, orphan] });
  assert.equal(out.notebooks.length, 1);
  assert.notEqual(out.notebooks[0].id, nb.id);
  assert.equal(out.pages.length, 1);
  assert.equal(out.pages[0].notebookId, out.notebooks[0].id);
  assert.equal(out.pages[0].template, 'grid');
  assert.throws(() => normaliseImport({}));
});

test('closed polygon interpretation includes its closing edge', async () => {
  const { interpPoints } = await import('../src/model.js');
  const pts = interpPoints({ kind: 'polygon', closed: true, points: [[0, 0], [10, 0], [10, 10], [0, 10]] });
  assert.ok(hitPolyline(pts, [0, 5], 1), 'left edge (closing segment) is hittable');
  assert.ok(!hitPolyline(interpPoints({ kind: 'polygon', closed: false, points: [[0, 0], [10, 0], [10, 10], [0, 10]] }), [0, 5], 1));
});

test('service worker precaches every app module and vendored file', async () => {
  const fs = await import('node:fs');
  const sw = fs.readFileSync(new URL('../sw.js', import.meta.url), 'utf8');
  const listed = new Set([...sw.matchAll(/'([^']+)'/g)].map((m) => m[1]));
  for (const f of fs.readdirSync(new URL('../src/', import.meta.url))) assert.ok(listed.has('src/' + f), `sw.js misses src/${f}`);
  for (const f of ['vendor/mathjax/tex-svg-full.js', 'vendor/pdfjs/pdf.min.mjs', 'vendor/pdfjs/pdf.worker.min.mjs']) {
    assert.ok(listed.has(f), `sw.js misses ${f}`);
    assert.ok(fs.existsSync(new URL('../' + f, import.meta.url)), `${f} missing on disk`);
  }
});
