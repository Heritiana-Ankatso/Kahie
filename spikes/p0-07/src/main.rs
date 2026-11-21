//! P0-07 — Giac as Kahie's CAS (decision D7): curriculum task set, correctness and time.
//! `cargo run --release` prints a Markdown report fragment.
use std::ffi::{c_char, CStr, CString};
use std::time::Instant;

extern "C" {
    fn giacshim_eval(src: *const c_char) -> *mut c_char;
    fn giacshim_free(s: *mut c_char);
}

fn eval(src: &str) -> String {
    let c = CString::new(src).unwrap();
    unsafe {
        let p = giacshim_eval(c.as_ptr());
        let s = CStr::from_ptr(p).to_string_lossy().into_owned();
        giacshim_free(p);
        s
    }
}

/// "0", "[0,0]", "[]" or tiny floats count as zero.
fn all_zero(s: &str) -> bool {
    let t = s.trim().trim_start_matches('[').trim_end_matches(']');
    if t.is_empty() {
        return s.trim() == "[]";
    }
    t.split(',').all(|v| v.trim().parse::<f64>().map(|x| x.abs() < 1e-9).unwrap_or(false))
}

fn main() {
    let tasks = std::fs::read_to_string("tasks.txt").unwrap();
    let mut by_cat: std::collections::BTreeMap<String, (u32, u32, f64)> = Default::default();
    let mut fails = vec![];
    let mut times = vec![];
    let t0 = Instant::now();
    eval("1+1"); // warm-up: context creation
    let init_ms = t0.elapsed().as_secs_f64() * 1000.0;
    for line in tasks.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split(" | ").map(str::trim).collect();
        let (cat, mode, input, expected) = (f[0], f[1], f[2], f[3]);
        let t = Instant::now();
        let got = eval(input);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        times.push(ms);
        // Compare inside Giac on the original expressions (printed forms such as
        // `list[...]` / `matrix[...]` do not parse back).
        let ok = !got.starts_with("ERROR")
            && match mode {
                "expr" => all_zero(&eval(&format!("simplify(({input})-({expected}))"))),
                "list" => {
                    let a = format!("sort(evalf(flatten([{input}]),12))");
                    let b = format!("sort(evalf(flatten({expected}),12))");
                    eval(&format!("size({a})")) == eval(&format!("size({b})"))
                        && (expected == "[]" || all_zero(&eval(&format!("({a})-({b})"))))
                }
                "eq" => matches!(eval(&format!("({input})==({expected})")).as_str(), "true" | "1"),
                "str" => got.trim_start_matches("list").replace(' ', "") == expected.replace(' ', ""),
                _ => false,
            };
        let e = by_cat.entry(cat.to_string()).or_default();
        e.1 += 1;
        e.2 += ms;
        if ok {
            e.0 += 1;
        } else {
            fails.push(format!("`{input}` → `{got}` (expected `{expected}`)"));
        }
    }
    let total: u32 = by_cat.values().map(|v| v.1).sum();
    let passed: u32 = by_cat.values().map(|v| v.0).sum();
    times.sort_by(f64::total_cmp);
    println!("Context creation: {init_ms:.0} ms\n");
    println!("| Category | Passed | Mean time (ms) |\n|---|---|---|");
    for (c, (p, n, t)) in &by_cat {
        println!("| {c} | {p}/{n} | {:.1} |", t / *n as f64);
    }
    println!("| **Total** | **{passed}/{total}** | median {:.1}, p95 {:.1}, max {:.1} |", times[times.len() / 2], times[times.len() * 95 / 100], times[times.len() - 1]);
    for f in fails {
        println!("\nFAIL {f}");
    }
}
