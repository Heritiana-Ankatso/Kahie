//! Golden-image tests of the rendering pipeline (P1-44).
//!
//! Each scene is turned into an `nb-render` display list, then
//! - rasterised on the desktop (tiny-skia, anti-aliased) and compared with
//!   `tests/golden/<scene>.png` (per-channel tolerance, for floating-point differences
//!   between machines), and
//! - exported to SVG and compared byte for byte with `tests/golden/<scene>.svg`.
//!
//! Regenerate after an intended change with `UPDATE_GOLDEN=1 cargo test -p nb-render --test golden`
//! and review the PNG diff. On a mismatch the actual image and a diff image are written to
//! `target/golden-failures/` (CI uploads them).

use nb_ink::Point;
use nb_render::{DisplayList, DrawOp, Rect, Scene};
use std::path::PathBuf;
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Channel difference ignored, and share of pixels allowed above it.
const TOLERANCE: u8 = 3;
const MAX_BAD_PIXELS: f64 = 0.002;

struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn wave(x: f64, y: f64, len: usize, amp: f64, step: f64, pressure: impl Fn(usize) -> f64) -> Vec<Point> {
    (0..len)
        .map(|k| Point::new(x + k as f64 * step, y + (k as f64 * 0.15).sin() * amp, pressure(k)))
        .collect()
}

