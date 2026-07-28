use std::collections::BTreeMap;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId, PortKind,
};
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::schema::NodeCatalog;
use jellyflow_runtime::{
    graph::DocumentEpoch, graph::GraphRevision, graph::PortInstanceKey, view::FrameNode,
    view::GeometrySnapshot, view::NodeInstanceKey,
};
use open_gpui::{
    AnyElement, AnyView, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div, px,
};

use crate::binding::OpenGpuiNodeBinding;
use crate::port_handle::{OpenGpuiPortAnchor, OpenGpuiPortSide};
use crate::presentation::OpenGpuiPortInteractionState;
use crate::scale_policy::{OpenGpuiNodeScaleBand, OpenGpuiNodeScalePolicy, OpenGpuiNodeScaleState};
use crate::theme::{OpenGpuiGraphPalette, OpenGpuiGraphTheme};

pub(crate) const OPEN_GPUI_OVERVIEW_NODE_SELECTOR: &str = "jellyflow-overview-node";
const MAX_OVERVIEW_MARKER_DIAMETER: f32 = 6.0;
const OVERVIEW_TITLE_BASE_TOP: f32 = 3.0;
const OVERVIEW_TITLE_BASE_INSET: f32 = 5.0;
const OVERVIEW_TITLE_LINE_HEIGHT: f32 = 12.0;
const OVERVIEW_TITLE_MIN_WIDTH: f32 = 20.0;
const OVERVIEW_TITLE_PORT_GAP: f32 = 4.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum OpenGpuiNodePresentation {
    Detail,
    Overview,
}

impl OpenGpuiNodePresentation {
    pub(crate) const fn from_scale_state(state: OpenGpuiNodeScaleState) -> Self {
        match state.band() {
            OpenGpuiNodeScaleBand::Detail => Self::Detail,
            OpenGpuiNodeScaleBand::Overview => Self::Overview,
        }
    }
}

#[derive(Clone, Debug)]
struct OpenGpuiOverviewPort {
    instance: PortInstanceKey,
    direction: PortDirection,
    kind: PortKind,
    detail_anchor: Option<NormalizedDetailPortAnchor>,
}

impl OpenGpuiOverviewPort {
    fn id(&self) -> PortId {
        self.instance.port()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OpenGpuiOverviewPortMarkerState {
    Disabled,
    InvalidTarget,
    ValidTarget,
    Source,
    Hovered,
    Connected,
    Idle,
}

impl From<&OpenGpuiPortInteractionState> for OpenGpuiOverviewPortMarkerState {
    fn from(state: &OpenGpuiPortInteractionState) -> Self {
        if state.disabled() {
            Self::Disabled
        } else if state.candidate().is_invalid_target() {
            Self::InvalidTarget
        } else if state.candidate().is_valid_target() {
            Self::ValidTarget
        } else if state.candidate().is_source() {
            Self::Source
        } else if state.hovered() {
            Self::Hovered
        } else if state.connected() {
            Self::Connected
        } else {
            Self::Idle
        }
    }
}

impl OpenGpuiOverviewPortMarkerState {
    const fn color(self, palette: OpenGpuiGraphPalette) -> Hsla {
        match self {
            Self::Disabled => palette.port_disabled,
            Self::InvalidTarget => palette.port_invalid,
            Self::ValidTarget => palette.port_valid,
            Self::Source => palette.port_source,
            Self::Hovered => palette.port_hovered,
            Self::Connected => palette.port_connected,
            Self::Idle => palette.port_idle,
        }
    }
}

/// Small overview-only visual recipe that preserves the semantic port handle's geometry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OpenGpuiOverviewPortMarkerRecipe {
    kind: PortKind,
    direction: PortDirection,
    state: OpenGpuiOverviewPortMarkerState,
    focused: bool,
}

impl OpenGpuiOverviewPortMarkerRecipe {
    fn from_port(port: &OpenGpuiOverviewPort, presentation: &OpenGpuiPortInteractionState) -> Self {
        Self {
            kind: port.kind,
            direction: port.direction,
            state: presentation.into(),
            focused: presentation.focused(),
        }
    }

    const fn layout_diameter(self, placement_diameter: f32) -> f32 {
        placement_diameter
    }

    const fn corner_radius(self, diameter: f32) -> f32 {
        match self.kind {
            PortKind::Data => diameter * 0.5,
            PortKind::Exec => {
                let radius = diameter * 0.2;
                if radius > 1.0 { radius } else { 1.0 }
            }
        }
    }

    const fn uses_filled_surface(self) -> bool {
        matches!(self.direction, PortDirection::Out)
    }

