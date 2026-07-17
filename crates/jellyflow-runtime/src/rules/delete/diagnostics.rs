use crate::rules::{Diagnostic, DiagnosticTarget};

pub(super) fn delete_diagnostic(
    key: impl Into<String>,
    target: DiagnosticTarget,
    message: impl Into<String>,
) -> Diagnostic {
    Diagnostic::error(key, target, message)
}
