use super::GraphTransaction;

mod coalesce;
mod noop;

use coalesce::coalesce_setter_chains;
use noop::op_is_noop;

pub fn normalize_transaction(mut tx: GraphTransaction) -> GraphTransaction {
    tx.ops = coalesce_setter_chains(tx.ops);
    tx.ops.retain(|op| !op_is_noop(op));
    tx
}
