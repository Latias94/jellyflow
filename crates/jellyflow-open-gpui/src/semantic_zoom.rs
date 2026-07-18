use jellyflow::core::{CanvasPoint, CanvasSize, PortDirection, PortId};
use jellyflow::runtime::FrameNode;
use jellyflow::runtime::schema::NodeCatalog;
use open_gpui::{AnyElement, InteractiveElement, IntoElement, ParentElement, Styled, div, px};

use crate::binding::OpenGpuiNodeBinding;
use crate::scale_policy::{OpenGpuiNodeScaleBand, OpenGpuiNodeScalePolicy, OpenGpuiNodeScaleState};
use crate::theme::OpenGpuiGraphTheme;

pub(crate) const OPEN_GPUI_OVERVIEW_NODE_SELECTOR: &str = "jellyflow-overview-node";
const MAX_OVERVIEW_MARKER_DIAMETER: f32 = 6.0;

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
    id: PortId,
    direction: PortDirection,
}

#[derive(Clone, Debug)]
pub(crate) struct OpenGpuiSemanticZoomState {
    canonical_origin: CanvasPoint,
    canonical_size: CanvasSize,
    ports: Vec<OpenGpuiOverviewPort>,
}

impl OpenGpuiSemanticZoomState {
    pub(crate) fn from_frame(frame: &FrameNode, catalog: &NodeCatalog) -> Self {
        let canonical_size = authoritative_size(frame, catalog).unwrap_or(frame.bounds().size);
        Self {
            canonical_origin: authoritative_origin(frame, canonical_size),
            canonical_size,
            ports: overview_ports(frame),
        }
    }

    pub(crate) fn reconcile(&mut self, frame: &FrameNode, catalog: &NodeCatalog) {
        if let Some(size) = authoritative_size(frame, catalog) {
            self.canonical_size = size;
        }
        self.canonical_origin = authoritative_origin(frame, self.canonical_size);
        self.ports = overview_ports(frame);
    }

    pub(crate) fn canonical_size(&self) -> CanvasSize {
        self.canonical_size
    }

    pub(crate) fn canonical_origin(&self) -> CanvasPoint {
        self.canonical_origin
    }

    pub(crate) fn screen_size(
        &self,
        zoom: f32,
        presentation: OpenGpuiNodePresentation,
        scale_policy: OpenGpuiNodeScalePolicy,
    ) -> CanvasSize {
        let scale = match (presentation, scale_policy) {
            (OpenGpuiNodePresentation::Overview, _)
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
        selected: bool,
        screen_size: CanvasSize,
        theme: OpenGpuiGraphTheme,
    ) -> AnyElement {
        let background = div()
            .absolute()
            .size_full()
            .overflow_hidden()
            .rounded(px(
                (screen_size.width.min(screen_size.height) * 0.08).min(6.0)
            ))
            .border_1()
            .border_color(if selected {
                theme.palette.selection
            } else {
                theme.palette.node_border
            })
            .bg(theme.palette.node_surface);
        let mut overview = div()
            .debug_selector(|| OPEN_GPUI_OVERVIEW_NODE_SELECTOR.to_owned())
            .relative()
            .size_full()
            .child(background);

        if screen_size.width >= 48.0 && screen_size.height >= 18.0 {
            overview = overview.child(
                div()
                    .absolute()
                    .left(px(5.0))
                    .right(px(5.0))
                    .top(px(3.0))
                    .truncate()
                    .text_size(px(10.0))
                    .text_color(theme.palette.label_text)
                    .child(accessible_name.to_owned()),
            );
        }

        let input_count = self
            .ports
            .iter()
            .filter(|port| port.direction == PortDirection::In)
            .count();
        let output_count = self.ports.len().saturating_sub(input_count);
        let mut input_index = 0;
        let mut output_index = 0;

        for port in &self.ports {
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
            let placement = overview_port_placement(port.direction, index, count, screen_size);
            let Some(handle) = binding.and_then(|binding| binding.cached_port_handle(port.id))
            else {
                continue;
            };
            let handle = handle
                .for_host_overview()
                .with_minimum_hit_diameter(theme.metrics.port_hit_diameter);
            let marker_color = match port.direction {
                PortDirection::In => theme.palette.edge_data,
                PortDirection::Out => theme.palette.edge_control,
            };
            let marker = div()
                .w(px(placement.diameter))
                .h(px(placement.diameter))
                .rounded_full()
                .bg(marker_color);
            let interactive = handle.interactive(marker, |_, _, _| {});
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

#[derive(Clone, Copy, Debug, PartialEq)]
struct OverviewPortPlacement {
    left: f32,
    top: f32,
    diameter: f32,
}

fn overview_port_placement(
    direction: PortDirection,
    index: usize,
    count: usize,
    screen_size: CanvasSize,
) -> OverviewPortPlacement {
    let count = count.max(1) as f32;
    let slot_height = screen_size.height / (count + 1.0);
    let diameter = MAX_OVERVIEW_MARKER_DIAMETER
        .min(screen_size.width / 4.0)
        .min(slot_height / 2.0);
    let center_y = slot_height * (index as f32 + 1.0);
    OverviewPortPlacement {
        left: match direction {
            PortDirection::In => 0.0,
            PortDirection::Out => screen_size.width - diameter,
        },
        top: center_y - diameter / 2.0,
        diameter,
    }
}

fn authoritative_size(frame: &FrameNode, catalog: &NodeCatalog) -> Option<CanvasSize> {
    let node = frame.node()?;
    node.size
        .filter(|size| size.is_positive_finite())
        .or_else(|| catalog.get(&node.kind).and_then(|spec| spec.default_size()))
}

fn authoritative_origin(frame: &FrameNode, size: CanvasSize) -> CanvasPoint {
    let Some(node) = frame.node() else {
        return frame.bounds().origin;
    };
    node.bounds_for_size(size)
        .map(|bounds| bounds.origin)
        .unwrap_or(frame.bounds().origin)
}

fn overview_ports(frame: &FrameNode) -> Vec<OpenGpuiOverviewPort> {
    frame
        .ports()
        .iter()
        .filter_map(|frame_port| {
            let port = frame_port.port()?;
            Some(OpenGpuiOverviewPort {
                id: frame_port.id(),
                direction: port.dir,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_detail_stays_native_while_overview_tracks_canvas_zoom() {
        let state = OpenGpuiSemanticZoomState {
            canonical_origin: CanvasPoint::default(),
            canonical_size: CanvasSize {
                width: 220.0,
                height: 120.0,
            },
            ports: Vec::new(),
        };

        assert_eq!(
            state.screen_size(
                0.5,
                OpenGpuiNodePresentation::Overview,
                OpenGpuiNodeScalePolicy::default(),
            ),
            CanvasSize {
                width: 110.0,
                height: 60.0,
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
        let first = overview_port_placement(PortDirection::In, 0, 2, size);
        let second = overview_port_placement(PortDirection::In, 1, 2, size);
        let output = overview_port_placement(PortDirection::Out, 0, 1, size);

        assert!(first.diameter > 0.0);
        assert!(first.top >= 0.0);
        assert!(second.top > first.top);
        assert!(second.top + second.diameter <= size.height);
        assert_eq!(first.left, 0.0);
        assert_eq!(output.left + output.diameter, size.width);
    }
}
