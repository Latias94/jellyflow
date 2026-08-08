use std::collections::BTreeMap;

use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId, PortKind, TypeDesc,
};
use jellyflow_runtime::geometry::{HandlePosition, ViewSize};
#[cfg(test)]
use jellyflow_runtime::schema::NodeCatalog;
use jellyflow_runtime::{
    graph::DocumentEpoch, graph::GraphRevision, graph::PortInstanceKey, view::FrameNode,
    view::GeometrySnapshot, view::NodeInstanceKey,
};
use open_gpui::{
    AnyElement, AnyView, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div, px,
};

use crate::binding::OpenGpuiNodeBinding;
use crate::chrome::{OpenGpuiEdgePainterRegistry, OpenGpuiPortTone, render_port_marker_element};
use crate::port_handle::{OpenGpuiPortAnchor, OpenGpuiPortSide};
#[cfg(test)]
use crate::presentation::OpenGpuiPortInteractionState;
use crate::scale_policy::{OpenGpuiNodeScaleBand, OpenGpuiNodeScalePolicy, OpenGpuiNodeScaleState};
use crate::theme::OpenGpuiGraphTheme;

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
    ty: Option<TypeDesc>,
    detail_anchor: Option<NormalizedDetailPortAnchor>,
}

impl OpenGpuiOverviewPort {
    fn id(&self) -> PortId {
        self.instance.port()
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
    pub(crate) fn from_frame(frame: &FrameNode, document_epoch: DocumentEpoch) -> Self {
        let canonical_size = authoritative_size(frame).unwrap_or(frame.bounds().size);
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
        }
    }

    pub(crate) fn reconcile(&mut self, frame: &FrameNode, document_epoch: DocumentEpoch) {
        if let Some(size) = authoritative_size(frame) {
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
            }
        }
        self.detail_geometry_freshness = Some(freshness);
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

    pub(crate) fn expected_layout_generation(&self) -> u64 {
        self.expected_layout_generation
    }

    pub(crate) fn install_application_overview(&mut self, overview: AnyView) {
        self.application_overview = Some(overview);
    }

    pub(crate) fn view_size(
        &self,
        zoom: f32,
        presentation: OpenGpuiNodePresentation,
        scale_policy: OpenGpuiNodeScalePolicy,
    ) -> ViewSize {
        let scale = match (presentation, scale_policy) {
            (OpenGpuiNodePresentation::Overview, OpenGpuiNodeScalePolicy::SemanticLod(policy)) => {
                (zoom / policy.overview_enter_zoom()).min(1.0)
            }
            (OpenGpuiNodePresentation::Overview, OpenGpuiNodeScalePolicy::ComponentManaged)
            | (OpenGpuiNodePresentation::Detail, OpenGpuiNodeScalePolicy::ComponentManaged) => zoom,
            (OpenGpuiNodePresentation::Detail, OpenGpuiNodeScalePolicy::SemanticLod(_)) => 1.0,
        };
        ViewSize::new(
            self.canonical_size.width * scale,
            self.canonical_size.height * scale,
        )
    }

    pub(crate) fn overview_element(
        &self,
        binding: Option<&OpenGpuiNodeBinding>,
        accessible_name: &str,
        view_size: ViewSize,
        theme: OpenGpuiGraphTheme,
        edge_painters: &OpenGpuiEdgePainterRegistry,
        interaction_enabled: bool,
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
            .rounded(px(overview_node_corner_radius(view_size, theme)))
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
                overview_port_placement(port, index, count, view_size)
            })
            .collect::<Vec<_>>();

        if let Some(title) = overview_title_layout(&placements, view_size) {
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
                .with_interaction_enabled(interaction_enabled)
                .with_side(port_side(placement.outward))
                .with_anchor(OpenGpuiPortAnchor::Center)
                .with_minimum_hit_diameter(theme.metrics.port_hit_diameter);
            let style_slot = port
                .ty
                .as_ref()
                .and_then(|ty| edge_painters.style_slot_for_type(ty));
            let style = theme.resolve_port_marker_style_slot(
                handle.presentation(),
                OpenGpuiPortTone::from(port.kind),
                style_slot,
            );
            let marker = render_port_marker_element(style, placement.diameter, placement.diameter);
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

pub(crate) fn overview_node_corner_radius(view_size: ViewSize, theme: OpenGpuiGraphTheme) -> f32 {
    let shortest_side = view_size.width.min(view_size.height).max(0.0);
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
    view_size: ViewSize,
) -> Option<OverviewTitleLayout> {
    if view_size.width < 48.0 || view_size.height < 18.0 {
        return None;
    }
    let top = placements
        .iter()
        .filter(|placement| placement.outward == HandlePosition::Top)
        .map(|placement| placement.top + placement.diameter + OVERVIEW_TITLE_PORT_GAP)
        .fold(OVERVIEW_TITLE_BASE_TOP, f32::max);
    let title_bottom = top + OVERVIEW_TITLE_LINE_HEIGHT;
    if title_bottom > view_size.height {
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
                right = right.max(view_size.width - placement.left + OVERVIEW_TITLE_PORT_GAP);
            }
            HandlePosition::Top | HandlePosition::Bottom => {}
        }
    }
    (view_size.width - left - right >= OVERVIEW_TITLE_MIN_WIDTH).then_some(OverviewTitleLayout {
        top,
        left,
        right,
    })
}

