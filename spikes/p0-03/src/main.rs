//! P0-03 — Loro validation benchmark.
//!
//! Runs the scenarios from docs/backlog/phase-0.md (P0-03) against Loro and prints
//! a Markdown report fragment. Spike code: not for production crates.
//!
//! Usage: cargo run --release [-- --strokes N]

use loro::{ExportMode, LoroDoc, LoroValue, TreeParentId, UndoManager};
use std::time::Instant;

// ---------- helpers ----------

/// Resident memory of this process in MiB (Linux).
fn rss_mib() -> f64 {
    let s = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    s.lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|kb| kb.parse::<f64>().ok())
        .map(|kb| kb / 1024.0)
        .unwrap_or(f64::NAN)
}

fn mib(bytes: usize) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Deterministic xorshift PRNG (no external dependency, reproducible runs).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn f(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Zig-zag + LEB128 varint.
fn put_varint(out: &mut Vec<u8>, v: i32) {
    let mut z = ((v << 1) ^ (v >> 31)) as u32;
    loop {
        let b = (z & 0x7f) as u8;
        z >>= 7;
        if z == 0 {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
}

/// Packed ink encoding candidate: points quantised to 1/8 page unit (x, y) and
/// 1/255 pressure, delta-coded as zig-zag varints. Header: colour (u32 LE), width (u8 ×4).
fn encode_stroke(points: &[(f32, f32, f32)], color: u32, width: f32) -> Vec<u8> {
    let mut out = Vec::with_capacity(5 + points.len() * 4);
    out.extend_from_slice(&color.to_le_bytes());
    out.push((width * 4.0).round().clamp(0.0, 255.0) as u8);
    let (mut px, mut py, mut pp) = (0i32, 0i32, 0i32);
    for &(x, y, p) in points {
        let (qx, qy, qp) = ((x * 8.0).round() as i32, (y * 8.0).round() as i32, (p * 255.0).round() as i32);
        put_varint(&mut out, qx - px);
        put_varint(&mut out, qy - py);
        put_varint(&mut out, qp - pp);
        (px, py, pp) = (qx, qy, qp);
    }
    out
}

/// A handwriting-like stroke: smooth random walk, ~1–2 page units per sample.
fn synth_stroke(rng: &mut Rng, n: usize) -> Vec<(f32, f32, f32)> {
    let (mut x, mut y) = (rng.f() * 794.0, rng.f() * 1123.0);
    let mut a = rng.f() * std::f64::consts::TAU;
    let mut p = 0.5;
    (0..n)
        .map(|_| {
            a += (rng.f() - 0.5) * 0.6;
            let step = 1.0 + rng.f();
            x += a.cos() * step;
            y += a.sin() * step;
            p = (p + (rng.f() - 0.5) * 0.05).clamp(0.05, 1.0);
            (x as f32, y as f32, p as f32)
        })
        .collect()
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

// ---------- scenarios ----------

/// S1: 100 000 strokes × 150 points. Each stroke is one entry in a page map:
/// key = stroke id, value = packed binary. One commit per 100 strokes (a "session").
fn s1_large_document(n_strokes: usize) {
    println!("## S1 — {n_strokes} strokes × 150 points\n");
    let mut rng = Rng(0x5eed);
    let doc = LoroDoc::new();
    doc.set_peer_id(1).unwrap();
    let strokes = doc.get_map("strokes");
    let order = doc.get_movable_list("order");
    let mut raw_points = 0usize;
    let mut encoded = 0usize;
    let rss0 = rss_mib();
    let t = Instant::now();
    for i in 0..n_strokes {
        let pts = synth_stroke(&mut rng, 150);
        raw_points += pts.len();
        let blob = encode_stroke(&pts, 0x1d1d1f, 3.0);
        encoded += blob.len();
        let key = format!("s{i:06x}");
        strokes.insert(&key, LoroValue::Binary(blob.into())).unwrap();
        order.push(key).unwrap();
        if i % 100 == 99 {
            doc.commit();
        }
    }
    doc.commit();
    let build_ms = ms(t);
    let rss_built = rss_mib() - rss0;

    let t = Instant::now();
    let snapshot = doc.export(ExportMode::Snapshot).unwrap();
    let export_ms = ms(t);
    let state_only = doc.export(ExportMode::StateOnly(None)).unwrap();
    let raw_f32 = raw_points * 12;
    drop(doc);

    let rss1 = rss_mib();
    let t = Instant::now();
    let loaded = LoroDoc::new();
    loaded.import(&snapshot).unwrap();
    let import_ms = ms(t);
    // Touch the state so lazy loading is included.
    let t2 = Instant::now();
    let n = loaded.get_map("strokes").len();
    let first_read_ms = ms(t2);
    let rss_loaded = rss_mib() - rss1;
    assert_eq!(n, n_strokes);

    let t = Instant::now();
    let shallow = LoroDoc::new();
    shallow.import(&state_only).unwrap();
    let _ = shallow.get_map("strokes").len();
    let import_state_ms = ms(t);

    println!("| Measure | Value |\n|---|---|");
    println!("| Points | {raw_points} |");
    println!("| Ink encoding (packed) | {:.2} bytes/point (target ≤ 6) |", encoded as f64 / raw_points as f64);
    println!("| Raw f32 (x,y,p) size | {:.1} MiB |", mib(raw_f32));
    println!("| Build time (insert + commits) | {build_ms:.0} ms |");
    println!("| RSS growth while building | {rss_built:.0} MiB |");
    println!("| Snapshot with history | {:.1} MiB (export {export_ms:.0} ms) |", mib(snapshot.len()));
    println!("| State-only snapshot (no history) | {:.1} MiB |", mib(state_only.len()));
    println!("| Load snapshot (import) | {import_ms:.0} ms |");
    println!("| First state read after load | {first_read_ms:.0} ms |");
    println!("| Load state-only snapshot + read | {import_state_ms:.0} ms |");
    println!("| RSS growth after load | {rss_loaded:.0} MiB |\n");
}

/// S2: two replicas diverge offline (5 000 ops each), then merge both ways.
fn s2_divergence() {
    println!("## S2 — Offline divergence: 5 000 ops per replica\n");
    let mut rng = Rng(42);
    let base = LoroDoc::new();
    base.set_peer_id(1).unwrap();
    let strokes = base.get_map("strokes");
    let order = base.get_movable_list("order");
    for i in 0..2_000 {
        let k = format!("b{i}");
        strokes.insert(&k, LoroValue::Binary(encode_stroke(&synth_stroke(&mut rng, 150), 0, 3.0).into())).unwrap();
        order.push(k).unwrap();
    }
    base.commit();
    let snap = base.export(ExportMode::Snapshot).unwrap();

    let make = |peer: u64, seed: u64| {
        let d = LoroDoc::new();
        d.import(&snap).unwrap();
        d.set_peer_id(peer).unwrap();
        let mut r = Rng(seed);
        let m = d.get_map("strokes");
        let o = d.get_movable_list("order");
        for i in 0..5_000 {
            match r.below(10) {
                // 70 % add a stroke, 20 % delete one, 10 % reorder.
                0..=6 => {
                    let k = format!("p{peer}-{i}");
                    m.insert(&k, LoroValue::Binary(encode_stroke(&synth_stroke(&mut r, 150), 0, 3.0).into())).unwrap();
                    o.push(k).unwrap();
                }
                7 | 8 if !o.is_empty() => {
                    let idx = r.below(o.len());
                    if let Some(v) = o.get(idx) {
                        if let Ok(LoroValue::String(k)) = v.into_value() {
                            m.delete(&k).unwrap();
                        }
                    }
                    o.delete(idx, 1).unwrap();
                }
                _ if o.len() > 1 => {
                    let (a, b) = (r.below(o.len()), r.below(o.len()));
                    o.mov(a, b).unwrap();
                }
                _ => {}
            }
            if i % 50 == 49 {
                d.commit();
            }
        }
        d.commit();
        d
    };
    let a = make(2, 7);
    let b = make(3, 11);
    let ua = a.export(ExportMode::updates(&b.oplog_vv())).unwrap();
    let ub = b.export(ExportMode::updates(&a.oplog_vv())).unwrap();
    let t = Instant::now();
    a.import(&ub).unwrap();
    let ta = ms(t);
    let t = Instant::now();
    b.import(&ua).unwrap();
    let tb = ms(t);
    let converged = a.get_deep_value() == b.get_deep_value();
    println!("| Measure | Value |\n|---|---|");
    println!("| Update size A→B / B→A | {:.1} / {:.1} MiB |", mib(ua.len()), mib(ub.len()));
    println!("| Merge time into A / into B | {ta:.0} / {tb:.0} ms |");
    println!("| Converged (deep value equal) | {converged} |\n");
    assert!(converged);
}

/// S3: movable tree (sections/pages) with concurrent moves, including a cycle attempt.
fn s3_tree() {
    println!("## S3 — Movable tree: concurrent moves\n");
    let a = LoroDoc::new();
    a.set_peer_id(1).unwrap();
    let t = a.get_tree("library");
    t.enable_fractional_index(0);
    let x = t.create(TreeParentId::Root).unwrap();
    let y = t.create(TreeParentId::Root).unwrap();
    let pages: Vec<_> = (0..200).map(|i| t.create(if i % 2 == 0 { x } else { y }).unwrap()).collect();
    a.commit();
    let b = LoroDoc::new();
    b.import(&a.export(ExportMode::Snapshot).unwrap()).unwrap();
    b.set_peer_id(2).unwrap();
    // Concurrent: A moves X under Y, B moves Y under X (would create a cycle).
    a.get_tree("library").mov(x, y).unwrap();
    b.get_tree("library").mov(y, x).unwrap();
    // Concurrent page shuffles on both sides.
    let mut r = Rng(3);
    for _ in 0..1_000 {
        let p = pages[r.below(pages.len())];
        let dst = if r.below(2) == 0 { x } else { y };
        let doc = if r.below(2) == 0 { &a } else { &b };
        doc.get_tree("library").mov(p, dst).unwrap();
    }
    a.commit();
    b.commit();
    let t0 = Instant::now();
    a.import(&b.export(ExportMode::updates(&a.oplog_vv())).unwrap()).unwrap();
    b.import(&a.export(ExportMode::updates(&b.oplog_vv())).unwrap()).unwrap();
    let merge = ms(t0);
    let converged = a.get_deep_value() == b.get_deep_value();
    let ta = a.get_tree("library");
    let px = ta.parent(x);
    let py = ta.parent(y);
    let acyclic = !(px == Some(TreeParentId::Node(y)) && py == Some(TreeParentId::Node(x)));
    let total_pages = ta.children(x).map_or(0, |c| c.len()) + ta.children(y).map_or(0, |c| c.len());
    println!("| Measure | Value |\n|---|---|");
    println!("| Merge time (both directions) | {merge:.1} ms |");
    println!("| Converged | {converged} |");
    println!("| No cycle after concurrent X→Y / Y→X | {acyclic} |");
    println!("| Pages still reachable (should be 200 + 1 section) | {total_pages} |\n");
    assert!(converged && acyclic);
}

/// S4: rich text with concurrent inserts and overlapping marks.
fn s4_rich_text() {
    println!("## S4 — Rich text: concurrent edits\n");
    let a = LoroDoc::new();
    a.set_peer_id(1).unwrap();
    a.get_text("t").insert(0, "Théorème de Pythagore : a² + b² = c²").unwrap();
    a.commit();
    let b = LoroDoc::new();
    b.import(&a.export(ExportMode::Snapshot).unwrap()).unwrap();
    b.set_peer_id(2).unwrap();
    let ta = a.get_text("t");
    let tb = b.get_text("t");
    ta.mark(0..8, "bold", true).unwrap();
    tb.insert(8, " fondamental").unwrap();
    tb.mark(4..20, "italic", true).unwrap();
    ta.insert(0, "Rappel. ").unwrap();
    a.commit();
    b.commit();
    let mut r = Rng(9);
    for i in 0..2_000 {
        let (d, t) = if i % 2 == 0 { (&a, &ta) } else { (&b, &tb) };
        let len = t.len_unicode();
        t.insert(r.below(len + 1), "x").unwrap();
        if i % 100 == 99 {
            d.commit();
        }
    }
    a.commit();
    b.commit();
    let t0 = Instant::now();
    a.import(&b.export(ExportMode::updates(&a.oplog_vv())).unwrap()).unwrap();
    b.import(&a.export(ExportMode::updates(&b.oplog_vv())).unwrap()).unwrap();
    let merge = ms(t0);
    let same = ta.to_delta() == tb.to_delta();
    println!("| Measure | Value |\n|---|---|");
    println!("| Merge time | {merge:.1} ms |");
    println!("| Same text and marks on both replicas | {same} |");
    println!("| Final length (chars) | {} |\n", ta.len_unicode());
    assert!(same);
}

/// S5: history — checkout version N, revert one change (undo), attribution per peer.
fn s5_history() {
    println!("## S5 — History, revert and attribution\n");
    let doc = LoroDoc::new();
    doc.set_peer_id(7).unwrap();
    doc.set_record_timestamp(true);
    let m = doc.get_map("strokes");
    let mut versions = Vec::new();
    for i in 0..1_000 {
        m.insert(&format!("s{i}"), i as i64).unwrap();
        doc.commit();
        versions.push(doc.oplog_frontiers());
    }
    let t = Instant::now();
    doc.checkout(&versions[499]).unwrap();
    let checkout_ms = ms(t);
    let at_500 = doc.get_map("strokes").len();
    doc.try_checkout_to_latest().unwrap();

    // Undo of the last local change, per-user (UndoManager only undoes local ops).
    let other = LoroDoc::new();
    other.import(&doc.export(ExportMode::Snapshot).unwrap()).unwrap();
    other.set_peer_id(8).unwrap();
    let mut undo = UndoManager::new(&doc);
    m.insert("mine", 1).unwrap();
    doc.commit();
    other.get_map("strokes").insert("theirs", 1).unwrap();
    other.commit();
    doc.import(&other.export(ExportMode::updates(&doc.oplog_vv())).unwrap()).unwrap();
    undo.undo().unwrap();
    let mine_gone = m.get("mine").is_none();
    let theirs_kept = m.get("theirs").is_some();

    // Attribution: which peer authored the change that last set "theirs".
    let last = other.oplog_frontiers();
    let id = last.iter().next().unwrap();
    let author = doc.get_change(id).map(|c| c.id.peer);

    println!("| Measure | Value |\n|---|---|");
    println!("| Checkout version 500 of 1 000 | {checkout_ms:.1} ms ({at_500} entries) |");
    println!("| Per-user undo removes own change | {mine_gone} |");
    println!("| …and keeps the other user's change | {theirs_kept} |");
    println!("| Author of remote change (expected peer 8) | {author:?} |\n");
    assert!(at_500 == 500 && mine_gone && theirs_kept && author == Some(8));
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n = args
        .iter()
        .position(|a| a == "--strokes")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(100_000);
    println!("# P0-03 measurements (Loro 1.18)\n");
    s1_large_document(n);
    s2_divergence();
    s3_tree();
    s4_rich_text();
    s5_history();
}
