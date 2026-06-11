use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::f32::consts::{PI, TAU};

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphTransaction, NodeId,
};

use crate::engine::{
    LayoutContext, LayoutDirection, LayoutEdgeRoute, LayoutEngine, LayoutEngineId, LayoutError,
    LayoutNodePosition, LayoutOptions, LayoutRequest, LayoutResult,
    MIND_MAP_RADIAL_LAYOUT_ENGINE_ID, node_rect_from_position, position_from_center,
    resolve_node_origin, resolve_node_size, union_bounds, validate_request,
};

/// Built-in radial mind-map engine.
#[derive(Debug, Default, Clone, Copy)]
pub struct MindMapRadialLayoutEngine;

impl LayoutEngine for MindMapRadialLayoutEngine {
    fn id(&self) -> LayoutEngineId {
        LayoutEngineId::new(MIND_MAP_RADIAL_LAYOUT_ENGINE_ID)
    }

    fn layout(
        &self,
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        layout_graph_with_mind_map_radial_context(graph, request, context)
    }
}

/// Runs the native radial mind-map engine.
pub fn layout_graph_with_mind_map_radial(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    layout_graph_with_mind_map_radial_context(graph, request, &LayoutContext::default())
}

/// Runs the native radial mind-map engine and converts the result into a transaction.
pub fn layout_graph_to_transaction_with_mind_map_radial(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<GraphTransaction, LayoutError> {
    layout_graph_with_mind_map_radial(graph, request)?.to_transaction(graph)
}

fn layout_graph_with_mind_map_radial_context(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
) -> Result<LayoutResult, LayoutError> {
    validate_request(graph, request)?;

    let mut projection = MindMapProjection::new(graph, request, context)?;
    projection.layout(request);
    projection.into_result()
}

struct MindMapProjection<'a> {
    graph: &'a Graph,
    request: &'a LayoutRequest,
    context: &'a LayoutContext,
    node_infos: BTreeMap<NodeId, MindMapNodeInfo>,
    visible_edges: Vec<MindMapEdge>,
    components: Vec<MindMapComponent>,
    placements: BTreeMap<NodeId, LayoutNodePosition>,
}

#[derive(Debug, Clone, Copy)]
struct MindMapNodeInfo {
    size: CanvasSize,
    origin: (f32, f32),
    current_center: CanvasPoint,
    arc_demand: f32,
    pinned: bool,
}

#[derive(Debug, Clone)]
struct MindMapEdge {
    id: EdgeId,
    source: NodeId,
    target: NodeId,
}

#[derive(Debug, Clone)]
struct MindMapComponent {
    root: NodeId,
    children: BTreeMap<NodeId, Vec<NodeId>>,
    demand: BTreeMap<NodeId, f32>,
    max_depth: usize,
}

struct MindMapLayoutNode<'a> {
    node: NodeId,
    center: CanvasPoint,
    start_angle: f32,
    end_angle: f32,
    ring_gap: f32,
    component: &'a MindMapComponent,
    depth: usize,
}

type VisibleEdgeProjection = (BTreeMap<NodeId, Vec<NodeId>>, Vec<MindMapEdge>);

impl<'a> MindMapProjection<'a> {
    fn new(
        graph: &'a Graph,
        request: &'a LayoutRequest,
        context: &'a LayoutContext,
    ) -> Result<Self, LayoutError> {
        let mut node_infos = BTreeMap::new();
        let mut visible_nodes = BTreeSet::new();

        for (id, node) in &graph.nodes {
            if node.hidden || !request.scope.contains(*id) {
                continue;
            }

            let size = resolve_node_size(graph, request, context, *id)?;
            let origin = resolve_node_origin(node.origin, request.options.node_origin, context);
            let current_center = center_from_position(node.pos, size, origin);
            let arc_demand = node_arc_demand(size, request.options);

            node_infos.insert(
                *id,
                MindMapNodeInfo {
                    size,
                    origin,
                    current_center,
                    arc_demand,
                    pinned: context.pinned_nodes.contains(id),
                },
            );
            visible_nodes.insert(*id);
        }

        let (outgoing, visible_edges) = build_visible_edges(graph, &visible_nodes)?;
        let components = build_components(&visible_nodes, &outgoing, &node_infos);

        Ok(Self {
            graph,
            request,
            context,
            node_infos,
            visible_edges,
            components,
            placements: BTreeMap::new(),
        })
    }

