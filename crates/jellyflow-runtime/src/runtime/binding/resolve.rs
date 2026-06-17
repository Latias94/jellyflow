use jellyflow_core::core::{
    BindingEndpoint, CanvasPoint, CanvasRect, Graph, GraphLocalBindingTarget, NodeId,
    PortDirection, PortId,
};

use crate::runtime::connection::ConnectionHandleRef;
use crate::runtime::geometry::{
    EdgeEndpointInput, HandleBounds, HandlePosition, edge_position, handle_anchor_position,
};
use crate::runtime::lookups::NodeGraphLookups;
use crate::runtime::utils::get_node_rect;

use super::query::{
    BindingEndpointResolution, BindingQueryOptions, BindingQueryResult, ResolvedBinding,
    ResolvedBindingEndpoint,
};

/// Resolves binding endpoints using graph facts, lookup measurements, and runtime node origin.
pub fn resolve_binding_query(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    revision: u64,
    node_origin: (f32, f32),
    options: BindingQueryOptions,
) -> BindingQueryResult {
    let bindings = graph
        .bindings()
        .iter()
        .map(|(id, binding)| {
            ResolvedBinding::new(
                *id,
                resolve_endpoint(graph, lookups, node_origin, options, &binding.subject),
                resolve_endpoint(graph, lookups, node_origin, options, &binding.target),
                binding.kind.clone(),
            )
        })
        .collect();

    BindingQueryResult::new(revision, bindings)
}

fn resolve_endpoint(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    node_origin: (f32, f32),
    options: BindingQueryOptions,
    endpoint: &BindingEndpoint,
) -> ResolvedBindingEndpoint {
    match endpoint {
        BindingEndpoint::Source { .. } => ResolvedBindingEndpoint::source(endpoint.clone()),
        BindingEndpoint::GraphLocal { target } => resolve_graph_local_target(
            graph,
            lookups,
            node_origin,
            options,
            endpoint.clone(),
            *target,
        ),
    }
}

fn resolve_graph_local_target(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    node_origin: (f32, f32),
    options: BindingQueryOptions,
    endpoint: BindingEndpoint,
    target: GraphLocalBindingTarget,
) -> ResolvedBindingEndpoint {
    let resolution = match target {
        GraphLocalBindingTarget::Graph => BindingEndpointResolution::Graph,
        GraphLocalBindingTarget::Node { id } => {
            resolve_node_target(graph, lookups, id, node_origin, options)
        }
        GraphLocalBindingTarget::Port { id } => {
            resolve_port_target(graph, lookups, id, node_origin, options)
        }
        GraphLocalBindingTarget::Edge { id } => {
            resolve_edge_target(graph, lookups, id, node_origin, options)
        }
        GraphLocalBindingTarget::Group { id } => graph
            .groups()
            .get(&id)
            .map(|group| BindingEndpointResolution::GroupRect {
                group: id,
                rect: group.rect,
                center: rect_center(group.rect),
            })
            .unwrap_or(BindingEndpointResolution::Unresolved),
        GraphLocalBindingTarget::StickyNote { id } => graph
            .sticky_notes()
            .get(&id)
            .map(|note| BindingEndpointResolution::StickyNoteRect {
                note: id,
                rect: note.rect,
                center: rect_center(note.rect),
            })
            .unwrap_or(BindingEndpointResolution::Unresolved),
    };

    ResolvedBindingEndpoint::new(endpoint, resolution)
}

fn resolve_node_target(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    node: NodeId,
    node_origin: (f32, f32),
    options: BindingQueryOptions,
) -> BindingEndpointResolution {
    let Some(model) = graph.nodes().get(&node) else {
        return BindingEndpointResolution::Unresolved;
    };
    if model.hidden && !options.include_hidden {
        return BindingEndpointResolution::Hidden;
    }
    let Some(rect) = get_node_rect(lookups, node, node_origin, options.fallback_node_size) else {
        return BindingEndpointResolution::Unresolved;
    };
    BindingEndpointResolution::NodeRect {
        node,
        rect,
        center: rect_center(rect),
    }
}

