//! Packed ink encoding (data-model §4): columnar, delta-coded, zig-zag varints.
//!
//! Layout (version 1):
//! ```text
//! u8 version = 1
//! varint n            number of points
//! n × varint          Δx  in 1/16 page unit
//! n × varint          Δy  in 1/16 page unit
//! n × varint          Δp  pressure in 1/1024
//! ```
//! Coordinates are quantised, so `decode(encode(p))` equals `quantize(p)`, not `p`.
//! Callers that need bit-identical replicas must store the quantised points.

use nb_ink::Point;

const VERSION: u8 = 1;
const XY_SCALE: f64 = 16.0;
const P_SCALE: f64 = 1024.0;

/// Error for malformed or unsupported ink blobs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub &'static str);

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ink decode: {}", self.0)
    }
}

impl std::error::Error for DecodeError {}

fn q(points: &[Point]) -> impl Iterator<Item = (i64, i64, i64)> + '_ {
    points.iter().map(|p| {
        (
            (p.x * XY_SCALE).round() as i64,
            (p.y * XY_SCALE).round() as i64,
            (p.p.clamp(0.0, 1.0) * P_SCALE).round() as i64,
        )
    })
}

/// Points as they will read back after a round trip through [`encode`]/[`decode`].
pub fn quantize(points: &[Point]) -> Vec<Point> {
    q(points)
        .map(|(x, y, p)| Point::new(x as f64 / XY_SCALE, y as f64 / XY_SCALE, p as f64 / P_SCALE))
        .collect()
}

fn put(out: &mut Vec<u8>, v: i64) {
    let mut z = ((v << 1) ^ (v >> 63)) as u64;
    loop {
        let b = (z & 0x7f) as u8;
        z >>= 7;
        if z == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

fn get(buf: &[u8], pos: &mut usize) -> Result<i64, DecodeError> {
    let mut z: u64 = 0;
    let mut shift = 0;
    loop {
        let b = *buf.get(*pos).ok_or(DecodeError("truncated"))?;
        *pos += 1;
        if shift >= 64 {
            return Err(DecodeError("varint too long"));
        }
        z |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Ok(((z >> 1) as i64) ^ -((z & 1) as i64));
        }
        shift += 7;
    }
}

/// Encode points; see the module docs for the layout.
pub fn encode(points: &[Point]) -> Vec<u8> {
    let qs: Vec<_> = q(points).collect();
    let mut out = Vec::with_capacity(4 + qs.len() * 4);
    out.push(VERSION);
    put(&mut out, qs.len() as i64);
    for ch in 0..3 {
        let mut prev = 0i64;
        for t in &qs {
            let v = [t.0, t.1, t.2][ch];
            put(&mut out, v - prev);
            prev = v;
        }
    }
    out
}

/// Decode a blob produced by [`encode`].
pub fn decode(buf: &[u8]) -> Result<Vec<Point>, DecodeError> {
    match buf.first() {
        Some(&VERSION) => {}
        Some(_) => return Err(DecodeError("unsupported version")),
        None => return Err(DecodeError("empty")),
    }
    let mut pos = 1;
    let n = get(buf, &mut pos)?;
    // Each point needs at least 3 bytes, which bounds n before allocating.
    if n < 0 || n as usize > buf.len() {
        return Err(DecodeError("bad point count"));
    }
    let n = n as usize;
    let mut cols = [vec![0i64; n], vec![0i64; n], vec![0i64; n]];
    for col in &mut cols {
        let mut acc = 0i64;
        for v in col.iter_mut() {
            acc = acc.checked_add(get(buf, &mut pos)?).ok_or(DecodeError("overflow"))?;
            *v = acc;
        }
    }
    if pos != buf.len() {
        return Err(DecodeError("trailing bytes"));
    }
    Ok((0..n)
        .map(|i| {
            Point::new(
                cols[0][i] as f64 / XY_SCALE,
                cols[1][i] as f64 / XY_SCALE,
                cols[2][i] as f64 / P_SCALE,
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(n: usize) -> Vec<Point> {
        let (mut x, mut y, mut a, mut p) = (100.0f64, 200.0f64, 0.3f64, 0.5f64);
        (0..n)
            .map(|i| {
                a += ((i * 7919) % 13) as f64 * 0.01 - 0.06;
                x += a.cos() * 1.5;
                y += a.sin() * 1.5;
                p = (p + ((i * 31) % 7) as f64 * 0.004 - 0.012).clamp(0.05, 1.0);
                Point::new(x, y, p)
            })
            .collect()
    }

    #[test]
    fn round_trip_equals_quantized() {
        let pts = walk(500);
        assert_eq!(decode(&encode(&pts)).unwrap(), quantize(&pts));
        let qp = quantize(&pts);
        assert_eq!(decode(&encode(&qp)).unwrap(), qp, "quantisation is idempotent");
        for (a, b) in pts.iter().zip(&qp) {
            assert!((a.x - b.x).abs() <= 1.0 / 32.0 + 1e-9 && (a.y - b.y).abs() <= 1.0 / 32.0 + 1e-9);
        }
    }

    #[test]
    fn handwriting_density_is_under_six_bytes_per_point() {
        let pts = walk(10_000);
        let bpp = encode(&pts).len() as f64 / pts.len() as f64;
        assert!(bpp <= 6.0, "{bpp:.2} bytes/point");
    }

    #[test]
    fn empty_and_corrupt_inputs() {
        assert_eq!(decode(&encode(&[])).unwrap(), vec![]);
        assert!(decode(&[]).is_err());
        assert!(decode(&[9]).is_err());
        let mut b = encode(&walk(10));
        b.pop();
        assert!(decode(&b).is_err());
        assert!(
            decode(&[1, 0xff, 0xff, 0xff, 0xff, 0x0f]).is_err(),
            "huge count rejected"
        );
    }
}
