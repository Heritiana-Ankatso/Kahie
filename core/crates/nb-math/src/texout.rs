//! Expressions as LaTeX (for display) and as pgfplots/PGF maths (for plots in exported
//! LaTeX documents, E2.12).

use crate::{Expr, Func};

fn prec(e: &Expr) -> u8 {
    match e {
        Expr::Bin('+' | '-', ..) => 1,
        Expr::Bin('*' | '/', ..) | Expr::Neg(_) => 2,
        Expr::Bin(..) => 3,
        _ => 4,
    }
}

fn num(v: f64) -> String {
    if v == std::f64::consts::PI {
        return "\\pi".into();
    }
    if v == std::f64::consts::E {
        return "e".into();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn latex_name(f: Func) -> &'static str {
    match f {
        Func::Sin => "\\sin",
        Func::Cos => "\\cos",
        Func::Tan => "\\tan",
        Func::Asin => "\\arcsin",
        Func::Acos => "\\arccos",
        Func::Atan => "\\arctan",
        Func::Sinh => "\\sinh",
        Func::Cosh => "\\cosh",
        Func::Tanh => "\\tanh",
        Func::Exp => "\\exp",
        Func::Ln => "\\ln",
        Func::Log => "\\log",
        _ => "",
    }
}

impl Expr {
    /// LaTeX for display, with `var` as the variable (e.g. `x`, `t`, `\theta`).
    pub fn to_latex(&self, var: &str) -> String {
        let wrap = |e: &Expr, min: u8| {
            let s = e.to_latex(var);
            if prec(e) < min {
                format!("\\left({s}\\right)")
            } else {
                s
            }
        };
        match self {
            Expr::Num(v) => num(*v),
            Expr::X => var.to_string(),
            Expr::Neg(a) => format!("-{}", wrap(a, 2)),
            Expr::Bin('+', a, b) => format!("{} + {}", a.to_latex(var), wrap(b, 1)),
            Expr::Bin('-', a, b) => format!("{} - {}", a.to_latex(var), wrap(b, 2)),
            Expr::Bin('*', a, b) => {
                // 2x, 3\sin x; otherwise an explicit dot.
                let (l, r) = (wrap(a, 2), wrap(b, 3));
                let implicit = matches!(**a, Expr::Num(_)) && !matches!(**b, Expr::Num(_));
                if implicit {
                    format!("{l}{r}")
                } else {
                    format!("{l} \\cdot {r}")
                }
            }
            Expr::Bin('/', a, b) => format!("\\frac{{{}}}{{{}}}", a.to_latex(var), b.to_latex(var)),
            Expr::Bin(_, a, b) => format!("{{{}}}^{{{}}}", wrap(a, 4), b.to_latex(var)),
            Expr::Call(f, a) => {
                let arg = a.to_latex(var);
                match f {
                    Func::Sqrt => format!("\\sqrt{{{arg}}}"),
                    Func::Cbrt => format!("\\sqrt[3]{{{arg}}}"),
                    Func::Abs => format!("\\left|{arg}\\right|"),
                    Func::Floor => format!("\\lfloor {arg} \\rfloor"),
                    Func::Ceil => format!("\\lceil {arg} \\rceil"),
                    Func::Sign => format!("\\operatorname{{sgn}}\\left({arg}\\right)"),
                    _ => format!("{}\\left({arg}\\right)", latex_name(*f)),
                }
            }
        }
    }

    /// PGF maths for `\addplot`, with `var` as the variable (`x` or `\t`). Trigonometric
    /// functions assume `trig format plots=rad` (set in the exported preamble).
    pub fn to_pgf(&self, var: &str) -> String {
        match self {
            Expr::Num(v) => {
                if *v == std::f64::consts::PI {
                    "pi".into()
                } else if *v == std::f64::consts::E {
                    "e".into()
                } else {
                    format!("{v}")
                }
            }
            Expr::X => var.to_string(),
            Expr::Neg(a) => format!("(-({}))", a.to_pgf(var)),
            Expr::Bin(op, a, b) => format!("({}{op}{})", a.to_pgf(var), b.to_pgf(var)),
            Expr::Call(f, a) => {
                let arg = a.to_pgf(var);
                match f {
                    Func::Ln => format!("ln({arg})"),
                    Func::Log => format!("log10({arg})"),
                    Func::Cbrt => format!("(sign({arg})*abs({arg})^(1/3))"),
                    Func::Sign => format!("sign({arg})"),
                    Func::Floor => format!("floor({arg})"),
                    Func::Ceil => format!("ceil({arg})"),
                    Func::Asin => format!("asin({arg})"),
                    Func::Acos => format!("acos({arg})"),
                    Func::Atan => format!("atan({arg})"),
                    Func::Sqrt => format!("sqrt({arg})"),
                    Func::Abs => format!("abs({arg})"),
                    _ => format!("{}({arg})", latex_name(*f).trim_start_matches('\\')),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::parse;

    #[test]
    fn latex_and_pgf() {
        let f = parse("2x^2 - 3sin(x)/(x+1)").unwrap();
        assert_eq!(f.to_latex("x"), "2{x}^{2} - \\frac{3\\sin\\left(x\\right)}{x + 1}");
        assert_eq!(
            parse("sqrt(x)+abs(x-1)").unwrap().to_latex("x"),
            "\\sqrt{x} + \\left|x - 1\\right|"
        );
        assert_eq!(parse("-(x+1)^2").unwrap().to_latex("t"), "-{\\left(t + 1\\right)}^{2}");
        let g = parse("2sin(pi x) + ln x").unwrap();
        assert_eq!(g.to_pgf("x"), "((2*sin((pi*x)))+ln(x))");
        // Same values as our evaluator, for the functions pgf names differently.
        assert_eq!(parse("log(x)").unwrap().to_pgf("\\t"), "log10(\\t)");
        assert!(parse("sqrt(x)").unwrap().to_pgf("x") == "sqrt(x)" && parse("abs(x)").unwrap().to_pgf("x") == "abs(x)");
    }
}
