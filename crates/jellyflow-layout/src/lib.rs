//! Optional headless layout adapters for Jellyflow.
//!
//! This crate keeps automatic layout outside the core document model. Layout engines receive a
//! projection of a Jellyflow graph and return normal [`GraphTransaction`] values that hosts can
//! apply explicitly.

#![deny(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use dugong::graphlib::{Graph as DugongGraph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir};
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphOp, GraphTransaction, NodeId,
};
use serde::{Deserialize, Serialize};

/// Direction for a layered graph layout.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutDirection {
    /// Top to bottom.
    #[default]
    TopToBottom,
    /// Bottom to top.
    BottomToTop,
    /// Left to right.
    LeftToRight,
    /// Right to left.
    RightToLeft,
}

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

/// Spacing knobs passed through to the Dagre-compatible layout.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutSpacing {
    pub nodesep: f32,
    pub ranksep: f32,
    pub edgesep: f32,
}

impl Default for LayoutSpacing {
    fn default() -> Self {
        Self {
            nodesep: 50.0,
            ranksep: 50.0,
            edgesep: 20.0,
        }
    }
}

/// Options shared by Jellyflow layout adapters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutOptions {
    pub direction: LayoutDirection,
    pub spacing: LayoutSpacing,
    pub margin: CanvasSize,
    pub default_node_size: CanvasSize,
    /// Fallback node origin used when a node has no per-node origin override.
    pub node_origin: (f32, f32),
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            direction: LayoutDirection::TopToBottom,
            spacing: LayoutSpacing::default(),
            margin: CanvasSize {
                width: 0.0,
                height: 0.0,
            },
            default_node_size: CanvasSize {
                width: 172.0,
                height: 36.0,
            },
            node_origin: (0.0, 0.0),
        }
    }
}

impl LayoutOptions {
    /// Uses a different fallback node size for nodes without explicit or measured size.
    pub fn with_default_node_size(mut self, size: CanvasSize) -> Self {
        self.default_node_size = size;
        self
    }

    /// Uses a different layered layout direction.
    pub fn with_direction(mut self, direction: LayoutDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Uses a different fallback node origin.
    pub fn with_node_origin(mut self, node_origin: (f32, f32)) -> Self {
        self.node_origin = node_origin;
        self
    }
}

/// Which nodes a layout request should include.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayoutScope {
    /// Include all non-hidden nodes.
    #[default]
    All,
    /// Include only these nodes. Hidden nodes are still ignored.
    Nodes { nodes: BTreeSet<NodeId> },
}

impl LayoutScope {
    fn contains(&self, node: NodeId) -> bool {
        match self {
            Self::All => true,
            Self::Nodes { nodes } => nodes.contains(&node),
        }
    }
}

/// A headless layout request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutRequest {
    pub options: LayoutOptions,
    #[serde(default)]
    pub scope: LayoutScope,
    /// Adapter-reported node sizes. Graph node sizes win over these facts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub measured_node_sizes: BTreeMap<NodeId, CanvasSize>,
}

impl Default for LayoutRequest {
    fn default() -> Self {
        Self {
            options: LayoutOptions::default(),
            scope: LayoutScope::All,
            measured_node_sizes: BTreeMap::new(),
        }
    }
}

impl LayoutRequest {
    /// Creates a request for all visible nodes.
    pub fn all() -> Self {
        Self::default()
    }

    /// Creates a request for a selected set of nodes.
    pub fn nodes(nodes: impl IntoIterator<Item = NodeId>) -> Self {
        Self {
            scope: LayoutScope::Nodes {
                nodes: nodes.into_iter().collect(),
            },
            ..Self::default()
        }
    }

    /// Adds adapter-reported node sizes.
    pub fn with_measured_node_sizes(
        mut self,
        sizes: impl IntoIterator<Item = (NodeId, CanvasSize)>,
    ) -> Self {
        self.measured_node_sizes.extend(sizes);
        self
    }

    /// Sets layout options.
    pub fn with_options(mut self, options: LayoutOptions) -> Self {
        self.options = options;
        self
    }
}

