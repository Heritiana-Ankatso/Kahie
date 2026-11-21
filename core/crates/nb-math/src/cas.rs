//! Computer algebra (decision D7, spike P0-07): a small trait so the app is not tied to one
//! engine, and the Giac implementation (cargo feature `giac`).

/// Result of a CAS evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct CasResult {
    /// Canonical result in the engine's syntax (e.g. `(x-2)*(x-3)`).
    pub text: String,
    /// The result as LaTeX, for display.
    pub latex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CasError {
    /// The engine rejected the input or failed to evaluate it.
    Eval(String),
    /// No CAS in this build.
    Unavailable,
}

impl std::fmt::Display for CasError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CasError::Eval(m) => write!(f, "CAS: {m}"),
            CasError::Unavailable => f.write_str("CAS not available in this build"),
        }
    }
}

impl std::error::Error for CasError {}

/// Brackets must balance: Giac's parser silently "repairs" input such as `factor(x^2`
/// (returning `x^2`), which would give the user a wrong answer instead of an error.
pub fn check_brackets(input: &str) -> Result<(), CasError> {
    let mut stack = vec![];
    let mut in_str = false;
    for c in input.chars() {
        match c {
            '"' => in_str = !in_str,
            _ if in_str => {}
            '(' | '[' | '{' => stack.push(c),
            ')' | ']' | '}' => {
                let open = match c {
                    ')' => '(',
                    ']' => '[',
                    _ => '{',
                };
                if stack.pop() != Some(open) {
                    return Err(CasError::Eval(format!("unbalanced '{c}'")));
                }
            }
            _ => {}
        }
    }
    match (in_str, stack.last()) {
        (true, _) => Err(CasError::Eval("unterminated string".into())),
        (_, Some(c)) => Err(CasError::Eval(format!("unclosed '{c}'"))),
        _ => Ok(()),
    }
}

pub trait CasBackend: Send + Sync {
    /// Evaluate a command in the engine's syntax (Giac: `factor(x^2-5x+6)`, `solve(...)`…).
    fn eval(&self, input: &str) -> Result<CasResult, CasError>;
}

/// The CAS for this build: Giac when compiled with the `giac` feature, otherwise none.
pub fn default_backend() -> Box<dyn CasBackend> {
    #[cfg(feature = "giac")]
    {
        Box::new(giac::GiacCas)
    }
    #[cfg(not(feature = "giac"))]
    {
        Box::new(NoCas)
    }
}

/// Placeholder backend for builds without a CAS.
pub struct NoCas;

impl CasBackend for NoCas {
    fn eval(&self, _input: &str) -> Result<CasResult, CasError> {
        Err(CasError::Unavailable)
    }
}

#[cfg(feature = "giac")]
pub mod giac {
    use super::{CasBackend, CasError, CasResult};
    use std::ffi::{c_char, c_int, CStr, CString};
    use std::sync::Mutex;

    extern "C" {
        fn giacshim_eval2(src: *const c_char, text: *mut *mut c_char, latex: *mut *mut c_char) -> c_int;
        fn giacshim_free(s: *mut c_char);
    }

    /// Giac keeps one global context: serialise every call.
    static LOCK: Mutex<()> = Mutex::new(());

    /// Giac (GPL-3), linked through `src/giac_shim.cpp`.
    pub struct GiacCas;

    impl CasBackend for GiacCas {
        fn eval(&self, input: &str) -> Result<CasResult, CasError> {
            super::check_brackets(input)?;
            let src = CString::new(input).map_err(|_| CasError::Eval("input contains a NUL byte".into()))?;
            let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
            let (mut t, mut l) = (std::ptr::null_mut(), std::ptr::null_mut());
            // SAFETY: the shim always sets both out-pointers to malloc'd C strings.
            let (code, text, latex) = unsafe {
                let code = giacshim_eval2(src.as_ptr(), &mut t, &mut l);
                let text = CStr::from_ptr(t).to_string_lossy().into_owned();
                let latex = CStr::from_ptr(l).to_string_lossy().into_owned();
                giacshim_free(t);
                giacshim_free(l);
                (code, text, latex)
            };
            if code != 0 {
                return Err(CasError::Eval(text));
            }
            if text == "undef" {
                return Err(CasError::Eval("undefined result (check the input)".into()));
            }
            Ok(CasResult { text, latex })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracket_check() {
        assert!(check_brackets("f([1,2],{a:\"(\"})").is_ok());
        assert!(check_brackets("factor(x^2").is_err());
        assert!(check_brackets("a)(").is_err());
        assert!(check_brackets("[1,2)").is_err());
        assert!(check_brackets("\"abc").is_err());
    }

    #[cfg(not(feature = "giac"))]
    #[test]
    fn without_giac_the_cas_says_so() {
        assert_eq!(default_backend().eval("1+1"), Err(CasError::Unavailable));
    }

    #[cfg(feature = "giac")]
    #[test]
    fn giac_factors_solves_and_reports_errors() {
        let cas = default_backend();
        let r = cas.eval("factor(x^2-5*x+6)").unwrap();
        let t = r.text.replace(' ', "");
        assert!(t == "(x-2)*(x-3)" || t == "(x-3)*(x-2)", "{t}");
        assert!(r.latex.contains("x-2"), "{}", r.latex);
        assert_eq!(cas.eval("simplify(sin(x)^2+cos(x)^2)").unwrap().text, "1");
        assert!(cas.eval("integrate(x^2,x,0,1)").unwrap().text.contains("1/3"));
        assert!(
            matches!(cas.eval("factor(x^2"), Err(CasError::Eval(_))),
            "unbalanced input is refused"
        );
        assert!(matches!(cas.eval("solve("), Err(CasError::Eval(_))));
        assert!(
            matches!(cas.eval("integrate(x,)"), Err(CasError::Eval(_))),
            "undef is an error"
        );
        // Concurrent callers are serialised, not racing on Giac's global context.
        let hs: Vec<_> = (0..4)
            .map(|k| std::thread::spawn(move || default_backend().eval(&format!("diff(x^{},x)", k + 2)).unwrap().text))
            .collect();
        let outs: Vec<String> = hs.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(outs[0], "2*x");
    }
}
