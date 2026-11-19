// Shape recognition: turns a hand-drawn stroke into a clean geometric
// interpretation. The raw ink is never modified (ADR-0003); the result is
// stored as `stroke.interp` and can be switched off again.

import { dist, pathLength, simplify, distToSegment } from './ink.js';

const DEG = Math.PI / 180;

export function recognize(points) {
  const pts = points.map((p) => [p[0], p[1]]);
  if (pts.length < 2) return null;
  const L = pathLength(pts);
  if (L < 12) return null;
  const d = dist(pts[0], pts[pts.length - 1]);

  // Straight line: endpoints far apart and every point close to the chord.
  if (d / L > 0.85) {
    let dev = 0;
    for (const p of pts) dev = Math.max(dev, distToSegment(p, pts[0], pts[pts.length - 1]));
    if (dev < Math.max(4, 0.06 * L)) return snapLine({ kind: 'line', a: pts[0], b: pts[pts.length - 1] });
  }

  const closed = d < Math.max(18, 0.2 * L);
  if (!closed) {
    // Open polyline with clear corners (e.g. an angle or a zig-zag of 2–3 segments).
    const poly = corners(pts, L);
    if (poly.length >= 3 && poly.length <= 4 && fitsPolyline(pts, poly, L)) {
      return { kind: 'polygon', points: poly, closed: false };
    }
    return null;
  }

  // Closed: try polygon first (corners are a stronger signal than roundness).
  const ring = pts.slice();
  ring[ring.length - 1] = ring[0];
  const poly = corners(ring, L).slice(0, -1);
  const polyOk = poly.length >= 3 && poly.length <= 6 && fitsPolyline(ring, [...poly, poly[0]], L);
  const e = fitEllipse(pts);
  // Few sharp corners beat a mediocre ellipse; a smooth 5–6 "corner" loop is an ellipse.
  if (polyOk && (!e || (poly.length <= 4 && e.err > 0.05))) return tidyPolygon(poly);
  if (e) { delete e.err; return e; }
  return null;
}

function corners(pts, L) {
  const simp = simplify(pts, Math.max(4, 0.045 * L));
  // Merge vertices that are very close together.
  const out = [simp[0]];
  for (let i = 1; i < simp.length; i++) if (dist(simp[i], out[out.length - 1]) > 0.08 * L) out.push(simp[i]);
  if (out.length > 1 && dist(out[out.length - 1], simp[simp.length - 1]) > 1e-9) out[out.length - 1] = simp[simp.length - 1];
  return out;
}

function fitsPolyline(pts, poly, L) {
  let total = 0;
  for (const p of pts) {
    let m = Infinity;
    for (let i = 1; i < poly.length; i++) m = Math.min(m, distToSegment(p, poly[i - 1], poly[i]));
    total += m;
  }
  return total / pts.length < Math.max(3, 0.025 * L);
}

function snapLine(line) {
  const dx = line.b[0] - line.a[0], dy = line.b[1] - line.a[1];
  const ang = Math.atan2(dy, dx);
  const len = Math.hypot(dx, dy);
  // Snap to horizontal / vertical / 45° when close.
  for (let k = -4; k <= 4; k++) {
    const target = k * 45 * DEG;
    if (Math.abs(ang - target) < 5 * DEG) {
      const mx = (line.a[0] + line.b[0]) / 2, my = (line.a[1] + line.b[1]) / 2;
      const hx = (Math.cos(target) * len) / 2, hy = (Math.sin(target) * len) / 2;
      return { kind: 'line', a: [mx - hx, my - hy], b: [mx + hx, my + hy] };
    }
  }
  return line;
}

