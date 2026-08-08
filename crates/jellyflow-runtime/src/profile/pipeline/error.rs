use crate::graph_kernel::ApplyError;
use crate::rules::Diagnostic;

#[derive(Debug, thiserror::Error)]
pub enum ApplyPipelineError {
    #[error("internal graph mutation failed: {message}")]
    InternalMutation { message: String },
    #[error("concretization did not converge within bound={bound}")]
    ConcretizeNonConvergent { bound: usize },
    #[error("transaction rejected by diagnostics: {message}")]
    Rejected {
        message: String,
        diagnostics: Vec<Diagnostic>,
    },
}

impl From<ApplyError> for ApplyPipelineError {
    fn from(source: ApplyError) -> Self {
        Self::InternalMutation {
            message: source.to_string(),
        }
    }
}

impl ApplyPipelineError {
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        match self {
            Self::Rejected { diagnostics, .. } => Some(diagnostics),
            _ => None,
        }
    }
}
