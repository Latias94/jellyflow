use crate::runtime::connection::{
    ConnectionHandleRef, ConnectionTargetCandidate, ConnectionTargetFromHandlesInput,
    ConnectionTargetHandle, ResolvedConnectionTarget, resolve_connection_target_from_handles,
};
use crate::runtime::geometry::{
    EdgeEndpointInput, EdgePosition, HandleBounds, HandlePosition, edge_position,
};
use crate::runtime::measurement::{LayoutEdgePosition, LayoutFactsQueryResult};
use crate::runtime::utils::get_node_rect;
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, PortDirection};

use super::backend::NodeGraphQuerySnapshot;
use super::rendering::resolve_rendering_read_model;

pub(crate) fn resolve_layout_facts_read_model(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    viewport_size: CanvasSize,
) -> LayoutFactsQueryResult {
    let rendering = resolve_rendering_read_model(snapshot, viewport_size);
    let visible_edge_positions = rendering
        .visible_edge_render_order
        .iter()
        .copied()
        .filter_map(|edge| {
            edge_position_from_layout_facts(snapshot, edge)
                .map(|position| LayoutEdgePosition::new(edge, position))
        })
        .collect();
    let connection_target_candidates = connection_target_candidates_from_layout_facts(snapshot);

    LayoutFactsQueryResult::new(
        snapshot.layout_facts_revision,
        rendering,
        visible_edge_positions,
        connection_target_candidates,
    )
}

pub(crate) fn connection_target_candidates_from_layout_facts(
    snapshot: &NodeGraphQuerySnapshot<'_>,
) -> Vec<ConnectionTargetCandidate> {
    let mut candidates = Vec::new();

    for (node_id, node) in snapshot.graph.nodes() {
        if node.hidden {
            continue;
        }
        let Some(entry) = snapshot.lookups.node_lookup.get(node_id) else {
            continue;
        };
        let Some(node_rect) =
            get_node_rect(snapshot.lookups, *node_id, snapshot.node_origin(), None)
        else {
            continue;
        };

        for measured in &entry.measured_handles {
            let Some(port) = snapshot.graph.ports().get(&measured.handle.port) else {
                continue;
            };
            if port.node != *node_id || measured.handle.node != *node_id {
                continue;
            }
            let policy = snapshot.interaction.port_interaction_policy(node, port);
            candidates.push(ConnectionTargetCandidate::new(
                ConnectionTargetHandle::new(
                    measured.handle,
                    policy.connectable,
                    policy.can_accept_connection(),
                ),
                node_rect,
                measured.bounds,
            ));
        }
    }

    candidates
}

pub(crate) fn resolve_connection_target_from_layout_facts(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    pointer: CanvasPoint,
    from: ConnectionHandleRef,
) -> ResolvedConnectionTarget {
    let connection = snapshot.interaction.connection_interaction();
    let candidates = connection_target_candidates_from_layout_facts(snapshot);
    resolve_connection_target_from_handles(ConnectionTargetFromHandlesInput::new(
        pointer,
        connection.connection_radius,
        from,
        &candidates,
        connection.connection_mode,
    ))
}

pub(crate) fn edge_position_from_layout_facts(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    edge: EdgeId,
) -> Option<EdgePosition> {
    let edge = snapshot.graph.edges().get(&edge)?;
    let from_port = snapshot.graph.ports().get(&edge.from)?;
    let to_port = snapshot.graph.ports().get(&edge.to)?;
    let source_node = snapshot.graph.nodes().get(&from_port.node)?;
    let target_node = snapshot.graph.nodes().get(&to_port.node)?;
    if source_node.hidden || target_node.hidden {
        return None;
    }

    let source_rect = get_node_rect(
        snapshot.lookups,
        from_port.node,
        snapshot.node_origin(),
        None,
    )?;
    let target_rect = get_node_rect(snapshot.lookups, to_port.node, snapshot.node_origin(), None)?;

    edge_position(
        EdgeEndpointInput {
            node_rect: source_rect,
            handle: measured_handle_bounds(
                snapshot,
                ConnectionHandleRef::new(from_port.node, edge.from, from_port.dir),
            ),
            fallback_position: fallback_handle_position(from_port.dir),
        },
        EdgeEndpointInput {
            node_rect: target_rect,
            handle: measured_handle_bounds(
                snapshot,
                ConnectionHandleRef::new(to_port.node, edge.to, to_port.dir),
            ),
            fallback_position: fallback_handle_position(to_port.dir),
        },
    )
}

fn measured_handle_bounds(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    handle: ConnectionHandleRef,
) -> Option<HandleBounds> {
    snapshot
        .lookups
        .node_lookup
        .get(&handle.node)?
        .measured_handles
        .iter()
        .find(|measured| measured.handle == handle)
        .map(|measured| measured.bounds)
}

fn fallback_handle_position(direction: PortDirection) -> HandlePosition {
    HandlePosition::fallback_for_direction(direction)
}
