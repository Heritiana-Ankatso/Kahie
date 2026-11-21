//! Plot specifications (E2.4, E2.5): the lines of a plot block, parsed and drawn in window
//! coordinates. A plot block stores plain text lines, so the document model and sync are
//! unchanged; each line is one of:
//!
//! | Line | Meaning |
//! |---|---|
//! | `y = x^2 - 2`, `f(x) = sin x`, `x^2` | function of x (named `f`, or numbered 1, 2, … by order) |
//! | `(cos t, sin t)`, `(t, t^2) t=-2..2` | parametric curve, t ∈ [0, 2π] unless given |
//! | `r = 1 + cos θ`, `r = t θ=0..6pi` | polar curve, θ ∈ [0, 2π] unless given |
//! | `(2, 1)` | point |
//! | `roots f` · `extrema f` · `inflections f` · `asymptotes f` | markers / dashed lines on function f |
//! | `intersections f g` | points where f and g meet |
//! | `tangent f at 1` · `normal f at 1` | line through (1, f(1)) |
//! | `area f from 0 to 2` · `area f g from 0 to 2` | shaded ∫ f, or area between f and g |
//!
//! French keywords work too (`racines`, `extremums`, `inflexions`, `tangente f en 1`,
//! `normale`, `aire f de 0 à 2`). Function references are names (`f`) or numbers (`1`).

use crate::analysis::{self, fmt_line, fmt_num, Extremum};
use crate::{parse, parse_in, parse_number, Expr, ParseError};
use std::f64::consts::TAU;

/// Window of a plot: [x0, x1] × [y0, y1].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerKind {
    Root,
    Max,
    Min,
    Inflection,
    Intersection,
    Point,
    /// A free point of a construction: the app lets the teacher drag it.
    Free,
    /// Text only (equation of a tangent or asymptote, a measurement).
    Label,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub x: f64,
    pub y: f64,
    pub kind: MarkerKind,
    pub label: String,
}

/// A shaded region (polygon in window coordinates) with its value.
#[derive(Clone, Debug, PartialEq)]
pub struct Shade {
    pub polygon: Vec<(f64, f64)>,
    pub label: String,
    pub at: (f64, f64),
}

/// What a plot draws, line by line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rendered {
    /// Polylines of each line (empty for markers-only lines and errors).
    pub curves: Vec<Vec<Vec<(f64, f64)>>>,
    /// Dashed lines (tangents, asymptotes) instead of solid curves.
    pub dashed: Vec<bool>,
    /// Error message per line (empty if fine).
    pub errors: Vec<String>,
    pub markers: Vec<Marker>,
    pub shades: Vec<Shade>,
    /// Draw axes and grid (`axes off` / `sans axes` hides them, e.g. for geometry).
    pub axes: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Fref {
    Name(String),
    Index(usize),
}

#[derive(Clone, Debug, PartialEq)]
enum Line {
    Function {
        name: Option<String>,
        f: Expr,
    },
    Parametric {
        x: Expr,
        y: Expr,
        t0: f64,
        t1: f64,
    },
    Polar {
        r: Expr,
        t0: f64,
        t1: f64,
    },
    Point(f64, f64),
    Roots(Fref),
    Extrema(Fref),
    Inflections(Fref),
    Asymptotes(Fref),
    Intersections(Fref, Fref),
    Tangent(Fref, f64, bool),
    Area(Fref, Option<Fref>, f64, f64),
    /// A construction object (see [`crate::geo`]): index into the evaluated values.
    Geo(String, Box<crate::geo::Def>),
    AxesOff,
}

fn perr<T>(m: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(m.into()))
}

fn fref(word: &str) -> Result<Fref, ParseError> {
    let w = word.trim();
    if let Ok(n) = w.parse::<usize>() {
        if n >= 1 {
            return Ok(Fref::Index(n));
        }
    }
    let ok = !w.is_empty() && w.chars().all(|c| c.is_alphabetic()) && w.chars().count() <= 3;
    if ok {
        Ok(Fref::Name(w.to_string()))
    } else {
        perr(format!("unknown function \"{w}\""))
    }
}

/// Split "… t=a..b" (or θ=a..b) off the end of a line.
/// A parameter range `(start, end)`.
type Range = Option<(f64, f64)>;

