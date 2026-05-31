use super::GraphValidationError;

#[derive(Debug, Default)]
pub struct GraphValidationReport {
    pub errors: Vec<GraphValidationError>,
}

impl GraphValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn errors(&self) -> &[GraphValidationError] {
        &self.errors
    }

    pub fn into_errors(self) -> Vec<GraphValidationError> {
        self.errors
    }

    pub(crate) fn push(&mut self, error: GraphValidationError) {
        self.errors.push(error);
    }

    pub(crate) fn has_unsupported_graph_version(&self) -> bool {
        self.errors
            .iter()
            .any(|error| matches!(error, GraphValidationError::UnsupportedGraphVersion { .. }))
    }
}
