//! Construction graph (E2.6): dynamic geometry written as plot lines.
//!
//! Each line names one object and defines it from earlier ones, so the lines form a
//! dependency graph evaluated in order; moving a free point (rewriting `A = (x, y)`)
//! updates everything built on it. An object whose construction is impossible (parallel
//! lines meeting, a circle missing a line) is *undefined*: it is not drawn, and everything
//! built on it is undefined too, until the figure changes back.
//!
//! | Line | Object |
//! |---|---|
//! | `A = (1, 2)` | free point (draggable) |
//! | `M = midpoint(A, B)` · `milieu` | point |
//! | `d = line(A, B)` · `droite` | line; also `segment(A, B)`, `ray(A, B)` · `demidroite` |
//! | `c = circle(A, B)` · `cercle` | circle with centre A through B; `circle(A, 2)`: radius 2 |
//! | `p = perpendicular(d, A)` · `perpendiculaire` | line through A perpendicular to d |
//! | `q = parallel(d, A)` · `parallele` | line through A parallel to d |
//! | `m = bisector(A, B)` · `mediatrice` | perpendicular bisector of [AB] |
//! | `b = anglebisector(A, B, C)` · `bissectrice` | bisector of angle ABC |
//! | `P = intersect(d, c, 2)` · `intersection` | intersection (n-th, for circles) |
//! | `P = on(c, 0.25)` · `sur` | point on a line (0 = first point, 1 = second) or circle (fraction of a turn) |
//! | `R = reflect(A, d)` · `symetrique` | reflection of any object in a line (or a point) |
//! | `rotate(T, O, 60)` · `dilate(c, O, 2)` · `homothetie` · `translate(T, A, B)` | image of any object (E2.11) |
//! | `T = polygon(A, B, C)` · `polygone` | polygon |
//! | `distance(A, B)` · `angle(A, B, C)` | measurements, shown as labels |

use crate::{parse_number, ParseError};

type P = (f64, f64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Line,
    Segment,
    Ray,
}

/// The value of an object for the current positions of the free points.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Point(P),
    /// Through `a` and `b` (distinct).
    Line {
        a: P,
        b: P,
        kind: LineKind,
    },
    Circle {
        c: P,
        r: f64,
    },
    Polygon(Vec<P>),
    /// A measurement, its label and where to show it.
    Measure(f64, String, P),
    Undefined,
}

