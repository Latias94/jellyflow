use std::collections::{BTreeMap, BTreeSet, VecDeque};

use jellyflow_core::{CanvasPoint, CanvasSize, EdgeId, Graph, NodeId};

use crate::engine::{
    LayoutContext, LayoutCoordinateFrame, LayoutEdgeRoute, LayoutError, LayoutNodePosition,
    LayoutPlacements, LayoutRequest, LayoutResult, PreparedLayoutEdge, PreparedLayoutInput,
    PreparedLayoutNode, checked_canvas_point, node_rect_from_position, normalized_node_origin,
    position_from_center, resolve_node_size, translate_point, union_bounds, validate_request,
};

pub(crate) struct PreparedLayoutRun<'graph> {
    input: PreparedLayoutInput<'graph>,
}

impl<'graph> PreparedLayoutRun<'graph> {
    pub(crate) fn prepare(
        graph: &'graph Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<Self, LayoutError> {
        validate_request(graph, request)?;

        let mut nodes = Vec::new();
        let mut node_indices = BTreeMap::new();
        let mut visible_nodes = BTreeSet::new();

        for (id, node) in graph.nodes() {
            if node.hidden || !request.scope.contains(*id) {
                continue;
            }

            let size = resolve_node_size(graph, request, context, *id)?;
            let origin = normalized_node_origin(node.origin);
            let current_center = center_from_position(node.pos, size, origin)
                .ok_or(LayoutError::InvalidResultNodeGeometry(*id))?;
            let index = nodes.len();
            nodes.push(PreparedLayoutNode {
                id: *id,
                node,
                size,
                origin,
                current_center,
                pinned: context.pinned_nodes.contains(id),
            });
            node_indices.insert(*id, index);
            visible_nodes.insert(*id);
        }

        let (outgoing, visible_edges) = build_visible_edge_projection(graph, &visible_nodes)?;
        let edges = visible_edges
            .into_iter()
            .map(|edge| PreparedLayoutEdge {
                id: edge.id,
                source: edge.source,
                target: edge.target,
            })
            .collect();

        Ok(Self {
            input: PreparedLayoutInput {
                options: request.options,
                nodes,
                node_indices,
                edges,
                outgoing,
            },
        })
    }

