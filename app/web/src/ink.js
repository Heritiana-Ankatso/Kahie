// Ink geometry: smoothing, simplification, variable-width outlines, hit testing.
// Points are [x, y, pressure] arrays in page coordinates.

export function dist(a, b) {
  return Math.hypot(a[0] - b[0], a[1] - b[1]);
}

export function pathLength(pts) {
  let L = 0;
  for (let i = 1; i < pts.length; i++) L += dist(pts[i - 1], pts[i]);
  return L;
}

export function distToSegment(p, a, b) {
  const dx = b[0] - a[0], dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  let t = len2 ? ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2 : 0;
  t = Math.max(0, Math.min(1, t));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

// Ramer–Douglas–Peucker simplification; keeps extra fields (pressure).
export function simplify(pts, eps) {
  if (pts.length < 3) return pts.slice();
  const keep = new Uint8Array(pts.length);
  keep[0] = keep[pts.length - 1] = 1;
  const stack = [[0, pts.length - 1]];
  while (stack.length) {
    const [i0, i1] = stack.pop();
    let dmax = 0, idx = -1;
    for (let i = i0 + 1; i < i1; i++) {
      const d = distToSegment(pts[i], pts[i0], pts[i1]);
      if (d > dmax) { dmax = d; idx = i; }
    }
    if (dmax > eps && idx > 0) {
      keep[idx] = 1;
      stack.push([i0, idx], [idx, i1]);
    }
  }
  return pts.filter((_, i) => keep[i]);
}

// Light exponential smoothing of raw input (reduces digitiser jitter).
export function smooth(pts, k = 0.5) {
  if (pts.length < 3) return pts.slice();
  const out = [pts[0]];
  for (let i = 1; i < pts.length - 1; i++) {
    const p = pts[i], q = out[i - 1];
    out.push([q[0] + (p[0] - q[0]) * k + (pts[i + 1][0] - p[0]) * k * 0.25,
              q[1] + (p[1] - q[1]) * k + (pts[i + 1][1] - p[1]) * k * 0.25,
              p[2] ?? 0.5]);
  }
  out.push(pts[pts.length - 1]);
  return out;
}

// Prepare a finished stroke for storage: smooth, simplify, round to 0.01 px.
export function finishPoints(raw, scale = 1) {
  const s = smooth(raw, 0.6);
  const simp = simplify(s, 0.35 / scale);
  return simp.map((p) => [round2(p[0]), round2(p[1]), round2(p[2] ?? 0.5)]);
}

const round2 = (v) => Math.round(v * 100) / 100;

// Stroke radius at a point given base width and pressure.
export function radiusAt(width, p, pressureSensitive = true) {
  if (!pressureSensitive) return width / 2;
  const pr = p === undefined || p === 0 ? 0.5 : p;
  return (width / 2) * (0.45 + 0.9 * pr);
}

// Variable-width outline polygon for a stroke (closed list of [x, y]).
export function outline(pts, width, pressureSensitive = true) {
  const n = pts.length;
  if (n === 0) return [];
  if (n === 1 || pathLength(pts) < 0.5) {
    const r = radiusAt(width, pts[0][2], pressureSensitive);
    return circlePoly(pts[0][0], pts[0][1], Math.max(r, 0.5), 12);
  }
  const left = [], right = [];
  for (let i = 0; i < n; i++) {
    const a = pts[Math.max(0, i - 1)], b = pts[Math.min(n - 1, i + 1)];
    let dx = b[0] - a[0], dy = b[1] - a[1];
    const len = Math.hypot(dx, dy) || 1;
    dx /= len; dy /= len;
    const r = radiusAt(width, pts[i][2], pressureSensitive);
    left.push([pts[i][0] - dy * r, pts[i][1] + dx * r]);
    right.push([pts[i][0] + dy * r, pts[i][1] - dx * r]);
  }
  const capEnd = cap(pts[n - 1], pts[n - 2], radiusAt(width, pts[n - 1][2], pressureSensitive));
  const capStart = cap(pts[0], pts[1], radiusAt(width, pts[0][2], pressureSensitive));
  return [...left, ...capEnd, ...right.reverse(), ...capStart];
}

function cap(p, prev, r) {
  const ang = Math.atan2(p[1] - prev[1], p[0] - prev[0]);
  const out = [];
  for (let i = 1; i < 8; i++) {
    const t = ang + Math.PI / 2 - (i / 8) * Math.PI;
    out.push([p[0] + Math.cos(t) * r, p[1] + Math.sin(t) * r]);
  }
  return out;
}

function circlePoly(cx, cy, r, n) {
  const out = [];
  for (let i = 0; i < n; i++) {
    const t = (i / n) * Math.PI * 2;
    out.push([cx + Math.cos(t) * r, cy + Math.sin(t) * r]);
  }
  return out;
}

export function polyToSvgPath(poly, closed = true) {
  if (!poly.length) return '';
  const f = (v) => Math.round(v * 100) / 100;
  let d = 'M' + f(poly[0][0]) + ' ' + f(poly[0][1]);
  for (let i = 1; i < poly.length; i++) d += 'L' + f(poly[i][0]) + ' ' + f(poly[i][1]);
  return closed ? d + 'Z' : d;
}

// True when point p is within r of the polyline.
export function hitPolyline(pts, p, r) {
  if (pts.length === 1) return dist(pts[0], p) <= r;
  for (let i = 1; i < pts.length; i++) if (distToSegment(p, pts[i - 1], pts[i]) <= r) return true;
  return false;
}

// True when segment a-b passes within r of the polyline (for fast erasing).
export function segmentHitsPolyline(pts, a, b, r) {
  const steps = Math.max(1, Math.ceil(dist(a, b) / Math.max(r, 1)));
  for (let i = 0; i <= steps; i++) {
    const t = i / steps;
    if (hitPolyline(pts, [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t], r)) return true;
  }
  return false;
}