/// A plane transformation.
#[derive(Clone, Debug, PartialEq)]
pub enum Tf {
    /// Reflection in a line, or point symmetry about a point.
    Reflect(String),
    /// Rotation about a point by an angle in degrees (anticlockwise).
    Rotate(String, f64),
    /// Homothety (dilation) with centre and ratio.
    Dilate(String, f64),
    /// Translation by the vector from the first point to the second.
    Translate(String, String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Def {
    Free(f64, f64),
    Midpoint(String, String),
    Line(String, String, LineKind),
    CircleThrough(String, String),
    CircleRadius(String, f64),
    Perpendicular(String, String),
    Parallel(String, String),
    PerpBisector(String, String),
    AngleBisector(String, String, String),
    Intersect(String, String, usize),
    On(String, f64),
    /// Image of an object (point, line, circle, polygon) by a transformation (E2.11).
    Transform(String, Tf),
    Polygon(Vec<String>),
    Distance(String, String),
    Angle(String, String, String),
}

fn kw(word: &str) -> Option<&'static str> {
    let w: String = word
        .to_lowercase()
        .chars()
        .map(|c| match c {
            'é' | 'è' | 'ê' => 'e',
            _ => c,
        })
        .filter(|c| *c != '-' && *c != '_')
        .collect();
    Some(match w.as_str() {
        "midpoint" | "milieu" => "midpoint",
        "line" | "droite" => "line",
        "segment" => "segment",
        "ray" | "demidroite" => "ray",
        "circle" | "cercle" => "circle",
        "perpendicular" | "perpendiculaire" | "perp" => "perpendicular",
        "parallel" | "parallele" => "parallel",
        "bisector" | "mediatrice" | "perpbisector" => "bisector",
        "anglebisector" | "bissectrice" => "anglebisector",
        "intersect" | "intersection" | "inter" => "intersect",
        "on" | "sur" | "pointon" => "on",
        "reflect" | "symetrique" | "reflection" | "mirror" => "reflect",
        "rotate" | "rotation" => "rotate",
        "dilate" | "homothetie" | "homothety" | "scale" | "enlarge" => "dilate",
        "translate" | "translation" => "translate",
        "polygon" | "polygone" | "triangle" => "polygon",
        "distance" | "dist" | "length" | "longueur" => "distance",
        "angle" => "angle",
        _ => return None,
    })
}

fn valid_name(n: &str) -> bool {
    let mut c = n.chars();
    c.next().is_some_and(|f| f.is_alphabetic())
        && c.all(|ch| ch.is_alphanumeric() || ch == '\'' || ch == '_')
        && n.chars().count() <= 8
}

/// Split "f(a, b, c)" into the keyword and its arguments (top-level commas).
pub(crate) fn call(s: &str) -> Option<(&str, Vec<&str>)> {
    let open = s.find('(')?;
    let inner = s[open + 1..].trim_end().strip_suffix(')')?;
    let mut args = vec![];
    let (mut depth, mut start) = (0, 0);
    for (i, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' | ';' if depth == 0 => {
                args.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim());
    Some((s[..open].trim(), args))
}

/// If `line` is a geometry line, its name (generated for measurements) and definition.
/// `Ok(None)`: not a geometry line.
pub fn parse_line(line: &str, index: usize) -> Result<Option<(String, Def)>, ParseError> {
    let s = line.trim();
    let (name, rhs) = match s.split_once('=') {
        Some((l, r)) if valid_name(l.trim()) => (l.trim().to_string(), r.trim()),
        Some(_) => return Ok(None),
        None => (format!("#{index}"), s),
    };
    let err = |m: String| Err(ParseError(m));
    // Free point: A = (x, y) — only with a name starting with a capital letter.
    if rhs.starts_with('(') && name.chars().next().is_some_and(|c| c.is_uppercase()) {
        if let Some((_, args)) = call(rhs) {
            if let [x, y] = args.as_slice() {
                return Ok(Some((name, Def::Free(parse_number(x)?, parse_number(y)?))));
            }
        }
        return err("write a point as A = (x, y)".into());
    }
    let Some((k, args)) = call(rhs) else { return Ok(None) };
    let Some(k) = kw(k) else { return Ok(None) };
    let a = |i: usize| -> Result<String, ParseError> {
        let v = args.get(i).copied().unwrap_or("");
        if valid_name(v) {
            Ok(v.to_string())
        } else {
            Err(ParseError(format!("{k}: expected a name, found \"{v}\"")))
        }
    };
    let n = args.len();
    let need = |want: usize| -> Result<(), ParseError> {
        if n == want {
            Ok(())
        } else {
            Err(ParseError(format!("{k} takes {want} arguments")))
        }
    };
    let def = match k {
        "midpoint" => {
            need(2)?;
            Def::Midpoint(a(0)?, a(1)?)
        }
        "line" | "segment" | "ray" => {
            need(2)?;
            let kind = match k {
                "line" => LineKind::Line,
                "segment" => LineKind::Segment,
                _ => LineKind::Ray,
            };
            Def::Line(a(0)?, a(1)?, kind)
        }
        "circle" => {
            need(2)?;
            match parse_number(args[1]) {
                Ok(r) if r > 0.0 => Def::CircleRadius(a(0)?, r),
                Ok(_) => return err("the radius must be positive".into()),
                Err(_) => Def::CircleThrough(a(0)?, a(1)?),
            }
        }
        "perpendicular" => {
            need(2)?;
            Def::Perpendicular(a(0)?, a(1)?)
        }
        "parallel" => {
            need(2)?;
            Def::Parallel(a(0)?, a(1)?)
        }
        "bisector" => {
            need(2)?;
            Def::PerpBisector(a(0)?, a(1)?)
        }
        "anglebisector" => {
            need(3)?;
            Def::AngleBisector(a(0)?, a(1)?, a(2)?)
        }
        "intersect" => {
            if !(n == 2 || n == 3) {
                return err("intersect takes 2 objects and an optional number".into());
            }
            let which = if n == 3 { parse_number(args[2])? as usize } else { 1 };
            Def::Intersect(a(0)?, a(1)?, which.max(1))
        }
        "on" => {
            need(2)?;
            Def::On(a(0)?, parse_number(args[1])?)
        }
        "reflect" => {
            need(2)?;
            Def::Transform(a(0)?, Tf::Reflect(a(1)?))
        }
        "rotate" | "dilate" => {
            need(3)?;
            let v = parse_number(args[2])?;
            let tf = if k == "rotate" {
                Tf::Rotate(a(1)?, v)
            } else {
                Tf::Dilate(a(1)?, v)
            };
            Def::Transform(a(0)?, tf)
        }
        "translate" => {
            need(3)?;
            Def::Transform(a(0)?, Tf::Translate(a(1)?, a(2)?))
        }
        "polygon" => {
            if n < 3 {
                return err("a polygon needs at least 3 points".into());
            }
            Def::Polygon((0..n).map(a).collect::<Result<_, _>>()?)
        }
        "distance" => {
            need(2)?;
            Def::Distance(a(0)?, a(1)?)
        }
        _ => {
            need(3)?;
            Def::Angle(a(0)?, a(1)?, a(2)?)
        }
    };
    Ok(Some((name, def)))
}

/// A `constrain …` line (E2.7): `constrain distance(A, B) = 5`, `angle(B, A, C) = 60`,
/// `horizontal(A, B)`, `vertical(A, B)`, `parallel(A, B, C, D)`, `perpendicular(A, B, C, D)`,
/// `equal(A, B, C, D)` (|AB| = |CD|). French: `contrainte` / `fixer`, `parallele`,
/// `perpendiculaire`, `egal`. `Ok(None)`: not a constraint line.
pub fn parse_constraint(line: &str) -> Result<Option<crate::solve::Constraint>, ParseError> {
    use crate::solve::Constraint as C;
    let s = line.trim();
    let Some((first, rest)) = s.split_once(char::is_whitespace) else {
        return Ok(None);
    };
    if !matches!(
        first.to_lowercase().as_str(),
        "constrain" | "constraint" | "contrainte" | "fixer" | "impose" | "imposer"
    ) {
        return Ok(None);
    }
    let (call_part, value) = match rest.rsplit_once('=') {
        Some((c, v)) => (c.trim(), Some(parse_number(v)?)),
        None => (rest.trim(), None),
    };
    let bad = || ParseError("write e.g. constrain distance(A, B) = 5".into());
    let (k, args) = call(call_part).ok_or_else(bad)?;
    if args.iter().any(|a| !valid_name(a)) {
        return Err(bad());
    }
    let a: Vec<String> = args.iter().map(|x| x.to_string()).collect();
    let k: String = k
        .to_lowercase()
        .chars()
        .map(|c| if "éèê".contains(c) { 'e' } else { c })
        .collect();
    let need = |n: usize, v: bool| {
        if a.len() == n && value.is_some() == v {
            Ok(())
        } else {
            Err(bad())
        }
    };
    Ok(Some(match k.as_str() {
        "distance" | "length" | "longueur" => {
            need(2, true)?;
            C::Distance(a[0].clone(), a[1].clone(), value.unwrap())
        }
        "angle" => {
            need(3, true)?;
            C::Angle(a[0].clone(), a[1].clone(), a[2].clone(), value.unwrap())
        }
        "horizontal" | "horizontale" => {
            need(2, false)?;
            C::Horizontal(a[0].clone(), a[1].clone())
        }
        "vertical" | "verticale" => {
            need(2, false)?;
            C::Vertical(a[0].clone(), a[1].clone())
        }
        "parallel" | "parallele" | "paralleles" => {
            need(4, false)?;
            C::Parallel(a[0].clone(), a[1].clone(), a[2].clone(), a[3].clone())
        }
        "perpendicular" | "perpendiculaire" | "perpendiculaires" => {
            need(4, false)?;
            C::Perpendicular(a[0].clone(), a[1].clone(), a[2].clone(), a[3].clone())
        }
        "equal" | "egal" | "egales" | "equallength" => {
            need(4, false)?;
            C::EqualLength(a[0].clone(), a[1].clone(), a[2].clone(), a[3].clone())
        }
        _ => return Err(bad()),
    }))
}

fn sub(a: P, b: P) -> P {
    (a.0 - b.0, a.1 - b.1)
}
fn add(a: P, b: P) -> P {
    (a.0 + b.0, a.1 + b.1)
}
fn mul(a: P, k: f64) -> P {
    (a.0 * k, a.1 * k)
}
fn dot(a: P, b: P) -> f64 {
    a.0 * b.0 + a.1 * b.1
}
fn cross(a: P, b: P) -> f64 {
    a.0 * b.1 - a.1 * b.0
}
fn norm(a: P) -> f64 {
    dot(a, a).sqrt()
}
const EPS: f64 = 1e-12;

/// Intersections of the infinite line (a, b) with the circle (c, r), ordered along a→b.
fn line_circle(a: P, b: P, c: P, r: f64) -> Vec<P> {
    let d = sub(b, a);
    let f = sub(a, c);
    let (qa, qb, qc) = (dot(d, d), 2.0 * dot(f, d), dot(f, f) - r * r);
    let disc = qb * qb - 4.0 * qa * qc;
    if disc < -1e-12 * qa.max(1.0) {
        return vec![];
    }
    let s = disc.max(0.0).sqrt();
    let (t1, t2) = ((-qb - s) / (2.0 * qa), (-qb + s) / (2.0 * qa));
    let mut out = vec![add(a, mul(d, t1))];
    if s > 1e-12 {
        out.push(add(a, mul(d, t2)));
    }
    out
}

/// Parameter t of p = a + t (b − a) (p is on the line).
fn param(a: P, b: P, p: P) -> f64 {
    let d = sub(b, a);
    dot(sub(p, a), d) / dot(d, d)
}

fn on_part(kind: LineKind, t: f64) -> bool {
    match kind {
        LineKind::Line => true,
        LineKind::Segment => (-1e-9..=1.0 + 1e-9).contains(&t),
        LineKind::Ray => t >= -1e-9,
    }
}

fn intersect(u: &Value, v: &Value, which: usize) -> Value {
    let pts: Vec<P> = match (u, v) {
        (Value::Line { a, b, kind: k1 }, Value::Line { a: c, b: d, kind: k2 }) => {
            let (r, s) = (sub(*b, *a), sub(*d, *c));
            let den = cross(r, s);
            if den.abs() < EPS * norm(r) * norm(s) {
                vec![]
            } else {
                let t = cross(sub(*c, *a), s) / den;
                let w = cross(sub(*c, *a), r) / den;
                if on_part(*k1, t) && on_part(*k2, w) {
                    vec![add(*a, mul(r, t))]
                } else {
                    vec![]
                }
            }
        }
        (Value::Line { a, b, kind }, Value::Circle { c, r }) | (Value::Circle { c, r }, Value::Line { a, b, kind }) => {
            line_circle(*a, *b, *c, *r)
                .into_iter()
                .filter(|p| on_part(*kind, param(*a, *b, *p)))
                .collect()
        }
        (Value::Circle { c: c1, r: r1 }, Value::Circle { c: c2, r: r2 }) => {
            let d = norm(sub(*c2, *c1));
            if d < EPS || d > r1 + r2 + 1e-12 || d < (r1 - r2).abs() - 1e-12 {
                vec![]
            } else {
                let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
                let h = (r1 * r1 - a * a).max(0.0).sqrt();
                let u = mul(sub(*c2, *c1), 1.0 / d);
                let m = add(*c1, mul(u, a));
                let n = (-u.1, u.0);
                if h < 1e-12 {
                    vec![m]
                } else {
                    // Left of c1→c2 first, then right.
                    vec![add(m, mul(n, h)), sub(m, mul(n, h))]
                }
            }
        }
        _ => vec![],
    };
    pts.get(which - 1).map_or(Value::Undefined, |p| Value::Point(*p))
}

fn reflect_point(p: P, a: P, b: P) -> P {
    let d = sub(b, a);
    let foot = add(a, mul(d, dot(sub(p, a), d) / dot(d, d)));
    sub(mul(foot, 2.0), p)
}

fn fmt(v: f64) -> String {
    crate::analysis::fmt_num(v)
}

/// Evaluate the definitions in order. Each may only use objects defined before it; a
/// reference to an unknown or later name is an error for that line (`Err`).
pub fn evaluate(defs: &[(String, Def)]) -> Vec<Result<Value, String>> {
    let mut known: std::collections::HashMap<&str, Value> = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(defs.len());
    for (name, def) in defs {
        let get = |n: &String| -> Result<Value, String> {
            known.get(n.as_str()).cloned().ok_or(format!("unknown object {n}"))
        };
        let pt = |n: &String| -> Result<Option<P>, String> {
            match get(n)? {
                Value::Point(p) => Ok(Some(p)),
                Value::Undefined => Ok(None),
                _ => Err(format!("{n} is not a point")),
            }
        };
        let line = |n: &String| -> Result<Option<(P, P)>, String> {
            match get(n)? {
                Value::Line { a, b, .. } => Ok(Some((a, b))),
                Value::Undefined => Ok(None),
                _ => Err(format!("{n} is not a line")),
            }
        };
        let r: Result<Value, String> = (|| {
            Ok(match def {
                Def::Free(x, y) => Value::Point((*x, *y)),
                Def::Midpoint(a, b) => match (pt(a)?, pt(b)?) {
                    (Some(a), Some(b)) => Value::Point(mul(add(a, b), 0.5)),
                    _ => Value::Undefined,
                },
                Def::Line(a, b, kind) => match (pt(a)?, pt(b)?) {
                    (Some(a), Some(b)) if norm(sub(b, a)) > EPS => Value::Line { a, b, kind: *kind },
                    _ => Value::Undefined,
                },
                Def::CircleThrough(c, p) => match (pt(c)?, pt(p)?) {
                    (Some(c), Some(p)) => Value::Circle { c, r: norm(sub(p, c)) },
                    _ => Value::Undefined,
                },
                Def::CircleRadius(c, r) => pt(c)?.map_or(Value::Undefined, |c| Value::Circle { c, r: *r }),
                Def::Perpendicular(l, p) | Def::Parallel(l, p) => match (line(l)?, pt(p)?) {
                    (Some((a, b)), Some(p)) => {
                        let d = sub(b, a);
                        let dir = if matches!(def, Def::Perpendicular(..)) {
                            (-d.1, d.0)
                        } else {
                            d
                        };
                        Value::Line {
                            a: p,
                            b: add(p, dir),
                            kind: LineKind::Line,
                        }
                    }
                    _ => Value::Undefined,
                },
                Def::PerpBisector(a, b) => match (pt(a)?, pt(b)?) {
                    (Some(a), Some(b)) if norm(sub(b, a)) > EPS => {
                        let m = mul(add(a, b), 0.5);
                        let d = sub(b, a);
                        Value::Line {
                            a: m,
                            b: add(m, (-d.1, d.0)),
                            kind: LineKind::Line,
                        }
                    }
                    _ => Value::Undefined,
                },
                Def::AngleBisector(a, v, c) => match (pt(a)?, pt(v)?, pt(c)?) {
                    (Some(a), Some(v), Some(c)) => {
                        let (u, w) = (sub(a, v), sub(c, v));
                        let (nu, nw) = (norm(u), norm(w));
                        if nu < EPS || nw < EPS {
                            Value::Undefined
                        } else {
                            let dir = add(mul(u, 1.0 / nu), mul(w, 1.0 / nw));
                            // A straight angle: the bisector is the perpendicular.
                            let dir = if norm(dir) < 1e-9 { (-u.1, u.0) } else { dir };
                            Value::Line {
                                a: v,
                                b: add(v, dir),
                                kind: LineKind::Ray,
                            }
                        }
                    }
                    _ => Value::Undefined,
                },
                Def::Intersect(u, v, which) => {
                    let (u, v) = (get(u)?, get(v)?);
                    for x in [&u, &v] {
                        if matches!(x, Value::Point(_) | Value::Polygon(_) | Value::Measure(..)) {
                            return Err("intersect needs lines or circles".into());
                        }
                    }
                    intersect(&u, &v, *which)
                }
                Def::On(o, t) => match get(o)? {
                    Value::Line { a, b, .. } => Value::Point(add(a, mul(sub(b, a), *t))),
                    Value::Circle { c, r } => {
                        let ang = t * std::f64::consts::TAU;
                        Value::Point((c.0 + r * ang.cos(), c.1 + r * ang.sin()))
                    }
                    Value::Undefined => Value::Undefined,
                    _ => return Err(format!("{o}: a point can only be on a line or a circle")),
                },
                Def::Transform(o, tf) => {
                    // The transformation as a map of points (None: undefined).
                    let map: Box<dyn Fn(P) -> P> = match tf {
                        Tf::Reflect(m) => match get(m)? {
                            Value::Line { a, b, .. } => Box::new(move |p| reflect_point(p, a, b)),
                            Value::Point(c) => Box::new(move |p| sub(mul(c, 2.0), p)),
                            Value::Undefined => return Ok(Value::Undefined),
                            _ => return Err(format!("{m}: reflect in a line or a point")),
                        },
                        Tf::Rotate(c, deg) => {
                            let Some(c) = pt(c)? else { return Ok(Value::Undefined) };
                            let (sn, cs) = deg.to_radians().sin_cos();
                            Box::new(move |p| {
                                let d = sub(p, c);
                                (c.0 + d.0 * cs - d.1 * sn, c.1 + d.0 * sn + d.1 * cs)
                            })
                        }
                        Tf::Dilate(c, k) => {
                            let Some(c) = pt(c)? else { return Ok(Value::Undefined) };
                            let k = *k;
                            Box::new(move |p| add(c, mul(sub(p, c), k)))
                        }
                        Tf::Translate(a, b) => match (pt(a)?, pt(b)?) {
                            (Some(a), Some(b)) => {
                                let v = sub(b, a);
                                Box::new(move |p| add(p, v))
                            }
                            _ => return Ok(Value::Undefined),
                        },
                    };
                    let ratio = if let Tf::Dilate(_, k) = tf { k.abs() } else { 1.0 };
                    match get(o)? {
                        Value::Point(p) => Value::Point(map(p)),
                        Value::Line { a, b, kind } => {
                            let (a, b) = (map(a), map(b));
                            if norm(sub(b, a)) > EPS {
                                Value::Line { a, b, kind }
                            } else {
                                Value::Undefined
                            }
                        }
                        Value::Circle { c, r } => Value::Circle {
                            c: map(c),
                            r: r * ratio,
                        },
                        Value::Polygon(ps) => Value::Polygon(ps.into_iter().map(&map).collect()),
                        Value::Undefined => Value::Undefined,
                        Value::Measure(..) => return Err(format!("{o}: a measurement cannot be transformed")),
                    }
                }
                Def::Polygon(ps) => {
                    let mut v = vec![];
                    for p in ps {
                        match pt(p)? {
                            Some(p) => v.push(p),
                            None => return Ok(Value::Undefined),
                        }
                    }
                    Value::Polygon(v)
                }
                Def::Distance(a, b) => match (pt(a)?, pt(b)?) {
                    (Some(pa), Some(pb)) => {
                        let d = norm(sub(pb, pa));
                        Value::Measure(d, format!("{a}{b} = {}", fmt(d)), mul(add(pa, pb), 0.5))
                    }
                    _ => Value::Undefined,
                },
                Def::Angle(a, v, c) => match (pt(a)?, pt(v)?, pt(c)?) {
                    (Some(pa), Some(pv), Some(pc)) => {
                        let (u, w) = (sub(pa, pv), sub(pc, pv));
                        if norm(u) < EPS || norm(w) < EPS {
                            Value::Undefined
                        } else {
                            let deg = cross(u, w).atan2(dot(u, w)).abs().to_degrees();
                            Value::Measure(deg, format!("∠{a}{v}{c} = {}°", fmt(deg)), pv)
                        }
                    }
                    _ => Value::Undefined,
                },
            })
        })();
        if let Ok(v) = &r {
            known.insert(name.as_str(), v.clone());
        }
        out.push(r);
    }
    out
}

/// Clip the infinite line through a, b (or the ray from a) to the box [x0, x1] × [y0, y1].
pub fn clip(a: P, b: P, kind: LineKind, x0: f64, x1: f64, y0: f64, y1: f64) -> Option<(P, P)> {
    if kind == LineKind::Segment {
        return Some((a, b));
    }
    let d = sub(b, a);
    let (mut t0, mut t1) = (
        if kind == LineKind::Ray { 0.0 } else { f64::NEG_INFINITY },
        f64::INFINITY,
    );
    for (p, q) in [(-d.0, a.0 - x0), (d.0, x1 - a.0), (-d.1, a.1 - y0), (d.1, y1 - a.1)] {
        if p.abs() < EPS {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    (t0 <= t1).then(|| (add(a, mul(d, t0)), add(a, mul(d, t1))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(lines: &[&str]) -> Vec<Result<Value, String>> {
        let defs: Vec<(String, Def)> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| parse_line(l, i).unwrap().unwrap_or_else(|| panic!("not geometry: {l}")))
            .collect();
        evaluate(&defs)
    }
    fn point(v: &Result<Value, String>) -> P {
        match v {
            Ok(Value::Point(p)) => *p,
            other => panic!("not a point: {other:?}"),
        }
    }
    fn near(a: P, b: P) {
        assert!((a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9, "{a:?} vs {b:?}");
    }

    #[test]
    fn classic_constructions() {
        let v = run(&[
            "A = (0, 0)",
            "B = (4, 0)",
            "C = (0, 3)",
            "M = milieu(A, B)",
            "c = cercle(A, C)",
            "d = droite(A, B)",
            "P = intersection(d, c, 2)",
            "m = mediatrice(B, C)",
            "distance(B, C)",
            "angle(B, A, C)",
            "h = perpendicular(d, C)",
            "H = intersect(h, d)",
            "R = symetrique(C, d)",
            "T = polygon(A, B, C)",
            "Q = on(c, 0.25)",
        ]);
        near(point(&v[3]), (2.0, 0.0));
        near(point(&v[6]), (3.0, 0.0));
        assert!(
            parse_line("O = intersect(m, bisector(A, B))", 0).is_err(),
            "nested calls are not allowed"
        );
        assert_eq!(v[8], Ok(Value::Measure(5.0, "BC = 5".into(), (2.0, 1.5))));
        assert_eq!(v[9], Ok(Value::Measure(90.0, "∠BAC = 90°".into(), (0.0, 0.0))));
        near(point(&v[11]), (0.0, 0.0));
        near(point(&v[12]), (0.0, -3.0));
        assert!(matches!(v[13], Ok(Value::Polygon(ref p)) if p.len() == 3));
        near(point(&v[14]), (0.0, 3.0));
    }

    #[test]
    fn circumcentre_moves_with_its_points_and_undefined_propagates() {
        let fig = |c: &str| {
            run(&[
                "A = (0, 0)",
                "B = (4, 0)",
                c,
                "m1 = bisector(A, B)",
                "m2 = bisector(B, C)",
                "O = intersect(m1, m2)",
                "k = circle(O, A)",
                "E = intersect(k, m1)",
            ])
        };
        let v = fig("C = (0, 4)");
        near(point(&v[5]), (2.0, 2.0));
        // Drag C: the circumcentre follows.
        let v = fig("C = (4, 4)");
        near(point(&v[5]), (2.0, 2.0));
        let v = fig("C = (1, 3)");
        let o = point(&v[5]);
        let d = |p: P| norm(sub(p, o));
        assert!((d((0.0, 0.0)) - d((4.0, 0.0))).abs() < 1e-9 && (d((0.0, 0.0)) - d((1.0, 3.0))).abs() < 1e-9);
        // C on line AB: the bisectors are parallel → O, k, E undefined (not errors).
        let v = fig("C = (8, 0)");
        assert_eq!(v[5], Ok(Value::Undefined));
        assert_eq!(
            (v[6].clone(), v[7].clone()),
            (Ok(Value::Undefined), Ok(Value::Undefined))
        );
    }

    #[test]
    fn intersections_and_errors() {
        let v = run(&[
            "A = (0, 0)",
            "B = (2, 0)",
            "c1 = circle(A, 2)",
            "c2 = circle(B, 2)",
            "P = intersect(c1, c2, 1)",
            "Q = intersect(c1, c2, 2)",
            "s = segment(A, B)",
            "C = (5, 5)",
            "D = (5, 6)",
            "t = segment(C, D)",
            "X = intersect(s, t)",
            "Y = intersect(c1, c2, 3)",
        ]);
        let (p, q) = (point(&v[4]), point(&v[5]));
        assert!((p.0 - 1.0).abs() < 1e-9 && (p.1.abs() - 3f64.sqrt()).abs() < 1e-9 && (p.1 + q.1).abs() < 1e-9);
        assert_eq!(v[10], Ok(Value::Undefined), "segments do not meet");
        assert_eq!(v[11], Ok(Value::Undefined), "no third point");
        assert!(run(&["M = midpoint(A, B)"])[0]
            .as_ref()
            .unwrap_err()
            .contains("unknown object A"));
        assert!(parse_line("c = circle(A)", 0).is_err());
        assert!(
            parse_line("y = x^2", 0).unwrap().is_none(),
            "functions are not geometry"
        );
        assert!(parse_line("f(x) = 2x", 0).unwrap().is_none());
        assert!(
            parse_line("(1, 2)", 0).unwrap().is_none(),
            "unnamed points stay plot points"
        );
        let (a, b) = clip((0.0, 0.0), (1.0, 1.0), LineKind::Line, -5.0, 5.0, -2.0, 2.0).unwrap();
        near(a, (-2.0, -2.0));
        near(b, (2.0, 2.0));
        let (a, _) = clip((0.0, 0.0), (1.0, 0.0), LineKind::Ray, -5.0, 5.0, -2.0, 2.0).unwrap();
        near(a, (0.0, 0.0));
    }

    #[test]
    fn transformations_of_any_object() {
        let v = run(&[
            "O = (0, 0)",
            "A = (2, 0)",
            "B = (2, 1)",
            "T = polygon(O, A, B)",
            "T2 = rotate(T, O, 90)",
            "c = circle(A, 1)",
            "c2 = homothetie(c, O, -2)",
            "d = line(O, B)",
            "A2 = symetrique(A, d)",
            "T3 = translate(T, O, B)",
            "U = rotate(A, Z, 1)",
        ]);
        match &v[4] {
            Ok(Value::Polygon(p)) => {
                near(p[1], (0.0, 2.0));
                near(p[2], (-1.0, 2.0));
            }
            other => panic!("{other:?}"),
        }
        match &v[6] {
            Ok(Value::Circle { c, r }) => {
                near(*c, (-4.0, 0.0));
                assert!((r - 2.0).abs() < 1e-12);
            }
            other => panic!("{other:?}"),
        }
        // Reflection of A(2, 0) in y = x/2: (1.2, 1.6).
        near(point(&v[8]), (1.2, 1.6));
        assert!(matches!(&v[9], Ok(Value::Polygon(p)) if (p[0].0 - 2.0).abs() < 1e-12 && (p[0].1 - 1.0).abs() < 1e-12));
        assert!(v[10].as_ref().unwrap_err().contains("unknown object Z"));
    }
}
