// Handwriting corpus collection tool (P0-16). Offline; the only output is a downloaded JSON file.
import { promptsFor, LANGUAGES } from './collect-prompts.js';

const $ = (id) => document.getElementById(id);

/** Random writer id, e.g. "w-3fa9c1d2". Not linked to the person anywhere. */
export function writerId(rand = crypto.getRandomValues.bind(crypto)) {
  const b = rand(new Uint8Array(4));
  return 'w-' + Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
}

/** Fisher–Yates shuffle (copy). */
export function shuffled(list, random = Math.random) {
  const a = list.slice();
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

/** The file written at the end of a session (format documented in the protocol). */
export function corpusFile({ writer, consent, lang, meta, device, samples }) {
  return {
    format: 'kahie-hwr-corpus',
    version: 1,
    writer: { id: writer, ...meta },
    consent,
    device,
    language: lang,
    samples,
  };
}

function setup() {
  let lang = 'fr';
  const langs = $('langs');
  for (const [code, name] of Object.entries(LANGUAGES)) {
    const l = document.createElement('label');
    l.innerHTML = `<input type="radio" name="lang" value="${code}" ${code === lang ? 'checked' : ''}> ${name}`;
    l.querySelector('input').addEventListener('change', () => { lang = code; });
    langs.append(l);
  }
  const update = () => {
    const ok = $('c-eval').checked || $('c-train').checked;
    $('start').disabled = !ok;
    $('why').style.visibility = ok ? 'hidden' : 'visible';
  };
  $('c-eval').addEventListener('change', update);
  $('c-train').addEventListener('change', update);
  $('start').addEventListener('click', () => {
    const meta = {};
    if ($('age').value) meta.age_band = $('age').value;
    if ($('hand').value) meta.handedness = $('hand').value;
    if ($('role').value) meta.role = $('role').value;
    meta.languages = [lang];
    session({ lang, meta, consent: { evaluation: $('c-eval').checked, training: $('c-train').checked } });
  });
}

function session({ lang, meta, consent }) {
  const prompts = shuffled(promptsFor(lang));
  const samples = [];
  const writer = writerId();
  let index = 0;
  let strokes = [];
  let current = null;
  let t0 = 0;
  let pointerKind = 'mouse';

  $('setup').style.display = 'none';
  $('session').style.display = 'flex';
  const pad = $('pad');
  const ctx = pad.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  const resize = () => {
    pad.width = pad.clientWidth * dpr;
    pad.height = pad.clientHeight * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    redraw();
  };
  const ink = getComputedStyle(document.body).color;
  function redraw() {
    ctx.clearRect(0, 0, pad.width, pad.height);
    ctx.strokeStyle = ink;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    for (const s of strokes.concat(current ? [current] : [])) {
      for (let i = 1; i < s.points.length; i++) {
        const [x0, y0] = s.points[i - 1];
        const [x1, y1, p] = s.points[i];
        ctx.lineWidth = 1 + 3 * p;
        ctx.beginPath();
        ctx.moveTo(x0, y0);
        ctx.lineTo(x1, y1);
        ctx.stroke();
      }
    }
  }
  function show() {
    const p = prompts[index];
    const label = p.kind === 'shape' ? (p.hint || p.label) : p.label;
    $('prompt').innerHTML = '';
    $('prompt').append(document.createTextNode(label));
    if (p.kind === 'math') {
      const s = document.createElement('small');
      s.textContent = p.hint ? `${p.hint} — LaTeX` : 'LaTeX';
      $('prompt').append(s);
    }
    $('count').textContent = `${index + 1} / ${prompts.length}`;
    strokes = [];
    t0 = 0;
    redraw();
  }
  const point = (e) => {
    const r = pad.getBoundingClientRect();
    if (!t0) t0 = e.timeStamp;
    const pressure = e.pointerType === 'pen' ? (e.pressure || 0.5) : 0.5;
    return [+(e.clientX - r.left).toFixed(2), +(e.clientY - r.top).toFixed(2), +pressure.toFixed(3), Math.round(e.timeStamp - t0)];
  };
  pad.addEventListener('pointerdown', (e) => {
    pad.setPointerCapture(e.pointerId);
    pointerKind = e.pointerType;
    current = { points: [point(e)] };
  });
  pad.addEventListener('pointermove', (e) => {
    if (!current) return;
    const evs = e.getCoalescedEvents ? e.getCoalescedEvents() : [e];
    for (const ce of evs) current.points.push(point(ce));
    redraw();
  });
  const end = () => {
    if (current && current.points.length) strokes.push(current);
    current = null;
    redraw();
  };
  pad.addEventListener('pointerup', end);
  pad.addEventListener('pointercancel', () => { current = null; redraw(); });

  const advance = (keep) => {
    const p = prompts[index];
    if (keep && strokes.length) samples.push({ prompt_id: p.id, kind: p.kind, label: p.label, strokes });
    index++;
    if (index < prompts.length) show();
    else finish();
  };
  $('clear').addEventListener('click', () => { strokes = []; t0 = 0; redraw(); });
  $('skip').addEventListener('click', () => advance(false));
  $('next').addEventListener('click', () => advance(true));

  function finish() {
    $('session').style.display = 'none';
    $('done').style.display = 'block';
    $('writer').textContent = writer;
    const file = corpusFile({
      writer, consent, lang, meta, samples,
      device: { user_agent: navigator.userAgent, screen: [screen.width, screen.height], dpr, pointer: pointerKind },
    });
    $('save').addEventListener('click', () => {
      const blob = new Blob([JSON.stringify(file)], { type: 'application/json' });
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = `kahie-corpus-${lang}-${writer}.json`;
      a.click();
      setTimeout(() => URL.revokeObjectURL(a.href), 10_000);
    });
  }

  window.addEventListener('resize', resize);
  resize();
  show();
}

if (typeof document !== 'undefined' && document.getElementById('setup')) setup();
