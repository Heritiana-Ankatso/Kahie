//! P0-05 benchmark: 50 000 strokes on an infinite canvas.
//! `cargo run --release -p nb-render --example bench_50k`

use nb_ink::Point;
use nb_render::{Rect, Scene};
use std::time::Instant;

struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn main() {
    let mut r = Rng(7);
    let n = 50_000;
    let world = 20_000.0;
    let mut strokes = Vec::with_capacity(n);
    for _ in 0..n {
        let (mut x, mut y, mut a) = (r.f() * world, r.f() * world, r.f() * std::f64::consts::TAU);
        let len = 40 + (r.f() * 160.0) as usize;
        strokes.push(
            (0..len)
                .map(|_| {
                    a += (r.f() - 0.5) * 0.5;
                    x += a.cos() * 1.5;
                    y += a.sin() * 1.5;
                    Point::new(x, y, 0.3 + r.f() * 0.5)
                })
                .collect::<Vec<_>>(),
        );
    }
    let points: usize = strokes.iter().map(Vec::len).sum();
    let t = Instant::now();
    let mut scene = Scene::new(256.0);
    for s in strokes {
        scene.add_stroke(s, 2.5, 0x1d1d1f, 1.0, true);
    }
    println!("# P0-05 core measurements\n");
    println!("{n} strokes, {points} points, world {world}×{world} page units\n");
    println!("Index build: {:.0} ms\n", t.elapsed().as_secs_f64() * 1000.0);
    println!("| Zoom (px/unit) | Tile (page units) | Strokes | Vertices | Cold (ms) | Warm (ms) |");
    println!("|---|---|---|---|---|---|");
    // A 512×512 px tile at different zooms; the page-space size grows as we zoom out.
    for scale in [4.0, 1.0, 0.25, 0.05, 0.01] {
        let side = 512.0 / scale;
        let mut cold = vec![];
        let mut warm = vec![];
        let mut last = (0, 0);
        for k in 0..20 {
            let x = 1000.0 + (k as f64 * 7919.0) % (world - side - 2000.0).max(1.0);
            let y = 1000.0 + (k as f64 * 104_729.0) % (world - side - 2000.0).max(1.0);
            let view = Rect::new(x, y, x + side, y + side);
            let t = Instant::now();
            let dl = scene.tile(view, scale);
            cold.push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            let dl2 = scene.tile(view, scale);
            warm.push(t.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(dl.ops.len(), dl2.ops.len());
            last = (dl.ops.len(), dl.vertices());
        }
        cold.sort_by(f64::total_cmp);
        warm.sort_by(f64::total_cmp);
        println!(
            "| {scale} | {side:.0} | {} | {} | {:.2} | {:.2} |",
            last.0, last.1, cold[10], warm[10]
        );
    }
}
