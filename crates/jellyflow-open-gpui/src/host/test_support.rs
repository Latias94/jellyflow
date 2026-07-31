use jellyflow_core::{EdgeId, NodeId, PortId};
use jellyflow_runtime::{
    geometry::{ViewPoint, ViewRect},
    graph::GraphRevision,
    interaction::CanvasInteractionPhase,
    viewport::ViewportTransform,
};

use super::OpenGpuiSceneHost;

/// Bounds of one node surface that participated in an actual native scene render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSmokeNodeBounds {
    node: NodeId,
    bounds: ViewRect,
}

impl OpenGpuiSmokeNodeBounds {
    pub(super) const fn new(node: NodeId, bounds: ViewRect) -> Self {
        Self { node, bounds }
    }

    #[must_use]
    pub const fn node(self) -> NodeId {
        self.node
    }

    #[must_use]
    pub const fn bounds(self) -> ViewRect {
        self.bounds
    }
}

/// Current semantic visual footprint of one retained node.
///
/// This includes parked nodes so native smoke assertions can distinguish a correctly off-screen
/// virtualized node from a node whose visible semantic footprint was incorrectly omitted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSmokeNodeFootprint {
    node: NodeId,
    bounds: ViewRect,
    materialized: bool,
}

impl OpenGpuiSmokeNodeFootprint {
    pub(super) const fn new(node: NodeId, bounds: ViewRect, materialized: bool) -> Self {
        Self {
            node,
            bounds,
            materialized,
        }
    }

    #[must_use]
    pub const fn node(self) -> NodeId {
        self.node
    }

    #[must_use]
    pub const fn bounds(self) -> ViewRect {
        self.bounds
    }

    #[must_use]
    pub const fn materialized(self) -> bool {
        self.materialized
    }
}

/// Endpoints of one authoritative route that participated in an actual native scene render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSmokeRoute {
    edge: EdgeId,
    source: ViewPoint,
    target: ViewPoint,
}

impl OpenGpuiSmokeRoute {
    pub(super) const fn new(edge: EdgeId, source: ViewPoint, target: ViewPoint) -> Self {
        Self {
            edge,
            source,
            target,
        }
    }

    #[must_use]
    pub const fn edge(self) -> EdgeId {
        self.edge
    }

    #[must_use]
    pub const fn source(self) -> ViewPoint {
        self.source
    }

    #[must_use]
    pub const fn target(self) -> ViewPoint {
        self.target
    }
}

/// Endpoints of the adapter-owned preview route painted by one native scene render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSmokePreviewRoute {
    source: ViewPoint,
    target: ViewPoint,
}

impl OpenGpuiSmokePreviewRoute {
    pub(super) const fn new(source: ViewPoint, target: ViewPoint) -> Self {
        Self { source, target }
    }

    #[must_use]
    pub const fn source(self) -> ViewPoint {
        self.source
    }

    #[must_use]
    pub const fn target(self) -> ViewPoint {
        self.target
    }
}

/// Port target used by the preview route in the same native scene render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSmokePreviewTarget {
    port: PortId,
    anchor: ViewPoint,
}

impl OpenGpuiSmokePreviewTarget {
    pub(super) const fn new(port: PortId, anchor: ViewPoint) -> Self {
        Self { port, anchor }
    }

    #[must_use]
    pub const fn port(self) -> PortId {
        self.port
    }

    #[must_use]
    pub const fn anchor(self) -> ViewPoint {
        self.anchor
    }
}

/// Scene facts assembled by the exact render pass observed by a native frame callback.
///
/// This snapshot is intentionally available only through the adapter's `test-support` feature.
/// It is evidence for real-window smoke tests, not a second public rendering protocol.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenGpuiSmokePresentedScene {
    render_sequence: u64,
    graph_revision: GraphRevision,
    phase: CanvasInteractionPhase,
    node_drag_active: bool,
    viewport: ViewportTransform,
    node_bounds: Vec<OpenGpuiSmokeNodeBounds>,
    chrome_bounds: Vec<OpenGpuiSmokeNodeBounds>,
    authoritative_routes: Vec<OpenGpuiSmokeRoute>,
    preview_route: Option<OpenGpuiSmokePreviewRoute>,
    preview_target: Option<OpenGpuiSmokePreviewTarget>,
    suppressed_edge: Option<EdgeId>,
}

impl OpenGpuiSmokePresentedScene {
    #[must_use]
    pub const fn render_sequence(&self) -> u64 {
        self.render_sequence
    }