    fn layout(&mut self, request: &LayoutRequest) {
        if self.node_infos.is_empty() {
            return;
        }

        let ring_gap = compute_ring_gap(request, &self.node_infos);
        let component_radius =
            compute_component_radius(ring_gap, &self.components, &self.node_infos);
        let angle_offset = direction_angle_offset(request.options.direction);
        let components = self.components.clone();
        let component_count = components.len().max(1) as f32;

        for (index, component) in components.iter().enumerate() {
            let root_center = if self.context.pinned_nodes.contains(&component.root) {
                self.node_infos[&component.root].current_center
            } else if self.components.len() == 1 {
                CanvasPoint { x: 0.0, y: 0.0 }
            } else {
                polar_point(
                    CanvasPoint { x: 0.0, y: 0.0 },
                    component_radius,
                    angle_offset + TAU * (index as f32 / component_count),
                )
            };

            self.layout_node(MindMapLayoutNode {
                node: component.root,
                center: root_center,
                start_angle: angle_offset,
                end_angle: angle_offset + TAU,
                ring_gap,
                component,
                depth: 0,
            });
        }
    }

    fn layout_node(&mut self, frame: MindMapLayoutNode<'_>) {
        let MindMapLayoutNode {
            node,
            center,
            start_angle,
            end_angle,
            ring_gap,
            component,
            depth,
        } = frame;
        let Some(info) = self.node_infos.get(&node).copied() else {
            return;
        };
        let final_center = if info.pinned {
            info.current_center
        } else {
            center
        };
        let pos = position_from_center(final_center, info.size, info.origin);

        self.placements.insert(
            node,
            LayoutNodePosition {
                node,
                pos,
                center: final_center,
                size: info.size,
            },
        );

        let Some(children) = component.children.get(&node) else {
            return;
        };
        if children.is_empty() {
            return;
        }

        let total_demand = children
            .iter()
            .map(|child| component.demand.get(child).copied().unwrap_or(1.0))
            .sum::<f32>()
            .max(1.0);
        let sector_span = end_angle - start_angle;
        let mut cursor = start_angle;

        for child in children {
            let child_demand = component.demand.get(child).copied().unwrap_or(1.0);
            let child_span = sector_span * (child_demand / total_demand);
            let child_angle = cursor + child_span * 0.5;
            let child_center =
                polar_point(final_center, ring_gap * (depth as f32 + 1.0), child_angle);
            self.layout_node(MindMapLayoutNode {
                node: *child,
                center: child_center,
                start_angle: cursor,
                end_angle: cursor + child_span,
                ring_gap,
                component,
                depth: depth + 1,
            });
            cursor += child_span;
        }
    }

