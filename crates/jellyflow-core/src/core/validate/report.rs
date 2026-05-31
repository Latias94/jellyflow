use super::GraphValidationError;

#[derive(Debug, Default)]
pub struct GraphValidationReport {
    pub errors: Vec<GraphValidationError>,
}

impl GraphValidationReport {
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }
}
