//! Safe maths expression parser for function plots. Port of `app/web/src/expr.js`.
//! Grammar: expr := term (('+'|'-') term)* ; term := unary (('*'|'/'|implicit) unary)*
//! unary := ('-'|'+') unary | power ; power := atom ('^' unary)?

pub mod analysis;
pub mod cas;
pub mod plot;
pub mod texout;

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    X,
    Neg(Box<Expr>),
    Bin(char, Box<Expr>, Box<Expr>),
    Call(Func, Box<Expr>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Exp,
    Ln,
    Log,
    Sqrt,
    Abs,
    Floor,
    Ceil,
    Sign,
    Cbrt,
}

const FUNCS: &[(&str, Func)] = &[
    ("arcsin", Func::Asin),
    ("arccos", Func::Acos),
    ("arctan", Func::Atan),
    ("asin", Func::Asin),
    ("acos", Func::Acos),
    ("atan", Func::Atan),
    ("sinh", Func::Sinh),
    ("cosh", Func::Cosh),
    ("tanh", Func::Tanh),
    ("floor", Func::Floor),
    ("ceil", Func::Ceil),
    ("sign", Func::Sign),
    ("cbrt", Func::Cbrt),
    ("sqrt", Func::Sqrt),
    ("sin", Func::Sin),
    ("cos", Func::Cos),
    ("tan", Func::Tan),
    ("exp", Func::Exp),
    ("abs", Func::Abs),
    ("log", Func::Log),
    ("ln", Func::Ln),
];

