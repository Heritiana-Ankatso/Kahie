//! Curriculum test suite (Phase 2 exit criterion): lycée exercises checked against the maths
//! engine. Each `tests/curriculum/*.json` file holds a list of tasks:
//!
//! ```json
//! { "id": "1re-deriv-003", "level": "premiere", "topic": "derivation",
//!   "statement": "Tangent to f(x) = x^2 at x = 3", "check": { "tangent": "x^2", "at": 3, "expect": [6, -9] } }
//! ```
//!
//! Checks (numbers compared to 1e-3, relative for large values):
//! - `eval` + `x`: f(x);  `roots`, `inflections`, `vertical_asymptotes` + `on: [a, b]`;
//! - `extrema` + `on`: `[[x, "min"|"max"], …]`;  `intersections: [f, g]` + `on`;
//! - `tangent` + `at`: `[slope, intercept]`;  `asymptote` + `dir` (±1): `[slope, intercept]` or null;
//! - `integral` + `on`: the value;  `derivative` + `at`: f′(x);
//! - `construction: [lines…]`: `expect` maps names to a point `[x, y]`, a number (measure
//!   or circle radius), `"undefined"`, or `{ "c": [x, y], "r": r }` for circles;
//! - `constraints: [lines…]` (points `A = (x, y)` as starting positions, `constrain …` lines):
//!   solved, then `expect` as for constructions, evaluated on the solved positions
//!   (`"conflict": true` expects the solver to report a conflict).
//!
//! A task with `"gap": "reason"` is a known gap: it is run and counted but may fail; if it
//! passes, the test asks for the `gap` to be removed.

use nb_math::analysis::{self, Extremum};
use nb_math::geo::{self, Def, Value};
use nb_math::{parse, solve};
use serde_json::Value as J;
use std::fs;
use std::path::Path;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-3 * b.abs().max(1.0)
}

fn num(j: &J) -> Result<f64, String> {
    j.as_f64().ok_or_else(|| format!("expected a number, found {j}"))
}

fn nums(j: &J) -> Result<Vec<f64>, String> {
    j.as_array().ok_or("expected a list")?.iter().map(num).collect()
}

fn interval(c: &J) -> Result<(f64, f64), String> {
    let v = nums(c.get("on").ok_or("missing \"on\"")?)?;
    match v.as_slice() {
        [a, b] => Ok((*a, *b)),
        _ => Err("\"on\" is [a, b]".into()),
    }
}

fn same_list(got: &[f64], want: &[f64]) -> Result<(), String> {
    if got.len() == want.len() && got.iter().zip(want).all(|(g, w)| close(*g, *w)) {
        Ok(())
    } else {
        Err(format!("got {got:?}, expected {want:?}"))
    }
}

fn expr(c: &J, key: &str) -> Result<nb_math::Expr, String> {
    let s = c.get(key).and_then(J::as_str).ok_or(format!("missing \"{key}\""))?;
    parse(s).map_err(|e| format!("parse {s}: {}", e.0))
}

fn lines(c: &J, key: &str) -> Result<Vec<String>, String> {
    c.get(key)
        .and_then(J::as_array)
        .ok_or(format!("\"{key}\" is a list of lines"))?
        .iter()
        .map(|l| l.as_str().map(str::to_string).ok_or("lines are strings".to_string()))
        .collect()
}

