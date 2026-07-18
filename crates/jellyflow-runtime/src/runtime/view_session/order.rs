use std::collections::BTreeMap;

/// Builds stable explicit ranks while preserving the first occurrence of duplicate IDs.
pub(super) fn index_draw_order<Id: Copy + Ord>(draw_order: &[Id]) -> BTreeMap<Id, usize> {
    let mut ranks = BTreeMap::new();
    for (rank, id) in draw_order.iter().copied().enumerate() {
        ranks.entry(id).or_insert(rank);
    }
    ranks
}

/// Sorts an already-admitted node subset into deterministic back-to-front order.
pub(super) fn sort_render_order<T, Id: Copy + Ord>(
    admitted: &mut [T],
    id: impl Fn(&T) -> Id,
    is_selected: impl Fn(&T) -> bool,
    explicit_ranks: &BTreeMap<Id, usize>,
) {
    admitted.sort_unstable_by_key(|candidate| {
        let id = id(candidate);
        let elevated = is_selected(candidate);
        match explicit_ranks.get(&id) {
            Some(rank) => (elevated, false, *rank, id),
            None => (elevated, true, 0, id),
        }
    });
}