    pub(crate) fn input(&self) -> &PreparedLayoutInput<'graph> {
        &self.input
    }

    pub(crate) fn finish(self, placements: LayoutPlacements) -> Result<LayoutResult, LayoutError> {
        let mut centers = BTreeMap::new();
        for placement in placements.nodes {
            if self.input.node(placement.node).is_none() {
                return Err(LayoutError::UnexpectedResultNode(placement.node));
            }
            if !placement.center.is_finite() {
                return Err(LayoutError::InvalidResultNodeGeometry(placement.node));
            }
            if centers.insert(placement.node, placement.center).is_some() {
                return Err(LayoutError::DuplicateResultNode(placement.node));
            }
        }

        for node in self.input.nodes() {
            if !centers.contains_key(&node.id()) {
                return Err(LayoutError::MissingNodePosition(node.id()));
            }
        }

        let expected_edges = self
            .input
            .edges()
            .iter()
            .map(PreparedLayoutEdge::id)
            .collect::<BTreeSet<_>>();
        let mut routes = BTreeMap::new();
        for route in placements.edge_routes {
            if !expected_edges.contains(&route.edge)
                || route.points.len() < 2
                || !route.points.iter().all(|point| point.is_finite())
            {
                return Err(LayoutError::InvalidResultEdgeRoute(route.edge));
            }
            let edge = route.edge;
            if routes.insert(edge, route).is_some() {
                return Err(LayoutError::DuplicateResultEdgeRoute(edge));
            }
        }

        let mut nodes = BTreeMap::new();
        let mut bounds = None;
        for node in self.input.nodes() {
            let center = centers[&node.id()];
            let pos = position_from_center(center, node.size(), node.origin())
                .ok_or(LayoutError::InvalidResultNodeGeometry(node.id()))?;
            let position = LayoutNodePosition {
                node: node.id(),
                pos,
                center,
                size: node.size(),
            };
            let node_bounds = node_rect_from_position(&position)
                .ok_or(LayoutError::InvalidResultNodeGeometry(node.id()))?;
            bounds =
                Some(union_bounds(bounds, node_bounds).ok_or(LayoutError::InvalidResultBounds)?);
            nodes.insert(node.id(), position);
        }

        let shift = match (placements.coordinate_frame, bounds) {
            (LayoutCoordinateFrame::NormalizeToMargin, Some(bounds)) => checked_canvas_point(
                f64::from(self.input.options.margin.width) - f64::from(bounds.origin.x),
                f64::from(self.input.options.margin.height) - f64::from(bounds.origin.y),
            )
            .ok_or(LayoutError::InvalidResultBounds)?,
            _ => CanvasPoint::default(),
        };

        if shift.x != 0.0 || shift.y != 0.0 {
            for position in nodes.values_mut() {
                position.pos = translate_point(position.pos, shift)
                    .ok_or(LayoutError::InvalidResultNodeGeometry(position.node))?;
                position.center = translate_point(position.center, shift)
                    .ok_or(LayoutError::InvalidResultNodeGeometry(position.node))?;
            }
            for route in routes.values_mut() {
                for point in &mut route.points {
                    *point = translate_point(*point, shift)
                        .ok_or(LayoutError::InvalidResultEdgeRoute(route.edge))?;
                }
            }
        }

        if (shift.x != 0.0 || shift.y != 0.0)
            && let Some(current) = bounds.as_mut()
        {
            current.origin =
                translate_point(current.origin, shift).ok_or(LayoutError::InvalidResultBounds)?;
        }

        Ok(LayoutResult {
            nodes: self
                .input
                .nodes()
                .filter_map(|node| nodes.get(&node.id()).copied())
                .collect(),
            edge_routes: self
                .input
                .edges()
                .iter()
                .filter_map(|edge| routes.remove(&edge.id()))
                .collect(),
            bounds,
        })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct VisibleLayoutEdge {
    pub(crate) id: EdgeId,
    pub(crate) source: NodeId,
    pub(crate) target: NodeId,
}

pub(crate) type VisibleEdgeProjection = (BTreeMap<NodeId, Vec<NodeId>>, Vec<VisibleLayoutEdge>);

/// A deterministic, breadth-first spanning forest of the visible graph.
#[derive(Debug, Clone)]
pub(crate) struct SpanningForestComponent {
    pub(crate) root: NodeId,
    pub(crate) children: BTreeMap<NodeId, Vec<NodeId>>,
    pub(crate) depth: BTreeMap<NodeId, usize>,
}

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

pub(crate) fn build_spanning_forest(
    visible_nodes: &BTreeSet<NodeId>,
    outgoing: &BTreeMap<NodeId, Vec<NodeId>>,
) -> Vec<SpanningForestComponent> {
    let mut incoming = visible_nodes
        .iter()
        .copied()
        .map(|node| (node, 0usize))
        .collect::<BTreeMap<_, _>>();

    for (source, children) in outgoing {
        if !visible_nodes.contains(source) {
            continue;
        }

        for child in children {
            if let Some(count) = incoming.get_mut(child) {
                *count = count.saturating_add(1);
            }
        }
    }

    let mut roots = visible_nodes
        .iter()
        .copied()
        .filter(|node| incoming.get(node).copied() == Some(0))
        .collect::<Vec<_>>();
    let root_set = roots.iter().copied().collect::<BTreeSet<_>>();
    roots.extend(
        visible_nodes
            .iter()
            .copied()
            .filter(|node| !root_set.contains(node)),
    );

    let mut visited = BTreeSet::new();
    let mut components = Vec::new();

    for root in roots {
        if !visited.insert(root) {
            continue;
        }

        let mut children: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
        let mut depth: BTreeMap<NodeId, usize> = BTreeMap::new();
        let mut queue = VecDeque::new();

        queue.push_back(root);
        depth.insert(root, 0);

        while let Some(node) = queue.pop_front() {
            let child_depth = depth.get(&node).copied().unwrap_or(0) + 1;
            let Some(next_nodes) = outgoing.get(&node) else {
                continue;
            };

            for child in next_nodes {
                if !visible_nodes.contains(child) || !visited.insert(*child) {
                    continue;
                }

                children.entry(node).or_default().push(*child);
                depth.insert(*child, child_depth);
                queue.push_back(*child);
            }
        }

        components.push(SpanningForestComponent {
            root,
            children,
            depth,
        });
    }

    components
}

pub(crate) fn center_from_position(
    pos: CanvasPoint,
    size: CanvasSize,
    origin: (f32, f32),
) -> Option<CanvasPoint> {
    checked_canvas_point(
        f64::from(pos.x) + f64::from(size.width) * (0.5 - f64::from(origin.0)),
        f64::from(pos.y) + f64::from(size.height) * (0.5 - f64::from(origin.1)),
    )
}

pub(crate) fn placements_from_centers(
    centers: BTreeMap<NodeId, CanvasPoint>,
    visible_edges: &[PreparedLayoutEdge],
    coordinate_frame: LayoutCoordinateFrame,
) -> Result<LayoutPlacements, LayoutError> {
    let edge_routes = visible_edges
        .iter()
        .map(|edge| {
            let source = centers
                .get(&edge.source())
                .copied()
                .ok_or(LayoutError::MissingNodePosition(edge.source()))?;
            let target = centers
                .get(&edge.target())
                .copied()
                .ok_or(LayoutError::MissingNodePosition(edge.target()))?;
            Ok(LayoutEdgeRoute {
                edge: edge.id(),
                points: vec![source, target],
            })
        })
        .collect::<Result<Vec<_>, LayoutError>>()?;

    Ok(LayoutPlacements {
        nodes: centers
            .into_iter()
            .map(|(node, center)| crate::engine::LayoutNodePlacement { node, center })
            .collect(),
        edge_routes,
        coordinate_frame,
    })
}
