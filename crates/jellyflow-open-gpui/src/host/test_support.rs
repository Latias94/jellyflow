use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use jellyflow_core::{EdgeId, NodeId, PortId};
use jellyflow_runtime::{
    geometry::{ViewPoint, ViewRect, ViewSize},
    graph::GraphRevision,
    interaction::CanvasInteractionPhase,
    viewport::ViewportTransform,
};
use open_gpui::{Bounds, Pixels, Point};

use super::OpenGpuiSceneHost;
use super::scene_render::reconnect_hit_center;

/// Resolves the exact reconnect hit center used by the production scene renderer.
///
/// This helper is available only with `test-support`; native-window drivers can target the real
/// renderer policy without copying its layout constants.
#[doc(hidden)]
#[must_use]
pub fn resolve_smoke_reconnect_hit_center(
    anchor: ViewPoint,
    outward: jellyflow_runtime::geometry::HandlePosition,
    port_hit_bounds: Option<ViewRect>,
    hit_diameter: f32,
) -> ViewPoint {
    reconnect_hit_center(anchor, outward, port_hit_bounds, hit_diameter)
}

/// Bounds of one node surface that participated in an actual native scene render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSmokeNodeBounds {
    node: NodeId,
    bounds: ViewRect,
    selected: bool,
    chrome_expected: bool,
    chrome_bounds: Option<ViewRect>,
}