impl Func {
    fn apply(self, v: f64) -> f64 {
        match self {
            Func::Sin => v.sin(),
            Func::Cos => v.cos(),
            Func::Tan => v.tan(),
            Func::Asin => v.asin(),
            Func::Acos => v.acos(),
            Func::Atan => v.atan(),
            Func::Sinh => v.sinh(),
            Func::Cosh => v.cosh(),
            Func::Tanh => v.tanh(),
            Func::Exp => v.exp(),
            Func::Ln => v.ln(),
            Func::Log => v.log10(),
            Func::Sqrt => v.sqrt(),
            Func::Abs => v.abs(),
            Func::Floor => v.floor(),
            Func::Ceil => v.ceil(),
            Func::Sign => {
                if v == 0.0 {
                    0.0
                } else {
                    v.signum()
                }
            }
            Func::Cbrt => v.cbrt(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    X,
    Const(f64),
    Fn(Func),
    Op(char),
}

fn err<T>(msg: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(msg.into()))
}

fn tokenize(src: &str, var: char) -> Result<Vec<Tok>, ParseError> {
    let c: Vec<char> = src.chars().collect();
    let mut toks = vec![];
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || ch == '.' {
            let mut j = i;
            while j < c.len() && (c[j].is_ascii_digit() || c[j] == '.') {
                j += 1;
            }
            // Scientific notation: 3e-2, 1e5 (but "2e" alone is 2·e).
            if j < c.len() && c[j] == 'e' {
                let mut k = j + 1;
                if k < c.len() && (c[k] == '+' || c[k] == '-') {
                    k += 1;
                }
                if k < c.len() && c[k].is_ascii_digit() {
                    while k < c.len() && c[k].is_ascii_digit() {
                        k += 1;
                    }
                    j = k;
                }
            }
            let s: String = c[i..j].iter().collect();
            match s.parse::<f64>() {
                Ok(v) if v.is_finite() => toks.push(Tok::Num(v)),
                _ => return err(format!("bad number {s}")),
            }
            i = j;
        } else if ch.is_ascii_alphabetic() || ch == 'π' || ch == 'θ' {
            let mut j = i;
            while j < c.len() && (c[j].is_ascii_alphabetic() || c[j] == 'π' || c[j] == 'θ') {
                j += 1;
            }
            let word: String = c[i..j].iter().collect();
            split_ident(&word, var, &mut toks)?;
            i = j;
        } else {
            match ch {
                '+' | '-' | '*' | '/' | '^' | '(' | ')' => toks.push(Tok::Op(ch)),
                '×' | '·' => toks.push(Tok::Op('*')),
                '÷' => toks.push(Tok::Op('/')),
                '²' => toks.extend([Tok::Op('^'), Tok::Num(2.0)]),
                '³' => toks.extend([Tok::Op('^'), Tok::Num(3.0)]),
                _ => return err(format!("unexpected \"{ch}\"")),
            }
            i += 1;
        }
    }
    Ok(toks)
}

/// "xsin" → x, sin ; "pix" → pi, x. Longest known name first.
fn split_ident(word: &str, var: char, out: &mut Vec<Tok>) -> Result<(), ParseError> {
    let mut rest = word;
    while !rest.is_empty() {
        if let Some((name, f)) = FUNCS.iter().find(|(n, _)| rest.starts_with(n)) {
            out.push(Tok::Fn(*f));
            rest = &rest[name.len()..];
        } else if let Some(r) = rest.strip_prefix("pi") {
            out.push(Tok::Const(std::f64::consts::PI));
            rest = r;
        } else if let Some(r) = rest.strip_prefix('π') {
            out.push(Tok::Const(std::f64::consts::PI));
            rest = r;
        } else if let Some(r) = rest.strip_prefix('e') {
            out.push(Tok::Const(std::f64::consts::E));
            rest = r;
        } else if let Some(r) = rest.strip_prefix(var) {
            out.push(Tok::X);
            rest = r;
        } else if var == 't' && rest.starts_with('θ') {
            // Polar curves may use θ or t.
            out.push(Tok::X);
            rest = &rest['θ'.len_utf8()..];
        } else {
            return err(format!("unknown name \"{rest}\""));
        }
    }
    Ok(())
}

struct Parser {
    toks: Vec<Tok>,
    k: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.k)
    }
    fn peek_op(&self, c: char) -> bool {
        self.peek() == Some(&Tok::Op(c))
    }
    fn starts_atom(&self) -> bool {
        matches!(
            self.peek(),
            Some(Tok::Num(_) | Tok::X | Tok::Const(_) | Tok::Fn(_) | Tok::Op('('))
        )
    }
    fn expr(&mut self) -> Result<Expr, ParseError> {
        let mut node = self.term()?;
        while self.peek_op('+') || self.peek_op('-') {
            let op = if self.peek_op('+') { '+' } else { '-' };
            self.k += 1;
            node = Expr::Bin(op, Box::new(node), Box::new(self.term()?));
        }
        Ok(node)
    }
    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut node = self.unary()?;
        loop {
            if self.peek_op('*') || self.peek_op('/') {
                let op = if self.peek_op('*') { '*' } else { '/' };
                self.k += 1;
                node = Expr::Bin(op, Box::new(node), Box::new(self.unary()?));
            } else if self.starts_atom() {
                node = Expr::Bin('*', Box::new(node), Box::new(self.power()?));
            } else {
                return Ok(node);
            }
        }
    }
    fn unary(&mut self) -> Result<Expr, ParseError> {
        if self.peek_op('-') {
            self.k += 1;
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.peek_op('+') {
            self.k += 1;
            return self.unary();
        }
        self.power()
    }
    fn power(&mut self) -> Result<Expr, ParseError> {
        let base = self.atom()?;
        if self.peek_op('^') {
            self.k += 1;
            return Ok(Expr::Bin('^', Box::new(base), Box::new(self.unary()?)));
        }
        Ok(base)
    }
    fn atom(&mut self) -> Result<Expr, ParseError> {
        let Some(tok) = self.toks.get(self.k).cloned() else {
            return err("unexpected end");
        };
        self.k += 1;
        match tok {
            Tok::Num(v) | Tok::Const(v) => Ok(Expr::Num(v)),
            Tok::X => Ok(Expr::X),
            Tok::Fn(f) => {
                // sin x, sin(x), sin^2 x
                let exp = if self.peek_op('^') {
                    self.k += 1;
                    Some(self.atom()?)
                } else {
                    None
                };
                let arg = if self.peek_op('(') {
                    self.k += 1;
                    let e = self.expr()?;
                    self.close()?;
                    e
                } else {
                    self.power()?
                };
                let call = Expr::Call(f, Box::new(arg));
                Ok(match exp {
                    Some(e) => Expr::Bin('^', Box::new(call), Box::new(e)),
                    None => call,
                })
            }
            Tok::Op('(') => {
                let e = self.expr()?;
                self.close()?;
                Ok(e)
            }
            Tok::Op(c) => err(format!("unexpected {c}")),
        }
    }
    fn close(&mut self) -> Result<(), ParseError> {
        if self.peek_op(')') {
            self.k += 1;
            Ok(())
        } else {
            err("expected )")
        }
    }
}

/// Parse "y = …", "f(x) = …" or a bare expression in x.
pub fn parse(src: &str) -> Result<Expr, ParseError> {
    let s = src.trim();
    let body = match s.split_once('=') {
        Some((lhs, rhs)) => {
            let l: String = lhs.chars().filter(|c| !c.is_whitespace()).collect();
            let is_fx = l.len() == 4 && l.ends_with("(x)") && l.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
            if l == "y" || is_fx {
                rhs
            } else {
                return err("unexpected =");
            }
        }
        None => s,
    };
    parse_in(body, 'x')
}

