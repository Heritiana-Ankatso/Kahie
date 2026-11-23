//! Raster formats Android cannot decode itself. TIFF (scans, faxes, exports from desktop
//! tools) is decoded here with the pure-Rust `tiff` crate and handed back to the app as PNG,
//! downsized like every inserted picture; the PNG is what the page stores.

use crate::CoreError;
use tiff::decoder::{Decoder, DecodingResult, Limits};
use tiff::ColorType;

/// Longest side of an inserted picture, in pixels (the app's limit for every format).
const MAX_SIDE: u32 = 1600;
/// Largest TIFF accepted, in decoded pixels (about 100 megapixels).
const MAX_PIXELS: u64 = 100_000_000;

/// A decoded picture: PNG bytes and their size in pixels.
#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct RasterImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Whether `bytes` start like a TIFF file (`II*\0` or `MM\0*`; BigTIFF too).
#[uniffi::export]
pub fn is_tiff(bytes: Vec<u8>) -> bool {
    tiff_magic(&bytes)
}

fn tiff_magic(b: &[u8]) -> bool {
    b.len() >= 4 && (b[..4] == *b"II*\0" || b[..4] == *b"MM\0*" || b[..4] == *b"II+\0" || b[..4] == *b"MM\0+")
}

fn bad(detail: impl std::fmt::Display) -> CoreError {
    CoreError::Format {
        detail: detail.to_string(),
    }
}

/// Decode the first page of a TIFF to PNG, at most 1600 px on its longer side.
/// Grey, grey + alpha, RGB, RGBA and CMYK at 1–16 bits per sample are supported
/// (uncompressed, LZW, Deflate, PackBits, JPEG and CCITT fax compression).
#[uniffi::export]
pub fn decode_tiff(bytes: Vec<u8>) -> Result<RasterImage, CoreError> {
    if !tiff_magic(&bytes) {
        return Err(bad("not a TIFF file"));
    }
    let mut limits = Limits::default();
    limits.decoding_buffer_size = 512 * 1024 * 1024;
    let mut dec = Decoder::new(std::io::Cursor::new(bytes))
        .map_err(bad)?
        .with_limits(limits);
    let (w, h) = dec.dimensions().map_err(bad)?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(bad(format!("unsupported TIFF size {w}×{h}")));
    }
    let color = dec.colortype().map_err(bad)?;
    let (channels, bits, cmyk) = match color {
        ColorType::Gray(b) => (1, b, false),
        ColorType::GrayA(b) => (2, b, false),
        ColorType::RGB(b) => (3, b, false),
        ColorType::RGBA(b) => (4, b, false),
        ColorType::CMYK(b) => (4, b, true),
        other => return Err(bad(format!("unsupported TIFF colour type {other:?}"))),
    };
    let samples: Vec<u8> = match dec.read_image().map_err(bad)? {
        DecodingResult::U8(v) if bits == 8 => v,
        DecodingResult::U8(v) if bits < 8 && channels == 1 => unpack(&v, w as usize, h as usize, bits),
        DecodingResult::U16(v) => v.iter().map(|s| (s >> 8) as u8).collect(),
        _ => return Err(bad(format!("unsupported TIFF sample format ({bits} bits)"))),
    };
    let n = w as usize * h as usize;
    if samples.len() < n * channels {
        return Err(bad("truncated TIFF image"));
    }
    // To RGBA8.
    let mut rgba = Vec::with_capacity(n * 4);
    for px in samples.chunks_exact(channels).take(n) {
        match (channels, cmyk) {
            (1, _) => rgba.extend_from_slice(&[px[0], px[0], px[0], 255]),
            (2, _) => rgba.extend_from_slice(&[px[0], px[0], px[0], px[1]]),
            (3, _) => rgba.extend_from_slice(&[px[0], px[1], px[2], 255]),
            (_, true) => {
                let k = 255 - u32::from(px[3]);
                let f = |c: u8| ((255 - u32::from(c)) * k / 255) as u8;
                rgba.extend_from_slice(&[f(px[0]), f(px[1]), f(px[2]), 255]);
            }
            _ => rgba.extend_from_slice(&[px[0], px[1], px[2], px[3]]),
        }
    }
    let (rgba, w, h) = downsize(rgba, w, h);
    let opaque = rgba.chunks_exact(4).all(|p| p[3] == 255);
    let mut png = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut png, w, h);
        enc.set_depth(png::BitDepth::Eight);
        let data: Vec<u8> = if opaque {
            enc.set_color(png::ColorType::Rgb);
            rgba.chunks_exact(4).flat_map(|p| [p[0], p[1], p[2]]).collect()
        } else {
            enc.set_color(png::ColorType::Rgba);
            rgba
        };
        let mut wr = enc.write_header().map_err(bad)?;
        wr.write_image_data(&data).map_err(bad)?;
        wr.finish().map_err(bad)?;
    }
    Ok(RasterImage {
        png,
        width: w,
        height: h,
    })
}

