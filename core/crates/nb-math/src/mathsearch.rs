//! Maths search (E2.13): find formulas by structure, e.g. `x^2 + ?`.
//!
//! LaTeX (maths blocks) and plot syntax (plot lines) are normalised to the same tokens:
//! `\frac{a}{b}` → `(a)/(b)`, `\cdot`/`\times`/`·` → `*`, braces → brackets, `\left`/`\right`
//! and spacing dropped, implicit products made explicit (`2x` → `2*x`). A query matches a
//! contiguous run of tokens; `?` stands for one *term* (numbers, letters, functions and
//! brackets joined by `*`, `/` or `^`, but not `+`, `-` or `=`). Commutativity is not
//! applied: `x^2 + ?` finds `x^2 + 3x` but not `3x + x^2`.

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Num(String),
    Name(String),
    Op(char),
    Open,
    Close,
    Any,
}

const WORDS: &[&str] = &[
    "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "sqrt", "sin", "cos", "tan", "exp", "ln", "log", "abs",
    "lim", "sum", "int", "pi", "theta", "alpha", "beta", "infty",
];

/// LaTeX to the plot-like linear syntax the tokenizer reads.
fn delatex(s: &str) -> String {
    let mut t = s.to_string();
    for (a, b) in [
        ("\\left", ""),
        ("\\right", ""),
        ("\\,", ""),
        ("\\;", ""),
        ("\\!", ""),
        ("\\quad", ""),
        ("\\cdot", "*"),
        ("\\times", "*"),
        ("\\div", "/"),
        ("\\dfrac", "\\frac"),
        ("\\tfrac", "\\frac"),
        ("\\operatorname", ""),
        ("\\mathrm", ""),
    ] {
        t = t.replace(a, b);
    }
    // \frac{a}{b} → {a}/{b}
    t = t.replace("\\frac", "\u{1}");
    let mut out = String::new();
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{1}' {
            // Copy the first group, then "/", then the second group follows as is.
            i += 1;
            let mut depth = 0;
            while i < chars.len() {
                let c = chars[i];
                out.push(c);
                i += 1;
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            out.push('/');
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out.replace('\\', "")
}

fn tokens(src: &str, latex: bool) -> Vec<Tok> {
    let s = if latex || src.contains('\\') {
        delatex(src)
    } else {
        src.to_string()
    };
    let c: Vec<char> = s.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let mut j = i;
            while j < c.len() && (c[j].is_ascii_digit() || c[j] == '.' || c[j] == ',') {
                j += 1;
            }
            out.push(Tok::Num(
                c[i..j]
                    .iter()
                    .filter(|x| **x != ',')
                    .collect::<String>()
                    .replace(',', "."),
            ));
            i = j;
        } else if ch.is_alphabetic() {
            let mut j = i;
            while j < c.len() && c[j].is_alphabetic() {
                j += 1;
            }
            let word: String = c[i..j].iter().collect::<String>().to_lowercase();
            let mut rest = word.as_str();
            while !rest.is_empty() {
                if let Some(w) = WORDS.iter().find(|w| rest.starts_with(**w)) {
                    out.push(Tok::Name((*w).to_string()));
                    rest = &rest[w.len()..];
                } else {
                    let n = rest.chars().next().unwrap();
                    out.push(Tok::Name(n.to_string()));
                    rest = &rest[n.len_utf8()..];
                }
            }
            i = j;
        } else {
            match ch {
                '(' | '{' | '[' => out.push(Tok::Open),
                ')' | '}' | ']' => out.push(Tok::Close),
                '?' => out.push(Tok::Any),
                '·' | '×' => out.push(Tok::Op('*')),
                '÷' => out.push(Tok::Op('/')),
                '−' => out.push(Tok::Op('-')),
                '²' => out.extend([Tok::Op('^'), Tok::Num("2".into())]),
                '³' => out.extend([Tok::Op('^'), Tok::Num("3".into())]),
                '+' | '-' | '*' | '/' | '^' | '=' | '<' | '>' | '!' | '|' | '_' => out.push(Tok::Op(ch)),
                _ => {}
            }
            i += 1;
        }
    }
    // {2} → 2: a group around a single number or letter is just that token.
    let mut k = 0;
    while k + 2 < out.len() {
        if out[k] == Tok::Open && matches!(out[k + 1], Tok::Num(_) | Tok::Name(_) | Tok::Any) && out[k + 2] == Tok::Close {
            let keep_group = k > 0 && matches!(&out[k - 1], Tok::Name(n) if is_function(n));
            if !keep_group {
                out.remove(k + 2);
                out.remove(k);
                continue;
            }
        }
        k += 1;
    }
    // Explicit products: 2x, x(…), )(, 3sin, )x.
    let mut res: Vec<Tok> = vec![];
    for t in out {
        if let Some(prev) = res.last() {
            let left = matches!(prev, Tok::Num(_) | Tok::Close) || matches!(prev, Tok::Name(n) if !is_function(n));
            let right = matches!(t, Tok::Num(_) | Tok::Name(_) | Tok::Open | Tok::Any);
            let num_num = matches!((prev, &t), (Tok::Num(_), Tok::Num(_)));
            if left && right && !num_num && !matches!(prev, Tok::Any) {
                res.push(Tok::Op('*'));
            }
        }
        res.push(t);
    }
    res
}

