//! P0-10 simulation: one teacher and 30 students drawing on one shared page through the relay.
//!
//! `cargo run --release -p nb-relay --example classroom [-- --minutes 10 --speedup 10]`
//!
//! Rates: each student commits a stroke every 10 s on average, the teacher every 3 s; strokes
//! have 150 points. Time is compressed by `speedup` (10 minutes in 60 s by default), which
//! makes the load 10× denser than a real lesson: latencies are pessimistic.
//!
//! All 31 clients share this machine, so only the teacher and one student are **full**
//! replicas (import everything, checked for convergence); the 29 others send real concurrent
//! updates and receive every frame but do not import them. That measures the relay, not the
//! simulator's CPU.

use futures_util::{SinkExt, StreamExt};
use nb_ink::Point;
use nb_model::crdt::CrdtPage;
use nb_model::{Content, Item, Op, Stroke, Tool};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_tungstenite::tungstenite::Message;

struct Stats {
    sent: Mutex<HashMap<u64, Instant>>,
    latencies: Mutex<Vec<f64>>,
    bytes_up: AtomicU64,
    bytes_down: AtomicU64,
    strokes: AtomicU64,
    disconnects: AtomicU64,
}

fn key(b: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
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

fn stroke(id: String, r: &mut Rng) -> Item {
    let (mut x, mut y, mut a) = (r.f() * 700.0, r.f() * 1000.0, r.f() * 6.0);
    Item {
        id,
        content: Content::Stroke(Stroke {
            frame: nb_model::IDENTITY,
            tool: Tool::Pen,
            color: 0x1d1d1f,
            width: 3.0,
            points: (0..150)
                .map(|_| {
                    a += (r.f() - 0.5) * 0.4;
                    x += a.cos();
                    y += a.sin();
                    Point::new(x, y, 0.5)
                })
                .collect(),
            interp: None,
        }),
    }
}

/// One participant on its own thread (CRDT pages are not shared across threads).
fn participant(
    base: String,
    who: u64,
    full: bool,
    init: Vec<u8>,
    period_s: f64,
    duration: Duration,
    stats: Arc<Stats>,
) -> CrdtPage {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async move {
        let (ws, _) = tokio_tungstenite::connect_async(format!("{base}/sync/class?after=0"))
            .await
            .unwrap();
        let (mut tx, mut rx) = ws.split();
        let mut page = CrdtPage::load(&[&init], who + 10).unwrap();
        let mut relay = page.version();
        let mut r = Rng(who * 7919 + 1);
        let start = Instant::now();
        let mut next = Duration::from_secs_f64(r.f() * period_s);
        let mut i = 0;
        let mut last_msg = Instant::now();
        loop {
            // After the lesson: stop once the backlog is drained (2 s of silence), 60 s at most.
            if start.elapsed() > duration
                && (last_msg.elapsed() > Duration::from_secs(2) || start.elapsed() > duration + Duration::from_secs(60))
            {
                break;
            }
            let wait = next.saturating_sub(start.elapsed());
            tokio::select! {
                msg = rx.next() => {
                    let b = match msg {
                        Some(Ok(Message::Binary(b))) => b,
                        Some(Ok(_)) => continue,
                        // Dropped by the relay (too slow) or network error.
                        _ => { stats.disconnects.fetch_add(1, Ordering::Relaxed); break; }
                    };
                    stats.bytes_down.fetch_add(b.len() as u64, Ordering::Relaxed);
                    last_msg = Instant::now();
                    let upd = &b[8..];
                    if upd.is_empty() { continue; } // ack
                    if let Some(t0) = stats.sent.lock().unwrap().get(&key(upd)) {
                        stats.latencies.lock().unwrap().push(t0.elapsed().as_secs_f64() * 1000.0);
                    }
                    if full {
                        page.sync_incoming(upd, &mut relay).unwrap();
                    }
                }
                _ = tokio::time::sleep(wait), if start.elapsed() < duration => {
                    let op = Op::add(page.page(), vec![stroke(format!("w{who}-{i}"), &mut r)]);
                    i += 1;
                    page.apply(&op);
                    let upd = page.sync_outgoing(&mut relay).unwrap();
                    stats.sent.lock().unwrap().insert(key(&upd), Instant::now());
                    stats.bytes_up.fetch_add(upd.len() as u64, Ordering::Relaxed);
                    stats.strokes.fetch_add(1, Ordering::Relaxed);
                    tx.send(Message::Binary(upd.into())).await.unwrap();
                    // Exponential inter-arrival times (Poisson process).
                    next += Duration::from_secs_f64(-period_s * (1.0 - r.f()).ln());
                }
                _ = tokio::time::sleep(Duration::from_millis(50)), if start.elapsed() >= duration => {
                    // Idle tick after the lesson: the loop head decides when to stop.
                }
            }
        }
        page
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |name: &str, d: f64| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let minutes = arg("--minutes", 10.0);
    let speedup = arg("--speedup", 10.0);
    let duration = Duration::from_secs_f64(minutes * 60.0 / speedup);

    let dir = std::env::temp_dir().join(format!("nb-relay-class-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let listener = rt.block_on(tokio::net::TcpListener::bind("127.0.0.1:0")).unwrap();
    let base = format!("ws://{}", listener.local_addr().unwrap());
    rt.spawn(async move {
        axum::serve(
            listener,
            nb_relay::router(nb_relay::Config {
                data_dir: dir.clone(),
                token: None,
            }),
        )
        .await
        .unwrap()
    });

    // The teacher creates the page and uploads its initial state.
    let mut teacher_page = CrdtPage::new("board", 1);
    let init = teacher_page.take_unsaved().unwrap();
    rt.block_on(async {
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("{base}/sync/class?after=0"))
            .await
            .unwrap();
        ws.send(Message::Binary(init.clone().into())).await.unwrap();
        let _ack = ws.next().await;
    });

    let stats = Arc::new(Stats {
        sent: Mutex::new(HashMap::new()),
        latencies: Mutex::new(vec![]),
        bytes_up: AtomicU64::new(0),
        bytes_down: AtomicU64::new(0),
        strokes: AtomicU64::new(0),
        disconnects: AtomicU64::new(0),
    });
    let t = Instant::now();
    let handles: Vec<_> = (0..31u64)
        .map(|who| {
            let (base, init, stats) = (base.clone(), init.clone(), stats.clone());
            // who 0 = teacher (every 3 s), others = students (every 10 s); periods compressed.
            let period = if who == 0 { 3.0 } else { 10.0 } / speedup;
            std::thread::spawn(move || participant(base, who, who <= 1, init, period, duration, stats))
        })
        .collect();
    let pages: Vec<CrdtPage> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    let wall = t.elapsed().as_secs_f64();

    let converged = pages[0].page() == pages[1].page();
    let items = pages[0].page().items.len();
    let mut lat = stats.latencies.lock().unwrap().clone();
    lat.sort_by(f64::total_cmp);
    let pct = |p: f64| lat[((lat.len() as f64 - 1.0) * p) as usize];
    let strokes = stats.strokes.load(Ordering::Relaxed);
    let up = stats.bytes_up.load(Ordering::Relaxed) as f64;
    let down = stats.bytes_down.load(Ordering::Relaxed) as f64;
    let db: u64 = std::fs::read_dir(std::env::temp_dir().join(format!("nb-relay-class-{}", std::process::id())))
        .map(|d| d.filter_map(|e| e.ok()?.metadata().ok()).map(|m| m.len()).sum())
        .unwrap_or(0);
    teacher_page.import(&pages[0].snapshot()).unwrap();

    println!("| Measure | Value |\n|---|---|");
    println!("| Participants | 1 teacher + 30 students, one shared page |");
    println!("| Simulated lesson | {minutes} min compressed {speedup}× into {wall:.0} s |");
    println!(
        "| Strokes committed | {strokes} ({:.0} per simulated minute) |",
        strokes as f64 / minutes
    );
    println!("| Mean update size per stroke | {:.0} bytes |", up / strokes as f64);
    println!("| Upload, all clients | {:.1} MB |", up / 1e6);
    println!("| Download, all clients (fan-out ×30) | {:.1} MB |", down / 1e6);
    println!("| Relay storage after the lesson | {:.1} MB |", db as f64 / 1e6);
    println!(
        "| End-to-end latency p50 / p95 / p99 (send → imported elsewhere) | {:.1} / {:.1} / {:.1} ms |",
        pct(0.5),
        pct(0.95),
        pct(0.99)
    );
    println!(
        "| Clients dropped by the relay | {} |",
        stats.disconnects.load(Ordering::Relaxed)
    );
    println!("| Full replicas (teacher, one student) converged | {converged} ({items} strokes) |");
    assert!(converged);
}
