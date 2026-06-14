use crate::core::Graph;
use crate::ops::GraphOp;

use super::ApplyError;

pub(super) fn apply_symbol_op(graph: &mut Graph, op: &GraphOp) -> Result<(), ApplyError> {
    match op {
        GraphOp::AddSymbol { id, symbol } => {
            if graph.symbols().contains_key(id) {
                return Err(ApplyError::SymbolAlreadyExists { id: *id });
            }
            graph.insert_symbol(*id, symbol.clone());
        }
        GraphOp::RemoveSymbol { id, symbol } => {
            let Some(current) = graph.symbols().get(id) else {
                return Err(ApplyError::MissingSymbol { id: *id });
            };
            if current.name != symbol.name
                || current.ty != symbol.ty
                || current.default_value != symbol.default_value
                || current.meta != symbol.meta
            {
                return Err(ApplyError::RemoveSymbolMismatch { id: *id });
            }
            graph.remove_symbol(id);
        }
        GraphOp::SetSymbolName { id, to, .. } => {
            let Some(symbol) = graph.symbol_mut(id) else {
                return Err(ApplyError::MissingSymbol { id: *id });
            };
            symbol.name = to.clone();
        }
        GraphOp::SetSymbolType { id, to, .. } => {
            let Some(symbol) = graph.symbol_mut(id) else {
                return Err(ApplyError::MissingSymbol { id: *id });
            };
            symbol.ty = to.clone();
        }
        GraphOp::SetSymbolDefaultValue { id, to, .. } => {
            let Some(symbol) = graph.symbol_mut(id) else {
                return Err(ApplyError::MissingSymbol { id: *id });
            };
            symbol.default_value = to.clone();
        }
        GraphOp::SetSymbolMeta { id, to, .. } => {
            let Some(symbol) = graph.symbol_mut(id) else {
                return Err(ApplyError::MissingSymbol { id: *id });
            };
            symbol.meta = to.clone();
        }
        _ => unreachable!("non-symbol op routed to symbol apply"),
    }
    Ok(())
}