fn split_range(s: &str) -> Result<(&str, Range), ParseError> {
    for key in ["t=", "θ=", "t =", "θ ="] {
        if let Some(i) = s.rfind(key) {
            // Only a trailing range: the part after it must contain "..".
            let rest = &s[i + key.len()..];
            if let Some((a, b)) = rest.split_once("..") {
                let (a, b) = (parse_number(a)?, parse_number(b)?);
                if a >= b {
                    return perr("range must go up (e.g. t=0..2pi)");
                }
                return Ok((s[..i].trim_end().trim_end_matches(',').trim_end(), Some((a, b))));
            }
        }
    }
    Ok((s, None))
}

/// Top-level comma split of "(a, b)" (commas inside brackets are kept).
fn pair(s: &str) -> Option<(&str, &str)> {
    let inner = s.trim().strip_prefix('(')?.strip_suffix(')')?;
    let mut depth = 0;
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' | ';' if depth == 0 => return Some((&inner[..i], &inner[i + 1..])),
            _ => {}
        }
    }
    None
}

fn keyword(first: &str) -> Option<&'static str> {
    Some(match first.to_lowercase().as_str() {
        "roots" | "root" | "zeros" | "zeroes" | "racines" | "racine" | "zéros" => "roots",
        "extrema" | "extremum" | "extremums" | "extrémums" | "max" | "min" => "extrema",
        "inflections" | "inflection" | "inflexions" | "inflexion" => "inflections",
        "asymptotes" | "asymptote" => "asymptotes",
        "intersections" | "intersection" | "intersect" => "intersections",
        "tangent" | "tangente" => "tangent",
        "normal" | "normale" => "normal",
        "area" | "aire" | "integral" | "intégrale" => "area",
        _ => return None,
    })
}

fn parse_line(src: &str, index: usize) -> Result<Line, ParseError> {
    let s = src.trim();
    if matches!(
        s.to_lowercase().as_str(),
        "axes off" | "no axes" | "sans axes" | "hide axes"
    ) {
        return Ok(Line::AxesOff);
    }
    if let Some((name, def)) = crate::geo::parse_line(s, index)? {
        return Ok(Line::Geo(name, Box::new(def)));
    }
    let mut words = s.split_whitespace();
    let first = words.next().unwrap_or("");
    if let Some(kw) = keyword(first) {
        let rest: Vec<&str> = words.collect();
        let lower: Vec<String> = rest.iter().map(|w| w.to_lowercase()).collect();
        let pos = |set: &[&str]| lower.iter().position(|w| set.contains(&w.as_str()));
        return match kw {
            "roots" | "extrema" | "inflections" | "asymptotes" => {
                let r = fref(rest.first().copied().unwrap_or("1"))?;
                Ok(match kw {
                    "roots" => Line::Roots(r),
                    "extrema" => Line::Extrema(r),
                    "inflections" => Line::Inflections(r),
                    _ => Line::Asymptotes(r),
                })
            }
            "intersections" => match rest.as_slice() {
                [a, b] => Ok(Line::Intersections(fref(a)?, fref(b)?)),
                [a, _and, b] => Ok(Line::Intersections(fref(a)?, fref(b)?)),
                _ => perr("write: intersections f g"),
            },
            "tangent" | "normal" => {
                let at = pos(&["at", "en", "x=", "@"]).ok_or(ParseError("write: tangent f at 1".into()))?;
                let f = if at == 0 { Fref::Index(1) } else { fref(rest[0])? };
                let x = parse_number(&rest[at + 1..].join(" "))?;
                Ok(Line::Tangent(f, x, kw == "normal"))
            }
            _ => {
                let from =
                    pos(&["from", "de", "between", "entre"]).ok_or(ParseError("write: area f from 0 to 2".into()))?;
                let to = pos(&["to", "à", "a", "and", "et"])
                    .filter(|&t| t > from + 1)
                    .ok_or(ParseError("write: area f from 0 to 2".into()))?;
                let fs = &rest[..from];
                let (f, g) = match fs {
                    [] => (Fref::Index(1), None),
                    [f] => (fref(f)?, None),
                    [f, g] => (fref(f)?, Some(fref(g)?)),
                    _ => return perr("write: area f g from 0 to 2"),
                };
                let a = parse_number(&rest[from + 1..to].join(" "))?;
                let b = parse_number(&rest[to + 1..].join(" "))?;
                Ok(Line::Area(f, g, a.min(b), a.max(b)))
            }
        };
    }
    // Polar: r = …
    if let Some(body) = s
        .strip_prefix("r =")
        .or_else(|| s.strip_prefix("r="))
        .or_else(|| s.strip_prefix("r(θ)="))
        .or_else(|| s.strip_prefix("r(θ) ="))
    {
        let (body, range) = split_range(body)?;
        let (t0, t1) = range.unwrap_or((0.0, TAU));
        return Ok(Line::Polar {
            r: parse_in(body, 't')?,
            t0,
            t1,
        });
    }
    // Parametric curve or point: (a, b)
    if s.starts_with('(') {
        let (body, range) = split_range(s)?;
        if let Some((a, b)) = pair(body) {
            if range.is_none() {
                if let (Ok(x), Ok(y)) = (parse_number(a), parse_number(b)) {
                    return Ok(Line::Point(x, y));
                }
            }
            let (t0, t1) = range.unwrap_or((0.0, TAU));
            return Ok(Line::Parametric {
                x: parse_in(a, 't')?,
                y: parse_in(b, 't')?,
                t0,
                t1,
            });
        }
    }
    // Function: name from "f(x) = …".
    let name = s.split_once('=').and_then(|(lhs, _)| {
        let l: String = lhs.chars().filter(|c| !c.is_whitespace()).collect();
        l.strip_suffix("(x)").filter(|n| !n.is_empty()).map(String::from)
    });
    Ok(Line::Function { name, f: parse(s)? })
}