/// A node position produced by a layout run.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutNodePosition {
    pub node: NodeId,
    pub pos: CanvasPoint,
    pub center: CanvasPoint,
    pub size: CanvasSize,
}

/// A layout-produced edge route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutEdgeRoute {
    pub edge: EdgeId,
    pub points: Vec<CanvasPoint>,
}

/// Result of a headless layout run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutResult {
    /// Engine-produced results contain at most one entry per node. If a caller manually constructs a
    /// result with duplicates, lookup returns the first entry and transaction conversion fails.
    pub nodes: Vec<LayoutNodePosition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_routes: Vec<LayoutEdgeRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<CanvasRect>,
}

impl LayoutResult {
    /// Finds one node position in this result.
    pub fn node_position(&self, node: NodeId) -> Option<LayoutNodePosition> {
        self.nodes
            .iter()
            .find(|position| position.node == node)
            .copied()
    }

    /// Converts node position changes into a Jellyflow transaction.
    pub fn to_transaction(&self, graph: &Graph) -> Result<GraphTransaction, LayoutError> {
        let mut seen = BTreeSet::new();
        let mut ops = Vec::new();

        for node in &self.nodes {
            if !seen.insert(node.node) {
                return Err(LayoutError::DuplicateResultNode(node.node));
            }

            let from = graph
                .nodes
                .get(&node.node)
                .ok_or(LayoutError::MissingTransactionNode(node.node))?
                .pos;
            if from != node.pos {
                ops.push(GraphOp::SetNodePos {
                    id: node.node,
                    from,
                    to: node.pos,
                });
            }
        }

        Ok(GraphTransaction::from_ops(ops).with_label("Layout graph"))
    }
}

/// Errors reported by layout projection or layout output conversion.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum LayoutError {
    #[error("layout default node size must be positive and finite: {0:?}")]
    InvalidDefaultNodeSize(CanvasSize),
    #[error("layout spacing values must be non-negative and finite: {0:?}")]
    InvalidSpacing(LayoutSpacing),
    #[error("layout margin must be non-negative and finite: {0:?}")]
    InvalidMargin(CanvasSize),
    #[error("layout node size must be positive and finite for node {node:?}: {size:?}")]
    InvalidNodeSize { node: NodeId, size: CanvasSize },
    #[error("layout scope references missing node: {0:?}")]
    MissingScopeNode(NodeId),
    #[error("layout edge references missing source port: {0:?}")]
    MissingSourcePort(EdgeId),
    #[error("layout edge references missing target port: {0:?}")]
    MissingTargetPort(EdgeId),
    #[error("layout edge source port references missing node: {edge:?}")]
    MissingSourceNode { edge: EdgeId },
    #[error("layout edge target port references missing node: {edge:?}")]
    MissingTargetNode { edge: EdgeId },
    #[error("layout engine did not return a node position for node {0:?}")]
    MissingNodePosition(NodeId),
    #[error("layout result contains a duplicate node position for node {0:?}")]
    DuplicateResultNode(NodeId),
    #[error("layout result references missing graph node: {0:?}")]
    MissingTransactionNode(NodeId),
    #[error("layout engine returned a non-finite node position for node {node:?}: ({x}, {y})")]
    NonFiniteNodePosition { node: NodeId, x: f64, y: f64 },
}

