use crate::core::Graph;
use crate::ops::GraphOp;

use super::ApplyError;

pub(super) fn apply_import_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddImport { id, import } => {
            if graph.imports().contains_key(id) {
                return Err(ApplyError::ImportAlreadyExists { id: *id });
            }
            graph.insert_import(*id, import.clone());
        }
        GraphOp::RemoveImport { id, .. } => {
            if !graph.imports().contains_key(id) {
                return Err(ApplyError::MissingImport { id: *id });
            }
            graph.remove_import(id);
        }
        GraphOp::SetImportAlias { id, to, .. } => {
            let Some(import) = graph.import_mut(id) else {
                return Err(ApplyError::MissingImport { id: *id });
            };
            import.alias = to.clone();
        }
        _ => unreachable!("non-import op routed to import apply"),
    }
    Ok(())
}
