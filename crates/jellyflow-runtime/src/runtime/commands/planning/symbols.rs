use std::collections::BTreeSet;

use crate::graph_kernel::GraphOp;
use jellyflow_core::core::{Symbol, SymbolId};

use crate::runtime::commands::SymbolEdit;
use crate::runtime::commands::model::SymbolOperation;

use super::GraphCommandPlanningError;
use super::context::PlanningContext;

pub(super) fn plan_symbol_edit(
    context: &mut PlanningContext<'_>,
    edit: &SymbolEdit,
) -> Result<(), GraphCommandPlanningError> {
    validate_symbol_lifecycles(edit)?;
    for operation in &edit.operations {
        match operation {
            SymbolOperation::Add { id, symbol } => {
                context.read_facts(1);
                if context.graph().symbol(id).is_some() {
                    return Err(GraphCommandPlanningError::SymbolAlreadyExists(*id));
                }
                context.stage([GraphOp::AddSymbol {
                    id: *id,
                    symbol: symbol.clone(),
                }])?;
            }
            SymbolOperation::Update { id, symbol } => {
                context.read_facts(1);
                let current = context
                    .graph()
                    .symbol(id)
                    .cloned()
                    .ok_or(GraphCommandPlanningError::MissingSymbol(*id))?;
                context.stage(symbol_updates(*id, &current, symbol))?;
            }
            SymbolOperation::Remove { id } => {
                context.read_facts(1);
                let current = context
                    .graph()
                    .symbol(id)
                    .cloned()
                    .ok_or(GraphCommandPlanningError::MissingSymbol(*id))?;
                let referrers = context.symbol_referrers(*id);
                if !referrers.is_empty() {
                    return Err(GraphCommandPlanningError::SymbolReferenced {
                        symbol: *id,
                        referrers,
                    });
                }
                context.stage([GraphOp::RemoveSymbol {
                    id: *id,
                    symbol: current,
                }])?;
            }
        }
    }
    Ok(())
}

fn validate_symbol_lifecycles(edit: &SymbolEdit) -> Result<(), GraphCommandPlanningError> {
    let mut added = BTreeSet::new();
    let mut removed = BTreeSet::new();
    for operation in &edit.operations {
        match operation {
            SymbolOperation::Add { id, .. } => {
                if removed.contains(id) {
                    return Err(GraphCommandPlanningError::SymbolLifecycleConflict(*id));
                }
                added.insert(*id);
            }
            SymbolOperation::Remove { id } => {
                if added.contains(id) {
                    return Err(GraphCommandPlanningError::SymbolLifecycleConflict(*id));
                }
                removed.insert(*id);
            }
            SymbolOperation::Update { .. } => {}
        }
    }
    Ok(())
}

fn symbol_updates(id: SymbolId, current: &Symbol, desired: &Symbol) -> Vec<GraphOp> {
    let mut operations = Vec::with_capacity(4);
    if current.name != desired.name {
        operations.push(GraphOp::SetSymbolName {
            id,
            from: current.name.clone(),
            to: desired.name.clone(),
        });
    }
    if current.ty != desired.ty {
        operations.push(GraphOp::SetSymbolType {
            id,
            from: current.ty.clone(),
            to: desired.ty.clone(),
        });
    }
    if current.default_value != desired.default_value {
        operations.push(GraphOp::SetSymbolDefaultValue {
            id,
            from: current.default_value.clone(),
            to: desired.default_value.clone(),
        });
    }
    if current.meta != desired.meta {
        operations.push(GraphOp::SetSymbolMeta {
            id,
            from: current.meta.clone(),
            to: desired.meta.clone(),
        });
    }
    operations
}
