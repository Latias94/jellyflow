use super::super::GraphDiffPlanner;
use crate::ops::GraphOp;

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_imports(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;

        for (id, import_to) in &to.imports {
            if let Some(import_from) = from.imports.get(id) {
                if import_from.alias != import_to.alias {
                    tx.ops.push(GraphOp::SetImportAlias {
                        id: *id,
                        from: import_from.alias.clone(),
                        to: import_to.alias.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddImport {
                    id: *id,
                    import: import_to.clone(),
                });
            }
        }

        for (id, import_from) in &from.imports {
            if !to.imports.contains_key(id) {
                tx.ops.push(GraphOp::RemoveImport {
                    id: *id,
                    import: import_from.clone(),
                });
            }
        }
    }
}