fn overview_port_placement(
    port: &OpenGpuiOverviewPort,
    index: usize,
    count: usize,
    view_size: ViewSize,
) -> OverviewPortPlacement {
    let count = count.max(1) as f32;
    let slot_height = view_size.height / (count + 1.0);
    let diameter = MAX_OVERVIEW_MARKER_DIAMETER
        .min(view_size.width / 4.0)
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
    let center_x = anchor.point.x * view_size.width;
    let center_y = anchor.point.y * view_size.height;
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

fn authoritative_size(frame: &FrameNode) -> Option<CanvasSize> {
    let node = frame.node()?;
    node.size.filter(|size| size.is_positive_finite())
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

fn ports_match_frame(ports: &[OpenGpuiOverviewPort], frame: &FrameNode) -> bool {
    let mut current = frame.ports().iter().filter_map(|frame_port| {
        frame_port
            .port()
            .map(|port| (frame_port.instance(), port.dir, port.kind, port.ty.as_ref()))
    });
    ports.iter().all(|cached| {
        current
            .next()
            .is_some_and(|(instance, direction, kind, ty)| {
                cached.instance == instance
                    && cached.direction == direction
                    && cached.kind == kind
                    && cached.ty.as_ref() == ty
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
                ty: port.ty.clone(),
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
                ty: None,
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
            state.view_size(
                0.5,
                OpenGpuiNodePresentation::Overview,
                OpenGpuiNodeScalePolicy::default(),
            ),
            ViewSize::new(129.41176, 70.588234),
        );

        assert_eq!(
            state.view_size(
                0.5,
                OpenGpuiNodePresentation::Detail,
                OpenGpuiNodeScalePolicy::default(),
            ),
            ViewSize::new(220.0, 120.0),
        );

        assert_eq!(
            state.view_size(
                0.5,
                OpenGpuiNodePresentation::Detail,
                OpenGpuiNodeScalePolicy::ComponentManaged,
            ),
            ViewSize::new(110.0, 60.0),
        );
    }

    #[test]
    fn overview_ports_are_ordered_inside_the_node_edges() {
        let size = ViewSize::new(11.0, 6.0);
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
        let size = ViewSize::new(100.0, 50.0);
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
        let size = ViewSize::new(48.0, 24.0);
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
            overview_node_corner_radius(ViewSize::new(100.0, 50.0), theme,),
            4.0
        );
        assert_eq!(
            overview_node_corner_radius(ViewSize::new(10.0, 4.0), theme,),
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
        let detail = state.view_size(0.86, OpenGpuiNodePresentation::Detail, policy);
        let threshold = state.view_size(0.85, OpenGpuiNodePresentation::Overview, policy);
        let below = state.view_size(0.84, OpenGpuiNodePresentation::Overview, policy);

        assert_eq!(detail, threshold);
        assert!((below.width / threshold.width - 0.84 / 0.85).abs() < 0.000_01);
        assert!((below.height / threshold.height - 0.84 / 0.85).abs() < 0.000_01);
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
        let at_threshold = state.view_size(0.85, OpenGpuiNodePresentation::Overview, policy);
        let below_threshold = state.view_size(0.84, OpenGpuiNodePresentation::Overview, policy);
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
        let mut state =
            OpenGpuiSemanticZoomState::from_frame(initial_node, initial_frame.document_epoch());
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
        state.reconcile(successor_node, successor_frame.document_epoch());

        assert_eq!(state.ports[0].instance, port_instance);
        assert_eq!(state.ports[0].detail_anchor, Some(remembered));
        let policy = OpenGpuiNodeScalePolicy::default();
        let detail = overview_port_placement(
            &state.ports[0],
            0,
            1,
            state.view_size(0.85, OpenGpuiNodePresentation::Overview, policy),
        );
        let overview = overview_port_placement(
            &state.ports[0],
            0,
            1,
            state.view_size(0.5, OpenGpuiNodePresentation::Overview, policy),
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
        let mut state =
            OpenGpuiSemanticZoomState::from_frame(initial_node, initial_frame.document_epoch());
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
        state.reconcile(replacement_node, replacement_frame.document_epoch());

        assert_eq!(state.ports[0].instance, replacement_instance);
        assert_eq!(state.ports[0].id(), port_id);
        assert_eq!(state.ports[0].direction, PortDirection::Out);
        assert_eq!(state.ports[0].kind, PortKind::Data);
        assert_eq!(state.ports[0].detail_anchor, None);
    }

    #[test]
    fn overview_ports_reuse_detail_marker_state_resolution_without_moving_placement() {
        use crate::presentation::OpenGpuiPortCandidateState;
        use crate::theme::{
            OpenGpuiGraphStyleCue, OpenGpuiGraphStyleRecipe, OpenGpuiGraphStyleSlot,
        };

        let slot = OpenGpuiGraphStyleSlot::new(3).expect("bounded test style slot");
        let typed_color: Hsla = open_gpui::rgb(0xe5b95c).into();
        let theme = OpenGpuiGraphTheme::default().with_style_slot(
            slot,
            OpenGpuiGraphStyleRecipe::new(typed_color).with_cue(OpenGpuiGraphStyleCue::Dashed),
        );
        let mut painters = OpenGpuiEdgePainterRegistry::builder();
        painters
            .register_type_style(TypeDesc::Float, slot)
            .expect("exact overview type style");
        let painters = painters.build();
        let mut port =
            exact_overview_ports(&[(PortId::from_u128(13), PortDirection::Out, PortKind::Exec)])
                .remove(0);
        port.ty = Some(TypeDesc::Float);
        let style_slot = port
            .ty
            .as_ref()
            .and_then(|ty| painters.style_slot_for_type(ty));
        let placement = overview_port_placement(&port, 0, 1, ViewSize::new(96.0, 48.0));
        let idle = theme.resolve_port_marker_style_slot(
            &OpenGpuiPortInteractionState::idle(),
            OpenGpuiPortTone::from(port.kind),
            style_slot,
        );
        let connected = theme.resolve_port_marker_style_slot(
            &OpenGpuiPortInteractionState::idle().with_connected(true),
            OpenGpuiPortTone::from(port.kind),
            style_slot,
        );
        let invalid = theme.resolve_port_marker_style_slot(
            &OpenGpuiPortInteractionState::idle()
                .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::new())),
            OpenGpuiPortTone::from(port.kind),
            style_slot,
        );
        let disabled = theme.resolve_port_marker_style_slot(
            &OpenGpuiPortInteractionState::idle()
                .with_candidate(OpenGpuiPortCandidateState::invalid(Vec::new()))
                .with_disabled(true),
            OpenGpuiPortTone::from(port.kind),
            style_slot,
        );

        assert_eq!(placement.diameter, 6.0);
        assert_eq!(idle.shape(), theme.port_marker_shapes.control);
        assert_eq!(idle.border_color(), typed_color);
        assert_eq!(idle.cue(), crate::chrome::OpenGpuiPortMarkerCue::StyleBar);
        assert_ne!(idle.fill_color(), connected.fill_color());
        assert!(invalid.has_dashed_border());
        assert!(disabled.has_dashed_border());
        assert_eq!(disabled.border_color(), theme.palette.port_invalid);

        let mut input =
            exact_overview_ports(&[(PortId::from_u128(14), PortDirection::In, PortKind::Exec)])
                .remove(0);
        input.ty = Some(TypeDesc::Float);
        let input_style = theme.resolve_port_marker_style_slot(
            &OpenGpuiPortInteractionState::idle(),
            OpenGpuiPortTone::from(input.kind),
            input
                .ty
                .as_ref()
                .and_then(|ty| painters.style_slot_for_type(ty)),
        );
        assert_eq!(
            input_style, idle,
            "direction must not change connection appearance"
        );
    }
}
