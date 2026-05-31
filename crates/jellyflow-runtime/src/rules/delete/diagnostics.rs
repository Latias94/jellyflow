use crate::rules::{DeletePlan, Diagnostic, DiagnosticTarget};

pub(super) fn rejected(diagnostics: Vec<Diagnostic>) -> DeletePlan {
    DeletePlan::reject_with_diagnostics(diagnostics)
}

pub(super) fn delete_diagnostic(
    key: impl Into<String>,
    target: DiagnosticTarget,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::error(key, target, message)
}

pub(super) fn planning_diagnostic(message: impl Into<String>) -> Diagnostic {
    delete_diagnostic("delete.planning_failed", DiagnosticTarget::Graph, message)
}