function tidyPolygon(poly) {
  if (poly.length === 4) {
    // Rectangle: all angles near 90°.
    const angs = poly.map((p, i) => angleAt(poly[(i + 3) % 4], p, poly[(i + 1) % 4]));
    if (angs.every((a) => Math.abs(a - 90) < 18)) {
      const r = bestRect(poly);
      return { kind: 'polygon', points: r, closed: true, label: 'rectangle' };
    }
  }
  if (poly.length === 3) return { kind: 'polygon', points: poly, closed: true, label: 'triangle' };
  return { kind: 'polygon', points: poly, closed: true };
}

function angleAt(a, b, c) {
  const v1 = [a[0] - b[0], a[1] - b[1]], v2 = [c[0] - b[0], c[1] - b[1]];
  const cos = (v1[0] * v2[0] + v1[1] * v2[1]) / (Math.hypot(...v1) * Math.hypot(...v2) || 1);
  return Math.acos(Math.max(-1, Math.min(1, cos))) / DEG;
}

// Rectangle through the corners' centroid with orientation from the first edge,
// snapped to axis-aligned when within 6°.
function bestRect(poly) {
  const cx = poly.reduce((s, p) => s + p[0], 0) / 4;
  const cy = poly.reduce((s, p) => s + p[1], 0) / 4;
  let ang = Math.atan2(poly[1][1] - poly[0][1], poly[1][0] - poly[0][0]);
  const q = Math.round(ang / (90 * DEG)) * 90 * DEG;
  if (Math.abs(ang - q) < 6 * DEG) ang = q;
  const ux = [Math.cos(ang), Math.sin(ang)], uy = [-Math.sin(ang), Math.cos(ang)];
  let hw = 0, hh = 0;
  for (const p of poly) {
    const dx = p[0] - cx, dy = p[1] - cy;
    hw += Math.abs(dx * ux[0] + dy * ux[1]);
    hh += Math.abs(dx * uy[0] + dy * uy[1]);
  }
  hw /= 4; hh /= 4;
  const pt = (sx, sy) => [cx + ux[0] * hw * sx + uy[0] * hh * sy, cy + ux[1] * hw * sx + uy[1] * hh * sy];
  return [pt(-1, -1), pt(1, -1), pt(1, 1), pt(-1, 1)];
}

// Ellipse from second moments; becomes a circle when axes are within 15 %.
function fitEllipse(pts) {
  const n = pts.length;
  let cx = 0, cy = 0;
  for (const p of pts) { cx += p[0]; cy += p[1]; }
  cx /= n; cy /= n;
  let sxx = 0, syy = 0, sxy = 0;
  for (const p of pts) {
    const dx = p[0] - cx, dy = p[1] - cy;
    sxx += dx * dx; syy += dy * dy; sxy += dx * dy;
  }
  sxx /= n; syy /= n; sxy /= n;
  const tr = sxx + syy, det = sxx * syy - sxy * sxy;
  const disc = Math.sqrt(Math.max(0, tr * tr / 4 - det));
  const l1 = tr / 2 + disc, l2 = tr / 2 - disc;
  let rx = Math.sqrt(2 * l1), ry = Math.sqrt(2 * Math.max(l2, 0));
  let angle = 0.5 * Math.atan2(2 * sxy, sxx - syy);
  if (ry < 3) return null;

  // Residual of the normalised radial distance.
  const c = Math.cos(-angle), s = Math.sin(-angle);
  let err = 0;
  for (const p of pts) {
    const dx = p[0] - cx, dy = p[1] - cy;
    const u = dx * c - dy * s, v = dx * s + dy * c;
    err += Math.abs(Math.hypot(u / rx, v / ry) - 1);
  }
  err /= n;
  if (err > 0.16) return null;
  if (rx / ry < 1.15) {
    const r = (rx + ry) / 2;
    return { kind: 'ellipse', cx, cy, rx: r, ry: r, angle: 0, label: 'circle', err };
  }
  const q = Math.round(angle / (90 * DEG)) * 90 * DEG;
  if (Math.abs(angle - q) < 8 * DEG) angle = q;
  return { kind: 'ellipse', cx, cy, rx, ry, angle, err };
}