/// Sample a parametric curve, splitting at undefined points and jumps.
fn sample_param(x: &dyn Fn(f64) -> f64, y: &dyn Fn(f64) -> f64, t0: f64, t1: f64, w: &Window) -> Vec<Vec<(f64, f64)>> {
    let n = 1500;
    let jump = 2.0 * ((w.x1 - w.x0).abs() + (w.y1 - w.y0).abs());
    let mut out = vec![];
    let mut cur: Vec<(f64, f64)> = vec![];
    for i in 0..=n {
        let t = t0 + (t1 - t0) * i as f64 / n as f64;
        let p = (x(t), y(t));
        let ok = p.0.is_finite() && p.1.is_finite() && p.0.abs() < 1e9 && p.1.abs() < 1e9;
        let far = ok && cur.last().is_some_and(|q| (p.0 - q.0).abs() + (p.1 - q.1).abs() > jump);
        if !ok || far {
            if cur.len() > 1 {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
            if ok {
                cur.push(p);
            }
        } else {
            cur.push(p);
        }
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}

/// A straight line across the window (`None` slope: vertical at `x`).
fn line_across(w: &Window, slope: Option<f64>, intercept_or_x: f64) -> Vec<Vec<(f64, f64)>> {
    match slope {
        Some(m) => vec![vec![
            (w.x0, m * w.x0 + intercept_or_x),
            (w.x1, m * w.x1 + intercept_or_x),
        ]],
        None => vec![vec![(intercept_or_x, w.y0), (intercept_or_x, w.y1)]],
    }
}

fn pt(x: f64, y: f64) -> String {
    format!("({}, {})", fmt_num(x), fmt_num(y))
}

/// Parse and draw every line of a plot in window `w`.
pub fn render(lines: &[String], w: &Window) -> Rendered {
    let parsed: Vec<Result<Line, ParseError>> = lines.iter().enumerate().map(|(i, l)| parse_line(l, i)).collect();
    // Constructions are evaluated together, in order (each object uses earlier ones).
    let geo_defs: Vec<(String, crate::geo::Def)> = parsed
        .iter()
        .filter_map(|l| match l {
            Ok(Line::Geo(n, d)) => Some((n.clone(), (**d).clone())),
            _ => None,
        })
        .collect();
    let mut geo_values = crate::geo::evaluate(&geo_defs).into_iter();
    // Functions by name and by order.
    let funcs: Vec<(Option<String>, &Expr)> = parsed
        .iter()
        .filter_map(|l| match l {
            Ok(Line::Function { name, f }) => Some((name.clone(), f)),
            _ => None,
        })
        .collect();
    let lookup = |r: &Fref| -> Result<&Expr, String> {
        match r {
            Fref::Index(i) => funcs.get(i - 1).map(|(_, f)| *f).ok_or(format!("no function {i}")),
            Fref::Name(n) => funcs
                .iter()
                .find(|(name, _)| name.as_deref() == Some(n.as_str()))
                .map(|(_, f)| *f)
                .ok_or(format!("no function {n}")),
        }
    };
    let (a, b) = (w.x0, w.x1);
    let mut out = Rendered {
        axes: true,
        ..Rendered::default()
    };
    for line in &parsed {
        let mut curves = vec![];
        let mut dashed = false;
        let mut error = String::new();
        match line {
            Err(e) => error = e.0.clone(),
            Ok(line) => {
                let r: Result<(), String> = (|| {
                    match line {
                        Line::AxesOff => out.axes = false,
                        Line::Geo(name, def) => {
                            use crate::geo::Value;
                            let value = geo_values.next().expect("one value per construction")?;
                            let label = if name.starts_with('#') {
                                String::new()
                            } else {
                                name.clone()
                            };
                            match value {
                                Value::Point((x, y)) => out.markers.push(Marker {
                                    x,
                                    y,
                                    kind: if matches!(**def, crate::geo::Def::Free(..)) {
                                        MarkerKind::Free
                                    } else {
                                        MarkerKind::Point
                                    },
                                    label,
                                }),
                                Value::Line { a, b, kind } => {
                                    let (lo_x, hi_x) = (w.x0.min(w.x1), w.x0.max(w.x1));
                                    let (lo_y, hi_y) = (w.y0.min(w.y1), w.y0.max(w.y1));
                                    if let Some((p, q)) = crate::geo::clip(a, b, kind, lo_x, hi_x, lo_y, hi_y) {
                                        curves.push(vec![p, q]);
                                    }
                                }
                                Value::Circle { c, r } => {
                                    curves.push(
                                        (0..=360)
                                            .map(|k| {
                                                let t = (k as f64).to_radians();
                                                (c.0 + r * t.cos(), c.1 + r * t.sin())
                                            })
                                            .collect(),
                                    );
                                }
                                Value::Polygon(pts) => {
                                    let mut closed = pts.clone();
                                    closed.push(pts[0]);
                                    curves.push(closed);
                                    let n = pts.len() as f64;
                                    let at = pts.iter().fold((0.0, 0.0), |s, p| (s.0 + p.0 / n, s.1 + p.1 / n));
                                    out.shades.push(Shade {
                                        polygon: pts,
                                        label: String::new(),
                                        at,
                                    });
                                }
                                Value::Measure(_, text, (x, y)) => out.markers.push(Marker {
                                    x,
                                    y,
                                    kind: MarkerKind::Label,
                                    label: text,
                                }),
                                Value::Undefined => {}
                            }
                        }
                        Line::Function { f, .. } => {
                            let n = 1000;
                            curves = crate::sample(f, a, b, w.y0, w.y1, n);
                        }
                        Line::Parametric { x, y, t0, t1 } => {
                            curves = sample_param(&|t| x.eval(t), &|t| y.eval(t), *t0, *t1, w);
                        }
                        Line::Polar { r, t0, t1 } => {
                            curves = sample_param(&|t| r.eval(t) * t.cos(), &|t| r.eval(t) * t.sin(), *t0, *t1, w);
                        }
                        Line::Point(x, y) => out.markers.push(Marker {
                            x: *x,
                            y: *y,
                            kind: MarkerKind::Point,
                            label: pt(*x, *y),
                        }),
                        Line::Roots(r) => {
                            for x in analysis::roots(lookup(r)?, a, b) {
                                out.markers.push(Marker {
                                    x,
                                    y: 0.0,
                                    kind: MarkerKind::Root,
                                    label: format!("x = {}", fmt_num(x)),
                                });
                            }
                        }
                        Line::Extrema(r) => {
                            let f = lookup(r)?;
                            for (x, k) in analysis::extrema(f, a, b) {
                                let y = f.eval(x);
                                out.markers.push(Marker {
                                    x,
                                    y,
                                    kind: if k == Extremum::Max {
                                        MarkerKind::Max
                                    } else {
                                        MarkerKind::Min
                                    },
                                    label: format!("{} {}", if k == Extremum::Max { "max" } else { "min" }, pt(x, y)),
                                });
                            }
                        }
                        Line::Inflections(r) => {
                            let f = lookup(r)?;
                            for x in analysis::inflections(f, a, b) {
                                out.markers.push(Marker {
                                    x,
                                    y: f.eval(x),
                                    kind: MarkerKind::Inflection,
                                    label: pt(x, f.eval(x)),
                                });
                            }
                        }
                        Line::Intersections(p, q) => {
                            let (f, g) = (lookup(p)?, lookup(q)?);
                            for x in analysis::intersections(f, g, a, b) {
                                out.markers.push(Marker {
                                    x,
                                    y: f.eval(x),
                                    kind: MarkerKind::Intersection,
                                    label: pt(x, f.eval(x)),
                                });
                            }
                        }
                        Line::Asymptotes(r) => {
                            let f = lookup(r)?;
                            dashed = true;
                            for x in analysis::vertical_asymptotes(f, a, b) {
                                curves.extend(line_across(w, None, x));
                                out.markers.push(Marker {
                                    x,
                                    y: w.y1 - 0.08 * (w.y1 - w.y0),
                                    kind: MarkerKind::Label,
                                    label: format!("x = {}", fmt_num(x)),
                                });
                            }
                            let mut seen: Vec<analysis::Line> = vec![];
                            for dir in [-1.0, 1.0] {
                                if let Some(l) = analysis::asymptote_at_infinity(f, dir) {
                                    if seen.iter().any(|s| {
                                        (s.slope - l.slope).abs() < 1e-6 && (s.intercept - l.intercept).abs() < 1e-4
                                    }) {
                                        continue;
                                    }
                                    seen.push(l);
                                    curves.extend(line_across(w, Some(l.slope), l.intercept));
                                    let x = if dir > 0.0 {
                                        a + 0.75 * (b - a)
                                    } else {
                                        a + 0.1 * (b - a)
                                    };
                                    out.markers.push(Marker {
                                        x,
                                        y: l.slope * x + l.intercept,
                                        kind: MarkerKind::Label,
                                        label: fmt_line(l),
                                    });
                                }
                            }
                        }
                        Line::Tangent(r, x0, normal) => {
                            let f = lookup(r)?;
                            let t = analysis::tangent(f, *x0).ok_or(format!("f is not defined at {}", fmt_num(*x0)))?;
                            let y0 = f.eval(*x0);
                            dashed = true;
                            let label = if !normal {
                                curves = line_across(w, Some(t.slope), t.intercept);
                                fmt_line(t)
                            } else if t.slope.abs() < 1e-9 {
                                curves = line_across(w, None, *x0);
                                format!("x = {}", fmt_num(*x0))
                            } else {
                                let m = -1.0 / t.slope;
                                let l = analysis::Line {
                                    slope: m,
                                    intercept: y0 - m * x0,
                                };
                                curves = line_across(w, Some(l.slope), l.intercept);
                                fmt_line(l)
                            };
                            out.markers.push(Marker {
                                x: *x0,
                                y: y0,
                                kind: MarkerKind::Point,
                                label,
                            });
                        }
                        Line::Area(p, q, x0, x1) => {
                            let f = lookup(p)?;
                            let g = q.as_ref().map(&lookup).transpose()?;
                            let lower = |x: f64| g.map_or(0.0, |g| g.eval(x));
                            let value = match g {
                                None => analysis::integrate(&|x| f.eval(x), *x0, *x1),
                                Some(g) => analysis::integrate(&|x| (f.eval(x) - g.eval(x)).abs(), *x0, *x1),
                            }
                            .ok_or("the integral does not converge on this interval")?;
                            let clamp = |y: f64| y.clamp(w.y0.min(w.y1), w.y0.max(w.y1));
                            let n = 200;
                            let xs: Vec<f64> = (0..=n).map(|i| x0 + (x1 - x0) * i as f64 / n as f64).collect();
                            let mut poly: Vec<(f64, f64)> = xs.iter().map(|&x| (x, clamp(f.eval(x)))).collect();
                            poly.extend(xs.iter().rev().map(|&x| (x, clamp(lower(x)))));
                            let mid = 0.5 * (x0 + x1);
                            let label = if g.is_some() {
                                format!("A = {}", fmt_num(value))
                            } else {
                                format!("∫ = {}", fmt_num(value))
                            };
                            out.shades.push(Shade {
                                polygon: poly,
                                label,
                                at: (mid, clamp(0.5 * (f.eval(mid) + lower(mid)))),
                            });
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = r {
                    error = e;
                }
            }
        }
        out.curves.push(curves);
        out.dashed.push(dashed);
        out.errors.push(error);
    }
    // Markers outside the window are not drawn.
    out.markers
        .retain(|m| m.x >= w.x0.min(w.x1) && m.x <= w.x0.max(w.x1) && m.y >= w.y0.min(w.y1) && m.y <= w.y0.max(w.y1));
    out
}

/// The plot as a pgfplots `tikzpicture` (E2.12): curves are written as formulas (so they stay
/// exact and vector), analysis results as marks, labels and filled areas. Needs `pgfplots`
/// with `trig format plots=rad` (see the exported preamble). `width_cm` sets the size.
pub fn to_pgfplots(lines: &[String], w: &Window, width_cm: f64) -> String {
    let colors = ["blue", "red", "green!60!black", "violet", "orange"];
    let r = render(lines, w);
    let height_cm = width_cm * 0.75;
    let mut out = String::new();
    out.push_str("\\begin{tikzpicture}\n\\begin{axis}[\n");
    out.push_str(&format!(
        "  width={width_cm:.1}cm, height={height_cm:.1}cm, axis lines=middle, grid=major, grid style={{gray!20}},\n  xmin={}, xmax={}, ymin={}, ymax={}, samples=300, unbounded coords=jump, restrict y to domain={}:{},\n  clip=true, axis on top=false]\n",
        w.x0,
        w.x1,
        w.y0,
        w.y1,
        w.y0 - 10.0 * (w.y1 - w.y0),
        w.y1 + 10.0 * (w.y1 - w.y0),
    ));
    let coords = |pts: &[(f64, f64)]| {
        pts.iter()
            .map(|(x, y)| format!("({x:.5},{y:.5})"))
            .collect::<Vec<_>>()
            .join(" ")
    };
    for sh in &r.shades {
        out.push_str(&format!(
            "  \\addplot[draw=none, fill=blue, fill opacity=0.2] coordinates {{{}}} -- cycle;\n",
            coords(&sh.polygon)
        ));
        out.push_str(&format!(
            "  \\node[font=\\small] at (axis cs:{:.5},{:.5}) {{{}}};\n",
            sh.at.0,
            sh.at.1,
            tex_escape(&sh.label)
        ));
    }
    if !r.axes {
        out = out.replace(
            "axis lines=middle, grid=major, grid style={gray!20}",
            "axis lines=none, axis equal image",
        );
    }
    for (i, line) in lines.iter().enumerate() {
        let color = colors[i % colors.len()];
        match parse_line(line, i) {
            Ok(Line::Geo(..)) => {
                for seg in r.curves.get(i).into_iter().flatten() {
                    out.push_str(&format!("  \\addplot[black, thick] coordinates {{{}}};\n", coords(seg)));
                }
            }
            Ok(Line::Function { f, .. }) => out.push_str(&format!(
                "  \\addplot[{color}, thick, domain={}:{}] {{{}}};\n",
                w.x0,
                w.x1,
                f.to_pgf("x")
            )),
            Ok(Line::Parametric { x, y, t0, t1 }) => out.push_str(&format!(
                "  \\addplot[{color}, thick, domain={t0}:{t1}, variable=\\t] ({{{}}}, {{{}}});\n",
                x.to_pgf("\\t"),
                y.to_pgf("\\t")
            )),
            Ok(Line::Polar { r: rr, t0, t1 }) => {
                let rad = rr.to_pgf("\\t");
                out.push_str(&format!(
                    "  \\addplot[{color}, thick, domain={t0}:{t1}, variable=\\t] ({{{rad}*cos(\\t)}}, {{{rad}*sin(\\t)}});\n"
                ))
            }
            // Tangents and asymptotes: straight dashed segments, already computed.
            Ok(_) if r.dashed.get(i).copied().unwrap_or(false) => {
                for seg in r.curves.get(i).into_iter().flatten() {
                    out.push_str(&format!("  \\addplot[gray, dashed] coordinates {{{}}};\n", coords(seg)));
                }
            }
            _ => {}
        }
    }
    for m in &r.markers {
        let mark = if m.kind == MarkerKind::Label {
            ""
        } else {
            "\\fill (axis cs:{x},{y}) circle (1.5pt); "
        };
        let mark = mark
            .replace("{x}", &format!("{:.5}", m.x))
            .replace("{y}", &format!("{:.5}", m.y));
        out.push_str(&format!(
            "  {mark}\\node[font=\\scriptsize, anchor=south west] at (axis cs:{:.5},{:.5}) {{{}}};\n",
            m.x,
            m.y,
            tex_escape(&m.label)
        ));
    }
    out.push_str("\\end{axis}\n\\end{tikzpicture}\n");
    out
}

/// Escape plain text for LaTeX (labels such as `max (1, -2)` or `y = 2x − 1`).
pub fn tex_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' | '}' | '$' | '&' | '#' | '%' | '_' => {
                out.push('\\');
                out.push(c);
            }
            '^' => out.push_str("\\^{}"),
            '~' => out.push_str("\\~{}"),
            '−' => out.push_str("$-$"),
            '∫' => out.push_str("$\\int$"),
            'θ' => out.push_str("$\\theta$"),
            'π' => out.push_str("$\\pi$"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Window = Window {
        x0: -5.0,
        x1: 5.0,
        y0: -5.0,
        y1: 5.0,
    };

    fn r(lines: &[&str]) -> Rendered {
        render(&lines.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &W)
    }

    #[test]
    fn functions_parametric_polar_points() {
        let out = r(&[
            "f(x) = x^2 - 2",
            "(cos t, sin t)",
            "r = 2 θ=0..pi",
            "(1, 2)",
            "(t, t^2) t=-1..1",
        ]);
        assert!(out.errors.iter().all(String::is_empty), "{:?}", out.errors);
        assert_eq!(out.curves.len(), 5);
        // Unit circle: every point at distance 1.
        assert!(out.curves[1]
            .iter()
            .flatten()
            .all(|(x, y)| ((x * x + y * y).sqrt() - 1.0).abs() < 1e-9));
        // Polar r = 2 over [0, π]: upper half circle of radius 2.
        assert!(out.curves[2]
            .iter()
            .flatten()
            .all(|(x, y)| ((x * x + y * y).sqrt() - 2.0).abs() < 1e-9 && *y >= -1e-9));
        assert!(out.curves[3].is_empty());
        assert_eq!(out.markers[0].label, "(1, 2)");
        let p = out.curves[4].iter().flatten().collect::<Vec<_>>();
        assert!((p[0].0 + 1.0).abs() < 1e-12 && (p.last().unwrap().0 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn analysis_lines_and_errors() {
        let out = r(&[
            "f(x) = x^3 - 3x",
            "g(x) = x",
            "racines f",
            "extrema f",
            "inflections 1",
            "intersections f g",
            "tangente f en 2",
            "aire f g de 0 à 2",
            "area g from -1 to 1",
            "asymptotes h",
            "roots",
            "tangent f",
        ]);
        let labels: Vec<(MarkerKind, String)> = out.markers.iter().map(|m| (m.kind, m.label.clone())).collect();
        let roots: Vec<&String> = labels
            .iter()
            .filter(|(k, _)| *k == MarkerKind::Root)
            .map(|(_, l)| l)
            .collect();
        // roots f and the bare "roots" (function 1) give the same three roots each.
        assert_eq!(
            roots,
            ["x = -1.732", "x = 0", "x = 1.732", "x = -1.732", "x = 0", "x = 1.732"]
        );
        assert!(labels.contains(&(MarkerKind::Max, "max (-1, 2)".into())));
        assert!(labels.contains(&(MarkerKind::Min, "min (1, -2)".into())));
        assert!(labels.contains(&(MarkerKind::Inflection, "(0, 0)".into())));
        let inter: Vec<&String> = labels
            .iter()
            .filter(|(k, _)| *k == MarkerKind::Intersection)
            .map(|(_, l)| l)
            .collect();
        assert_eq!(inter, ["(-2, -2)", "(0, 0)", "(2, 2)"]);
        // Tangent at 2: f(2) = 2, f'(2) = 9 → y = 9x − 16.
        assert!(labels.contains(&(MarkerKind::Point, "y = 9x − 16".into())));
        assert!(out.dashed[6] && !out.dashed[0]);
        // Area between f and g on [0, 2]: ∫|x³ − 4x| = 4. Signed ∫ of x on [−1, 1] = 0.
        assert_eq!(out.shades[0].label, "A = 4");
        assert_eq!(out.shades[1].label, "∫ = 0");
        assert_eq!(out.errors[9], "no function h");
        assert!(out.errors[11].contains("tangent f at 1"));
        assert!(out.errors[..9].iter().all(String::is_empty), "{:?}", out.errors);
    }

    #[test]
    fn asymptote_lines() {
        let out = r(&["(2x+1)/(x-1)", "asymptotes 1"]);
        let labels: Vec<&str> = out.markers.iter().map(|m| m.label.as_str()).collect();
        assert!(labels.contains(&"x = 1") && labels.contains(&"y = 2"), "{labels:?}");
        assert_eq!(out.curves[1].len(), 2, "one vertical, one horizontal (same at ±∞)");
        assert!(r(&["(t, t) t=2..1"]).errors[0].contains("range"));
    }

    #[test]
    fn pgfplots_output() {
        let lines: Vec<String> = [
            "f(x) = x^2 - 1",
            "(cos t, sin t)",
            "roots f",
            "tangent f at 1",
            "area f from -1 to 1",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let tex = to_pgfplots(&lines, &W, 8.0);
        assert!(
            tex.contains("\\addplot[blue, thick, domain=-5:5] {((x^2)-1)};"),
            "{tex}"
        );
        assert!(
            tex.contains("variable=\\t] {cos(\\t)}, {sin(\\t)}") || tex.contains("({cos(\\t)}, {sin(\\t)})"),
            "{tex}"
        );
        assert!(tex.contains("gray, dashed") && tex.contains("fill opacity=0.2") && tex.contains("$\\int$ = -1.333"));
        assert_eq!(tex.matches("\\begin{").count(), tex.matches("\\end{").count());
        assert_eq!(tex_escape("a_b & 50%"), "a\\_b \\& 50\\%");
    }

    #[test]
    fn constructions_in_a_plot() {
        let out = r(&[
            "A = (0, 0)",
            "B = (3, 0)",
            "c = circle(A, B)",
            "d = line(A, B)",
            "P = intersect(c, d, 1)",
            "T = polygon(A, B, X)",
            "axes off",
            "y = x",
        ]);
        assert!(!out.axes);
        assert_eq!(out.errors[5], "unknown object X");
        assert!(
            out.errors.iter().enumerate().all(|(i, e)| i == 5 || e.is_empty()),
            "{:?}",
            out.errors
        );
        let free: Vec<&str> = out
            .markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Free)
            .map(|m| m.label.as_str())
            .collect();
        assert_eq!(free, ["A", "B"]);
        let p = out.markers.iter().find(|m| m.label == "P").unwrap();
        assert!((p.x + 3.0).abs() < 1e-9 && p.y.abs() < 1e-9 && p.kind == MarkerKind::Point);
        assert_eq!(out.curves[2].len(), 1, "circle");
        // The line is clipped to the window [−5, 5].
        assert_eq!(out.curves[3], vec![vec![(-5.0, 0.0), (5.0, 0.0)]]);
        let tex = to_pgfplots(
            &["A = (0, 0)".into(), "c = circle(A, 2)".into(), "axes off".into()],
            &W,
            6.0,
        );
        assert!(tex.contains("axis lines=none") && tex.contains("black, thick"));
    }
}
