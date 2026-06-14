use dugong::graphlib::{Graph as DugongGraph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir};
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphTransaction, NodeId,
};

use crate::engine::{
    DUGONG_LAYOUT_ENGINE_ID, LayoutContext, LayoutDirection, LayoutEdgeRoute, LayoutEngine,
    LayoutEngineId, LayoutError, LayoutNodePosition, LayoutOptions, LayoutRequest, LayoutResult,
    node_rect_from_position, position_from_center, resolve_node_origin, resolve_node_size,
    union_bounds, validate_request,
};

impl LayoutDirection {
    fn as_dugong_rankdir(self) -> RankDir {
        match self {
            Self::TopToBottom => RankDir::TB,
            Self::BottomToTop => RankDir::BT,
            Self::LeftToRight => RankDir::LR,
            Self::RightToLeft => RankDir::RL,
        }
    }
}

/// Built-in Dagre-compatible layout engine powered by `dugong`.
#[derive(Debug, Default, Clone, Copy)]
pub struct DugongLayoutEngine;

impl LayoutEngine for DugongLayoutEngine {
    fn id(&self) -> LayoutEngineId {
        LayoutEngineId::new(DUGONG_LAYOUT_ENGINE_ID)
    }

    fn layout(
        &self,
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        layout_graph_with_dugong_context(graph, request, context)
    }
}

/// Runs a Dagre-compatible layout using the `dugong` backend.
pub fn layout_graph_with_dugong(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    layout_graph_with_dugong_context(graph, request, &LayoutContext::default())
}

/// Runs `dugong` and converts the node positions into a Jellyflow transaction.
pub fn layout_graph_to_transaction_with_dugong(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<GraphTransaction, LayoutError> {
    layout_graph_with_dugong(graph, request)?.to_transaction(graph)
}

fn layout_graph_with_dugong_context(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
) -> Result<LayoutResult, LayoutError> {
    validate_request(graph, request)?;

    let mut ctx = ProjectionContext::new(graph, request, context)?;
    dugong::layout_dagreish(&mut ctx.graph);
    ctx.into_result()
}

struct ProjectionContext {
    graph: DugongGraph<NodeLabel, EdgeLabel, GraphLabel>,
    nodes: Vec<ProjectedNode>,
    edges: Vec<ProjectedEdge>,
    bounds: Option<CanvasRect>,
}

struct ProjectedNode {
    id: NodeId,
    key: String,
    size: CanvasSize,
    origin: (f32, f32),
}

struct ProjectedEdge {
    id: EdgeId,
    source: String,
    target: String,
    name: String,
}

impl ProjectionContext {
    fn new(
        graph: &Graph,
        request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<Self, LayoutError> {
        let mut projected = DugongGraph::with_capacity(
            GraphOptions {
                multigraph: true,
                ..GraphOptions::default()
            },
            graph.nodes().len(),
            graph.edges().len(),
        );
        projected.set_graph(graph_label(request.options));

        let mut node_keys = std::collections::BTreeMap::new();
        let mut nodes = Vec::new();

        for (id, node) in graph.nodes() {
            if node.hidden || !request.scope.contains(*id) {
                continue;
            }

            let size = resolve_node_size(graph, request, context, *id)?;
            let key = node_key(*id);
            projected.set_node(
                key.clone(),
                NodeLabel {
                    width: f64::from(size.width),
                    height: f64::from(size.height),
                    ..NodeLabel::default()
                },
            );
            node_keys.insert(*id, key.clone());
            nodes.push(ProjectedNode {
                id: *id,
                key,
                size,
                origin: resolve_node_origin(node.origin, request.options.node_origin, context),
            });
        }

        let mut edges = Vec::new();
        for (id, edge) in graph.edges() {
            if edge.hidden {
                continue;
            }

            let source_port = graph
                .ports()
                .get(&edge.from)
                .ok_or(LayoutError::MissingSourcePort(*id))?;
            let target_port = graph
                .ports()
                .get(&edge.to)
                .ok_or(LayoutError::MissingTargetPort(*id))?;
            if !graph.nodes().contains_key(&source_port.node) {
                return Err(LayoutError::MissingSourceNode { edge: *id });
            }
            if !graph.nodes().contains_key(&target_port.node) {
                return Err(LayoutError::MissingTargetNode { edge: *id });
            }

            let Some(source) = node_keys.get(&source_port.node).cloned() else {
                continue;
            };
            let Some(target) = node_keys.get(&target_port.node).cloned() else {
                continue;
            };

            let name = edge_key(*id);
            projected.set_edge_named(
                source.clone(),
                target.clone(),
                Some(name.clone()),
                Some(EdgeLabel::default()),
            );
            edges.push(ProjectedEdge {
                id: *id,
                source,
                target,
                name,
            });
        }

        Ok(Self {
            graph: projected,
            nodes,
            edges,
            bounds: None,
        })
    }

    fn into_result(self) -> Result<LayoutResult, LayoutError> {
        let mut positions = Vec::with_capacity(self.nodes.len());
        let mut bounds = self.bounds;

        for node in self.nodes {
            let label = self
                .graph
                .node(&node.key)
                .ok_or(LayoutError::MissingNodePosition(node.id))?;
            let x = label.x.ok_or(LayoutError::MissingNodePosition(node.id))?;
            let y = label.y.ok_or(LayoutError::MissingNodePosition(node.id))?;
            if !x.is_finite() || !y.is_finite() {
                return Err(LayoutError::NonFiniteNodePosition {
                    node: node.id,
                    x,
                    y,
                });
            }

            let center = CanvasPoint {
                x: x as f32,
                y: y as f32,
            };
            let pos = position_from_center(center, node.size, node.origin);
            let node_position = LayoutNodePosition {
                node: node.id,
                pos,
                center,
                size: node.size,
            };
            bounds = union_bounds(bounds, node_rect_from_position(&node_position));
            positions.push(node_position);
        }

        let mut edge_routes = Vec::new();
        for edge in self.edges {
            let Some(label) = self
                .graph
                .edge(&edge.source, &edge.target, Some(&edge.name))
            else {
                continue;
            };
            let points = label
                .points
                .iter()
                .map(|point| CanvasPoint {
                    x: point.x as f32,
                    y: point.y as f32,
                })
                .collect();
            edge_routes.push(LayoutEdgeRoute {
                edge: edge.id,
                points,
            });
        }

        Ok(LayoutResult {
            nodes: positions,
            edge_routes,
            bounds,
        })
    }
}

fn graph_label(options: LayoutOptions) -> GraphLabel {
    GraphLabel {
        rankdir: options.direction.as_dugong_rankdir(),
        nodesep: f64::from(options.spacing.nodesep),
        ranksep: f64::from(options.spacing.ranksep),
        edgesep: f64::from(options.spacing.edgesep),
        marginx: f64::from(options.margin.width),
        marginy: f64::from(options.margin.height),
        ..GraphLabel::default()
    }
}

fn node_key(node: NodeId) -> String {
    node.0.to_string()
}

fn edge_key(edge: EdgeId) -> String {
    edge.0.to_string()
}
