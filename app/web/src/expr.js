// Safe maths expression parser for function plots (no eval).
// Grammar: expr := term (('+'|'-') term)* ; term := unary (('*'|'/'|implicit) unary)*
// unary := ('-'|'+') unary | power ; power := atom ('^' unary)?

const FUNCS = {
  sin: Math.sin, cos: Math.cos, tan: Math.tan,
  asin: Math.asin, acos: Math.acos, atan: Math.atan, arcsin: Math.asin, arccos: Math.acos, arctan: Math.atan,
  sinh: Math.sinh, cosh: Math.cosh, tanh: Math.tanh,
  exp: Math.exp, ln: Math.log, log: Math.log10, sqrt: Math.sqrt, abs: Math.abs,
  floor: Math.floor, ceil: Math.ceil, sign: Math.sign, cbrt: Math.cbrt,
};
const CONSTS = { pi: Math.PI, e: Math.E, π: Math.PI };

export class ParseError extends Error {}

function tokenize(src) {
  const toks = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (/\s/.test(c)) { i++; continue; }
    if (/[0-9.]/.test(c)) {
      let j = i;
      while (j < src.length && /[0-9.]/.test(src[j])) j++;
      if (src[j] === 'e' && /[0-9+-]/.test(src[j + 1] || '') && /[0-9]/.test(src[j + 2] || src[j + 1])) {
        j++;
        if (/[+-]/.test(src[j])) j++;
        while (j < src.length && /[0-9]/.test(src[j])) j++;
      }
      const v = Number(src.slice(i, j));
      if (!isFinite(v)) throw new ParseError('bad number ' + src.slice(i, j));
      toks.push({ t: 'num', v });
      i = j; continue;
    }
    if (/[a-zA-Zπ]/.test(c)) {
      let j = i;
      while (j < src.length && /[a-zA-Zπ]/.test(src[j])) j++;
      toks.push(...splitIdent(src.slice(i, j)));
      i = j; continue;
    }
    if ('+-*/^(),'.includes(c)) { toks.push({ t: c }); i++; continue; }
    if (c === '×' || c === '·') { toks.push({ t: '*' }); i++; continue; }
    if (c === '÷') { toks.push({ t: '/' }); i++; continue; }
    if (c === '²') { toks.push({ t: '^' }, { t: 'num', v: 2 }); i++; continue; }
    if (c === '³') { toks.push({ t: '^' }, { t: 'num', v: 3 }); i++; continue; }
    throw new ParseError('unexpected "' + c + '"');
  }
  return toks;
}

// "xsin" -> x, sin ; "pix" -> pi, x. Longest known name first.
function splitIdent(word) {
  const names = [...Object.keys(FUNCS), ...Object.keys(CONSTS), 'x'].sort((a, b) => b.length - a.length);
  const out = [];
  let i = 0;
  while (i < word.length) {
    const name = names.find((n) => word.startsWith(n, i));
    if (!name) throw new ParseError('unknown name "' + word.slice(i) + '"');
    out.push(name in FUNCS ? { t: 'fn', v: name } : { t: 'id', v: name });
    i += name.length;
  }
  return out;
}

export function parse(src) {
  let s = String(src).trim();
  // Accept "y = ...", "f(x) = ..."
  const m = s.match(/^(?:y|[a-zA-Z]\s*\(\s*x\s*\))\s*=(.*)$/);
  if (m) s = m[1];
  if (!s.trim()) throw new ParseError('empty expression');
  const toks = tokenize(s);
  let k = 0;
  const peek = () => toks[k];
  const eat = (t) => {
    if (toks[k]?.t !== t) throw new ParseError('expected ' + t);
    return toks[k++];
  };
  const startsAtom = (tok) => tok && (tok.t === 'num' || tok.t === 'id' || tok.t === 'fn' || tok.t === '(');

  function expr() {
    let node = term();
    while (peek() && (peek().t === '+' || peek().t === '-')) {
      const op = toks[k++].t;
      node = { op, a: node, b: term() };
    }
    return node;
  }
  function term() {
    let node = unary();
    for (;;) {
      const p = peek();
      if (p && (p.t === '*' || p.t === '/')) { k++; node = { op: p.t, a: node, b: unary() }; }
      else if (startsAtom(p)) node = { op: '*', a: node, b: power() };
      else return node;
    }
  }
  function unary() {
    const p = peek();
    if (p && p.t === '-') { k++; return { op: 'neg', a: unary() }; }
    if (p && p.t === '+') { k++; return unary(); }
    return power();
  }
  function power() {
    const base = atom();
    if (peek()?.t === '^') { k++; return { op: '^', a: base, b: unary() }; }
    return base;
  }
  function atom() {
    const p = toks[k++];
    if (!p) throw new ParseError('unexpected end');
    if (p.t === 'num') return { num: p.v };
    if (p.t === 'id') return p.v === 'x' ? { x: true } : { num: CONSTS[p.v] };
    if (p.t === 'fn') {
      // sin x, sin(x), sin^2 x
      let exp = null;
      if (peek()?.t === '^') { k++; exp = atom(); }
      let arg;
      if (peek()?.t === '(') { k++; arg = expr(); eat(')'); }
      else arg = power();
      const call = { fn: p.v, a: arg };
      return exp ? { op: '^', a: call, b: exp } : call;
    }
    if (p.t === '(') { const e = expr(); eat(')'); return e; }
    throw new ParseError('unexpected ' + p.t);
  }

  const tree = expr();
  if (k < toks.length) throw new ParseError('unexpected ' + toks[k].t);
  return tree;
}

export function evaluate(node, x) {
  if ('num' in node) return node.num;
  if (node.x) return x;
  if (node.fn) return FUNCS[node.fn](evaluate(node.a, x));
  const a = evaluate(node.a, x);
  switch (node.op) {
    case 'neg': return -a;
    case '+': return a + evaluate(node.b, x);
    case '-': return a - evaluate(node.b, x);
    case '*': return a * evaluate(node.b, x);
    case '/': return a / evaluate(node.b, x);
    case '^': return Math.pow(a, evaluate(node.b, x));
  }
  throw new Error('bad node');
}

export function compile(src) {
  const tree = parse(src);
  return (x) => evaluate(tree, x);
}

// Sample f on [x0, x1]; returns polylines split at discontinuities.
export function sample(f, x0, x1, y0, y1, n = 600) {
  const lines = [];
  let cur = [];
  const span = y1 - y0;
  let prev = null;
  for (let i = 0; i <= n; i++) {
    const x = x0 + ((x1 - x0) * i) / n;
    let y;
    try { y = f(x); } catch { y = NaN; }
    const ok = Number.isFinite(y) && Math.abs(y) < 1e9;
    const jump = prev !== null && ok && Math.abs(y - prev) > span * 2;
    if (!ok || jump) {
      if (cur.length > 1) lines.push(cur);
      cur = ok ? [[x, y]] : [];
    } else cur.push([x, y]);
    prev = ok ? y : null;
  }
  if (cur.length > 1) lines.push(cur);
  return lines;
}

// Nice tick step for an axis range.
export function tickStep(span, target = 8) {
  const raw = span / target;
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  const m = raw / p;
  return (m < 1.5 ? 1 : m < 3.5 ? 2 : m < 7.5 ? 5 : 10) * p;
}
