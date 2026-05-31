use super::GraphTransaction;

mod coalesce;
mod noop;

use coalesce::coalesce_setter_chains;
use noop::op_is_noop;

pub fn normalize_transaction(tx: GraphTransaction) -> GraphTransaction {
    let (label, ops) = tx.into_parts();
    let ops = coalesce_setter_chains(ops)
        .into_iter()
        .filter(|op| !op_is_noop(op));
    GraphTransaction::from_parts(label, ops)
}
