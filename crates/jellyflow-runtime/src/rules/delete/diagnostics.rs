use crate::rules::{DeleteDecision, DeletePlan, Diagnostic, DiagnosticSeverity, DiagnosticTarget};

pub(super) fn rejected(diagnostics: Vec<Diagnostic>) -> DeletePlan {
    DeletePlan {
        decision: DeleteDecision::Reject,
        diagnostics,
        ops: Vec::new(),
    }
}

pub(super) fn delete_diagnostic(
    key: impl Into<String>,
    target: DiagnosticTarget,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic {
        key: key.into(),
        severity: DiagnosticSeverity::Error,
        target,
        message: message.into(),
        fixes: Vec::new(),
    }
}

pub(super) fn planning_diagnostic(message: impl Into<String>) -> Diagnostic {
    delete_diagnostic("delete.planning_failed", DiagnosticTarget::Graph, message)
}