/// Compare the evaluated objects with `expect`.
fn check_values(defs: &[(String, Def)], expect: &J) -> Result<(), String> {
    let vals = geo::evaluate(defs);
    let want = expect.as_object().ok_or("\"expect\" maps names to values")?;
    for (name, w) in want {
        let i = defs
            .iter()
            .position(|(n, _)| n == name)
            .ok_or(format!("no object {name}"))?;
        let v = vals[i].clone().map_err(|e| format!("{name}: {e}"))?;
        let ok = match (&v, w) {
            (Value::Undefined, J::String(s)) => s == "undefined",
            (Value::Point(p), J::Array(_)) => {
                let x = nums(w)?;
                x.len() == 2 && close(p.0, x[0]) && close(p.1, x[1])
            }
            (Value::Measure(m, ..), J::Number(_)) => close(*m, num(w)?),
            (Value::Circle { r, .. }, J::Number(_)) => close(*r, num(w)?),
            (Value::Circle { c, r }, J::Object(o)) => {
                let cc = nums(o.get("c").ok_or("circle: c")?)?;
                close(c.0, cc[0]) && close(c.1, cc[1]) && close(*r, num(o.get("r").ok_or("circle: r")?)?)
            }
            _ => false,
        };
        if !ok {
            return Err(format!("{name} = {v:?}, expected {w}"));
        }
    }
    Ok(())
}

fn parse_defs(ls: &[String]) -> Result<Vec<(String, Def)>, String> {
    let mut defs = vec![];
    for (i, l) in ls.iter().enumerate() {
        match geo::parse_line(l, i).map_err(|e| format!("{l}: {}", e.0))? {
            Some(d) => defs.push(d),
            None => return Err(format!("not a construction line: {l}")),
        }
    }
    Ok(defs)
}

fn run(c: &J) -> Result<(), String> {
    let expect = c.get("expect").ok_or("missing \"expect\"")?;
    if c.get("eval").is_some() {
        let f = expr(c, "eval")?;
        let (got, want) = (f.eval(num(c.get("x").ok_or("missing x")?)?), num(expect)?);
        return close(got, want)
            .then_some(())
            .ok_or(format!("got {got}, expected {want}"));
    }
    if c.get("derivative").is_some() {
        let f = expr(c, "derivative")?;
        let (got, want) = (
            analysis::derivative(&f, num(c.get("at").ok_or("missing at")?)?),
            num(expect)?,
        );
        return close(got, want)
            .then_some(())
            .ok_or(format!("got {got}, expected {want}"));
    }
    for key in ["roots", "inflections", "vertical_asymptotes"] {
        if c.get(key).is_some() {
            let f = expr(c, key)?;
            let (a, b) = interval(c)?;
            let got = match key {
                "roots" => analysis::roots(&f, a, b),
                "inflections" => analysis::inflections(&f, a, b),
                _ => analysis::vertical_asymptotes(&f, a, b),
            };
            return same_list(&got, &nums(expect)?);
        }
    }
    if c.get("extrema").is_some() {
        let f = expr(c, "extrema")?;
        let (a, b) = interval(c)?;
        let got = analysis::extrema(&f, a, b);
        let want = expect.as_array().ok_or("extrema: [[x, \"min\"], …]")?;
        let ok = got.len() == want.len()
            && got.iter().zip(want).all(|((x, k), w)| {
                let kind = if *k == Extremum::Min { "min" } else { "max" };
                w[0].as_f64().is_some_and(|wx| close(*x, wx)) && w[1].as_str() == Some(kind)
            });
        return ok.then_some(()).ok_or(format!("got {got:?}, expected {expect}"));
    }
    if let Some(fg) = c.get("intersections") {
        let pair: Vec<&str> = fg
            .as_array()
            .ok_or("intersections: [f, g]")?
            .iter()
            .filter_map(J::as_str)
            .collect();
        let [f, g] = pair.as_slice() else {
            return Err("intersections: [f, g]".into());
        };
        let (f, g) = (parse(f).map_err(|e| e.0)?, parse(g).map_err(|e| e.0)?);
        let (a, b) = interval(c)?;
        return same_list(&analysis::intersections(&f, &g, a, b), &nums(expect)?);
    }
    if c.get("tangent").is_some() {
        let f = expr(c, "tangent")?;
        let l = analysis::tangent(&f, num(c.get("at").ok_or("missing at")?)?).ok_or("no tangent")?;
        return same_list(&[l.slope, l.intercept], &nums(expect)?);
    }
    if c.get("asymptote").is_some() {
        let f = expr(c, "asymptote")?;
        let got = analysis::asymptote_at_infinity(&f, num(c.get("dir").ok_or("missing dir")?)?);
        return match (got, expect) {
            (None, J::Null) => Ok(()),
            (Some(l), J::Array(_)) => same_list(&[l.slope, l.intercept], &nums(expect)?),
            (g, _) => Err(format!("got {g:?}, expected {expect}")),
        };
    }
    if c.get("integral").is_some() {
        let f = expr(c, "integral")?;
        let (a, b) = interval(c)?;
        let got = analysis::integrate(&|x| f.eval(x), a, b).ok_or("integral diverges")?;
        let want = num(expect)?;
        return close(got, want)
            .then_some(())
            .ok_or(format!("got {got}, expected {want}"));
    }
    if c.get("construction").is_some() {
        return check_values(&parse_defs(&lines(c, "construction")?)?, expect);
    }
    if c.get("constraints").is_some() {
        let ls = lines(c, "constraints")?;
        let (mut geo_lines, mut cons) = (vec![], vec![]);
        for l in &ls {
            match geo::parse_constraint(l).map_err(|e| format!("{l}: {}", e.0))? {
                Some(k) => cons.push(k),
                None => geo_lines.push(l.clone()),
            }
        }
        let mut defs = parse_defs(&geo_lines)?;
        let free: Vec<(usize, String, (f64, f64))> = defs
            .iter()
            .enumerate()
            .filter_map(|(i, (n, d))| match d {
                Def::Free(x, y) => Some((i, n.clone(), (*x, *y))),
                _ => None,
            })
            .collect();
        let names: Vec<String> = free.iter().map(|f| f.1.clone()).collect();
        let start: Vec<(f64, f64)> = free.iter().map(|f| f.2).collect();
        let sol = solve::solve(&names, &start, &cons);
        if c.get("conflict").and_then(J::as_bool) == Some(true) {
            return (!sol.conflicts.is_empty())
                .then_some(())
                .ok_or("expected a conflict".into());
        }
        if !sol.conflicts.is_empty() {
            return Err(format!("conflicts {:?}", sol.conflicts));
        }
        for ((i, ..), p) in free.iter().zip(&sol.points) {
            defs[*i].1 = Def::Free(p.0, p.1);
        }
        return check_values(&defs, expect);
    }
    Err("unknown check".into())
}

