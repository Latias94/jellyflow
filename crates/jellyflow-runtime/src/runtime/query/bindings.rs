use crate::runtime::binding::{BindingQueryOptions, BindingQueryResult};

use super::backend::NodeGraphQuerySnapshot;

pub(crate) fn resolve_binding_read_model(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    options: BindingQueryOptions,
) -> BindingQueryResult {
    crate::runtime::binding::resolve_binding_query(
        snapshot.graph,
        snapshot.lookups,
        snapshot.layout_facts_revision,
        snapshot.node_origin(),
        options,
    )
}