/// Runs a Dagre-compatible layout using the `dugong` backend.
pub fn layout_graph_with_dugong(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<LayoutResult, LayoutError> {
    validate_request(graph, request)?;

    let mut ctx = ProjectionContext::new(graph, request)?;
    dugong::layout_dagreish(&mut ctx.graph);
    ctx.into_result()
}

/// Runs `dugong` and converts the node positions into a Jellyflow transaction.
pub fn layout_graph_to_transaction_with_dugong(
    graph: &Graph,
    request: &LayoutRequest,
) -> Result<GraphTransaction, LayoutError> {
    layout_graph_with_dugong(graph, request)?.to_transaction(graph)
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
    fn new(graph: &Graph, request: &LayoutRequest) -> Result<Self, LayoutError> {
        let mut projected = DugongGraph::with_capacity(
            GraphOptions {
                multigraph: true,
                ..GraphOptions::default()
            },
            graph.nodes.len(),
            graph.edges.len(),
        );
        projected.set_graph(graph_label(request.options));

        let mut node_keys = BTreeMap::new();
        let mut nodes = Vec::new();

        for (id, node) in &graph.nodes {
            if node.hidden || !request.scope.contains(*id) {
                continue;
            }

            let size = resolve_node_size(graph, request, *id)?;
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
                origin: resolve_node_origin(node.origin, request.options.node_origin),
            });
        }

        let mut edges = Vec::new();
        for (id, edge) in &graph.edges {
            if edge.hidden {
                continue;
            }

            let source_port = graph
                .ports
                .get(&edge.from)
                .ok_or(LayoutError::MissingSourcePort(*id))?;
            let target_port = graph
                .ports
                .get(&edge.to)
                .ok_or(LayoutError::MissingTargetPort(*id))?;
            if !graph.nodes.contains_key(&source_port.node) {
                return Err(LayoutError::MissingSourceNode { edge: *id });
            }
            if !graph.nodes.contains_key(&target_port.node) {
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

fn validate_request(graph: &Graph, request: &LayoutRequest) -> Result<(), LayoutError> {
    if !request.options.default_node_size.is_positive_finite() {
        return Err(LayoutError::InvalidDefaultNodeSize(
            request.options.default_node_size,
        ));
    }
    if !is_non_negative_finite_spacing(request.options.spacing) {
        return Err(LayoutError::InvalidSpacing(request.options.spacing));
    }
    if !is_non_negative_finite_size(request.options.margin) {
        return Err(LayoutError::InvalidMargin(request.options.margin));
    }

    if let LayoutScope::Nodes { nodes } = &request.scope {
        for node in nodes {
            if !graph.nodes.contains_key(node) {
                return Err(LayoutError::MissingScopeNode(*node));
            }
        }
    }

    Ok(())
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

fn is_non_negative_finite_spacing(spacing: LayoutSpacing) -> bool {
    spacing.nodesep.is_finite()
        && spacing.ranksep.is_finite()
        && spacing.edgesep.is_finite()
        && spacing.nodesep >= 0.0
        && spacing.ranksep >= 0.0
        && spacing.edgesep >= 0.0
}

fn is_non_negative_finite_size(size: CanvasSize) -> bool {
    size.is_finite() && size.width >= 0.0 && size.height >= 0.0
}

fn resolve_node_size(
    graph: &Graph,
    request: &LayoutRequest,
    node: NodeId,
) -> Result<CanvasSize, LayoutError> {
    let size = graph
        .nodes
        .get(&node)
        .and_then(|node| node.size)
        .or_else(|| request.measured_node_sizes.get(&node).copied())
        .unwrap_or(request.options.default_node_size);

    if size.is_positive_finite() {
        Ok(size)
    } else {
        Err(LayoutError::InvalidNodeSize { node, size })
    }
}

fn node_key(node: NodeId) -> String {
    node.0.to_string()
}

fn edge_key(edge: EdgeId) -> String {
    edge.0.to_string()
}

fn resolve_node_origin(
    origin: Option<jellyflow_core::NodeOrigin>,
    fallback: (f32, f32),
) -> (f32, f32) {
    let (x, y) = origin.map(|origin| origin.as_tuple()).unwrap_or(fallback);
    (normalize_origin_component(x), normalize_origin_component(y))
}

fn normalize_origin_component(component: f32) -> f32 {
    if component.is_finite() {
        component.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn position_from_center(center: CanvasPoint, size: CanvasSize, origin: (f32, f32)) -> CanvasPoint {
    CanvasPoint {
        x: center.x - size.width * (0.5 - origin.0),
        y: center.y - size.height * (0.5 - origin.1),
    }
}

fn node_rect_from_position(node: &LayoutNodePosition) -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: node.center.x - node.size.width * 0.5,
            y: node.center.y - node.size.height * 0.5,
        },
        size: node.size,
    }
}

fn union_bounds(bounds: Option<CanvasRect>, next: CanvasRect) -> Option<CanvasRect> {
    if !next.is_positive_finite() {
        return bounds;
    }

    let Some(bounds) = bounds else {
        return Some(next);
    };

    let min_x = bounds.origin.x.min(next.origin.x);
    let min_y = bounds.origin.y.min(next.origin.y);
    let max_x = (bounds.origin.x + bounds.size.width).max(next.origin.x + next.size.width);
    let max_y = (bounds.origin.y + bounds.size.height).max(next.origin.y + next.size.height);

    Some(CanvasRect {
        origin: CanvasPoint { x: min_x, y: min_y },
        size: CanvasSize {
            width: max_x - min_x,
            height: max_y - min_y,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow_core::{
        Edge, EdgeKind, GraphId, Node, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
        PortKey, PortKind,
    };

    #[test]
    fn dugong_layout_emits_node_position_transaction() {
        let (mut graph, a, b, _edge) = connected_graph();
        graph.nodes.get_mut(&a).unwrap().pos = CanvasPoint {
            x: 1000.0,
            y: 1000.0,
        };
        graph.nodes.get_mut(&b).unwrap().pos = CanvasPoint {
            x: 2000.0,
            y: 2000.0,
        };
        let request = LayoutRequest::all().with_options(LayoutOptions {
            default_node_size: CanvasSize {
                width: 100.0,
                height: 40.0,
            },
            ..LayoutOptions::default()
        });

        let tx = layout_graph_to_transaction_with_dugong(&graph, &request).expect("layout");

        assert_eq!(tx.label(), Some("Layout graph"));
        assert_eq!(tx.ops().len(), 2);
        assert!(
            tx.ops()
                .iter()
                .any(|op| matches!(op, GraphOp::SetNodePos { id, .. } if *id == a))
        );
        assert!(
            tx.ops()
                .iter()
                .any(|op| matches!(op, GraphOp::SetNodePos { id, .. } if *id == b))
        );
    }

    #[test]
    fn layout_direction_changes_axis_ordering() {
        let (graph, a, b, _edge) = connected_graph();
        let tb = layout_graph_with_dugong(&graph, &LayoutRequest::all()).expect("tb layout");
        let lr = layout_graph_with_dugong(
            &graph,
            &LayoutRequest::all().with_options(
                LayoutOptions::default().with_direction(LayoutDirection::LeftToRight),
            ),
        )
        .expect("lr layout");

        let tb_a = tb.node_position(a).expect("tb a");
        let tb_b = tb.node_position(b).expect("tb b");
        let lr_a = lr.node_position(a).expect("lr a");
        let lr_b = lr.node_position(b).expect("lr b");

        assert!(tb_b.center.y > tb_a.center.y);
        assert!((tb_b.center.x - tb_a.center.x).abs() <= 1.0e-3);
        assert!(lr_b.center.x > lr_a.center.x);
        assert!((lr_b.center.y - lr_a.center.y).abs() <= 1.0e-3);
    }

    #[test]
    fn node_origin_controls_written_position_from_dugong_center() {
        let (mut graph, a, _b, _edge) = connected_graph();
        graph.nodes.get_mut(&a).unwrap().origin =
            Some(jellyflow_core::NodeOrigin { x: 0.5, y: 0.5 });
        let request = LayoutRequest::all().with_options(LayoutOptions {
            default_node_size: CanvasSize {
                width: 100.0,
                height: 40.0,
            },
            ..LayoutOptions::default()
        });

        let result = layout_graph_with_dugong(&graph, &request).expect("layout");
        let node = result.node_position(a).expect("node");

        assert!((node.pos.x - node.center.x).abs() <= 1.0e-3);
        assert!((node.pos.y - node.center.y).abs() <= 1.0e-3);
    }

    #[test]
    fn layout_scope_uses_only_requested_nodes_and_internal_edges() {
        let (graph, a, b, _edge) = connected_graph();

        let result = layout_graph_with_dugong(&graph, &LayoutRequest::nodes([a])).expect("layout");

        assert!(result.node_position(a).is_some());
        assert!(result.node_position(b).is_none());
        assert!(result.edge_routes.is_empty());
    }

    #[test]
    fn node_size_resolution_prefers_graph_then_measured_then_default() {
        let (mut graph, a, b, _edge) = connected_graph();
        let graph_size = size(300.0, 70.0);
        let measured_size = size(80.0, 50.0);
        graph.nodes.get_mut(&a).unwrap().size = Some(graph_size);
        let request = LayoutRequest::all()
            .with_measured_node_sizes([(a, size(10.0, 10.0)), (b, measured_size)])
            .with_options(LayoutOptions {
                default_node_size: size(20.0, 20.0),
                ..LayoutOptions::default()
            });

        let result = layout_graph_with_dugong(&graph, &request).expect("layout");

        assert_eq!(result.node_position(a).expect("a").size, graph_size);
        assert_eq!(result.node_position(b).expect("b").size, measured_size);
    }

    #[test]
    fn hidden_nodes_and_edges_are_excluded_from_projection() {
        let (mut graph, a, b, edge) = connected_graph();
        graph.nodes.get_mut(&b).unwrap().hidden = true;

        let hidden_node_result =
            layout_graph_with_dugong(&graph, &LayoutRequest::all()).expect("hidden node layout");

        assert!(hidden_node_result.node_position(a).is_some());
        assert!(hidden_node_result.node_position(b).is_none());
        assert!(hidden_node_result.edge_routes.is_empty());

        graph.nodes.get_mut(&b).unwrap().hidden = false;
        graph.edges.get_mut(&edge).unwrap().hidden = true;

        let hidden_edge_result =
            layout_graph_with_dugong(&graph, &LayoutRequest::all()).expect("hidden edge layout");

        assert!(hidden_edge_result.node_position(a).is_some());
        assert!(hidden_edge_result.node_position(b).is_some());
        assert!(hidden_edge_result.edge_routes.is_empty());
    }

    #[test]
    fn layout_reports_projected_edge_routes() {
        let (graph, _a, _b, edge) = connected_graph();

        let result = layout_graph_with_dugong(&graph, &LayoutRequest::all()).expect("layout");
        let route = result
            .edge_routes
            .iter()
            .find(|route| route.edge == edge)
            .expect("edge route");

        assert!(!route.points.is_empty());
        assert!(route.points.iter().all(|point| point.is_finite()));
    }

    #[test]
    fn parallel_edges_between_same_nodes_keep_distinct_routes() {
        let (mut graph, _a, _b, first_edge) = connected_graph();
        let second_edge = EdgeId::from_u128(6);
        let endpoints = {
            let edge = graph.edges.get(&first_edge).expect("first edge");
            (edge.from, edge.to)
        };
        graph
            .edges
            .insert(second_edge, data_edge(endpoints.0, endpoints.1));

        let result = layout_graph_with_dugong(&graph, &LayoutRequest::all()).expect("layout");

        assert_eq!(result.edge_routes.len(), 2);
        assert!(
            result
                .edge_routes
                .iter()
                .any(|route| route.edge == first_edge)
        );
        assert!(
            result
                .edge_routes
                .iter()
                .any(|route| route.edge == second_edge)
        );
    }

    #[test]
    fn empty_graph_layout_is_empty() {
        let graph = Graph::new(GraphId::from_u128(42));

        let result = layout_graph_with_dugong(&graph, &LayoutRequest::all()).expect("layout");
        let tx =
            layout_graph_to_transaction_with_dugong(&graph, &LayoutRequest::all()).expect("tx");

        assert!(result.nodes.is_empty());
        assert!(result.edge_routes.is_empty());
        assert!(result.bounds.is_none());
        assert!(tx.ops().is_empty());
    }

    #[test]
    fn invalid_size_is_reported_before_layout() {
        let (graph, a, _b, _edge) = connected_graph();
        let request = LayoutRequest::all().with_measured_node_sizes([(
            a,
            CanvasSize {
                width: 0.0,
                height: 1.0,
            },
        )]);

        let err = layout_graph_with_dugong(&graph, &request).expect_err("invalid size");

        assert_eq!(
            err,
            LayoutError::InvalidNodeSize {
                node: a,
                size: CanvasSize {
                    width: 0.0,
                    height: 1.0,
                },
            }
        );
    }

    #[test]
    fn unused_measured_sizes_do_not_fail_scoped_layout() {
        let (mut graph, a, b, _edge) = connected_graph();
        graph.nodes.get_mut(&b).unwrap().hidden = true;
        let request = LayoutRequest::nodes([a]).with_measured_node_sizes([
            (
                b,
                CanvasSize {
                    width: 0.0,
                    height: 1.0,
                },
            ),
            (
                NodeId::from_u128(99),
                CanvasSize {
                    width: f32::NAN,
                    height: 1.0,
                },
            ),
        ]);

        let result = layout_graph_with_dugong(&graph, &request).expect("layout");

        assert!(result.node_position(a).is_some());
        assert!(result.node_position(b).is_none());
    }

    #[test]
    fn invalid_spacing_and_margin_are_reported_before_layout() {
        let (graph, _a, _b, _edge) = connected_graph();
        let spacing = LayoutSpacing {
            nodesep: -1.0,
            ..LayoutSpacing::default()
        };
        let spacing_request = LayoutRequest::all().with_options(LayoutOptions {
            spacing,
            ..LayoutOptions::default()
        });

        let err = layout_graph_with_dugong(&graph, &spacing_request).expect_err("spacing");

        assert_eq!(err, LayoutError::InvalidSpacing(spacing));

        let margin = CanvasSize {
            width: f32::INFINITY,
            height: 0.0,
        };
        let margin_request = LayoutRequest::all().with_options(LayoutOptions {
            margin,
            ..LayoutOptions::default()
        });

        let err = layout_graph_with_dugong(&graph, &margin_request).expect_err("margin");

        assert_eq!(err, LayoutError::InvalidMargin(margin));
    }

    #[test]
    fn invalid_scope_node_is_reported_before_layout() {
        let (graph, _a, _b, _edge) = connected_graph();
        let missing = NodeId::from_u128(99);

        let err = layout_graph_with_dugong(&graph, &LayoutRequest::nodes([missing]))
            .expect_err("missing scope node");

        assert_eq!(err, LayoutError::MissingScopeNode(missing));
    }

    #[test]
    fn missing_source_and_target_ports_are_reported() {
        let (mut missing_source, _a, _b, edge) = connected_graph();
        let source_port = missing_source.edges.get(&edge).unwrap().from;
        missing_source.ports.remove(&source_port);

        let err = layout_graph_with_dugong(&missing_source, &LayoutRequest::all())
            .expect_err("missing source port");

        assert_eq!(err, LayoutError::MissingSourcePort(edge));

        let (mut missing_target, _a, _b, edge) = connected_graph();
        let target_port = missing_target.edges.get(&edge).unwrap().to;
        missing_target.ports.remove(&target_port);

        let err = layout_graph_with_dugong(&missing_target, &LayoutRequest::all())
            .expect_err("missing target port");

        assert_eq!(err, LayoutError::MissingTargetPort(edge));
    }

    #[test]
    fn missing_source_and_target_nodes_are_reported() {
        let (mut missing_source, a, _b, edge) = connected_graph();
        missing_source.nodes.remove(&a);

        let err = layout_graph_with_dugong(&missing_source, &LayoutRequest::all())
            .expect_err("missing source node");

        assert_eq!(err, LayoutError::MissingSourceNode { edge });

        let (mut missing_target, _a, b, edge) = connected_graph();
        missing_target.nodes.remove(&b);

        let err = layout_graph_with_dugong(&missing_target, &LayoutRequest::all())
            .expect_err("missing target node");

        assert_eq!(err, LayoutError::MissingTargetNode { edge });
    }

    #[test]
    fn result_to_transaction_rejects_duplicates_and_missing_nodes() {
        let (graph, a, _b, _edge) = connected_graph();
        let first = LayoutNodePosition {
            node: a,
            pos: CanvasPoint { x: 1.0, y: 2.0 },
            center: CanvasPoint { x: 51.0, y: 22.0 },
            size: size(100.0, 40.0),
        };
        let duplicate = LayoutResult {
            nodes: vec![first, first],
            edge_routes: Vec::new(),
            bounds: None,
        };

        let err = duplicate
            .to_transaction(&graph)
            .expect_err("duplicate node");

        assert_eq!(err, LayoutError::DuplicateResultNode(a));

        let missing = NodeId::from_u128(99);
        let missing_result = LayoutResult {
            nodes: vec![LayoutNodePosition {
                node: missing,
                ..first
            }],
            edge_routes: Vec::new(),
            bounds: None,
        };

        let err = missing_result
            .to_transaction(&graph)
            .expect_err("missing transaction node");

        assert_eq!(err, LayoutError::MissingTransactionNode(missing));
    }

    #[test]
    fn bounds_track_visual_rect_independent_of_node_origin_anchor() {
        let (mut graph, a, _b, _edge) = connected_graph();
        graph.nodes.get_mut(&a).unwrap().origin =
            Some(jellyflow_core::NodeOrigin { x: 1.0, y: 1.0 });
        let request = LayoutRequest::nodes([a]).with_options(LayoutOptions {
            default_node_size: size(100.0, 40.0),
            ..LayoutOptions::default()
        });

        let result = layout_graph_with_dugong(&graph, &request).expect("layout");
        let node = result.node_position(a).expect("node");
        let bounds = result.bounds.expect("bounds");

        assert!((node.pos.x - (node.center.x + node.size.width * 0.5)).abs() <= 1.0e-3);
        assert!((node.pos.y - (node.center.y + node.size.height * 0.5)).abs() <= 1.0e-3);
        assert!((bounds.origin.x - (node.center.x - node.size.width * 0.5)).abs() <= 1.0e-3);
        assert!((bounds.origin.y - (node.center.y - node.size.height * 0.5)).abs() <= 1.0e-3);
        assert_eq!(bounds.size, node.size);
    }

    fn connected_graph() -> (Graph, NodeId, NodeId, EdgeId) {
        let mut graph = Graph::new(GraphId::from_u128(1));
        let a = NodeId::from_u128(1);
        let b = NodeId::from_u128(2);
        let out = PortId::from_u128(3);
        let inn = PortId::from_u128(4);
        let edge = EdgeId::from_u128(5);

        graph.nodes.insert(a, node("demo.a", vec![out]));
        graph.nodes.insert(b, node("demo.b", vec![inn]));
        graph.ports.insert(out, port(a, "out", PortDirection::Out));
        graph.ports.insert(inn, port(b, "in", PortDirection::In));
        graph.edges.insert(edge, data_edge(out, inn));

        (graph, a, b, edge)
    }

    fn size(width: f32, height: f32) -> CanvasSize {
        CanvasSize { width, height }
    }

    fn data_edge(from: PortId, to: PortId) -> Edge {
        Edge {
            kind: EdgeKind::Data,
            from,
            to,
            hidden: false,
            selectable: None,
            focusable: None,
            interaction_width: None,
            deletable: None,
            reconnectable: None,
        }
    }

    fn node(kind: &str, ports: Vec<PortId>) -> Node {
        Node {
            kind: NodeKindKey::new(kind),
            kind_version: 1,
            pos: CanvasPoint { x: 0.0, y: 0.0 },
            origin: None,
            selectable: None,
            focusable: None,
            draggable: None,
            connectable: None,
            deletable: None,
            parent: None,
            extent: None,
            expand_parent: None,
            size: None,
            hidden: false,
            collapsed: false,
            ports,
            data: serde_json::Value::Null,
        }
    }

    fn port(node: NodeId, key: &str, dir: PortDirection) -> Port {
        Port {
            node,
            key: PortKey::new(key),
            dir,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        }
    }
}