    fn into_result(mut self) -> Result<LayoutResult, LayoutError> {
        if self.placements.is_empty() {
            return Ok(LayoutResult {
                nodes: Vec::new(),
                edge_routes: Vec::new(),
                bounds: None,
            });
        }

        let mut bounds = None;
        for node in self.placements.values() {
            bounds = union_bounds(bounds, node_rect_from_position(node));
        }

        let shift = bounds.map_or(
            CanvasPoint {
                x: self.request.options.margin.width,
                y: self.request.options.margin.height,
            },
            |bounds| CanvasPoint {
                x: self.request.options.margin.width - bounds.origin.x,
                y: self.request.options.margin.height - bounds.origin.y,
            },
        );

        if shift.x != 0.0 || shift.y != 0.0 {
            for node in self.placements.values_mut() {
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

        let nodes = self
            .graph
            .nodes
            .keys()
            .filter_map(|node| self.placements.get(node).copied())
            .collect::<Vec<_>>();

        let edge_routes = self
            .visible_edges
            .iter()
            .filter_map(|edge| {
                let source = self.placements.get(&edge.source)?;
                let target = self.placements.get(&edge.target)?;
                Some(LayoutEdgeRoute {
                    edge: edge.id,
                    points: vec![source.center, target.center],
                })
            })
            .collect::<Vec<_>>();

        Ok(LayoutResult {
            nodes,
            edge_routes,
            bounds,
        })
    }
}

fn build_visible_edges(
    graph: &Graph,
    visible_nodes: &BTreeSet<NodeId>,
) -> Result<VisibleEdgeProjection, LayoutError> {
    let mut outgoing: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    let mut visible_edges = Vec::new();

    for (edge_id, edge) in &graph.edges {
        if edge.hidden {
            continue;
        }

        let source_port = graph
            .ports
            .get(&edge.from)
            .ok_or(LayoutError::MissingSourcePort(*edge_id))?;
        let target_port = graph
            .ports
            .get(&edge.to)
            .ok_or(LayoutError::MissingTargetPort(*edge_id))?;
        if !graph.nodes.contains_key(&source_port.node) {
            return Err(LayoutError::MissingSourceNode { edge: *edge_id });
        }
        if !graph.nodes.contains_key(&target_port.node) {
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
        visible_edges.push(MindMapEdge {
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

fn build_components(
    visible_nodes: &BTreeSet<NodeId>,
    outgoing: &BTreeMap<NodeId, Vec<NodeId>>,
    node_infos: &BTreeMap<NodeId, MindMapNodeInfo>,
) -> Vec<MindMapComponent> {
    let mut roots = visible_nodes
        .iter()
        .copied()
        .filter(|node| incoming_count(*node, outgoing) == 0)
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
            let next_nodes = outgoing.get(&node).cloned().unwrap_or_default();

            for child in next_nodes {
                if !visible_nodes.contains(&child) || !visited.insert(child) {
                    continue;
                }
                children.entry(node).or_default().push(child);
                depth.insert(child, child_depth);
                queue.push_back(child);
            }
        }

        let mut demand = BTreeMap::new();
        let max_depth = depth.values().copied().max().unwrap_or(0);
        compute_subtree_demand(root, &children, node_infos, &mut demand);

        components.push(MindMapComponent {
            root,
            children,
            demand,
            max_depth,
        });
    }

    components
}

fn compute_subtree_demand(
    node: NodeId,
    children: &BTreeMap<NodeId, Vec<NodeId>>,
    node_infos: &BTreeMap<NodeId, MindMapNodeInfo>,
    memo: &mut BTreeMap<NodeId, f32>,
) -> f32 {
    if let Some(value) = memo.get(&node) {
        return *value;
    }

    let mut total = node_infos
        .get(&node)
        .map(|info| info.arc_demand)
        .unwrap_or(1.0);
    if let Some(next) = children.get(&node) {
        for child in next {
            total += compute_subtree_demand(*child, children, node_infos, memo);
        }
    }

    memo.insert(node, total);
    total
}

fn incoming_count(node: NodeId, outgoing: &BTreeMap<NodeId, Vec<NodeId>>) -> usize {
    outgoing
        .values()
        .flat_map(|children| children.iter())
        .filter(|child| **child == node)
        .count()
}

fn compute_ring_gap(
    request: &LayoutRequest,
    node_infos: &BTreeMap<NodeId, MindMapNodeInfo>,
) -> f32 {
    let widest = node_infos
        .values()
        .map(|info| info.size.width.max(info.size.height))
        .fold(0.0, f32::max);
    let spacing = request.options.spacing;
    (widest * 1.25 + spacing.ranksep.max(spacing.nodesep.max(24.0))).max(80.0)
}

fn compute_component_radius(
    ring_gap: f32,
    components: &[MindMapComponent],
    node_infos: &BTreeMap<NodeId, MindMapNodeInfo>,
) -> f32 {
    if components.len() <= 1 {
        return 0.0;
    }

    let max_depth = components
        .iter()
        .map(|component| component.max_depth)
        .max()
        .unwrap_or(0);
    let widest = node_infos
        .values()
        .map(|info| info.size.width.max(info.size.height))
        .fold(0.0, f32::max);

    ring_gap * (max_depth as f32 + 2.0) * 2.0 + widest.max(ring_gap)
}

fn node_arc_demand(size: CanvasSize, options: LayoutOptions) -> f32 {
    let baseline = options.spacing.nodesep.max(24.0);
    let arc = size.width.max(size.height) + baseline;
    (arc / options.spacing.ranksep.max(1.0)).max(1.0)
}

fn direction_angle_offset(direction: LayoutDirection) -> f32 {
    match direction {
        LayoutDirection::TopToBottom => -PI / 2.0,
        LayoutDirection::BottomToTop => PI / 2.0,
        LayoutDirection::LeftToRight => 0.0,
        LayoutDirection::RightToLeft => PI,
    }
}

fn polar_point(center: CanvasPoint, radius: f32, angle: f32) -> CanvasPoint {
    CanvasPoint {
        x: center.x + radius * angle.cos(),
        y: center.y + radius * angle.sin(),
    }
}

fn center_from_position(pos: CanvasPoint, size: CanvasSize, origin: (f32, f32)) -> CanvasPoint {
    CanvasPoint {
        x: pos.x + size.width * (0.5 - origin.0),
        y: pos.y + size.height * (0.5 - origin.1),
    }
}
