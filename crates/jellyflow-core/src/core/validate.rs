use crate::core::Graph;

mod error;
mod report;
mod storage;
mod structural;

pub use error::GraphValidationError;
pub use report::GraphValidationReport;
pub use storage::validate_graph_storage;
pub use structural::validate_graph_structural;

pub fn validate_graph(graph: &Graph) -> GraphValidationReport {
    validate_graph_structural(graph)
}
