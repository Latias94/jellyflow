use super::GraphDiffPlanner;
use crate::ops::{GraphMutationPlanner, GraphOp};

impl<'a> GraphDiffPlanner<'a> {
    pub(super) fn diff_imports(&mut self) {
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

    pub(super) fn diff_symbols(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;

        for (id, sym_to) in &to.symbols {
            if let Some(sym_from) = from.symbols.get(id) {
                if sym_from.name != sym_to.name {
                    tx.ops.push(GraphOp::SetSymbolName {
                        id: *id,
                        from: sym_from.name.clone(),
                        to: sym_to.name.clone(),
                    });
                }
                if sym_from.ty != sym_to.ty {
                    tx.ops.push(GraphOp::SetSymbolType {
                        id: *id,
                        from: sym_from.ty.clone(),
                        to: sym_to.ty.clone(),
                    });
                }
                if sym_from.default_value != sym_to.default_value {
                    tx.ops.push(GraphOp::SetSymbolDefaultValue {
                        id: *id,
                        from: sym_from.default_value.clone(),
                        to: sym_to.default_value.clone(),
                    });
                }
                if sym_from.meta != sym_to.meta {
                    tx.ops.push(GraphOp::SetSymbolMeta {
                        id: *id,
                        from: sym_from.meta.clone(),
                        to: sym_to.meta.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddSymbol {
                    id: *id,
                    symbol: sym_to.clone(),
                });
            }
        }

        for (id, sym_from) in &from.symbols {
            if !to.symbols.contains_key(id) {
                tx.ops.push(GraphOp::RemoveSymbol {
                    id: *id,
                    symbol: sym_from.clone(),
                });
            }
        }
    }

    pub(super) fn diff_groups(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;

        for (id, group_to) in &to.groups {
            if let Some(group_from) = from.groups.get(id) {
                if group_from.color != group_to.color {
                    tx.ops.push(GraphOp::SetGroupColor {
                        id: *id,
                        from: group_from.color.clone(),
                        to: group_to.color.clone(),
                    });
                }

                if group_from.rect != group_to.rect {
                    tx.ops.push(GraphOp::SetGroupRect {
                        id: *id,
                        from: group_from.rect,
                        to: group_to.rect,
                    });
                }
                if group_from.title != group_to.title {
                    tx.ops.push(GraphOp::SetGroupTitle {
                        id: *id,
                        from: group_from.title.clone(),
                        to: group_to.title.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddGroup {
                    id: *id,
                    group: group_to.clone(),
                });
            }
        }

        for (id, group_from) in &from.groups {
            if !to.groups.contains_key(id) {
                if let Ok(op) = GraphMutationPlanner::new(from).remove_group_op(*id) {
                    tx.ops.push(op);
                } else {
                    let detached: Vec<(crate::core::NodeId, Option<crate::core::GroupId>)> = from
                        .nodes
                        .iter()
                        .filter_map(|(node_id, node)| {
                            (node.parent == Some(*id)).then_some((*node_id, Some(*id)))
                        })
                        .collect();
                    tx.ops.push(GraphOp::RemoveGroup {
                        id: *id,
                        group: group_from.clone(),
                        detached,
                    });
                }
            }
        }
    }

    pub(super) fn diff_sticky_notes(&mut self) {
        let from = self.from;
        let to = self.to;
        let tx = &mut self.tx;

        for (id, note_to) in &to.sticky_notes {
            if let Some(note_from) = from.sticky_notes.get(id) {
                if note_from.text != note_to.text {
                    tx.ops.push(GraphOp::SetStickyNoteText {
                        id: *id,
                        from: note_from.text.clone(),
                        to: note_to.text.clone(),
                    });
                }
                if note_from.rect != note_to.rect {
                    tx.ops.push(GraphOp::SetStickyNoteRect {
                        id: *id,
                        from: note_from.rect,
                        to: note_to.rect,
                    });
                }
                if note_from.color != note_to.color {
                    tx.ops.push(GraphOp::SetStickyNoteColor {
                        id: *id,
                        from: note_from.color.clone(),
                        to: note_to.color.clone(),
                    });
                }
            } else {
                tx.ops.push(GraphOp::AddStickyNote {
                    id: *id,
                    note: note_to.clone(),
                });
            }
        }

        for (id, note_from) in &from.sticky_notes {
            if !to.sticky_notes.contains_key(id) {
                tx.ops.push(GraphOp::RemoveStickyNote {
                    id: *id,
                    note: note_from.clone(),
                });
            }
        }
    }
}
