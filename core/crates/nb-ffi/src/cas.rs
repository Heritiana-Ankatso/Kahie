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
}