fn is_function(n: &str) -> bool {
    matches!(
        n,
        "arcsin"
            | "arccos"
            | "arctan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "sqrt"
            | "sin"
            | "cos"
            | "tan"
            | "exp"
            | "ln"
            | "log"
            | "abs"
            | "lim"
            | "sum"
            | "int"
    )
}

/// Length of the term starting at `i` (0 if none): atoms joined by `*`, `/`, `^`.
fn term_len(t: &[Tok], i: usize) -> usize {
    fn atom(t: &[Tok], i: usize) -> usize {
        match t.get(i) {
            Some(Tok::Num(_)) => 1,
            Some(Tok::Name(n)) if is_function(n) => {
                // f(…) or f x or f^2(…)
                let mut j = i + 1;
                if t.get(j) == Some(&Tok::Op('^')) {
                    j += 1 + atom(t, j + 1);
                }
                let a = atom(t, j);
                if a == 0 {
                    j - i
                } else {
                    j - i + a
                }
            }
            Some(Tok::Name(_)) => 1,
            Some(Tok::Open) => {
                let mut depth = 0;
                for (k, x) in t.iter().enumerate().skip(i) {
                    match x {
                        Tok::Open => depth += 1,
                        Tok::Close => {
                            depth -= 1;
                            if depth == 0 {
                                return k + 1 - i;
                            }
                        }
                        _ => {}
                    }
                }
                0
            }
            Some(Tok::Op('-')) => {
                let a = atom(t, i + 1);
                if a == 0 {
                    0
                } else {
                    a + 1
                }
            }
            _ => 0,
        }
    }
    let mut j = i;
    let a = atom(t, j);
    if a == 0 {
        return 0;
    }
    j += a;
    while let Some(Tok::Op('*' | '/' | '^')) = t.get(j) {
        let b = atom(t, j + 1);
        if b == 0 {
            break;
        }
        j += 1 + b;
    }
    j - i
}

/// Whether the query tokens match `t` starting at `i`; returns the end on success.
fn match_at(q: &[Tok], t: &[Tok], i: usize) -> Option<usize> {
    let Some((first, rest)) = q.split_first() else {
        return Some(i);
    };
    if *first == Tok::Any {
        // "(?)": inside brackets, ? stands for everything up to the closing bracket.
        if rest.first() == Some(&Tok::Close) {
            let mut depth = 0;
            for (k, x) in t.iter().enumerate().skip(i) {
                match x {
                    Tok::Open => depth += 1,
                    Tok::Close if depth == 0 => return if k > i { match_at(rest, t, k) } else { None },
                    Tok::Close => depth -= 1,
                    _ => {}
                }
            }
            return None;
        }
        // Try the longest term first, then shorter prefixes made of whole atoms.
        let n = term_len(t, i);
        if n == 0 {
            return None;
        }
        for len in (1..=n).rev() {
            if let Some(end) = match_at(rest, t, i + len) {
                return Some(end);
            }
        }
        return None;
    }
    if t.get(i) == Some(first) {
        match_at(rest, t, i + 1)
    } else {
        None
    }
}

/// A parsed maths query.
#[derive(Clone, Debug)]
pub struct MathQuery(Vec<Tok>);

impl MathQuery {
    /// `None` for queries that are plain words (searched as text instead).
    pub fn parse(query: &str) -> Option<Self> {
        let q = query.trim();
        let mathy = q.chars().any(|c| "?^\\/*=()+²³·×".contains(c))
            || q.chars()
                .zip(q.chars().skip(1))
                .any(|(a, b)| a.is_ascii_digit() && b.is_alphabetic());
        if !mathy {
            return None;
        }
        let t = tokens(q, false);
        (!t.is_empty()).then_some(Self(t))
    }

    /// Whether `formula` (LaTeX if `latex`, else plot syntax) contains the query.
    pub fn matches(&self, formula: &str, latex: bool) -> bool {
        let t = tokens(formula, latex);
        (0..t.len()).any(|i| match_at(&self.0, &t, i).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(q: &str, f: &str, latex: bool) -> bool {
        MathQuery::parse(q).unwrap().matches(f, latex)
    }

    #[test]
    fn structure_with_wildcards() {
        assert!(m("x^2 + ?", "2x^2 + 3x - 1", false));
        assert!(m("x^2 + ?", "f(x) = x^{2} + \\frac{1}{2}", true));
        assert!(!m("x^2 + ?", "3x + x^2", false), "no commutativity");
        assert!(
            m("\\frac{?}{2}", "y = \\dfrac{x+1}{2}", true),
            "a bracketed group is one term"
        );
        assert!(m("?/2", "(x+1)/2", false));
        assert!(m("sin(?)", "y = 2\\sin\\left(3x\\right)", true));
        assert!(m("2x", "y = 2 \\cdot x + 1", true), "2x and 2·x are the same");
        assert!(m("x²", "x^2", false));
        assert!(!m("x^3", "x^2 + 3", false));
        assert!(m("a^2 + b^2 = c^2", "a^{2}+b^{2}=c^{2}", true));
        assert!(MathQuery::parse("pythagore").is_none(), "plain words are text search");
        assert!(MathQuery::parse("2x").is_some());
    }
}