    fn element(self, placement_diameter: f32, theme: OpenGpuiGraphTheme) -> AnyElement {
        let diameter = self.layout_diameter(placement_diameter);
        let corner_radius = px(self.corner_radius(diameter));
        let color = self.state.color(theme.palette);
        let mut visual = div()
            .absolute()
            .inset_0()
            .rounded(corner_radius)
            .border_1()
            .border_color(color);
        visual = if self.uses_filled_surface() {
            visual.bg(color)
        } else {
            visual.bg(theme.palette.node_surface)
        };
        if self.state == OpenGpuiOverviewPortMarkerState::InvalidTarget {
            visual = visual.border_dashed();
        }

        let mut marker = div()
            .relative()
            .w(px(diameter))
            .h(px(diameter))
            .flex_none()
            .child(visual);
        if self.focused {
            marker = marker.child(
                div()
                    .absolute()
                    .inset_0()
                    .rounded(corner_radius)
                    .border_1()
                    .border_color(theme.palette.focus),
            );
        }
        marker.into_any_element()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct NormalizedDetailPortAnchor {
    point: CanvasPoint,
    outward: HandlePosition,
}

#[derive(Clone, Debug)]
pub(crate) struct OpenGpuiSemanticZoomState {
    canonical_origin: CanvasPoint,
    canonical_size: CanvasSize,
    ports: Vec<OpenGpuiOverviewPort>,
    application_overview: Option<AnyView>,
    structural_signature: Option<OpenGpuiSemanticStructureSignature>,
    expected_layout_generation: u64,
    detail_geometry_freshness: Option<OpenGpuiDetailGeometryFreshness>,
    #[cfg(test)]
    structural_rebuilds: u64,
    #[cfg(test)]
    detail_geometry_updates: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct OpenGpuiSemanticStructureSignature {
    instance: NodeInstanceKey,
    binding_revision: GraphRevision,
    catalog_document_epoch: DocumentEpoch,
    canonical_origin: CanvasPoint,
    canonical_size: CanvasSize,
}

impl OpenGpuiSemanticStructureSignature {
    fn matches(
        &self,
        frame: &FrameNode,
        document_epoch: DocumentEpoch,
        canonical_origin: CanvasPoint,
        canonical_size: CanvasSize,
    ) -> bool {
        &self.instance == frame.instance()
            && self.binding_revision == frame.binding_revision()
            && self.catalog_document_epoch == document_epoch
            && self.canonical_origin == canonical_origin
            && self.canonical_size == canonical_size
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OpenGpuiDetailGeometryFreshness {
    layout_revision: u64,
    layout_generation: u64,
}

impl OpenGpuiSemanticZoomState {
    pub(crate) fn from_frame(
        frame: &FrameNode,
        catalog: &NodeCatalog,
        document_epoch: DocumentEpoch,
    ) -> Self {
        let canonical_size = authoritative_size(frame, catalog).unwrap_or(frame.bounds().size);
        let canonical_origin = authoritative_origin(frame, canonical_size);
        Self {
            canonical_origin,
            canonical_size,
            ports: overview_ports(frame, BTreeMap::new()),
            application_overview: None,
            structural_signature: Some(structural_signature(
                frame,
                document_epoch,
                canonical_origin,
                canonical_size,
            )),
            expected_layout_generation: frame.layout_generation(),
            detail_geometry_freshness: None,
            #[cfg(test)]
            structural_rebuilds: 1,
            #[cfg(test)]
            detail_geometry_updates: 0,
        }
    }

    pub(crate) fn reconcile(
        &mut self,
        frame: &FrameNode,
        catalog: &NodeCatalog,
        document_epoch: DocumentEpoch,
    ) {
        if let Some(size) = authoritative_size(frame, catalog) {
            self.canonical_size = size;
        }
        self.canonical_origin = authoritative_origin(frame, self.canonical_size);
        if self.expected_layout_generation != frame.layout_generation() {
            self.expected_layout_generation = frame.layout_generation();
            self.detail_geometry_freshness = None;
        }
        if self.structural_signature.as_ref().is_some_and(|signature| {
            signature.matches(
                frame,
                document_epoch,
                self.canonical_origin,
                self.canonical_size,
            )
        }) && ports_match_frame(&self.ports, frame)
        {
            return;
        }
        let cached = self
            .ports
            .iter()
            .filter_map(|port| port.detail_anchor.map(|anchor| (port.instance, anchor)))
            .collect();
        self.ports = overview_ports(frame, cached);
        self.structural_signature = Some(structural_signature(
            frame,
            document_epoch,
            self.canonical_origin,
            self.canonical_size,
        ));
        self.detail_geometry_freshness = None;
        #[cfg(test)]
        {
            self.structural_rebuilds = self.structural_rebuilds.saturating_add(1);
        }
    }

    pub(crate) fn remember_detail_geometry(&mut self, node: NodeId, geometry: &GeometrySnapshot) {
        let freshness = OpenGpuiDetailGeometryFreshness {
            layout_revision: geometry.layout_revision(),
            layout_generation: self.expected_layout_generation,
        };
        if self.detail_geometry_freshness == Some(freshness) {
            return;
        }
        let Some(node_geometry) = geometry.node(node).filter(|geometry| geometry.is_fresh()) else {
            return;
        };
        let Some(signature) = self.structural_signature.as_ref() else {
            return;
        };
        if node_geometry.lifetime() != signature.instance.lifetime()
            || node_geometry.renderer() != signature.instance.renderer()
            || node_geometry.layout_generation() != self.expected_layout_generation
        {
            return;
        }
        let node_bounds = node_geometry.bounds();
        if !node_bounds.is_positive_finite() {
            return;
        }
        #[cfg(test)]
        let mut changed = false;
        for port in &mut self.ports {
            let Some(port_geometry) = geometry
                .port(port.id())
                .filter(|port_geometry| port_geometry.node() == node)
            else {
                continue;
            };
            let Some(anchor) = normalized_port_anchor(
                node_bounds,
                port_geometry.route_anchor(),
                port_geometry.outward(),
            ) else {
                continue;
            };
            if port.detail_anchor != Some(anchor) {
                port.detail_anchor = Some(anchor);
                #[cfg(test)]
                {
                    changed = true;
                }
            }
        }
        self.detail_geometry_freshness = Some(freshness);
        #[cfg(test)]
        if changed {
            self.detail_geometry_updates = self.detail_geometry_updates.saturating_add(1);
        }
    }

    #[cfg(test)]
    fn remember_detail_ports(
        &mut self,
        node_bounds: CanvasRect,
        measurements: impl IntoIterator<Item = (PortInstanceKey, CanvasPoint, HandlePosition)>,
    ) {
        if !node_bounds.is_positive_finite() {
            return;
        }
        for (port_instance, anchor, outward) in measurements {
            let Some(normalized) = normalized_port_anchor(node_bounds, anchor, outward) else {
                continue;
            };
            if let Some(port) = self
                .ports
                .iter_mut()
                .find(|port| port.instance == port_instance)
            {
                port.detail_anchor = Some(normalized);
            }
        }
    }

    pub(crate) fn canonical_size(&self) -> CanvasSize {
        self.canonical_size
    }

    pub(crate) fn canonical_origin(&self) -> CanvasPoint {
        self.canonical_origin
    }

    pub(crate) fn install_application_overview(&mut self, overview: AnyView) {
        self.application_overview = Some(overview);
    }

    #[cfg(test)]
    pub(crate) fn reconcile_usage(&self) -> (u64, u64) {
        (self.structural_rebuilds, self.detail_geometry_updates)
    }

    pub(crate) fn screen_size(
        &self,
        zoom: f32,
        presentation: OpenGpuiNodePresentation,
        scale_policy: OpenGpuiNodeScalePolicy,
    ) -> CanvasSize {
        let scale = match (presentation, scale_policy) {
            (OpenGpuiNodePresentation::Overview, OpenGpuiNodeScalePolicy::SemanticLod(policy)) => {
                (zoom / policy.overview_enter_zoom()).min(1.0)
            }
            (OpenGpuiNodePresentation::Overview, OpenGpuiNodeScalePolicy::ComponentManaged)
            | (OpenGpuiNodePresentation::Detail, OpenGpuiNodeScalePolicy::ComponentManaged) => zoom,
            (OpenGpuiNodePresentation::Detail, OpenGpuiNodeScalePolicy::SemanticLod(_)) => 1.0,
        };
        CanvasSize {
            width: self.canonical_size.width * scale,
            height: self.canonical_size.height * scale,
        }
    }

    pub(crate) fn overview_element(
        &self,
        binding: Option<&OpenGpuiNodeBinding>,
        accessible_name: &str,
        screen_size: CanvasSize,
        theme: OpenGpuiGraphTheme,
    ) -> AnyElement {
        if let Some(overview) = &self.application_overview {
            return div()
                .debug_selector(|| OPEN_GPUI_OVERVIEW_NODE_SELECTOR.to_owned())
                .relative()
                .size_full()
                .child(overview.clone())
                .into_any_element();
        }

        let background = div()
            .absolute()
            .size_full()
            .overflow_hidden()
            .rounded(px(overview_node_corner_radius(screen_size, theme)))
            .border_1()
            .border_color(overview_node_border(theme))
            .bg(theme.palette.node_surface);
        let mut overview = div()
            .debug_selector(|| OPEN_GPUI_OVERVIEW_NODE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .child(background);

        let input_count = self
            .ports
            .iter()
            .filter(|port| port.direction == PortDirection::In)
            .count();
        let output_count = self.ports.len().saturating_sub(input_count);
        let mut input_index = 0;
        let mut output_index = 0;
        let placements = self
            .ports
            .iter()
            .map(|port| {
                let (index, count) = match port.direction {
                    PortDirection::In => {
                        let index = input_index;
                        input_index += 1;
                        (index, input_count)
                    }
                    PortDirection::Out => {
                        let index = output_index;
                        output_index += 1;
                        (index, output_count)
                    }
                };
                overview_port_placement(port, index, count, screen_size)
            })
            .collect::<Vec<_>>();

        if let Some(title) = overview_title_layout(&placements, screen_size) {
            overview = overview.child(
                div()
                    .absolute()
                    .left(px(title.left))
                    .right(px(title.right))
                    .top(px(title.top))
                    .truncate()
                    .text_size(px(10.0))
                    .text_color(theme.palette.label_text)
                    .child(accessible_name.to_owned()),
            );
        }

        for (port, placement) in self.ports.iter().zip(placements) {
            let Some(handle) = binding
                .and_then(|binding| binding.cached_port_handle(port.id()))
                .filter(|handle| handle.port_instance() == port.instance)
            else {
                continue;
            };
            let handle = handle
                .for_host_overview()
                .with_side(port_side(placement.outward))
                .with_anchor(OpenGpuiPortAnchor::Center)
                .with_minimum_hit_diameter(theme.metrics.port_hit_diameter);
            let marker = OpenGpuiOverviewPortMarkerRecipe::from_port(port, handle.presentation())
                .element(placement.diameter, theme);
            let interactive = handle.interactive(marker);
            overview = overview.child(
                div()
                    .absolute()
                    .left(px(placement.left))
                    .top(px(placement.top))
                    .child(interactive),
            );
        }

        overview.into_any_element()
    }
}

// Selection and focus chrome belongs to the host node wrapper, including in overview mode.
const fn overview_node_border(theme: OpenGpuiGraphTheme) -> Hsla {
    theme.palette.node_border
}

pub(crate) fn overview_node_corner_radius(
    screen_size: CanvasSize,
    theme: OpenGpuiGraphTheme,
) -> f32 {
    let shortest_side = screen_size.width.min(screen_size.height).max(0.0);
    (shortest_side * 0.08)
        .min(theme.metrics.node_corner_radius)
        .min(shortest_side * 0.5)
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OverviewPortPlacement {
    left: f32,
    top: f32,
    diameter: f32,
    outward: HandlePosition,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct OverviewTitleLayout {
    top: f32,
    left: f32,
    right: f32,
}

fn overview_title_layout(
    placements: &[OverviewPortPlacement],
    screen_size: CanvasSize,
) -> Option<OverviewTitleLayout> {
    if screen_size.width < 48.0 || screen_size.height < 18.0 {
        return None;
    }
    let top = placements
        .iter()
        .filter(|placement| placement.outward == HandlePosition::Top)
        .map(|placement| placement.top + placement.diameter + OVERVIEW_TITLE_PORT_GAP)
        .fold(OVERVIEW_TITLE_BASE_TOP, f32::max);
    let title_bottom = top + OVERVIEW_TITLE_LINE_HEIGHT;
    if title_bottom > screen_size.height {
        return None;
    }

    let mut left = OVERVIEW_TITLE_BASE_INSET;
    let mut right = OVERVIEW_TITLE_BASE_INSET;
    for placement in placements {
        let marker_bottom = placement.top + placement.diameter;
        if placement.top >= title_bottom || marker_bottom <= top {
            continue;
        }
        match placement.outward {
            HandlePosition::Left => {
                left = left.max(placement.left + placement.diameter + OVERVIEW_TITLE_PORT_GAP);
            }
            HandlePosition::Right => {
                right = right.max(screen_size.width - placement.left + OVERVIEW_TITLE_PORT_GAP);
            }
            HandlePosition::Top | HandlePosition::Bottom => {}
        }
    }
    (screen_size.width - left - right >= OVERVIEW_TITLE_MIN_WIDTH).then_some(OverviewTitleLayout {
        top,
        left,
        right,
    })
}

fn overview_port_placement(
    port: &OpenGpuiOverviewPort,
    index: usize,
    count: usize,
    screen_size: CanvasSize,
) -> OverviewPortPlacement {
    let count = count.max(1) as f32;
    let slot_height = screen_size.height / (count + 1.0);
    let diameter = MAX_OVERVIEW_MARKER_DIAMETER
        .min(screen_size.width / 4.0)
        .min(slot_height / 2.0);
    let fallback = NormalizedDetailPortAnchor {
        point: CanvasPoint {
            x: match port.direction {
                PortDirection::In => 0.0,
                PortDirection::Out => 1.0,
            },
            y: (index as f32 + 1.0) / (count + 1.0),
        },
        outward: match port.direction {
            PortDirection::In => HandlePosition::Left,
            PortDirection::Out => HandlePosition::Right,
        },
    };
    let anchor = port.detail_anchor.unwrap_or(fallback);
    let center_x = anchor.point.x * screen_size.width;
    let center_y = anchor.point.y * screen_size.height;
    OverviewPortPlacement {
        left: center_x - diameter / 2.0,
        top: center_y - diameter / 2.0,
        diameter,
        outward: anchor.outward,
    }
}

fn normalized_port_anchor(
    node_bounds: CanvasRect,
    anchor: CanvasPoint,
    outward: HandlePosition,
) -> Option<NormalizedDetailPortAnchor> {
    if !anchor.is_finite() || !node_bounds.is_positive_finite() {
        return None;
    }
    Some(NormalizedDetailPortAnchor {
        point: CanvasPoint {
            x: ((anchor.x - node_bounds.origin.x) / node_bounds.size.width).clamp(0.0, 1.0),
            y: ((anchor.y - node_bounds.origin.y) / node_bounds.size.height).clamp(0.0, 1.0),
        },
        outward,
    })
}

const fn port_side(position: HandlePosition) -> OpenGpuiPortSide {
    match position {
        HandlePosition::Top => OpenGpuiPortSide::Top,
        HandlePosition::Right => OpenGpuiPortSide::Right,
        HandlePosition::Bottom => OpenGpuiPortSide::Bottom,
        HandlePosition::Left => OpenGpuiPortSide::Left,
    }
}

fn authoritative_size(frame: &FrameNode, catalog: &NodeCatalog) -> Option<CanvasSize> {
    let node = frame.node()?;
    node.size
        .filter(|size| size.is_positive_finite())
        .or_else(|| catalog_default_size(frame, catalog))
}

fn structural_signature(
    frame: &FrameNode,
    document_epoch: DocumentEpoch,
    canonical_origin: CanvasPoint,
    canonical_size: CanvasSize,
) -> OpenGpuiSemanticStructureSignature {
    OpenGpuiSemanticStructureSignature {
        instance: frame.instance().clone(),
        binding_revision: frame.binding_revision(),
        catalog_document_epoch: document_epoch,
        canonical_origin,
        canonical_size,
    }
}

fn catalog_default_size(frame: &FrameNode, catalog: &NodeCatalog) -> Option<CanvasSize> {
    frame
        .node()
        .and_then(|node| catalog.get(&node.kind))
        .and_then(|spec| spec.default_size())
}

fn ports_match_frame(ports: &[OpenGpuiOverviewPort], frame: &FrameNode) -> bool {
    let mut current = frame.ports().iter().filter_map(|frame_port| {
        frame_port
            .port()
            .map(|port| (frame_port.instance(), port.dir, port.kind))
    });
    ports.iter().all(|cached| {
        current.next().is_some_and(|(instance, direction, kind)| {
            cached.instance == instance && cached.direction == direction && cached.kind == kind
        })
    }) && current.next().is_none()
}

fn authoritative_origin(frame: &FrameNode, size: CanvasSize) -> CanvasPoint {
    let Some(node) = frame.node() else {
        return frame.bounds().origin;
    };
    node.bounds_for_size(size)
        .map(|bounds| bounds.origin)
        .unwrap_or(frame.bounds().origin)
}

fn overview_ports(
    frame: &FrameNode,
    cached: BTreeMap<PortInstanceKey, NormalizedDetailPortAnchor>,
) -> Vec<OpenGpuiOverviewPort> {
    frame
        .ports()
        .iter()
        .filter_map(|frame_port| {
            let port = frame_port.port()?;
            let instance = frame_port.instance();
            Some(OpenGpuiOverviewPort {
                instance,
                direction: port.dir,
                kind: port.kind,
                detail_anchor: cached.get(&instance).copied(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use jellyflow_core::{GraphBuilder, GraphId, Node, NodeKindKey, Port, PortCapacity, PortKey};
    use jellyflow_runtime::{
        commands::GraphCommand, commands::IncidentEdgePolicy, commands::NodeGeometryEdit,
        commands::TopologyEdit, commands::TopologyPort, graph::GraphRuntime, view::FrameRequest,
        view::RendererStamp, view::ViewSession, view::ViewSessionOptions,
    };

    use super::*;

    const TEST_KIND: &str = "test.semantic-zoom";
    const TEST_RENDERER: &str = "test.semantic-zoom-renderer";

    fn test_node_id() -> NodeId {
        NodeId::from_u128(0x5e01)
    }

    fn semantic_runtime(
        ports: &[(PortId, PortDirection, PortKind)],
    ) -> (GraphRuntime, ViewSession) {
        let mut graph = GraphBuilder::new(GraphId::from_u128(0x5e00));
        graph.insert_node(
            test_node_id(),
            Node {
                kind: NodeKindKey::new(TEST_KIND),
                kind_version: 1,
                pos: CanvasPoint { x: 40.0, y: 20.0 },
                origin: None,
                selectable: None,
                draggable: None,
                connectable: None,
                deletable: None,
                parent: None,
                extent: None,
                expand_parent: None,
                size: Some(CanvasSize {
                    width: 220.0,
                    height: 120.0,
                }),
                hidden: false,
                collapsed: false,
                ports: ports.iter().map(|(id, _, _)| *id).collect(),
                data: serde_json::Value::Null,
            },
        );
        for (index, (id, direction, kind)) in ports.iter().copied().enumerate() {
            graph.insert_port(id, test_port(format!("port-{index}"), direction, kind));
        }
        let runtime = GraphRuntime::builder(NodeCatalog::default())
            .build(graph.build().expect("valid semantic zoom graph"))
            .expect("semantic zoom runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("semantic zoom view");
        (runtime, view)
    }

    fn test_port(key: impl Into<String>, direction: PortDirection, kind: PortKind) -> Port {
        Port {
            node: test_node_id(),
            key: PortKey::new(key),
            dir: direction,
            kind,
            capacity: PortCapacity::Single,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        }
    }

    fn frame_request() -> FrameRequest {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: -100.0,
                    y: -100.0,
                },
                size: CanvasSize {
                    width: 600.0,
                    height: 400.0,
                },
            },
            RendererStamp::new(TEST_RENDERER, 1),
        )
    }

    fn exact_overview_ports(
        ports: &[(PortId, PortDirection, PortKind)],
    ) -> Vec<OpenGpuiOverviewPort> {
        let (_runtime, view) = semantic_runtime(ports);
        ports
            .iter()
            .map(|(id, direction, kind)| OpenGpuiOverviewPort {
                instance: view
                    .port_instance_key(*id)
                    .expect("port identity query")
                    .expect("current port identity"),
                direction: *direction,
                kind: *kind,
                detail_anchor: None,
            })
            .collect()
    }

    fn semantic_state(
        canonical_origin: CanvasPoint,
        canonical_size: CanvasSize,
        ports: Vec<OpenGpuiOverviewPort>,
    ) -> OpenGpuiSemanticZoomState {
        OpenGpuiSemanticZoomState {
            canonical_origin,
            canonical_size,
            ports,
            application_overview: None,
            structural_signature: None,
            expected_layout_generation: 0,
            detail_geometry_freshness: None,
            structural_rebuilds: 0,
            detail_geometry_updates: 0,
        }
    }

    #[test]
    fn semantic_detail_stays_native_and_overview_scales_below_its_entry_threshold() {
        let state = semantic_state(
            CanvasPoint::default(),
            CanvasSize {
                width: 220.0,
                height: 120.0,
            },
            Vec::new(),
        );

        assert_eq!(
            state.screen_size(
                0.5,
                OpenGpuiNodePresentation::Overview,
                OpenGpuiNodeScalePolicy::default(),
            ),
            CanvasSize {
                width: 129.41176,
                height: 70.588234,
            },
        );

        assert_eq!(
            state.screen_size(
                0.5,
                OpenGpuiNodePresentation::Detail,
                OpenGpuiNodeScalePolicy::default(),
            ),
            CanvasSize {
                width: 220.0,
                height: 120.0,
            },
        );

        assert_eq!(
            state.screen_size(
                0.5,
                OpenGpuiNodePresentation::Detail,
                OpenGpuiNodeScalePolicy::ComponentManaged,
            ),
            CanvasSize {
                width: 110.0,
                height: 60.0,
            },
        );
    }

    #[test]
    fn overview_ports_are_ordered_inside_the_node_edges() {
        let size = CanvasSize {
            width: 11.0,
            height: 6.0,
        };
        let ports = exact_overview_ports(&[
            (PortId::from_u128(1), PortDirection::In, PortKind::Data),
            (PortId::from_u128(2), PortDirection::Out, PortKind::Data),
        ]);
        let input = &ports[0];
        let output_port = &ports[1];
        let first = overview_port_placement(input, 0, 2, size);
        let second = overview_port_placement(input, 1, 2, size);
        let output = overview_port_placement(output_port, 0, 1, size);

        assert!(first.diameter > 0.0);
        assert!(second.top > first.top);
        assert_eq!(first.left + first.diameter / 2.0, 0.0);
        assert_eq!(output.left + output.diameter / 2.0, size.width);
    }

    #[test]
    fn overview_title_reserves_top_and_header_side_port_gutters() {
        let size = CanvasSize {
            width: 100.0,
            height: 50.0,
        };
        let top = OverviewPortPlacement {
            left: 20.0,
            top: -3.0,
            diameter: 6.0,
            outward: HandlePosition::Top,
        };
        let left = OverviewPortPlacement {
            left: -3.0,
            top: 20.0,
            diameter: 6.0,
            outward: HandlePosition::Left,
        };
        let header_left = OverviewPortPlacement { top: 5.0, ..left };
        let header_right = OverviewPortPlacement {
            left: 97.0,
            top: 5.0,
            outward: HandlePosition::Right,
            ..left
        };

        assert_eq!(
            overview_title_layout(&[left], size),
            Some(OverviewTitleLayout {
                top: 3.0,
                left: 5.0,
                right: 5.0,
            })
        );
        assert_eq!(overview_title_layout(&[top], size).unwrap().top, 7.0);
        assert_eq!(
            overview_title_layout(&[header_left], size).unwrap().left,
            7.0
        );
        assert_eq!(
            overview_title_layout(&[header_right], size).unwrap().right,
            7.0
        );
    }

    #[test]
    fn overview_title_is_omitted_when_port_gutters_exhaust_it() {
        let size = CanvasSize {
            width: 48.0,
            height: 24.0,
        };
        let left = OverviewPortPlacement {
            left: -12.0,
            top: 3.0,
            diameter: 24.0,
            outward: HandlePosition::Left,
        };
        let right = OverviewPortPlacement {
            left: 36.0,
            outward: HandlePosition::Right,
            ..left
        };

        assert!(overview_title_layout(&[left, right], size).is_none());
    }

    #[test]
    fn overview_corner_radius_is_theme_bounded_and_size_safe() {
        let mut theme = OpenGpuiGraphTheme::default();
        theme.metrics.node_corner_radius = 6.0;

        assert_eq!(
            overview_node_corner_radius(
                CanvasSize {
                    width: 100.0,
                    height: 50.0,
                },
                theme,
            ),
            4.0
        );
        assert_eq!(
            overview_node_corner_radius(
                CanvasSize {
                    width: 10.0,
                    height: 4.0,
                },
                theme,
            ),
            0.32
        );
    }

    #[test]
    fn semantic_lod_footprint_is_continuous_across_default_entry_threshold() {
        let state = semantic_state(
            CanvasPoint::default(),
            CanvasSize {
                width: 220.0,
                height: 120.0,
            },
            Vec::new(),
        );
        let policy = OpenGpuiNodeScalePolicy::default();
        let detail = state.screen_size(0.86, OpenGpuiNodePresentation::Detail, policy);
        let threshold = state.screen_size(0.85, OpenGpuiNodePresentation::Overview, policy);
        let below = state.screen_size(0.84, OpenGpuiNodePresentation::Overview, policy);

        assert_eq!(detail, threshold);
        assert!((below.width / threshold.width - 0.84 / 0.85).abs() < 0.000_01);
        assert!((below.height / threshold.height - 0.84 / 0.85).abs() < 0.000_01);
    }

    #[test]
    fn overview_surface_leaves_selection_chrome_to_the_host_across_zoom_boundaries() {
        let state = semantic_state(
            CanvasPoint::default(),
            CanvasSize {
                width: 220.0,
                height: 120.0,
            },
            Vec::new(),
        );
        let theme = OpenGpuiGraphTheme::default();
        let policy = OpenGpuiNodeScalePolicy::default();

        for (zoom, presentation) in [
            (0.84, OpenGpuiNodePresentation::Overview),
            (0.85, OpenGpuiNodePresentation::Overview),
            (1.0, OpenGpuiNodePresentation::Detail),
        ] {
            assert!(
                state
                    .screen_size(zoom, presentation, policy)
                    .is_positive_finite()
            );
            assert_eq!(overview_node_border(theme), theme.palette.node_border);
        }
    }

    #[test]
    fn detail_port_anchor_survives_overview_round_trip() {
        let port_id = PortId::from_u128(7);
        let ports = exact_overview_ports(&[(port_id, PortDirection::In, PortKind::Data)]);
        let port_instance = ports[0].instance;
        let mut state = semantic_state(
            CanvasPoint { x: 40.0, y: 20.0 },
            CanvasSize {
                width: 220.0,
                height: 120.0,
            },
            ports,
        );
        state.remember_detail_ports(
            CanvasRect {
                origin: CanvasPoint { x: 40.0, y: 20.0 },
                size: state.canonical_size,
            },
            [(
                port_instance,
                CanvasPoint { x: 40.0, y: 70.4 },
                HandlePosition::Left,
            )],
        );
        let policy = OpenGpuiNodeScalePolicy::default();
        let at_threshold = state.screen_size(0.85, OpenGpuiNodePresentation::Overview, policy);
        let below_threshold = state.screen_size(0.84, OpenGpuiNodePresentation::Overview, policy);
        let first = overview_port_placement(&state.ports[0], 0, 1, at_threshold);
        let below = overview_port_placement(&state.ports[0], 0, 1, below_threshold);
        let round_trip = overview_port_placement(&state.ports[0], 0, 1, at_threshold);

        assert!((first.left + first.diameter / 2.0).abs() < 0.000_01);
        assert!((first.top + first.diameter / 2.0 - 50.4).abs() < 0.000_01);
        assert!((below.top + below.diameter / 2.0 - 50.4 * 0.84 / 0.85).abs() < 0.000_01);
        assert_eq!(first, round_trip);
        assert_eq!(first.outward, HandlePosition::Left);
    }

    #[test]
    fn same_port_instance_preserves_detail_anchor_across_lod_and_successor_frames() {
        let port_id = PortId::from_u128(0x5e10);
        let (runtime, view) = semantic_runtime(&[(port_id, PortDirection::Out, PortKind::Data)]);
        let initial_frame = view.frame(frame_request()).expect("initial semantic frame");
        let initial_node = &initial_frame.nodes()[0];
        let port_instance = initial_node.ports()[0].instance();
        let mut state = OpenGpuiSemanticZoomState::from_frame(
            initial_node,
            view.catalog(),
            initial_frame.document_epoch(),
        );
        state.remember_detail_ports(
            initial_node.bounds(),
            [(
                port_instance,
                CanvasPoint { x: 260.0, y: 92.0 },
                HandlePosition::Right,
            )],
        );
        let remembered = state.ports[0]
            .detail_anchor
            .expect("detail anchor is remembered");

        runtime
            .execute(GraphCommand::set_node_geometry(
                initial_frame.graph_version(),
                [NodeGeometryEdit::new(test_node_id())
                    .with_position(CanvasPoint { x: 60.0, y: 40.0 })],
            ))
            .into_result()
            .expect("geometry command")
            .applied()
            .expect("geometry change applies");
        let successor_frame = view
            .frame(frame_request())
            .expect("successor semantic frame");
        let successor_node = &successor_frame.nodes()[0];

        assert_eq!(successor_node.ports()[0].instance(), port_instance);
        state.reconcile(
            successor_node,
            view.catalog(),
            successor_frame.document_epoch(),
        );

        assert_eq!(state.ports[0].instance, port_instance);
        assert_eq!(state.ports[0].detail_anchor, Some(remembered));
        let policy = OpenGpuiNodeScalePolicy::default();
        let detail = overview_port_placement(
            &state.ports[0],
            0,
            1,
            state.screen_size(0.85, OpenGpuiNodePresentation::Overview, policy),
        );
        let overview = overview_port_placement(
            &state.ports[0],
            0,
            1,
            state.screen_size(0.5, OpenGpuiNodePresentation::Overview, policy),
        );
        assert_eq!(detail.outward, HandlePosition::Right);
        assert_eq!(overview.outward, HandlePosition::Right);
        assert!(overview.left < detail.left);
    }

    #[test]
    fn same_id_port_replacement_does_not_inherit_retired_detail_anchor() {
        let port_id = PortId::from_u128(0x5e20);
        let (runtime, view) = semantic_runtime(&[(port_id, PortDirection::Out, PortKind::Data)]);
        let initial_frame = view.frame(frame_request()).expect("initial semantic frame");
        let initial_node = &initial_frame.nodes()[0];
        let retired_instance = initial_node.ports()[0].instance();
        let mut state = OpenGpuiSemanticZoomState::from_frame(
            initial_node,
            view.catalog(),
            initial_frame.document_epoch(),
        );
        state.remember_detail_ports(
            initial_node.bounds(),
            [(
                retired_instance,
                CanvasPoint { x: 260.0, y: 92.0 },
                HandlePosition::Right,
            )],
        );
        assert!(state.ports[0].detail_anchor.is_some());

        runtime
            .execute(GraphCommand::topology(
                initial_frame.graph_version(),
                TopologyEdit::new().replace_node_ports(
                    test_node_id(),
                    [TopologyPort::new(
                        port_id,
                        test_port("replacement-port", PortDirection::Out, PortKind::Data),
                    )],
                    IncidentEdgePolicy::Reject,
                ),
            ))
            .into_result()
            .expect("same-ID port replacement command")
            .applied()
            .expect("same-ID port replacement applies");
        let replacement_frame = view
            .frame(frame_request())
            .expect("replacement semantic frame");
        let replacement_node = &replacement_frame.nodes()[0];
        let replacement_instance = replacement_node.ports()[0].instance();

        assert_ne!(replacement_instance, retired_instance);
        state.reconcile(
            replacement_node,
            view.catalog(),
            replacement_frame.document_epoch(),
        );

        assert_eq!(state.ports[0].instance, replacement_instance);
        assert_eq!(state.ports[0].id(), port_id);
        assert_eq!(state.ports[0].direction, PortDirection::Out);
        assert_eq!(state.ports[0].kind, PortKind::Data);
        assert_eq!(state.ports[0].detail_anchor, None);
    }

    #[test]
    fn overview_marker_recipe_encodes_port_kind_and_direction() {
        let ports = exact_overview_ports(&[
            (PortId::from_u128(11), PortDirection::Out, PortKind::Data),
            (PortId::from_u128(12), PortDirection::In, PortKind::Exec),
        ]);
        let data_output = &ports[0];
        let exec_input = &ports[1];

        let data_recipe = OpenGpuiOverviewPortMarkerRecipe::from_port(
            data_output,
            &OpenGpuiPortInteractionState::idle(),
        );
        let exec_recipe = OpenGpuiOverviewPortMarkerRecipe::from_port(
            exec_input,
            &OpenGpuiPortInteractionState::idle(),
        );

        assert!(data_recipe.uses_filled_surface());
        assert!(!exec_recipe.uses_filled_surface());
        assert_eq!(data_recipe.corner_radius(6.0), 3.0);
        assert_eq!(exec_recipe.corner_radius(6.0), 1.2);
    }

    #[test]
    fn overview_marker_recipe_keeps_port_state_priority_and_placement_stable() {
        use crate::presentation::OpenGpuiPortCandidateState;

        let theme = OpenGpuiGraphTheme::default();
        let port =
            exact_overview_ports(&[(PortId::from_u128(13), PortDirection::Out, PortKind::Exec)])
                .remove(0);
        let placement = overview_port_placement(
            &port,
            0,
            1,
            CanvasSize {
                width: 96.0,
                height: 48.0,
            },
        );
        let cases = [
            (
                OpenGpuiPortInteractionState::idle(),
                OpenGpuiOverviewPortMarkerState::Idle,
                theme.palette.port_idle,
            ),
            (
                OpenGpuiPortInteractionState::idle().with_connected(true),
                OpenGpuiOverviewPortMarkerState::Connected,
                theme.palette.port_connected,
            ),
            (
                OpenGpuiPortInteractionState::idle()
                    .with_connected(true)
                    .with_hovered(true),
                OpenGpuiOverviewPortMarkerState::Hovered,
                theme.palette.port_hovered,
            ),
            (
                OpenGpuiPortInteractionState::idle()
                    .with_hovered(true)
                    .with_candidate(OpenGpuiPortCandidateState::Source),
                OpenGpuiOverviewPortMarkerState::Source,
                theme.palette.port_source,
            ),
            (
                OpenGpuiPortInteractionState::idle()
                    .with_candidate(OpenGpuiPortCandidateState::ValidTarget),
                OpenGpuiOverviewPortMarkerState::ValidTarget,
                theme.palette.port_valid,
            ),
            (
                OpenGpuiPortInteractionState::idle()
                    .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::new())),
                OpenGpuiOverviewPortMarkerState::InvalidTarget,
                theme.palette.port_invalid,
            ),
            (
                OpenGpuiPortInteractionState::idle()
                    .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::new()))
                    .with_disabled(true),
                OpenGpuiOverviewPortMarkerState::Disabled,
                theme.palette.port_disabled,
            ),
        ];

        for (presentation, state, color) in cases {
            let recipe = OpenGpuiOverviewPortMarkerRecipe::from_port(&port, &presentation);
            assert_eq!(recipe.kind, PortKind::Exec);
            assert_eq!(recipe.direction, PortDirection::Out);
            assert_eq!(recipe.state, state);
            assert_eq!(recipe.state.color(theme.palette), color);
            assert_eq!(
                recipe.layout_diameter(placement.diameter),
                placement.diameter
            );
        }
        assert!(
            (OpenGpuiOverviewPortMarkerRecipe::from_port(
                &port,
                &OpenGpuiPortInteractionState::idle(),
            )
            .corner_radius(placement.diameter)
                - placement.diameter * 0.2)
                .abs()
                < f32::EPSILON
        );
    }
}