#[test]
fn curriculum() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/curriculum");
    let mut files: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut total, mut passed, mut gaps) = (0, 0, 0);
    let mut failures = vec![];
    let mut fixed_gaps = vec![];
    let mut ids = std::collections::HashSet::new();
    for path in files.iter().filter(|p| p.extension().is_some_and(|e| e == "json")) {
        let tasks: Vec<J> = serde_json::from_str(&fs::read_to_string(path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut file_pass = 0;
        for t in &tasks {
            let id = t["id"].as_str().expect("every task has an id").to_string();
            assert!(ids.insert(id.clone()), "duplicate id {id}");
            total += 1;
            let gap = t.get("gap").and_then(J::as_str);
            match run(&t["check"]) {
                Ok(()) => {
                    passed += 1;
                    file_pass += 1;
                    if gap.is_some() {
                        fixed_gaps.push(id);
                    }
                }
                Err(e) if gap.is_some() => {
                    gaps += 1;
                    eprintln!("  known gap {id}: {e}");
                }
                Err(e) => failures.push(format!("{id}: {e}")),
            }
        }
        eprintln!(
            "{}: {file_pass}/{} passed",
            path.file_name().unwrap().to_string_lossy(),
            tasks.len()
        );
    }
    eprintln!("curriculum: {passed}/{total} passed ({gaps} known gaps)");
    assert!(total > 0, "no curriculum tasks found");
    assert!(failures.is_empty(), "failing tasks:\n{}", failures.join("\n"));
    assert!(
        fixed_gaps.is_empty(),
        "known gaps now pass, remove \"gap\": {fixed_gaps:?}"
    );
}
