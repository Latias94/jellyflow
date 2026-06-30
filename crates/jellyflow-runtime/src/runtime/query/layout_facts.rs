use crate::runtime::connection::{
    ConnectionHandleRef, ConnectionTargetCandidate, ConnectionTargetFromHandlesInput,
    ConnectionTargetHandle, ResolvedConnectionTarget, resolve_connection_target_from_handles,
};
use crate::runtime::geometry::{
    EdgeEndpointInput, EdgeInteractionFacts, EdgePosition, HandlePosition, edge_position,
    resolve_edge_route_path,
};
use crate::runtime::measurement::{
    LayoutEdgePosition, LayoutEdgeRouteFacts, LayoutFactsQueryResult, LayoutNodeMeasurementStatus,
    NodeMeasurementStatus, resolve_handle_measurement,
};
use crate::runtime::utils::get_node_rect;
use jellyflow_core::core::{CanvasPoint, CanvasSize, EdgeId, PortDirection};

use super::backend::NodeGraphQuerySnapshot;
use super::rendering::resolve_rendering_read_model;

pub(crate) fn resolve_layout_facts_read_model(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    viewport_size: CanvasSize,
) -> LayoutFactsQueryResult {
    let rendering = resolve_rendering_read_model(snapshot, viewport_size);
    let visible_edge_positions: Vec<LayoutEdgePosition> = rendering
        .visible_edge_render_order
        .iter()
        .copied()
        .filter_map(|edge| {
            edge_position_from_layout_facts(snapshot, edge)
                .map(|position| LayoutEdgePosition::new(edge, position))
        })
        .collect();
    let visible_edge_route_facts =
        visible_edge_route_facts_from_layout_facts(snapshot, &visible_edge_positions);
    let connection_target_candidates = connection_target_candidates_from_layout_facts(snapshot);

    let node_measurement_statuses = rendering
        .visible_node_ids
        .iter()
        .copied()
        .map(|node| {
            LayoutNodeMeasurementStatus::new(
                node,
                snapshot
                    .lookups
                    .node_lookup
                    .get(&node)
                    .map(|entry| entry.measurement_status())
                    .unwrap_or(NodeMeasurementStatus::Missing),
            )
        })
        .collect::<Vec<_>>();

    LayoutFactsQueryResult::new(
        snapshot.layout_facts_revision,
        rendering,
        visible_edge_positions,
        connection_target_candidates,
    )
    .with_node_measurement_statuses(node_measurement_statuses)
    .with_edge_route_facts(visible_edge_route_facts)
}

pub(crate) fn visible_edge_route_facts_from_layout_facts(
    snapshot: &NodeGraphQuerySnapshot<'_>,
    edge_positions: &[LayoutEdgePosition],
) -> Vec<LayoutEdgeRouteFacts> {
    edge_positions
        .iter()
        .filter_map(|edge_position| {
            let edge = snapshot.graph.edges().get(&edge_position.edge)?;
            let policy = snapshot.interaction.edge_interaction_policy(edge);
            resolve_edge_route_path(edge, edge_position.position)
                .map(|facts| {
                    facts
                        .with_hit_test(edge_route_hit_test_options(
                            snapshot.interaction.edge_hit_test_options_for(edge),
                            edge.view.hit_target_width,
                        ))
                        .with_interaction(EdgeInteractionFacts {
                            selectable: policy.selectable,
                            selected: snapshot
                                .view_state
                                .selected_edges
                                .contains(&edge_position.edge),
                            focusable: policy.focusable,
                            deletable: policy.deletable,
                            reconnect_source: policy.reconnect_source,
                            reconnect_target: policy.reconnect_target,
                        })
                })
                .map(|facts| LayoutEdgeRouteFacts::new(edge_position.edge, facts))
        })
        .collect()
}

fn edge_route_hit_test_options(
    base: crate::runtime::geometry::EdgeHitTestOptions,
    hit_target_width: Option<f32>,
) -> crate::runtime::geometry::EdgeHitTestOptions {
    hit_target_width
        .filter(|width| width.is_finite() && *width > 0.0)
        .map(
            |interaction_width| crate::runtime::geometry::EdgeHitTestOptions {
                interaction_width,
                ..base
            },
        )
        .unwrap_or(base)
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

        for port_id in &entry.ports {
            let Some(port) = snapshot.graph.ports().get(port_id) else {
                continue;
            };
            let handle = ConnectionHandleRef::new(*node_id, *port_id, port.dir);
            let Some(bounds) =
                resolve_handle_measurement(snapshot.graph, snapshot.lookups, handle).bounds
            else {
                continue;
            };
            let policy = snapshot.interaction.port_interaction_policy(node, port);
            candidates.push(ConnectionTargetCandidate::new(
                ConnectionTargetHandle::new(
                    handle,
                    policy.connectable,
                    policy.can_accept_connection(),
                ),
                node_rect,
                bounds,
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
            handle: resolve_handle_measurement(
                snapshot.graph,
                snapshot.lookups,
                ConnectionHandleRef::new(from_port.node, edge.from, from_port.dir),
            )
            .bounds,
            fallback_position: fallback_handle_position(from_port.dir),
        },
        EdgeEndpointInput {
            node_rect: target_rect,
            handle: resolve_handle_measurement(
                snapshot.graph,
                snapshot.lookups,
                ConnectionHandleRef::new(to_port.node, edge.to, to_port.dir),
            )
            .bounds,
            fallback_position: fallback_handle_position(to_port.dir),
        },
    )
}

fn fallback_handle_position(direction: PortDirection) -> HandlePosition {
    HandlePosition::fallback_for_direction(direction)
}