    #[must_use]
    pub const fn graph_revision(&self) -> GraphRevision {
        self.graph_revision
    }

    #[must_use]
    pub const fn phase(&self) -> CanvasInteractionPhase {
        self.phase
    }

    #[must_use]
    pub const fn node_drag_active(&self) -> bool {
        self.node_drag_active
    }

    #[must_use]
    pub const fn viewport(&self) -> ViewportTransform {
        self.viewport
    }

    #[must_use]
    pub fn node_bounds(&self) -> &[OpenGpuiSmokeNodeBounds] {
        &self.node_bounds
    }

    #[must_use]
    pub fn chrome_bounds(&self) -> &[OpenGpuiSmokeNodeBounds] {
        &self.chrome_bounds
    }

    #[must_use]
    pub fn authoritative_routes(&self) -> &[OpenGpuiSmokeRoute] {
        &self.authoritative_routes
    }

    #[must_use]
    pub const fn preview_route(&self) -> Option<OpenGpuiSmokePreviewRoute> {
        self.preview_route
    }

    #[must_use]
    pub const fn preview_target(&self) -> Option<OpenGpuiSmokePreviewTarget> {
        self.preview_target
    }

    #[must_use]
    pub const fn suppressed_edge(&self) -> Option<EdgeId> {
        self.suppressed_edge
    }
}

#[derive(Debug, Default)]
pub(super) struct OpenGpuiSmokePresentedSceneOwner {
    next_render_sequence: u64,
    latest: Option<OpenGpuiSmokePresentedScene>,
}

impl OpenGpuiSmokePresentedSceneOwner {
    pub(super) fn begin(
        &mut self,
        graph_revision: GraphRevision,
        phase: CanvasInteractionPhase,
        node_drag_active: bool,
        viewport: ViewportTransform,
    ) -> OpenGpuiSmokePresentedSceneBuilder {
        let render_sequence = self.next_render_sequence;
        self.next_render_sequence = self.next_render_sequence.saturating_add(1);
        OpenGpuiSmokePresentedSceneBuilder {
            scene: OpenGpuiSmokePresentedScene {
                render_sequence,
                graph_revision,
                phase,
                node_drag_active,
                viewport,
                node_bounds: Vec::new(),
                chrome_bounds: Vec::new(),
                authoritative_routes: Vec::new(),
                preview_route: None,
                preview_target: None,
                suppressed_edge: None,
            },
        }
    }

    pub(super) fn publish(&mut self, builder: OpenGpuiSmokePresentedSceneBuilder) {
        self.latest = Some(builder.scene);
    }

    pub(super) fn latest(&self) -> Option<OpenGpuiSmokePresentedScene> {
        self.latest.clone()
    }
}

pub(super) struct OpenGpuiSmokePresentedSceneBuilder {
    scene: OpenGpuiSmokePresentedScene,
}

impl OpenGpuiSmokePresentedSceneBuilder {
    pub(super) fn record_node(&mut self, node: NodeId, bounds: ViewRect, chrome: bool) {
        let bounds = OpenGpuiSmokeNodeBounds::new(node, bounds);
        self.scene.node_bounds.push(bounds);
        if chrome {
            self.scene.chrome_bounds.push(bounds);
        }
    }

    pub(super) fn record_route(&mut self, route: OpenGpuiSmokeRoute) {
        self.scene.authoritative_routes.push(route);
    }

    pub(super) fn set_preview(
        &mut self,
        route: Option<OpenGpuiSmokePreviewRoute>,
        target: Option<OpenGpuiSmokePreviewTarget>,
        suppressed_edge: Option<EdgeId>,
    ) {
        self.scene.preview_route = route;
        self.scene.preview_target = target;
        self.scene.suppressed_edge = suppressed_edge;
    }
}

impl OpenGpuiSceneHost {
    /// Returns facts from the most recent native scene render.
    #[must_use]
    pub fn smoke_presented_scene_snapshot(&self) -> Option<OpenGpuiSmokePresentedScene> {
        self.smoke_presented_scene.latest()
    }

    /// Returns semantic visual footprints for all retained nodes at the current viewport.
    #[must_use]
    pub fn smoke_node_footprints(&self) -> Vec<OpenGpuiSmokeNodeFootprint> {
        self.view
            .viewport_transform()
            .ok()
            .map(|transform| self.retained_nodes.smoke_node_footprints(transform))
            .unwrap_or_default()
    }
}
