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
    recent_interaction: Option<Id>,
) {
    admitted.sort_unstable_by_key(|candidate| {
        let id = id(candidate);
        render_order_key(
            id,
            is_selected(candidate),
            recent_interaction == Some(id),
            explicit_ranks,
        )
    });
}

pub(super) fn render_order_key<Id: Copy + Ord>(
    id: Id,
    is_selected: bool,
    is_recent_interaction: bool,
    explicit_ranks: &BTreeMap<Id, usize>,
) -> (bool, bool, bool, usize, Id) {
    let elevated = is_selected && is_recent_interaction;
    match explicit_ranks.get(&id) {
        Some(rank) => (is_selected, elevated, false, *rank, id),
        None => (is_selected, elevated, true, 0, id),
    }
}
