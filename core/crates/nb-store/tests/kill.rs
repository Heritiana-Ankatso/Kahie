//! Kill-test harness (P0-12): a child process appends updates (and sometimes compacts)
//! as fast as it can and prints each sequence number after the commit returns. The parent
//! SIGKILLs it at a random moment, reopens the database and checks that
//! - the database passes `PRAGMA integrity_check`;
//! - every update the child reported as committed is still there (zero loss);
//! - updates are intact (payload matches its index) and in order (zero corruption).
//!
//! `KILL_ITERATIONS` sets the number of runs (default 1000, as in the acceptance criteria).

use nb_store::Store;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

const CHILD_ENV: &str = "NB_STORE_KILL_CHILD";

/// Payload for update number `i`: recognisable and variable-length.
fn payload(i: u64) -> Vec<u8> {
    let mut v = format!("update-{i}:").into_bytes();
    v.extend(std::iter::repeat((i % 251) as u8).take((i % 97) as usize * 13));
    v
}

fn index_of(bytes: &[u8]) -> u64 {
    let s = std::str::from_utf8(&bytes[..bytes.iter().position(|&b| b == b':').unwrap()]).unwrap();
    s.strip_prefix("update-").unwrap().parse().unwrap()
}

/// Child mode: write until killed. A snapshot is the concatenation of indices so far.
fn child(path: &str) {
    let mut store = Store::open(path).unwrap();
    let l = store.load().unwrap();
    let mut next = l.updates.last().map(|u| index_of(u) + 1).unwrap_or_else(|| {
        l.snapshot
            .as_deref()
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|s| s.rsplit(',').next().and_then(|n| n.parse::<u64>().ok()))
            .map_or(0, |n| n + 1)
    });
    let out = std::io::stdout();
    loop {
        store.append_update(&payload(next)).unwrap();
        // Reported only after the commit returned: these must survive the kill.
        {
            use std::io::Write;
            let mut o = out.lock();
            writeln!(o, "{next}").unwrap();
            o.flush().unwrap();
        }
        if next % 37 == 36 {
            let snap: Vec<String> = (0..=next).map(|i| i.to_string()).collect();
            store.compact(snap.join(",").as_bytes()).unwrap();
        }
        next += 1;
    }
}

#[test]
fn kill_during_write_loses_and_corrupts_nothing() {
    if let Ok(path) = std::env::var(CHILD_ENV) {
        child(&path);
        return;
    }
    let iterations: usize = std::env::var("KILL_ITERATIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let dir = std::env::temp_dir().join(format!("nb-store-kill-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("k.db");
    let exe = std::env::current_exe().unwrap();
    let mut rng = 0x2545_f491_4f6c_dd1du64;
    let mut max_committed: Option<u64> = None;
    for it in 0..iterations {
        let mut c = Command::new(&exe)
            .args([
                "--exact",
                "kill_during_write_loses_and_corrupts_nothing",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_ENV, &path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdout = c.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            let mut last = None;
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Ok(n) = line.trim().parse::<u64>() {
                    last = Some(n);
                }
            }
            last
        });
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        std::thread::sleep(Duration::from_micros(2_000 + rng % 30_000));
        c.kill().unwrap(); // SIGKILL on Unix
        c.wait().unwrap();
        if let Some(n) = reader.join().unwrap() {
            max_committed = Some(max_committed.map_or(n, |m: u64| m.max(n)));
        }

        let store = Store::open(&path).unwrap();
        assert!(store.integrity_ok().unwrap(), "iteration {it}: integrity check failed");
        let l = store.load().unwrap();
        let mut seen: Vec<u64> = l
            .snapshot
            .as_deref()
            .map(|s| {
                std::str::from_utf8(s)
                    .unwrap()
                    .split(',')
                    .map(|n| n.parse().unwrap())
                    .collect()
            })
            .unwrap_or_default();
        for u in &l.updates {
            let i = index_of(u);
            assert_eq!(u, &payload(i), "iteration {it}: update {i} corrupted");
            seen.push(i);
        }
        let expected: Vec<u64> = (0..seen.len() as u64).collect();
        assert_eq!(seen, expected, "iteration {it}: gap or reordering");
        if let Some(m) = max_committed {
            assert!(
                seen.len() as u64 > m,
                "iteration {it}: committed update {m} lost (have {})",
                seen.len()
            );
        }
    }
    let total = max_committed.expect("the child never committed anything");
    assert!(
        total as usize > iterations,
        "too few writes ({total}) to exercise the store"
    );
    eprintln!("{iterations} kills, {} updates committed", total + 1);
    std::fs::remove_dir_all(dir).unwrap();
}
