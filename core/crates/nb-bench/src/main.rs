//! Performance CI (P1-43): the budgets of `docs/architecture/overview.md` §6 that the core
//! can measure on a desktop, with a regression check against a stored baseline.
//!
//! ```text
//! cargo run --release -p nb-bench                       # measure and print
//! cargo run --release -p nb-bench -- --check            # also compare with baseline.json
//! cargo run --release -p nb-bench -- --write-baseline   # record a new baseline
//! ```
//!
//! **Noise.** CI runners differ in speed from run to run. Every metric is the median of many
//! repetitions, measured in three interleaved rounds of which the best is kept, and is
//! compared *normalised*: divided by the time of a fixed calibration workload measured in
//! the same round, so a uniformly faster or slower machine cancels out. A metric fails when its normalised value is more than 10 % above the
//! baseline's; a failing metric is measured twice more and only fails if all three runs
//! regress. Absolute budgets (ms) are checked too, with the desktop figure standing in for
//! the device until device benchmarks exist (P0-15).

use nb_ffi::{InkTool, NotebookDoc, PageDoc};
use nb_ink::Point;
use nb_render::{Rect, Scene};
use std::path::PathBuf;
use std::time::Instant;

/// Allowed regression of a normalised metric against the baseline.
const MAX_REGRESSION: f64 = 0.10;
/// Metrics dominated by disk flushes or by a tail percentile vary by ±30 % between
/// identical runs on a shared machine (measured: three runs on the dev container); they
/// get a wider band so CI does not fail on noise. Everything else uses [`MAX_REGRESSION`].
const NOISY: [&str; 4] = [
    "commit_stroke_durable",
    "tile_50k_strokes_p95",
    "pdf_first_page_shown",
    "pdf_import_300_pages",
];
const NOISY_REGRESSION: f64 = 0.35;
/// Interleaved measurement rounds (best kept).
const ROUNDS: usize = 3;

struct Metric {
    name: &'static str,
    /// Budget in ms (overview §6), checked against the measured median.
    budget_ms: f64,
    what: &'static str,
    run: fn(&Ctx) -> f64,
}

