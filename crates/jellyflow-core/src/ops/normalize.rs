use super::GraphTransaction;

mod coalesce;
mod noop;

use coalesce::coalesce_setter_chains;
use noop::op_is_noop;

pub fn normalize_transaction(tx: GraphTransaction) -> GraphTransaction {
    let mut tx = tx.map_ops(coalesce_setter_chains);
    tx.retain_ops(|op| !op_is_noop(op));
    tx
}
