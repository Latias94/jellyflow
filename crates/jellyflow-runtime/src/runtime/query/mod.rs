//! Store-level runtime read models.
//!
//! The first backend intentionally preserves the existing deterministic linear scans. Future
//! indexes can implement the same backend contract after equivalence tests pin the public results.

pub(crate) mod backend;
pub(crate) mod bindings;
pub(crate) mod layout_facts;
pub(crate) mod linear;
pub(crate) mod rendering;
pub(crate) mod spatial;

use crate::runtime::binding::{BindingQueryOptions, BindingQueryResult};
use crate::runtime::connection::{
    ConnectionHandleRef, ConnectionTargetCandidate, ResolvedConnectionTarget,
};
use crate::runtime::geometry::EdgePosition;
use crate::runtime::measurement::LayoutFactsQueryResult;
use crate::runtime::rendering::RenderingQueryResult;
use crate::runtime::store::NodeGraphStore;
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId};

use backend::{NodeGraphQuerySnapshot, QueryBackend};
use linear::LinearQueryBackend;
use spatial::SpatialQueryBackend;

pub(crate) fn store_query_snapshot(store: &NodeGraphStore) -> NodeGraphQuerySnapshot<'_> {
    NodeGraphQuerySnapshot::new(
        store.graph(),
        store.lookups(),
        store.view_state(),
        store.resolved_interaction_state(),
        store.layout_facts_revision(),
    )
}

pub(crate) fn rendering_query(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
) -> RenderingQueryResult {
    let snapshot = store_query_snapshot(store);
    if snapshot.interaction.spatial_index.enabled {
        SpatialQueryBackend.rendering_query(&snapshot, viewport_size)
    } else {
        LinearQueryBackend.rendering_query(&snapshot, viewport_size)
    }
}

pub(crate) fn layout_facts_query(
    store: &NodeGraphStore,
    viewport_size: CanvasSize,
) -> LayoutFactsQueryResult {
    let snapshot = store_query_snapshot(store);
    if snapshot.interaction.spatial_index.enabled {
        SpatialQueryBackend.layout_facts_query(&snapshot, viewport_size)
    } else {
        LinearQueryBackend.layout_facts_query(&snapshot, viewport_size)
    }
}

pub(crate) fn binding_query(
    store: &NodeGraphStore,
    options: BindingQueryOptions,
) -> BindingQueryResult {
    let snapshot = store_query_snapshot(store);
    if snapshot.interaction.spatial_index.enabled {
        SpatialQueryBackend.binding_query(&snapshot, options)
    } else {
        LinearQueryBackend.binding_query(&snapshot, options)
    }
}

pub(crate) fn connection_target_candidates_from_layout_facts(
    store: &NodeGraphStore,
) -> Vec<ConnectionTargetCandidate> {
    let snapshot = store_query_snapshot(store);
    layout_facts::connection_target_candidates_from_layout_facts(&snapshot)
}

pub(crate) fn resolve_connection_target_from_layout_facts(
    store: &NodeGraphStore,
    pointer: CanvasPoint,
    from: ConnectionHandleRef,
) -> ResolvedConnectionTarget {
    let snapshot = store_query_snapshot(store);
    layout_facts::resolve_connection_target_from_layout_facts(&snapshot, pointer, from)
}

pub(crate) fn edge_position_from_layout_facts(
    store: &NodeGraphStore,
    edge: EdgeId,
) -> Option<EdgePosition> {
    let snapshot = store_query_snapshot(store);
    layout_facts::edge_position_from_layout_facts(&snapshot, edge)
}