/// Shared fixtures (built once, outside the timings).
struct Ctx {
    dir: PathBuf,
    scene: Scene,
    page_2000: String,
    pdf_300: Vec<u8>,
    pdf_300_path: String,
    reps: usize,
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn percentile(mut v: Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

struct Rng(u64);
impl Rng {
    fn f(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn scribble(r: &mut Rng, x: f64, y: f64, n: usize) -> Vec<Point> {
    let (mut x, mut y, mut a) = (x, y, r.f() * std::f64::consts::TAU);
    (0..n)
        .map(|_| {
            a += (r.f() - 0.5) * 0.5;
            x += a.cos() * 1.5;
            y += a.sin() * 1.5;
            Point::new(x, y, 0.3 + r.f() * 0.5)
        })
        .collect()
}

/// Fixed CPU + memory workload used to normalise the other metrics.
fn calibration() -> f64 {
    let t = Instant::now();
    let mut v: Vec<u64> = (0..200_000u64).map(|i| i.wrapping_mul(0x9E37_79B9_7F4A_7C15)).collect();
    v.sort_unstable();
    let mut acc = 0f64;
    for (i, x) in v.iter().enumerate() {
        acc += ((*x >> 40) as f64).sqrt() * (i as f64).sin();
    }
    std::hint::black_box(acc);
    ms(t)
}

fn stroke_into(page: &PageDoc, pts: &[Point]) {
    page.begin_stroke(InkTool::Pen, 0x1d1d1f, 3.0, pts[0].x, pts[0].y, pts[0].p);
    page.add_points(pts[1..].iter().flat_map(|p| [p.x, p.y, p.p]).collect());
    page.end_stroke(1.0, false);
}

fn metrics() -> Vec<Metric> {
    vec![
        Metric {
            name: "open_page_2000_strokes",
            budget_ms: 400.0,
            what: "open a stored page with 2 000 strokes and build the geometry of every stroke",
            run: |c| {
                median(
                    (0..c.reps.min(9))
                        .map(|_| {
                            let t = Instant::now();
                            let p = PageDoc::open(c.page_2000.clone()).expect("open");
                            let n: usize = p
                                .layout()
                                .into_iter()
                                .filter_map(|k| p.geometry(k))
                                .map(|g| g.coords.len())
                                .sum();
                            std::hint::black_box(n);
                            ms(t)
                        })
                        .collect(),
                )
            },
        },
        Metric {
            name: "tile_50k_strokes_p95",
            budget_ms: 18.0,
            // The frame budget applies to tiles whose geometry is cached (the app caches tiles
            // as bitmaps); a tile's first build also computes its LOD outlines (P0-05: up to
            // ~6 ms per tile at 0.05×), which is not timed here.
            what: "p95 time of one 512 px tile display list, geometry cached, 50 000-stroke canvas (zooms 4 … 0.05)",
            run: |c| {
                let mut times = vec![];
                for scale in [4.0f64, 1.0, 0.25, 0.05] {
                    let side = 512.0 / scale;
                    for k in 0..c.reps * 2 {
                        let x = 1000.0 + (k as f64 * 7919.0) % (18_000.0 - side).max(1.0);
                        let y = 1000.0 + (k as f64 * 104_729.0) % (18_000.0 - side).max(1.0);
                        let view = Rect::new(x, y, x + side, y + side);
                        std::hint::black_box(c.scene.tile(view, scale));
                        let t = Instant::now();
                        std::hint::black_box(c.scene.tile(view, scale));
                        times.push(ms(t));
                    }
                }
                percentile(times, 0.95)
            },
        },
        Metric {
            name: "commit_stroke_durable",
            budget_ms: 1000.0,
            what: "pen-up of a 200-point stroke until it is written to the page database",
            run: |c| {
                let p = PageDoc::open(c.dir.join("commit.kpage").to_string_lossy().into()).expect("open");
                let mut r = Rng(3);
                median(
                    (0..c.reps * 3)
                        .map(|_| {
                            let pts = scribble(&mut r, 300.0, 300.0, 200);
                            p.begin_stroke(InkTool::Pen, 0x1d1d1f, 3.0, pts[0].x, pts[0].y, pts[0].p);
                            p.add_points(pts[1..].iter().flat_map(|q| [q.x, q.y, q.p]).collect());
                            let t = Instant::now();
                            p.end_stroke(1.0, false);
                            ms(t)
                        })
                        .collect(),
                )
            },
        },
        Metric {
            name: "shape_recognition",
            budget_ms: 50.0,
            what: "recognise a hand-drawn circle (300 points) on pen-up",
            run: |c| {
                let mut r = Rng(5);
                let circle: Vec<Point> = (0..300)
                    .map(|k| {
                        let a = k as f64 / 299.0 * std::f64::consts::TAU;
                        let rad = 80.0 + (r.f() - 0.5) * 4.0;
                        Point::new(200.0 + rad * a.cos(), 200.0 + rad * a.sin(), 0.5)
                    })
                    .collect();
                median(
                    (0..c.reps * 5)
                        .map(|_| {
                            let t = Instant::now();
                            std::hint::black_box(nb_recog::recognize(&circle));
                            ms(t)
                        })
                        .collect(),
                )
            },
        },
        Metric {
            name: "pdf_300_pages_first_page",
            budget_ms: 2000.0,
            what: "parse a 300-page PDF, size its first page and read its text layer",
            run: |c| {
                median(
                    (0..c.reps.min(9))
                        .map(|_| {
                            let t = Instant::now();
                            let pdf = nb_pdf::PdfDoc::load(&c.pdf_300).expect("pdf");
                            std::hint::black_box((pdf.page_info(0), pdf.words(0)));
                            ms(t)
                        })
                        .collect(),
                )
            },
        },
        Metric {
            name: "pdf_first_page_shown",
            budget_ms: 2000.0,
            what: "300-page PDF: import its first page (the app shows it, then imports the rest)",
            run: |c| {
                median(
                    (0..c.reps.min(9))
                        .map(|i| {
                            let dir = c.dir.join(format!("first-{i}"));
                            let _ = std::fs::remove_dir_all(&dir);
                            let nb = NotebookDoc::open(dir.to_string_lossy().into(), "B".into()).expect("nb");
                            let size = nb_ffi::PdfPageSize {
                                width_pt: 595.0,
                                height_pt: 842.0,
                            };
                            let t = Instant::now();
                            nb_ffi::pdf_import(nb, None, c.pdf_300_path.clone(), vec![size], 0).expect("import");
                            let v = ms(t);
                            let _ = std::fs::remove_dir_all(&dir);
                            v
                        })
                        .collect(),
                )
            },
        },
        Metric {
            name: "pdf_import_300_pages",
            // Background work after the first page is shown: tracked for regressions only.
            budget_ms: 60_000.0,
            what: "import a 300-page PDF into a notebook (store once, 300 pages, text layers)",
            run: |c| {
                median(
                    (0..3)
                        .map(|i| {
                            let dir = c.dir.join(format!("import-{i}-{}", std::process::id()));
                            let _ = std::fs::remove_dir_all(&dir);
                            let nb = NotebookDoc::open(dir.to_string_lossy().into(), "B".into()).expect("nb");
                            let t = Instant::now();
                            nb_ffi::pdf_import(nb, None, c.pdf_300_path.clone(), vec![], 0).expect("import");
                            let v = ms(t);
                            let _ = std::fs::remove_dir_all(&dir);
                            v
                        })
                        .collect(),
                )
            },
        },
    ]
}

fn setup(reps: usize) -> Ctx {
    let dir = std::env::temp_dir().join(format!("kahie-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let mut r = Rng(7);
    let mut scene = Scene::new(256.0);
    for _ in 0..50_000 {
        let n = 40 + (r.f() * 160.0) as usize;
        let (x, y) = (r.f() * 20_000.0, r.f() * 20_000.0);
        scene.add_stroke(scribble(&mut r, x, y, n), 2.5, 0x1d1d1f, 1.0, true);
    }
    let page_2000 = dir.join("p2000.kpage").to_string_lossy().into_owned();
    {
        let p = PageDoc::open(page_2000.clone()).expect("page");
        for k in 0..2000 {
            let pts = scribble(&mut r, 40.0 + (k % 25) as f64 * 30.0, 40.0 + (k / 25) as f64 * 13.0, 60);
            stroke_into(&p, &pts);
        }
        p.save().expect("save");
    }
    let pdf_300 = nb_pdf::testgen::generate(&nb_pdf::testgen::Spec::simple("bench", 300));
    let pdf_path = dir.join("300.pdf");
    std::fs::write(&pdf_path, &pdf_300).expect("write pdf");
    Ctx {
        dir,
        scene,
        page_2000,
        pdf_300,
        pdf_300_path: pdf_path.to_string_lossy().into_owned(),
        reps,
    }
}

fn baseline_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("baseline.json")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let check = args.iter().any(|a| a == "--check");
    let write = args.iter().any(|a| a == "--write-baseline");
    let reps = if args.iter().any(|a| a == "--quick") { 5 } else { 21 };
    let json_out = args
        .iter()
        .position(|a| a == "--json")
        .and_then(|i| args.get(i + 1))
        .cloned();

    eprintln!("setting up fixtures…");
    let ctx = setup(reps);
    // Each metric is measured in ROUNDS interleaved rounds, each round with a fresh
    // calibration; the best (lowest) normalised value is kept, which filters out rounds
    // slowed down by other work on the runner.
    let calib = || {
        let v: Vec<f64> = (0..15).map(|_| calibration()).collect();
        v.into_iter().fold(f64::INFINITY, f64::min)
    };
    let all = metrics();
    let mut results = serde_json::Map::new();
    let measure = |m: &Metric, cal: f64| {
        let v = (m.run)(&ctx);
        (v, v / cal)
    };
    let mut measured = vec![(f64::INFINITY, f64::INFINITY); all.len()];
    let mut cal = f64::INFINITY;
    for _ in 0..ROUNDS {
        let c = calib();
        cal = cal.min(c);
        for (i, m) in all.iter().enumerate() {
            let (v, n) = measure(m, c);
            if n < measured[i].1 {
                measured[i] = (v, n);
            }
        }
    }

    let baseline: serde_json::Value = std::fs::read(baseline_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or(serde_json::Value::Null);
    // Normalised figures still depend on the machine type (CPU vs disk speed): compare only
    // with a baseline recorded on the same kind of runner (KAHIE_BENCH_RUNNER).
    let runner = std::env::var("KAHIE_BENCH_RUNNER").unwrap_or_else(|_| "local".into());
    let comparable = baseline["runner"].as_str() == Some(runner.as_str());
    if check && !comparable {
        println!(
            "note: baseline recorded on {:?}, this is {runner:?}: only absolute budgets are checked; \
             record a baseline here with --write-baseline\n",
            baseline["runner"].as_str().unwrap_or("nothing")
        );
    }
    println!("calibration: {cal:.2} ms\n");
    println!("| Metric | Median (ms) | Budget (ms) | Normalised | Baseline | Change |");
    println!("|---|---|---|---|---|---|");
    let mut failures = vec![];
    for (i, m) in all.iter().enumerate() {
        let (mut v, mut n) = measured[i];
        let base = baseline["metrics"][m.name]["normalised"].as_f64();
        let tolerance = if NOISY.contains(&m.name) {
            NOISY_REGRESSION
        } else {
            MAX_REGRESSION
        };
        let regressed = |n: f64| comparable && base.is_some_and(|b| n > b * (1.0 + tolerance));
        if check && regressed(n) {
            // Retry twice before calling it a regression (runner noise).
            for _ in 0..2 {
                cal = calib();
                let (v2, n2) = measure(m, cal);
                if n2 < n {
                    (v, n) = (v2, n2);
                }
                if !regressed(n) {
                    break;
                }
            }
        }
        let change = base.map_or("—".to_string(), |b| format!("{:+.1} %", (n / b - 1.0) * 100.0));
        println!(
            "| {} | {v:.3} | {} | {n:.4} | {} | {change} |",
            m.name,
            m.budget_ms,
            base.map_or("—".into(), |b| format!("{b:.4}"))
        );
        if v > m.budget_ms {
            failures.push(format!(
                "{}: {v:.1} ms over the {} ms budget ({})",
                m.name, m.budget_ms, m.what
            ));
        }
        if check && regressed(n) {
            failures.push(format!(
                "{}: {:.1} % slower than the baseline (normalised {n:.4} vs {:.4})",
                m.name,
                (n / base.unwrap_or(n) - 1.0) * 100.0,
                base.unwrap_or(0.0)
            ));
        }
        results.insert(
            m.name.into(),
            serde_json::json!({ "median_ms": v, "normalised": n, "budget_ms": m.budget_ms, "what": m.what }),
        );
    }
    let report = serde_json::json!({
        "calibration_ms": cal,
        "reps": reps,
        "metrics": results,
        "runner": runner,
        "note": "normalised = median_ms / calibration_ms; regenerate with --write-baseline on the CI runner type",
    });
    if let Some(p) = json_out {
        std::fs::write(p, serde_json::to_vec_pretty(&report).expect("json")).expect("write json");
    }
    if write {
        std::fs::write(baseline_path(), serde_json::to_vec_pretty(&report).expect("json")).expect("write baseline");
        println!("\nbaseline written to {}", baseline_path().display());
    }
    let _ = std::fs::remove_dir_all(&ctx.dir);
    if !failures.is_empty() {
        eprintln!("\nPerformance check failed:\n{}", failures.join("\n"));
        std::process::exit(1);
    }
}