impl OpenGpuiSmokeNodeBounds {
    const fn new(
        node: NodeId,
        bounds: ViewRect,
        selected: bool,
        chrome_expected: bool,
        chrome_bounds: Option<ViewRect>,
    ) -> Self {
        Self {
            node,
            bounds,
            selected,
            chrome_expected,
            chrome_bounds,
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
    pub const fn selected(self) -> bool {
        self.selected
    }

    #[must_use]
    pub const fn chrome_expected(self) -> bool {
        self.chrome_expected
    }

    #[must_use]
    pub const fn chrome_bounds(self) -> Option<ViewRect> {
        self.chrome_bounds
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

/// Geometry of one real graph port painted by the same native scene render.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OpenGpuiSmokePortGeometry {
    node: NodeId,
    port: PortId,
    visual_bounds: ViewRect,
    hit_bounds: ViewRect,
    interaction_bounds: Option<ViewRect>,
    route_anchor: ViewPoint,
}

impl OpenGpuiSmokePortGeometry {
    const fn new(
        node: NodeId,
        port: PortId,
        visual_bounds: ViewRect,
        hit_bounds: ViewRect,
        interaction_bounds: Option<ViewRect>,
        route_anchor: ViewPoint,
    ) -> Self {
        Self {
            node,
            port,
            visual_bounds,
            hit_bounds,
            interaction_bounds,
            route_anchor,
        }
    }

    #[must_use]
    pub const fn node(self) -> NodeId {
        self.node
    }

    #[must_use]
    pub const fn port(self) -> PortId {
        self.port
    }

    #[must_use]
    pub const fn visual_bounds(self) -> ViewRect {
        self.visual_bounds
    }

    #[must_use]
    pub const fn hit_bounds(self) -> ViewRect {
        self.hit_bounds
    }

    #[must_use]
    pub const fn interaction_bounds(self) -> Option<ViewRect> {
        self.interaction_bounds
    }

    #[must_use]
    pub const fn route_anchor(self) -> ViewPoint {
        self.route_anchor
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
    node_drag_nodes: Vec<NodeId>,
    viewport: ViewportTransform,
    node_bounds: Vec<OpenGpuiSmokeNodeBounds>,
    port_geometries: Vec<OpenGpuiSmokePortGeometry>,
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
    pub fn node_drag_nodes(&self) -> &[NodeId] {
        &self.node_drag_nodes
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
    pub fn port_geometries(&self) -> &[OpenGpuiSmokePortGeometry] {
        &self.port_geometries
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

#[derive(Debug, Clone, Copy)]
struct OpenGpuiSmokeRawNodeBounds {
    bounds: Option<Bounds<Pixels>>,
    selected: bool,
    chrome_expected: bool,
    chrome_bounds: Option<Bounds<Pixels>>,
}

#[derive(Debug, Clone, Copy)]
struct OpenGpuiSmokeRawRoute {
    source: Point<Pixels>,
    target: Point<Pixels>,
}

#[derive(Debug, Clone, Copy)]
struct OpenGpuiSmokeRawPortGeometry {
    node: NodeId,
    port: PortId,
    visual_bounds: ViewRect,
    hit_bounds: ViewRect,
    interaction_bounds: Option<ViewRect>,
    route_anchor: ViewPoint,
}

#[derive(Debug)]
struct OpenGpuiSmokePresentedFrameDraftState {
    graph_revision: GraphRevision,
    phase: CanvasInteractionPhase,
    node_drag_nodes: Vec<NodeId>,
    viewport: ViewportTransform,
    scene_origin: Option<Point<Pixels>>,
    node_bounds: BTreeMap<NodeId, OpenGpuiSmokeRawNodeBounds>,
    port_geometries: Vec<OpenGpuiSmokeRawPortGeometry>,
    authoritative_routes: BTreeMap<EdgeId, OpenGpuiSmokeRawRoute>,
    preview_route: Option<OpenGpuiSmokeRawRoute>,
    preview_target: Option<OpenGpuiSmokePreviewTarget>,
    suppressed_edge: Option<EdgeId>,
}

/// Facts written only by the production frame's native prepaint callbacks.
#[derive(Debug, Clone, Default)]
pub(crate) struct OpenGpuiSmokePresentedFrameDraft {
    state: Rc<RefCell<Option<OpenGpuiSmokePresentedFrameDraftState>>>,
}

impl OpenGpuiSmokePresentedFrameDraft {
    pub(super) fn begin(
        &self,
        graph_revision: GraphRevision,
        phase: CanvasInteractionPhase,
        node_drag_nodes: &[NodeId],
        viewport: ViewportTransform,
    ) {
        self.state
            .replace(Some(OpenGpuiSmokePresentedFrameDraftState {
                graph_revision,
                phase,
                node_drag_nodes: node_drag_nodes.to_vec(),
                viewport,
                scene_origin: None,
                node_bounds: BTreeMap::new(),
                port_geometries: Vec::new(),
                authoritative_routes: BTreeMap::new(),
                preview_route: None,
                preview_target: None,
                suppressed_edge: None,
            }));
    }

    pub(super) fn record_scene_bounds(&self, bounds: Bounds<Pixels>) {
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state.scene_origin = Some(bounds.origin);
        }
    }

    pub(super) fn record_node_state(&self, node: NodeId, selected: bool, chrome_expected: bool) {
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state.node_bounds.insert(
                node,
                OpenGpuiSmokeRawNodeBounds {
                    bounds: None,
                    selected,
                    chrome_expected,
                    chrome_bounds: None,
                },
            );
        }
    }

    pub(super) fn record_node_bounds(&self, node: NodeId, bounds: Bounds<Pixels>) {
        if let Some(node) = self
            .state
            .borrow_mut()
            .as_mut()
            .and_then(|state| state.node_bounds.get_mut(&node))
        {
            node.bounds = Some(bounds);
        }
    }

    pub(super) fn record_node_chrome_bounds(&self, node: NodeId, bounds: Bounds<Pixels>) {
        if let Some(node) = self
            .state
            .borrow_mut()
            .as_mut()
            .and_then(|state| state.node_bounds.get_mut(&node))
        {
            node.chrome_bounds = Some(bounds);
        }
    }

    pub(crate) fn record_port_geometry(
        &self,
        node: NodeId,
        port: PortId,
        visual_bounds: ViewRect,
        hit_bounds: ViewRect,
        interaction_bounds: Option<ViewRect>,
        route_anchor: ViewPoint,
    ) {
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state.port_geometries.push(OpenGpuiSmokeRawPortGeometry {
                node,
                port,
                visual_bounds,
                hit_bounds,
                interaction_bounds,
                route_anchor,
            });
        }
    }

    pub(super) fn record_authoritative_route(
        &self,
        edge: EdgeId,
        source: Point<Pixels>,
        target: Point<Pixels>,
    ) {
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state
                .authoritative_routes
                .insert(edge, OpenGpuiSmokeRawRoute { source, target });
        }
    }

    pub(super) fn record_preview_route(&self, source: Point<Pixels>, target: Point<Pixels>) {
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state.preview_route = Some(OpenGpuiSmokeRawRoute { source, target });
        }
    }

    pub(super) fn set_preview_metadata(
        &self,
        target: Option<OpenGpuiSmokePreviewTarget>,
        suppressed_edge: Option<EdgeId>,
    ) {
        if let Some(state) = self.state.borrow_mut().as_mut() {
            state.preview_target = target;
            state.suppressed_edge = suppressed_edge;
        }
    }

    fn finish(&self, render_sequence: u64) -> Option<OpenGpuiSmokePresentedScene> {
        let state = self.state.borrow_mut().take()?;
        let scene_origin = state.scene_origin?;
        let node_bounds = state
            .node_bounds
            .into_iter()
            .map(|(node, raw)| {
                Some(OpenGpuiSmokeNodeBounds::new(
                    node,
                    view_rect(raw.bounds?, scene_origin),
                    raw.selected,
                    raw.chrome_expected,
                    raw.chrome_bounds
                        .map(|bounds| view_rect(bounds, scene_origin)),
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        let port_geometries = state
            .port_geometries
            .into_iter()
            .map(|raw| {
                OpenGpuiSmokePortGeometry::new(
                    raw.node,
                    raw.port,
                    relative_view_rect(raw.visual_bounds, scene_origin),
                    relative_view_rect(raw.hit_bounds, scene_origin),
                    raw.interaction_bounds
                        .map(|bounds| relative_view_rect(bounds, scene_origin)),
                    relative_view_point(raw.route_anchor, scene_origin),
                )
            })
            .collect();
        let authoritative_routes = state
            .authoritative_routes
            .into_iter()
            .map(|(edge, route)| {
                OpenGpuiSmokeRoute::new(
                    edge,
                    view_point(route.source, scene_origin),
                    view_point(route.target, scene_origin),
                )
            })
            .collect();
        let preview_route = state.preview_route.map(|route| {
            OpenGpuiSmokePreviewRoute::new(
                view_point(route.source, scene_origin),
                view_point(route.target, scene_origin),
            )
        });
        Some(OpenGpuiSmokePresentedScene {
            render_sequence,
            graph_revision: state.graph_revision,
            phase: state.phase,
            node_drag_nodes: state.node_drag_nodes,
            viewport: state.viewport,
            node_bounds,
            port_geometries,
            authoritative_routes,
            preview_route,
            preview_target: state.preview_target,
            suppressed_edge: state.suppressed_edge,
        })
    }
}

fn view_point(point: Point<Pixels>, scene_origin: Point<Pixels>) -> ViewPoint {
    ViewPoint::new(
        (point.x - scene_origin.x).as_f32(),
        (point.y - scene_origin.y).as_f32(),
    )
}

fn view_rect(bounds: Bounds<Pixels>, scene_origin: Point<Pixels>) -> ViewRect {
    ViewRect::new(
        view_point(bounds.origin, scene_origin),
        ViewSize::new(bounds.size.width.as_f32(), bounds.size.height.as_f32()),
    )
}

fn relative_view_point(point: ViewPoint, scene_origin: Point<Pixels>) -> ViewPoint {
    ViewPoint::new(
        point.x - scene_origin.x.as_f32(),
        point.y - scene_origin.y.as_f32(),
    )
}

fn relative_view_rect(bounds: ViewRect, scene_origin: Point<Pixels>) -> ViewRect {
    ViewRect::new(
        relative_view_point(bounds.origin, scene_origin),
        bounds.size,
    )
}

#[derive(Debug, Default)]
pub(super) struct OpenGpuiSmokePresentedSceneOwner {
    next_render_sequence: u64,
    presented: Option<OpenGpuiSmokePresentedScene>,
}

impl OpenGpuiSmokePresentedSceneOwner {
    pub(super) fn present(&mut self, draft: &OpenGpuiSmokePresentedFrameDraft) {
        let render_sequence = self
            .presented
            .as_ref()
            .map_or(self.next_render_sequence, |scene| scene.render_sequence());
        let Some(scene) = draft.finish(render_sequence) else {
            return;
        };
        if self.presented.is_none() {
            let Some(next_render_sequence) = render_sequence.checked_add(1) else {
                return;
            };
            self.next_render_sequence = next_render_sequence;
        }
        // Open GPUI test support may evaluate several dirty render passes before one platform
        // callback. Only the newest draft matches the framebuffer that callback exposes.
        self.presented = Some(scene);
    }

    fn drain(&mut self) -> Vec<OpenGpuiSmokePresentedScene> {
        self.presented.take().into_iter().collect()
    }
}

impl OpenGpuiSceneHost {
    /// Drains native scene facts finalized after their corresponding window frames rendered.
    #[must_use]
    pub fn drain_smoke_presented_scenes(&mut self) -> Vec<OpenGpuiSmokePresentedScene> {
        self.smoke_presented_scene.drain()
    }
}