/// Rasterise `dl` showing `view` into a `w`×`h` image on white paper.
fn rasterise(dl: &DisplayList, view: Rect, w: u32, h: u32) -> Pixmap {
    let mut px = Pixmap::new(w, h).expect("pixmap");
    px.fill(Color::WHITE);
    let s = (f64::from(w) / (view.x1 - view.x0)) as f32;
    let t = Transform::from_row(s, 0.0, 0.0, s, -view.x0 as f32 * s, -view.y0 as f32 * s);
    for op in &dl.ops {
        let (color, alpha, coords, stroke) = match op {
            DrawOp::Fill { color, alpha, coords } => (*color, *alpha, coords, None),
            DrawOp::Stroke {
                color,
                alpha,
                width,
                closed,
                coords,
            } => (*color, *alpha, coords, Some((*width, *closed))),
        };
        let mut pb = PathBuilder::new();
        for (i, xy) in coords.chunks_exact(2).enumerate() {
            if i == 0 {
                pb.move_to(xy[0], xy[1]);
            } else {
                pb.line_to(xy[0], xy[1]);
            }
        }
        if coords.len() == 4 && coords[0] == coords[2] && coords[1] == coords[3] {
            // A dot: a zero-length segment, drawn by its round caps.
            pb.line_to(coords[0] + 0.001, coords[1]);
        }
        if stroke.is_none_or(|(_, closed)| closed) {
            pb.close();
        }
        let Some(path) = pb.finish() else { continue };
        let mut paint = Paint::default();
        paint.set_color_rgba8(
            (color >> 16) as u8,
            (color >> 8) as u8,
            color as u8,
            (alpha * 255.0).round() as u8,
        );
        paint.anti_alias = true;
        match stroke {
            None => px.fill_path(&path, &paint, FillRule::Winding, t, None),
            Some((width, _)) => {
                let st = Stroke {
                    width,
                    line_cap: LineCap::Round,
                    line_join: LineJoin::Round,
                    ..Stroke::default()
                };
                px.stroke_path(&path, &paint, &st, t, None);
            }
        }
    }
    px
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn failures_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/golden-failures");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Compare (or, with `UPDATE_GOLDEN`, write) the PNG and SVG goldens of one scene.
fn check(name: &str, dl: &DisplayList, view: Rect, w: u32, h: u32) -> Result<(), String> {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    let png_path = golden_dir().join(format!("{name}.png"));
    let svg_path = golden_dir().join(format!("{name}.svg"));
    let actual = rasterise(dl, view, w, h);
    let svg = dl.to_svg(view);
    if update {
        actual.save_png(&png_path).unwrap();
        std::fs::write(&svg_path, &svg).unwrap();
        return Ok(());
    }
    let mut errors = vec![];
    match std::fs::read_to_string(&svg_path) {
        Ok(g) if g == svg => {}
        Ok(_) => {
            std::fs::write(failures_dir().join(format!("{name}.svg")), &svg).unwrap();
            errors.push(format!("{name}.svg differs"));
        }
        Err(e) => errors.push(format!("{name}.svg: {e} (run with UPDATE_GOLDEN=1)")),
    }
    let golden = std::fs::read(&png_path)
        .map_err(|e| format!("{name}.png: {e} (run with UPDATE_GOLDEN=1)"))
        .and_then(|b| Pixmap::decode_png(&b).map_err(|e| format!("{name}.png: {e}")))?;
    if (golden.width(), golden.height()) != (w, h) {
        errors.push(format!(
            "{name}.png has size {}x{}, expected {w}x{h}",
            golden.width(),
            golden.height()
        ));
    } else {
        let mut diff = Pixmap::new(w, h).unwrap();
        let mut bad = 0usize;
        for (i, (a, g)) in actual
            .data()
            .chunks_exact(4)
            .zip(golden.data().chunks_exact(4))
            .enumerate()
        {
            let d = a.iter().zip(g).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
            if d > TOLERANCE {
                bad += 1;
                diff.data_mut()[i * 4..i * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let share = bad as f64 / f64::from(w * h);
        if share > MAX_BAD_PIXELS {
            actual.save_png(failures_dir().join(format!("{name}.png"))).unwrap();
            diff.save_png(failures_dir().join(format!("{name}-diff.png"))).unwrap();
            errors.push(format!("{name}.png: {bad} pixels differ ({:.3} %)", share * 100.0));
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

/// Pressure-sensitive pen strokes of several widths and colours, at 1:1 and zoomed in.
fn ink_scene() -> Scene {
    let mut s = Scene::new(256.0);
    let colours = [0x1d1d1f, 0x2b59c3, 0xd1495b, 0x2a9d8f, 0xe09f3e];
    for (i, c) in colours.iter().enumerate() {
        let y = 30.0 + i as f64 * 40.0;
        let width = 1.5 + i as f64;
        s.add_stroke(
            wave(20.0, y, 120, 8.0, 2.0, |k| 0.2 + 0.6 * (k as f64 / 120.0)),
            width,
            *c,
            1.0,
            true,
        );
    }
    s
}

#[test]
fn golden_images() {
    let mut failures = vec![];
    let mut run = |name: &str, dl: DisplayList, view: Rect, w: u32, h: u32| {
        if let Err(e) = check(name, &dl, view, w, h) {
            failures.push(e);
        }
    };

    // 1. Ink at 1:1 and at 3× zoom on a detail.
    let s = ink_scene();
    let view = Rect::new(0.0, 0.0, 280.0, 230.0);
    run("ink", s.tile(view, 1.0), view, 280, 230);
    let zoom = Rect::new(30.0, 20.0, 110.0, 90.0);
    run("ink-zoom", s.tile(zoom, 3.0), zoom, 240, 210);

    // 2. Highlighter (translucent, overlapping) over pen writing.
    let mut s = Scene::new(256.0);
    for k in 0..4 {
        s.add_stroke(
            wave(10.0, 30.0 + k as f64 * 18.0, 90, 2.0, 2.0, |_| 0.5),
            2.0,
            0x1d1d1f,
            1.0,
            true,
        );
    }
    s.add_stroke(wave(5.0, 32.0, 95, 0.5, 2.0, |_| 0.5), 16.0, 0xffe066, 0.35, false);
    s.add_stroke(wave(60.0, 50.0, 60, 0.5, 2.0, |_| 0.5), 16.0, 0x7cc6fe, 0.35, false);
    let view = Rect::new(0.0, 0.0, 200.0, 110.0);
    run("highlighter", s.tile(view, 1.0), view, 400, 220);

    // 3. A tidied shape (closed polyline) and an open one, as the core emits them.
    let dl = DisplayList {
        ops: vec![
            DrawOp::Stroke {
                color: 0x2b59c3,
                alpha: 1.0,
                width: 2.5,
                closed: true,
                coords: vec![20.0, 20.0, 120.0, 20.0, 120.0, 80.0, 20.0, 80.0],
            },
            DrawOp::Stroke {
                color: 0xd1495b,
                alpha: 1.0,
                width: 2.0,
                closed: false,
                coords: vec![20.0, 100.0, 70.0, 140.0, 120.0, 100.0],
            },
        ],
    };
    let view = Rect::new(0.0, 0.0, 140.0, 160.0);
    run("shapes", dl, view, 280, 320);

    // 4. Far zoom on many strokes: levels of detail, centre lines and dots.
    let mut r = Rng(42);
    let mut s = Scene::new(256.0);
    for _ in 0..400 {
        let (x, y) = (r.f() * 4000.0, r.f() * 3000.0);
        let len = 5 + (r.f() * 120.0) as usize;
        s.add_stroke(wave(x, y, len, 3.0, 1.5, |_| 0.5), 2.5, 0x1d1d1f, 1.0, true);
    }
    let view = Rect::new(0.0, 0.0, 4000.0, 3000.0);
    run("far", s.tile(view, 0.08), view, 320, 240);

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The golden check itself catches a change (guards against a test that always passes).
#[test]
fn a_changed_image_is_detected() {
    let s = ink_scene();
    let view = Rect::new(0.0, 0.0, 280.0, 230.0);
    let mut dl = s.tile(view, 1.0);
    if let Some(DrawOp::Fill { color, .. }) = dl.ops.first_mut() {
        *color = 0xff0000;
    }
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        return;
    }
    let e = check("ink", &dl, view, 280, 230).unwrap_err();
    assert!(e.contains("ink.svg differs") && e.contains("pixels differ"), "{e}");
}
