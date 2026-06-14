use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, NodeId};

use crate::engine::{
    LayoutEdgeRoute, LayoutError, LayoutNodePosition, LayoutOptions, LayoutResult,
    node_rect_from_position, union_bounds,
};

#[derive(Debug, Clone)]
pub(crate) struct VisibleLayoutEdge {
    pub(crate) id: EdgeId,
    pub(crate) source: NodeId,
    pub(crate) target: NodeId,
}

pub(crate) type VisibleEdgeProjection = (BTreeMap<NodeId, Vec<NodeId>>, Vec<VisibleLayoutEdge>);

pub(crate) fn build_visible_edge_projection(
    graph: &Graph,
    visible_nodes: &BTreeSet<NodeId>,
) -> Result<VisibleEdgeProjection, LayoutError> {
    let mut outgoing: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    let mut visible_edges = Vec::new();

    for (edge_id, edge) in graph.edges() {
        if edge.hidden {
            continue;
        }

        let source_port = graph
            .ports()
            .get(&edge.from)
            .ok_or(LayoutError::MissingSourcePort(*edge_id))?;
        let target_port = graph
            .ports()
            .get(&edge.to)
            .ok_or(LayoutError::MissingTargetPort(*edge_id))?;
        if !graph.nodes().contains_key(&source_port.node) {
            return Err(LayoutError::MissingSourceNode { edge: *edge_id });
        }
        if !graph.nodes().contains_key(&target_port.node) {
            return Err(LayoutError::MissingTargetNode { edge: *edge_id });
        }
        if !visible_nodes.contains(&source_port.node) || !visible_nodes.contains(&target_port.node)
        {
            continue;
        }

        outgoing
            .entry(source_port.node)
            .or_default()
            .push(target_port.node);
        visible_edges.push(VisibleLayoutEdge {
            id: *edge_id,
            source: source_port.node,
            target: target_port.node,
        });
    }

    for children in outgoing.values_mut() {
        children.sort();
        children.dedup();
    }

    Ok((outgoing, visible_edges))
}

pub(crate) fn build_visible_edge_list(
    graph: &Graph,
    visible_nodes: &BTreeSet<NodeId>,
) -> Result<Vec<VisibleLayoutEdge>, LayoutError> {
    build_visible_edge_projection(graph, visible_nodes).map(|(_, edges)| edges)
}

pub(crate) fn center_from_position(
    pos: CanvasPoint,
    size: CanvasSize,
    origin: (f32, f32),
) -> CanvasPoint {
    CanvasPoint {
        x: pos.x + size.width * (0.5 - origin.0),
        y: pos.y + size.height * (0.5 - origin.1),
    }
}

pub(crate) fn result_from_placements(
    graph: &Graph,
    options: LayoutOptions,
    placements: &mut BTreeMap<NodeId, LayoutNodePosition>,
    visible_edges: &[VisibleLayoutEdge],
) -> LayoutResult {
    if placements.is_empty() {
        return LayoutResult {
            nodes: Vec::new(),
            edge_routes: Vec::new(),
            bounds: None,
        };
    }

    let mut bounds = None;
    for node in placements.values() {
        bounds = union_bounds(bounds, node_rect_from_position(node));
    }

    let shift = bounds.map_or(
        CanvasPoint {
            x: options.margin.width,
            y: options.margin.height,
        },
        |bounds| CanvasPoint {
            x: options.margin.width - bounds.origin.x,
            y: options.margin.height - bounds.origin.y,
        },
    );

    if shift.x != 0.0 || shift.y != 0.0 {
        for node in placements.values_mut() {
            node.pos.x += shift.x;
            node.pos.y += shift.y;
            node.center.x += shift.x;
            node.center.y += shift.y;
        }
        bounds = bounds.map(|bounds| CanvasRect {
            origin: CanvasPoint {
                x: bounds.origin.x + shift.x,
                y: bounds.origin.y + shift.y,
            },
            size: bounds.size,
        });
    }

    let nodes = graph
        .nodes()
        .keys()
        .filter_map(|node| placements.get(node).copied())
        .collect::<Vec<_>>();

    let edge_routes = visible_edges
        .iter()
        .filter_map(|edge| {
            let source = placements.get(&edge.source)?;
            let target = placements.get(&edge.target)?;
            Some(LayoutEdgeRoute {
                edge: edge.id,
                points: vec![source.center, target.center],
            })
        })
        .collect::<Vec<_>>();

    LayoutResult {
        nodes,
        edge_routes,
        bounds,
    }
}
