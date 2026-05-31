use super::super::GraphDiffPlanner;
use crate::core::{GraphId, GraphImport};
use crate::ops::GraphOp;

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_imports(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, import_to) in &to.imports {
            if let Some(import_from) = from.imports.get(id) {
                self.diff_existing_import(*id, import_from, import_to);
            } else {
                self.tx.push(GraphOp::AddImport {
                    id: *id,
                    import: import_to.clone(),
                });
            }
        }

        for (id, import_from) in &from.imports {
            if !to.imports.contains_key(id) {
                self.tx.push(GraphOp::RemoveImport {
                    id: *id,
                    import: import_from.clone(),
                });
            }
        }
    }

    fn diff_existing_import(
        &mut self,
        id: GraphId,
        import_from: &GraphImport,
        import_to: &GraphImport,
    ) {
        if import_from.alias != import_to.alias {
            self.tx.push(GraphOp::SetImportAlias {
                id,
                from: import_from.alias.clone(),
                to: import_to.alias.clone(),
            });
        }
    }
}