fn resolve_port_target(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    port: PortId,
    node_origin: (f32, f32),
    options: BindingQueryOptions,
) -> BindingEndpointResolution {
    let Some(model) = graph.ports().get(&port) else {
        return BindingEndpointResolution::Unresolved;
    };
    let Some(node) = graph.nodes().get(&model.node) else {
        return BindingEndpointResolution::Unresolved;
    };
    if node.hidden && !options.include_hidden {
        return BindingEndpointResolution::Hidden;
    }
    let Some(node_rect) =
        get_node_rect(lookups, model.node, node_origin, options.fallback_node_size)
    else {
        return BindingEndpointResolution::Unresolved;
    };
    let Some(point) = handle_anchor_position(
        node_rect,
        measured_handle_bounds(
            lookups,
            ConnectionHandleRef::new(model.node, port, model.dir),
        ),
        fallback_handle_position(model.dir),
    )
    .map(|endpoint| endpoint.point) else {
        return BindingEndpointResolution::Unresolved;
    };

    BindingEndpointResolution::PortAnchor {
        node: model.node,
        point,
    }
}

fn resolve_edge_target(
    graph: &Graph,
    lookups: &NodeGraphLookups,
    edge: jellyflow_core::core::EdgeId,
    node_origin: (f32, f32),
    options: BindingQueryOptions,
) -> BindingEndpointResolution {
    let Some(edge_model) = graph.edges().get(&edge) else {
        return BindingEndpointResolution::Unresolved;
    };
    if edge_model.hidden && !options.include_hidden {
        return BindingEndpointResolution::Hidden;
    }
    let Some(from_port) = graph.ports().get(&edge_model.from) else {
        return BindingEndpointResolution::Unresolved;
    };
    let Some(to_port) = graph.ports().get(&edge_model.to) else {
        return BindingEndpointResolution::Unresolved;
    };
    let Some(source_node) = graph.nodes().get(&from_port.node) else {
        return BindingEndpointResolution::Unresolved;
    };
    let Some(target_node) = graph.nodes().get(&to_port.node) else {
        return BindingEndpointResolution::Unresolved;
    };
    if (source_node.hidden || target_node.hidden) && !options.include_hidden {
        return BindingEndpointResolution::Hidden;
    }

    let Some(source_rect) = get_node_rect(
        lookups,
        from_port.node,
        node_origin,
        options.fallback_node_size,
    ) else {
        return BindingEndpointResolution::Unresolved;
    };
    let Some(target_rect) = get_node_rect(
        lookups,
        to_port.node,
        node_origin,
        options.fallback_node_size,
    ) else {
        return BindingEndpointResolution::Unresolved;
    };
    let Some(position) = edge_position(
        EdgeEndpointInput {
            node_rect: source_rect,
            handle: measured_handle_bounds(
                lookups,
                ConnectionHandleRef::new(from_port.node, edge_model.from, from_port.dir),
            ),
            fallback_position: fallback_handle_position(from_port.dir),
        },
        EdgeEndpointInput {
            node_rect: target_rect,
            handle: measured_handle_bounds(
                lookups,
                ConnectionHandleRef::new(to_port.node, edge_model.to, to_port.dir),
            ),
            fallback_position: fallback_handle_position(to_port.dir),
        },
    ) else {
        return BindingEndpointResolution::Unresolved;
    };

    BindingEndpointResolution::EdgePosition { edge, position }
}

fn measured_handle_bounds(
    lookups: &NodeGraphLookups,
    handle: ConnectionHandleRef,
) -> Option<HandleBounds> {
    lookups
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

fn rect_center(rect: CanvasRect) -> CanvasPoint {
    CanvasPoint {
        x: rect.origin.x + rect.size.width * 0.5,
        y: rect.origin.y + rect.size.height * 0.5,
    }
}
