use std::collections::BTreeMap;

use jellyflow_core::core::NodeId;

/// Builds the stable explicit rank used by candidate-only native frame ordering.
pub(super) fn index_node_draw_order(draw_order: &[NodeId]) -> BTreeMap<NodeId, usize> {
    let mut ranks = BTreeMap::new();
    for (rank, node) in draw_order.iter().copied().enumerate() {
        ranks.entry(node).or_insert(rank);
    }
    ranks
}

/// Sorts an already-admitted node subset into deterministic back-to-front order.
pub(super) fn sort_admitted_node_render_order<T>(
    admitted: &mut [T],
    node_id: impl Fn(&T) -> NodeId,
    is_selected: impl Fn(&T) -> bool,
    explicit_ranks: &BTreeMap<NodeId, usize>,
) {
    admitted.sort_unstable_by_key(|candidate| {
        let node = node_id(candidate);
        let elevated = is_selected(candidate);
        match explicit_ranks.get(&node) {
            Some(rank) => (elevated, false, *rank, node),
            None => (elevated, true, 0, node),
        }
    });
}