/// Grey samples of fewer than 8 bits (rows padded to whole bytes) scaled to 8 bits.
fn unpack(packed: &[u8], w: usize, h: usize, bits: u8) -> Vec<u8> {
    let bits = bits as usize;
    let stride = (w * bits).div_ceil(8);
    let max = (1u32 << bits) - 1;
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = &packed[(y * stride).min(packed.len())..((y + 1) * stride).min(packed.len())];
        for x in 0..w {
            let bit = x * bits;
            let byte = row.get(bit / 8).copied().unwrap_or(0);
            let v = (u32::from(byte) >> (8 - bits - bit % 8)) & max;
            out.push((v * 255 / max) as u8);
        }
    }
    out
}

/// Box-filter `rgba` (w × h) down so its longer side is at most [`MAX_SIDE`].
fn downsize(rgba: Vec<u8>, w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let long = w.max(h);
    if long <= MAX_SIDE {
        return (rgba, w, h);
    }
    let nw = ((u64::from(w) * u64::from(MAX_SIDE)) / u64::from(long)).max(1) as u32;
    let nh = ((u64::from(h) * u64::from(MAX_SIDE)) / u64::from(long)).max(1) as u32;
    let (w, h, nw, nh) = (w as usize, h as usize, nw as usize, nh as usize);
    let mut out = vec![0u8; nw * nh * 4];
    for ny in 0..nh {
        let (y0, y1) = (ny * h / nh, ((ny + 1) * h / nh).max(ny * h / nh + 1));
        for nx in 0..nw {
            let (x0, x1) = (nx * w / nw, ((nx + 1) * w / nw).max(nx * w / nw + 1));
            let mut acc = [0u64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * w + x) * 4;
                    for c in 0..4 {
                        acc[c] += u64::from(rgba[i + c]);
                    }
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u64;
            let o = (ny * nw + nx) * 4;
            for c in 0..4 {
                out[o + c] = (acc[c] / count) as u8;
            }
        }
    }
    (out, nw as u32, nh as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiff::encoder::{colortype, TiffEncoder};

    fn encode<C: colortype::ColorType<Inner = u8>>(w: u32, h: u32, data: &[u8]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        TiffEncoder::new(&mut buf)
            .unwrap()
            .write_image::<C>(w, h, data)
            .unwrap();
        buf.into_inner()
    }

    fn png_pixels(png: &[u8]) -> (png::OutputInfo, Vec<u8>) {
        let mut r = png::Decoder::new(std::io::Cursor::new(png)).read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size().unwrap()];
        let info = r.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info, buf)
    }

    #[test]
    fn rgb_tiff_becomes_png() {
        let data: Vec<u8> = (0..4 * 3 * 3).map(|i| (i * 7) as u8).collect();
        let t = encode::<colortype::RGB8>(4, 3, &data);
        assert!(is_tiff(t.clone()));
        let img = decode_tiff(t).unwrap();
        assert_eq!((img.width, img.height), (4, 3));
        let (info, px) = png_pixels(&img.png);
        assert_eq!(info.color_type, png::ColorType::Rgb);
        assert_eq!(px, data);
    }

    #[test]
    fn gray_and_rgba_tiffs() {
        let g = decode_tiff(encode::<colortype::Gray8>(2, 1, &[0, 200])).unwrap();
        assert_eq!(png_pixels(&g.png).1, [0, 0, 0, 200, 200, 200]);
        let a = decode_tiff(encode::<colortype::RGBA8>(1, 1, &[10, 20, 30, 40])).unwrap();
        let (info, px) = png_pixels(&a.png);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        assert_eq!(px, [10, 20, 30, 40]);
    }

    #[test]
    fn large_tiff_is_downsized() {
        let t = encode::<colortype::Gray8>(3200, 10, &vec![128; 32_000]);
        let img = decode_tiff(t).unwrap();
        assert_eq!((img.width, img.height), (1600, 5));
        assert!(png_pixels(&img.png).1.iter().all(|&v| v == 128));
    }

    #[test]
    fn bilevel_rows_are_unpacked() {
        // 10 px wide, 1 bit: two bytes per row.
        assert_eq!(
            unpack(&[0b1010_0000, 0b0100_0000], 10, 1, 1),
            [255, 0, 255, 0, 0, 0, 0, 0, 0, 255]
        );
    }

    #[test]
    fn rejects_non_tiff() {
        assert!(!is_tiff(b"\x89PNG\r\n".to_vec()));
        assert!(decode_tiff(b"II*\0garbage".to_vec()).is_err());
    }
}
