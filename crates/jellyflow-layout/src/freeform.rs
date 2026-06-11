use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphTransaction, NodeId,
};

use crate::engine::{
    LayoutContext, LayoutDirection, LayoutEdgeRoute, LayoutEngine, LayoutEngineId, LayoutError,
    LayoutNodePosition, LayoutOptions, LayoutRequest, LayoutResult,
    MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID, node_rect_from_position, position_from_center,
    resolve_node_origin, resolve_node_size, union_bounds, validate_request,
};

/// Built-in freeform mind-map engine that only resolves overlaps.
#[derive(Debug, Default, Clone, Copy)]
pub struct MindMapFreeformLayoutEngine;

impl LayoutEngine for MindMapFreeformLayoutEngine {
    fn id(&self) -> LayoutEngineId {
        LayoutEngineId::new(MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID)
    }

    fn layout(
        &self,
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        layout_graph_with_mind_map_freeform_context(graph, request, context)
    }
}

/// Runs the native freeform mind-map engine.
pub fn layout_graph_with_mind_map_freeform(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    layout_graph_with_mind_map_freeform_context(graph, request, &LayoutContext::default())
}

/// Runs the native freeform mind-map engine and converts the result into a transaction.
pub fn layout_graph_to_transaction_with_mind_map_freeform(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<GraphTransaction, LayoutError> {
    layout_graph_with_mind_map_freeform(graph, request)?.to_transaction(graph)
}

fn layout_graph_with_mind_map_freeform_context(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
) -> Result<LayoutResult, LayoutError> {
    validate_request(graph, request)?;

    let mut projection = FreeformProjection::new(graph, request, context)?;
    projection.layout(request.options);
    projection.into_result()
}

struct FreeformProjection<'a> {
    graph: &'a Graph,
    request: &'a LayoutRequest,
    node_infos: BTreeMap<NodeId, FreeformNodeInfo>,
    visible_edges: Vec<FreeformEdge>,
    placements: BTreeMap<NodeId, LayoutNodePosition>,
}

#[derive(Debug, Clone, Copy)]
struct FreeformNodeInfo {
    size: CanvasSize,
    origin: (f32, f32),
    current_center: CanvasPoint,
    pinned: bool,
}

#[derive(Debug, Clone)]
struct FreeformEdge {
    id: EdgeId,
    source: NodeId,
    target: NodeId,
}

impl<'a> FreeformProjection<'a> {
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

            node_infos.insert(
                *id,
                FreeformNodeInfo {
                    size,
                    origin,
                    current_center,
                    pinned: context.pinned_nodes.contains(id),
                },
            );
            visible_nodes.insert(*id);
        }

        let visible_edges = build_visible_edges(graph, &visible_nodes)?;

        Ok(Self {
            graph,
            request,
            node_infos,
            visible_edges,
            placements: BTreeMap::new(),
        })
    }

    fn layout(&mut self, options: LayoutOptions) {
        if self.node_infos.is_empty() {
            return;
        }

        let gap = freeform_gap(options);
        let mut node_order = self.node_infos.keys().copied().collect::<Vec<_>>();
        node_order.sort_by(|left, right| self.compare_nodes(*left, *right, options.direction));

        for node in node_order {
            let Some(info) = self.node_infos.get(&node).copied() else {
                continue;
            };

            let center = if info.pinned {
                info.current_center
            } else {
                self.resolve_node_center(info, gap, options.direction)
            };
            let pos = position_from_center(center, info.size, info.origin);

            self.placements.insert(
                node,
                LayoutNodePosition {
                    node,
                    pos,
                    center,
                    size: info.size,
                },
            );
        }
    }

    fn compare_nodes(&self, left: NodeId, right: NodeId, direction: LayoutDirection) -> Ordering {
        let left_info = self.node_infos.get(&left).expect("left node info");
        let right_info = self.node_infos.get(&right).expect("right node info");

        right_info
            .pinned
            .cmp(&left_info.pinned)
            .then_with(|| match direction {
                LayoutDirection::TopToBottom | LayoutDirection::BottomToTop => left_info
                    .current_center
                    .y
                    .total_cmp(&right_info.current_center.y)
                    .then_with(|| {
                        left_info
                            .current_center
                            .x
                            .total_cmp(&right_info.current_center.x)
                    }),
                LayoutDirection::LeftToRight | LayoutDirection::RightToLeft => left_info
                    .current_center
                    .x
                    .total_cmp(&right_info.current_center.x)
                    .then_with(|| {
                        left_info
                            .current_center
                            .y
                            .total_cmp(&right_info.current_center.y)
                    }),
            })
            .then_with(|| left.cmp(&right))
    }

    fn resolve_node_center(
        &self,
        info: FreeformNodeInfo,
        gap: f32,
        direction: LayoutDirection,
    ) -> CanvasPoint {
        let mut center = info.current_center;

        for _ in 0..64 {
            let rect = rect_from_center(center, info.size, info.origin);
            let Some(other) = self
                .placements
                .values()
                .map(node_rect_from_position)
                .find(|other| rects_overlap_with_gap(rect, *other, gap))
            else {
                break;
            };

            center =
                shift_center_out_of_rect(center, info.size, info.origin, other, direction, gap);
        }

        center
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
) -> Result<Vec<FreeformEdge>, LayoutError> {
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

        visible_edges.push(FreeformEdge {
            id: *edge_id,
            source: source_port.node,
            target: target_port.node,
        });
    }

    Ok(visible_edges)
}

