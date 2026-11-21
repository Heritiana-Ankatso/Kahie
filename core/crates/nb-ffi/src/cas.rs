//! Computer algebra for the app (nb-math's `CasBackend`; Giac when built with `giac`).

use crate::CoreError;
use std::sync::OnceLock;

#[derive(uniffi::Record, Clone, Debug, PartialEq)]
pub struct CasAnswer {
    /// Result in CAS syntax, e.g. `(x-2)*(x-3)`.
    pub text: String,
    /// Result as LaTeX.
    pub latex: String,
}

fn backend() -> &'static dyn nb_math::cas::CasBackend {
    static CAS: OnceLock<Box<dyn nb_math::cas::CasBackend>> = OnceLock::new();
    CAS.get_or_init(nb_math::cas::default_backend).as_ref()
}

/// Whether this build includes a CAS (the UI hides "Compute" otherwise).
#[uniffi::export]
pub fn cas_available() -> bool {
    cfg!(feature = "giac")
}

/// Evaluate a CAS command such as `factor(x^2-5x+6)` or `solve(x^2=2,x)`.
#[uniffi::export]
pub fn cas_eval(input: String) -> Result<CasAnswer, CoreError> {
    backend()
        .eval(&input)
        .map(|r| CasAnswer {
            text: r.text,
            latex: r.latex,
        })
        .map_err(|e| CoreError::Format { detail: e.to_string() })
}

/// Operations the text dialog offers (E2.3), applied to what the teacher typed.
#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasOp {
    /// The line as typed (a Giac command or an expression).
    AsTyped,
    Solve,
    Factor,
    Expand,
    Simplify,
    Derivative,
    Antiderivative,
    /// Decimal value (evalf).
    Numeric,
}

/// The variable of an expression: `x` if present, else `t`, `n`, `y`, else the first
/// single-letter name (function names and constants are skipped).
pub fn main_variable(expr: &str) -> String {
    const WORDS: &[&str] = &[
        "arcsin", "arccos", "arctan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "sin", "cos", "tan", "exp", "ln",
        "log", "sqrt", "abs", "floor", "ceil", "sign", "pi", "e", "i",
    ];
    let mut letters: Vec<char> = vec![];
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_alphabetic() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_alphabetic() {
                j += 1;
            }
            let word: String = chars[i..j].iter().collect();
            // Split "xsin" / "2ax" like the parser: known words first, other letters alone.
            let mut rest = word.as_str();
            while !rest.is_empty() {
                if let Some(w) = WORDS.iter().filter(|w| w.len() > 1).find(|w| rest.starts_with(**w)) {
                    rest = &rest[w.len()..];
                } else {
                    let c = rest.chars().next().unwrap();
                    if c != 'e' && c != 'i' {
                        letters.push(c);
                    }
                    rest = &rest[1..];
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    for v in ['x', 't', 'n', 'y'] {
        if letters.contains(&v) {
            return v.to_string();
        }
    }
    letters.first().map_or("x".into(), |c| c.to_string())
}

/// The Giac command for `op` applied to `input` (one line, as typed), e.g.
/// `Solve` on `x^2 = 2` → `solve(x^2 = 2, x)`.
#[uniffi::export]
pub fn cas_command(op: CasOp, input: String) -> String {
    let e = input.trim().trim_start_matches('=').trim();
    let v = main_variable(e);
    match op {
        CasOp::AsTyped => e.to_string(),
        CasOp::Solve => {
            let eq = if e.contains('=') {
                e.to_string()
            } else {
                format!("{e} = 0")
            };
            format!("solve({eq}, {v})")
        }
        CasOp::Factor => format!("factor({e})"),
        CasOp::Expand => format!("expand({e})"),
        CasOp::Simplify => format!("simplify({e})"),
        CasOp::Derivative => format!("diff({e}, {v})"),
        CasOp::Antiderivative => format!("integrate({e}, {v})"),
        CasOp::Numeric => format!("evalf({e})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cas_reports_its_availability() {
        assert_eq!(cas_available(), cfg!(feature = "giac"));
        if cas_available() {
            assert_eq!(cas_eval("diff(x^3,x)".into()).unwrap().text, "3*x^2");
        } else {
            assert!(cas_eval("1+1".into()).is_err());
        }
    }

    #[test]
    fn commands_from_what_the_teacher_typed() {
        assert_eq!(cas_command(CasOp::Solve, "x^2 = 2".into()), "solve(x^2 = 2, x)");
        assert_eq!(cas_command(CasOp::Solve, "t^2 - 4".into()), "solve(t^2 - 4 = 0, t)");
        assert_eq!(
            cas_command(CasOp::Derivative, "sin(x)exp(x)".into()),
            "diff(sin(x)exp(x), x)"
        );
        assert_eq!(cas_command(CasOp::Antiderivative, "2a".into()), "integrate(2a, a)");
        assert_eq!(cas_command(CasOp::Numeric, "= sqrt(2)".into()), "evalf(sqrt(2))");
        assert_eq!(main_variable("e^x + pi"), "x");
        assert_eq!(main_variable("exp(n)"), "n");
        assert_eq!(main_variable("3"), "x");
    }
}
