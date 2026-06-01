use crate::rules::Diagnostic;
use jellyflow_core::ops::ApplyError;

#[derive(Debug, thiserror::Error)]
pub enum ApplyPipelineError {
    #[error("failed to apply transaction ops")]
    Apply(#[from] ApplyError),
    #[error("concretization did not converge within bound={bound}")]
    ConcretizeNonConvergent { bound: usize },
    #[error("transaction rejected by diagnostics: {message}")]
    Rejected {
        message: String,
        diagnostics: Vec<Diagnostic>,
    },
}

impl ApplyPipelineError {
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Rejected { diagnostics, .. } => Some(diagnostics),
            _ => None,
        }
    }
}
