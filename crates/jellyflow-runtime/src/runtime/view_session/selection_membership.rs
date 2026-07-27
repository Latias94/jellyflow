use std::collections::HashSet;

use jellyflow_core::EdgeId;

use crate::runtime::graph_runtime::EdgeInstanceKey;

/// Authoritative selected-edge membership shared by view-local query and geometry paths.
///
/// The backing collection stays private to this module and the type intentionally does not
/// implement `Clone`. Read paths can only probe membership; a real selection change replaces the
/// collection once and consumes the previous owner for its difference.
#[derive(Debug)]
pub(super) struct SelectedEdgeMembership {
    edges: HashSet<EdgeInstanceKey>,
    ids: HashSet<EdgeId>,
}

impl SelectedEdgeMembership {
    pub(super) fn new(edges: impl IntoIterator<Item = EdgeInstanceKey>) -> Self {
        let edges = edges.into_iter().collect::<HashSet<_>>();
        let ids = edges.iter().map(|instance| instance.edge()).collect();
        Self { edges, ids }
    }

    pub(super) fn contains(&self, edge: EdgeInstanceKey) -> bool {
        self.edges.contains(&edge)
    }

    pub(super) fn contains_id(&self, edge: EdgeId) -> bool {
        self.ids.contains(&edge)
    }

    pub(super) fn symmetric_difference<'a>(
        &'a self,
        other: &'a Self,
    ) -> impl Iterator<Item = &'a EdgeInstanceKey> {
        self.edges.symmetric_difference(&other.edges)
    }

    pub(super) fn replace(&mut self, edges: impl IntoIterator<Item = EdgeInstanceKey>) -> Self {
        std::mem::replace(self, Self::new(edges))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::runtime::graph_runtime::{
        EdgeIncarnation, EdgeInstanceKey, RuntimeId, RuntimeProvenance,
    };

    use super::*;

    fn instance(edge: u128, incarnation: u64) -> EdgeInstanceKey {
        EdgeInstanceKey::new(
            RuntimeProvenance::new(RuntimeId::UNAVAILABLE),
            EdgeId::from_u128(edge),
            EdgeIncarnation::from_u64_for_test(incarnation),
        )
    }

    #[test]
    fn replacement_consumes_the_previous_owner_and_exposes_only_the_membership_diff() {
        let retained = instance(2, 2);
        let removed = instance(1, 1);
        let added = instance(3, 3);
        let mut membership = SelectedEdgeMembership::new([removed, retained]);

        let previous = membership.replace([retained, added]);

        assert!(membership.contains(retained));
        assert!(membership.contains(added));
        assert!(!membership.contains(removed));
        assert_eq!(
            previous
                .symmetric_difference(&membership)
                .copied()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([removed, added]),
        );
    }
}
