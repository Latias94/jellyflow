use dugong::graphlib::{Graph as DugongGraph, GraphOptions};
use dugong::{EdgeLabel, GraphLabel, NodeLabel, RankDir};
use jellyflow_core::{CanvasPoint, EdgeId, NodeId};

use crate::engine::{
    LayoutCoordinateFrame, LayoutDirection, LayoutEdgeRoute, LayoutEngine, LayoutError,
    LayoutNodePlacement, LayoutOptions, LayoutPlacements, PreparedLayoutInput, checked_f32,
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
    fn place(&self, input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        run_dugong_layout(input)
    }
}

fn run_dugong_layout(input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
    let mut workspace = DugongLayoutWorkspace::new(input);
    workspace.solve();
    workspace.into_result()
}

struct DugongLayoutWorkspace {
    graph: DugongGraph<NodeLabel, EdgeLabel, GraphLabel>,
    nodes: Vec<ProjectedNode>,
    edges: Vec<ProjectedEdge>,
}

struct ProjectedNode {
    id: NodeId,
    key: String,
}

struct ProjectedEdge {
    id: EdgeId,
    source: String,
    target: String,
    name: String,
}

impl DugongLayoutWorkspace {
    fn new(input: &PreparedLayoutInput<'_>) -> Self {
        let mut projected = DugongGraph::with_capacity(
            GraphOptions {
                multigraph: true,
                ..GraphOptions::default()
            },
            input.nodes().len(),
            input.edges().len(),
        );
        projected.set_graph(graph_label(input.options()));

        let mut node_keys = std::collections::BTreeMap::new();
        let mut nodes = Vec::new();

        for node in input.nodes() {
            let key = node_key(node.id());
            projected.set_node(
                key.clone(),
                NodeLabel {
                    width: f64::from(node.size().width),
                    height: f64::from(node.size().height),
                    ..NodeLabel::default()
                },
            );
            node_keys.insert(node.id(), key.clone());
            nodes.push(ProjectedNode { id: node.id(), key });
        }

        let mut edges = Vec::new();
        for edge in input.edges() {
            let Some(source) = node_keys.get(&edge.source()).cloned() else {
                continue;
            };
            let Some(target) = node_keys.get(&edge.target()).cloned() else {
                continue;
            };

            let name = edge_key(edge.id());
            projected.set_edge_named(
                source.clone(),
                target.clone(),
                Some(name.clone()),
                Some(EdgeLabel::default()),
            );
            edges.push(ProjectedEdge {
                id: edge.id(),
                source,
                target,
                name,
            });
        }

        Self {
            graph: projected,
            nodes,
            edges,
        }
    }

    fn solve(&mut self) {
        dugong::layout_dagreish(&mut self.graph);
    }

    fn into_result(self) -> Result<LayoutPlacements, LayoutError> {
        let mut positions = Vec::with_capacity(self.nodes.len());

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
                x: checked_f32(x).ok_or(LayoutError::InvalidResultNodeGeometry(node.id))?,
                y: checked_f32(y).ok_or(LayoutError::InvalidResultNodeGeometry(node.id))?,
            };
            positions.push(LayoutNodePlacement {
                node: node.id,
                center,
            });
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
                .map(|point| {
                    Some(CanvasPoint {
                        x: checked_f32(point.x)?,
                        y: checked_f32(point.y)?,
                    })
                })
                .collect::<Option<Vec<_>>>()
                .ok_or(LayoutError::InvalidResultEdgeRoute(edge.id))?;
            edge_routes.push(LayoutEdgeRoute {
                edge: edge.id,
                points,
            });
        }

        Ok(LayoutPlacements {
            nodes: positions,
            edge_routes,
            coordinate_frame: LayoutCoordinateFrame::NormalizeToMargin,
        })
    }
}

#[cfg(feature = "benchmark-internals")]
#[doc(hidden)]
pub mod benchmark_internals {
    use jellyflow_core::Graph;

    use crate::engine::{LayoutContext, LayoutError, LayoutPlacements, LayoutRequest};
    use crate::projection::PreparedLayoutRun;

    use super::DugongLayoutWorkspace;

    /// Projected `dugong` state used by internal benchmarks.
    pub struct DugongBenchmarkWorkspace {
        workspace: DugongLayoutWorkspace,
    }

    impl DugongBenchmarkWorkspace {
        /// Builds the `dugong` input graph without running layout.
        pub fn project(
            graph: &Graph,
            request: &LayoutRequest,
            context: &LayoutContext,
        ) -> Result<Self, LayoutError> {
            let run = PreparedLayoutRun::prepare(graph, request, context)?;
            Ok(Self {
                workspace: DugongLayoutWorkspace::new(run.input()),
            })
        }

        /// Runs `dugong` layout on the projected graph.
        pub fn solve(&mut self) {
            self.workspace.solve();
        }

        /// Converts solved `dugong` output into engine placements.
        pub fn into_placements(self) -> Result<LayoutPlacements, LayoutError> {
            self.workspace.into_result()
        }
    }
}

fn graph_label(options: LayoutOptions) -> GraphLabel {
    GraphLabel {
        rankdir: options.direction.as_dugong_rankdir(),
        nodesep: f64::from(options.spacing.nodesep),
        ranksep: f64::from(options.spacing.ranksep),
        edgesep: f64::from(options.spacing.edgesep),
        marginx: 0.0,
        marginy: 0.0,
        ..GraphLabel::default()
    }
}

fn node_key(node: NodeId) -> String {
    node.0.to_string()
}

fn edge_key(edge: EdgeId) -> String {
    edge.0.to_string()
}