fn freeform_gap(options: LayoutOptions) -> f32 {
    options.spacing.nodesep.max(24.0)
}

fn center_from_position(pos: CanvasPoint, size: CanvasSize, origin: (f32, f32)) -> CanvasPoint {
    CanvasPoint {
        x: pos.x + size.width * (0.5 - origin.0),
        y: pos.y + size.height * (0.5 - origin.1),
    }
}

fn rect_from_center(center: CanvasPoint, size: CanvasSize, origin: (f32, f32)) -> CanvasRect {
    CanvasRect {
        origin: position_from_center(center, size, origin),
        size,
    }
}

fn shift_center_out_of_rect(
    center: CanvasPoint,
    size: CanvasSize,
    origin: (f32, f32),
    other: CanvasRect,
    direction: LayoutDirection,
    gap: f32,
) -> CanvasPoint {
    let rect = rect_from_center(center, size, origin);
    let expanded = expand_rect(other, gap);

    match direction {
        LayoutDirection::TopToBottom => CanvasPoint {
            x: center.x,
            y: center.y + (expanded.origin.y + expanded.size.height - rect.origin.y),
        },
        LayoutDirection::BottomToTop => CanvasPoint {
            x: center.x,
            y: center.y - (rect.origin.y + rect.size.height - expanded.origin.y),
        },
        LayoutDirection::LeftToRight => CanvasPoint {
            x: center.x + (expanded.origin.x + expanded.size.width - rect.origin.x),
            y: center.y,
        },
        LayoutDirection::RightToLeft => CanvasPoint {
            x: center.x - (rect.origin.x + rect.size.width - expanded.origin.x),
            y: center.y,
        },
    }
}

fn rects_overlap_with_gap(candidate: CanvasRect, other: CanvasRect, gap: f32) -> bool {
    let other = expand_rect(other, gap);
    let candidate_right = candidate.origin.x + candidate.size.width;
    let candidate_bottom = candidate.origin.y + candidate.size.height;
    let other_right = other.origin.x + other.size.width;
    let other_bottom = other.origin.y + other.size.height;

    candidate.origin.x < other_right
        && candidate_right > other.origin.x
        && candidate.origin.y < other_bottom
        && candidate_bottom > other.origin.y
}

fn expand_rect(rect: CanvasRect, gap: f32) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: rect.origin.x - gap,
            y: rect.origin.y - gap,
        },
        size: CanvasSize {
            width: rect.size.width + gap * 2.0,
            height: rect.size.height + gap * 2.0,
        },
    }
}
