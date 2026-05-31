use super::super::GraphDiffPlanner;
use crate::core::{Symbol, SymbolId};
use crate::ops::GraphOp;

impl<'a> GraphDiffPlanner<'a> {
    pub(crate) fn diff_symbols(&mut self) {
        let from = self.from;
        let to = self.to;

        for (id, sym_to) in &to.symbols {
            if let Some(sym_from) = from.symbols.get(id) {
                self.diff_existing_symbol(*id, sym_from, sym_to);
            } else {
                self.tx.ops.push(GraphOp::AddSymbol {
                    id: *id,
                    symbol: sym_to.clone(),
                });
            }
        }

        for (id, sym_from) in &from.symbols {
            if !to.symbols.contains_key(id) {
                self.tx.ops.push(GraphOp::RemoveSymbol {
                    id: *id,
                    symbol: sym_from.clone(),
                });
            }
        }
    }

    fn diff_existing_symbol(&mut self, id: SymbolId, sym_from: &Symbol, sym_to: &Symbol) {
        if sym_from.name != sym_to.name {
            self.tx.ops.push(GraphOp::SetSymbolName {
                id,
                from: sym_from.name.clone(),
                to: sym_to.name.clone(),
            });
        }
        if sym_from.ty != sym_to.ty {
            self.tx.ops.push(GraphOp::SetSymbolType {
                id,
                from: sym_from.ty.clone(),
                to: sym_to.ty.clone(),
            });
        }
        if sym_from.default_value != sym_to.default_value {
            self.tx.ops.push(GraphOp::SetSymbolDefaultValue {
                id,
                from: sym_from.default_value.clone(),
                to: sym_to.default_value.clone(),
            });
        }
        if sym_from.meta != sym_to.meta {
            self.tx.ops.push(GraphOp::SetSymbolMeta {
                id,
                from: sym_from.meta.clone(),
                to: sym_to.meta.clone(),
            });
        }
    }
}