/// Parse an expression whose variable is `var` (`x`, `t`; with `t`, `θ` is accepted too).
/// With any other character (e.g. `'\0'`), the expression must be a constant.
pub fn parse_in(body: &str, var: char) -> Result<Expr, ParseError> {
    if body.trim().is_empty() {
        return err("empty expression");
    }
    let mut p = Parser {
        toks: tokenize(body, var)?,
        k: 0,
    };
    let tree = p.expr()?;
    if p.k < p.toks.len() {
        return err(format!("unexpected {:?}", p.toks[p.k]));
    }
    Ok(tree)
}

/// Value of a constant expression such as `pi/2` or `-3.5`.
pub fn parse_number(src: &str) -> Result<f64, ParseError> {
    let v = parse_in(src, '\0')?.eval(0.0);
    if v.is_finite() {
        Ok(v)
    } else {
        err(format!("not a number: {}", src.trim()))
    }
}

impl Expr {
    pub fn eval(&self, x: f64) -> f64 {
        match self {
            Expr::Num(v) => *v,
            Expr::X => x,
            Expr::Neg(a) => -a.eval(x),
            Expr::Call(f, a) => f.apply(a.eval(x)),
            Expr::Bin(op, a, b) => {
                let (a, b) = (a.eval(x), b.eval(x));
                match op {
                    '+' => a + b,
                    '-' => a - b,
                    '*' => a * b,
                    '/' => a / b,
                    _ => a.powf(b),
                }
            }
        }
    }
}

/// Sample f on [x0, x1]; polylines are split at undefined points and poles.
pub fn sample(f: &Expr, x0: f64, x1: f64, y0: f64, y1: f64, n: usize) -> Vec<Vec<(f64, f64)>> {
    let span = y1 - y0;
    let mut lines = vec![];
    let mut cur: Vec<(f64, f64)> = vec![];
    let mut prev: Option<f64> = None;
    for i in 0..=n {
        let x = x0 + (x1 - x0) * i as f64 / n as f64;
        let y = f.eval(x);
        let ok = y.is_finite() && y.abs() < 1e9;
        let jump = ok && prev.is_some_and(|p| (y - p).abs() > span * 2.0);
        if !ok || jump {
            if cur.len() > 1 {
                lines.push(std::mem::take(&mut cur));
            }
            cur = if ok { vec![(x, y)] } else { vec![] };
        } else {
            cur.push((x, y));
        }
        prev = ok.then_some(y);
    }
    if cur.len() > 1 {
        lines.push(cur);
    }
    lines
}

/// Nice tick step for an axis range.
pub fn tick_step(span: f64, target: f64) -> f64 {
    let raw = span / target;
    let p = 10f64.powf(raw.log10().floor());
    let m = raw / p;
    (if m < 1.5 {
        1.0
    } else if m < 3.5 {
        2.0
    } else if m < 7.5 {
        5.0
    } else {
        10.0
    }) * p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str, x: f64) -> f64 {
        parse(src).unwrap_or_else(|e| panic!("{src}: {e}")).eval(x)
    }
    fn near(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "{a} != {b}");
    }

    #[test]
    fn precedence_implicit_mult_functions() {
        near(at("2x^2 - 3x + 1", 2.0), 3.0);
        near(at("-x^2", 3.0), -9.0);
        near(at("y = sin(pi x)", 0.5), 1.0);
        near(at("f(x) = 2^x^2", 2.0), 16.0);
        near(at("sqrt(x)ln x", std::f64::consts::E), std::f64::consts::E.sqrt());
        near(at("sin^2 x + cos^2 x", 1.3), 1.0);
        near(at("x²+1", 3.0), 10.0);
        near(at("1/(x-1)", 3.0), 0.5);
        near(at("3e-2", 0.0), 0.03);
        near(at("e^x", 1.0), std::f64::consts::E);
        near(at("2e", 0.0), 2.0 * std::f64::consts::E);
    }

    #[test]
    fn rejects_bad_input() {
        for s in ["2+", "foo(x)", "alert(1)", "", "(x", "z = x"] {
            assert!(parse(s).is_err(), "{s} should fail");
        }
    }

    #[test]
    fn sampling_splits_at_poles() {
        assert!(sample(&parse("1/x").unwrap(), -2.0, 2.0, -5.0, 5.0, 401).len() >= 2);
        assert_eq!(tick_step(10.0, 8.0), 1.0);
        assert_eq!(tick_step(100.0, 8.0), 10.0);
        assert_eq!(tick_step(3.0, 8.0), 0.5);
    }
}
