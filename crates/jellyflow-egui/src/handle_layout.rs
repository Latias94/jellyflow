use std::borrow::Cow;
use std::collections::BTreeMap;

use jellyflow::core::{CanvasPoint, CanvasRect, CanvasSize, NodeId, PortDirection, PortId};
use jellyflow::runtime::runtime::connection::ConnectionHandleRef;
use jellyflow::runtime::runtime::geometry::{HandleBounds, HandlePosition};
use jellyflow::runtime::schema::{PortDecl, PortHandleVisibility, PortViewSide};

use crate::bridge::DEFAULT_HANDLE_SIZE;

#[derive(Debug, Clone, Copy)]
pub(crate) struct HandleLayoutPort<'a> {
    pub(crate) id: PortId,
    pub(crate) direction: PortDirection,
    pub(crate) decl: Option<&'a PortDecl>,
}

#[derive(Debug, Clone)]
pub(crate) struct HandleAnchorRegion<'a> {
    pub(crate) key: Cow<'a, str>,
    pub(crate) port_key: Option<Cow<'a, str>>,
    pub(crate) rect: CanvasRect,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct SideOrderKey<'a> {
    group: Option<&'a str>,
    explicit_order: Option<i32>,
    port_index: usize,
}

pub(crate) fn handle_bounds_for_node<'a>(
    node: NodeId,
    ports: impl IntoIterator<Item = HandleLayoutPort<'a>>,
    node_size: CanvasSize,
    anchor_regions: &[HandleAnchorRegion<'_>],
) -> Vec<(ConnectionHandleRef, HandleBounds)> {
    let anchor_regions = anchor_regions
        .iter()
        .filter(|region| region.rect.is_positive_finite())
        .map(|region| {
            (
                (region.key.as_ref(), region.port_key.as_deref()),
                region.rect,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut by_side: BTreeMap<
        PortViewSide,
        Vec<(SideOrderKey<'a>, ConnectionHandleRef, Option<CanvasRect>)>,
    > = BTreeMap::new();

    for (port_index, port) in ports.into_iter().enumerate() {
        let view = port.decl.map(|decl| &decl.view);
        if matches!(
            view.and_then(|view| view.visibility),
            Some(PortHandleVisibility::Hidden | PortHandleVisibility::Collapsed)
        ) {
            continue;
        }
        let side = view
            .and_then(|view| view.side)
            .unwrap_or_else(|| PortViewSide::fallback_for_direction(port.direction));
        let handle = ConnectionHandleRef::new(node, port.id, port.direction);
        let anchor = view
            .and_then(|view| view.anchor.as_deref())
            .and_then(|key| {
                port.decl
                    .and_then(|decl| anchor_regions.get(&(key, Some(decl.key.0.as_str()))))
                    .or_else(|| anchor_regions.get(&(key, None)))
                    .copied()
            });
        by_side.entry(side).or_default().push((
            SideOrderKey {
                group: view.and_then(|view| view.group.as_deref()),
                explicit_order: view.and_then(|view| view.order),
                port_index,
            },
            handle,
            anchor,
        ));
    }

    let mut handles = Vec::new();
    for (side, mut side_ports) in by_side {
        side_ports.sort_by(|(a_key, a_handle, _), (b_key, b_handle, _)| {
            (a_key, a_handle.port).cmp(&(b_key, b_handle.port))
        });
        let count = side_ports.len().max(1) as f32;
        for (index, (_key, handle, anchor)) in side_ports.into_iter().enumerate() {
            let rect = anchor
                .map(|anchor| anchored_handle_rect_for_side(side, anchor, node_size))
                .unwrap_or_else(|| handle_rect_for_side(side, index, count, node_size));
            handles.push((
                handle,
                HandleBounds {
                    rect,
                    position: handle_position(side),
                },
            ));
        }
    }
    handles
}

fn handle_rect_for_side(
    side: PortViewSide,
    index: usize,
    count: f32,
    node_size: CanvasSize,
) -> CanvasRect {
    let ratio = (index + 1) as f32 / (count + 1.0);
    let half = DEFAULT_HANDLE_SIZE * 0.5;
    let (x, y) = match side {
        PortViewSide::Top => (ratio * node_size.width - half, -half),
        PortViewSide::Right => (node_size.width - half, ratio * node_size.height - half),
        PortViewSide::Bottom => (ratio * node_size.width - half, node_size.height - half),
        PortViewSide::Left => (-half, ratio * node_size.height - half),
    };

    CanvasRect {
        origin: CanvasPoint { x, y },
        size: CanvasSize {
            width: DEFAULT_HANDLE_SIZE,
            height: DEFAULT_HANDLE_SIZE,
        },
    }
}

fn anchored_handle_rect_for_side(
    side: PortViewSide,
    anchor: CanvasRect,
    node_size: CanvasSize,
) -> CanvasRect {
    let half = DEFAULT_HANDLE_SIZE * 0.5;
    let center_x = anchor.origin.x + anchor.size.width * 0.5;
    let center_y = anchor.origin.y + anchor.size.height * 0.5;
    let (x, y) = match side {
        PortViewSide::Top => (clamp_handle_axis(center_x, node_size.width), -half),
        PortViewSide::Right => (
            node_size.width - half,
            clamp_handle_axis(center_y, node_size.height),
        ),
        PortViewSide::Bottom => (
            clamp_handle_axis(center_x, node_size.width),
            node_size.height - half,
        ),
        PortViewSide::Left => (-half, clamp_handle_axis(center_y, node_size.height)),
    };

    CanvasRect {
        origin: CanvasPoint { x, y },
        size: CanvasSize {
            width: DEFAULT_HANDLE_SIZE,
            height: DEFAULT_HANDLE_SIZE,
        },
    }
}

fn clamp_handle_axis(center: f32, limit: f32) -> f32 {
    let half = DEFAULT_HANDLE_SIZE * 0.5;
    (center - half).clamp(-half, limit - half)
}

fn handle_position(side: PortViewSide) -> HandlePosition {
    match side {
        PortViewSide::Top => HandlePosition::Top,
        PortViewSide::Right => HandlePosition::Right,
        PortViewSide::Bottom => HandlePosition::Bottom,
        PortViewSide::Left => HandlePosition::Left,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jellyflow::core::{PortCapacity, PortDirection, PortKey, PortKind};
    use jellyflow::runtime::schema::{PortDecl, PortViewDescriptor};

    #[test]
    fn handle_bounds_for_node_skips_hidden_and_collapsed_handles() {
        let node = NodeId::from_u128(1);
        let hidden_port = PortId::from_u128(2);
        let collapsed_port = PortId::from_u128(3);
        let visible_port = PortId::from_u128(4);
        let hidden = PortDecl::data_input("hidden").with_view(PortViewDescriptor::left().hidden());
        let collapsed =
            PortDecl::data_input("collapsed").with_view(PortViewDescriptor::left().collapsed());
        let visible = PortDecl::data_output("visible").on_right();

        let handles = handle_bounds_for_node(
            node,
            [
                HandleLayoutPort {
                    id: hidden_port,
                    direction: PortDirection::In,
                    decl: Some(&hidden),
                },
                HandleLayoutPort {
                    id: collapsed_port,
                    direction: PortDirection::In,
                    decl: Some(&collapsed),
                },
                HandleLayoutPort {
                    id: visible_port,
                    direction: PortDirection::Out,
                    decl: Some(&visible),
                },
            ],
            CanvasSize {
                width: 160.0,
                height: 80.0,
            },
            &[],
        );

        assert_eq!(handles.len(), 1);
        assert_eq!(handles[0].0.port, visible_port);
        assert_eq!(handles[0].1.position, HandlePosition::Right);
    }

    #[test]
    fn handle_bounds_falls_back_to_direction_when_view_is_missing() {
        let node = NodeId::from_u128(10);
        let port = PortId::from_u128(11);
        let decl = PortDecl::new(
            PortKey::new("legacy"),
            PortDirection::In,
            PortKind::Data,
            PortCapacity::Single,
        );

        let handles = handle_bounds_for_node(
            node,
            [HandleLayoutPort {
                id: port,
                direction: PortDirection::In,
                decl: Some(&decl),
            }],
            CanvasSize {
                width: 160.0,
                height: 80.0,
            },
            &[],
        );

        assert_eq!(handles[0].1.position, HandlePosition::Left);
    }

    #[test]
    fn handle_bounds_aligns_anchor_regions_on_declared_side() {
        let node = NodeId::from_u128(20);
        let input = PortId::from_u128(21);
        let output = PortId::from_u128(22);
        let anchored_input = PortDecl::data_input("fk")
            .on_left()
            .with_view_anchor("field.foreign_key");
        let anchored_output = PortDecl::data_output("pk")
            .on_right()
            .with_view_anchor("field.primary_key");

        let handles = handle_bounds_for_node(
            node,
            [
                HandleLayoutPort {
                    id: input,
                    direction: PortDirection::In,
                    decl: Some(&anchored_input),
                },
                HandleLayoutPort {
                    id: output,
                    direction: PortDirection::Out,
                    decl: Some(&anchored_output),
                },
            ],
            CanvasSize {
                width: 200.0,
                height: 120.0,
            },
            &[
                HandleAnchorRegion {
                    key: Cow::Borrowed("field.primary_key"),
                    port_key: None,
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 16.0, y: 44.0 },
                        size: CanvasSize {
                            width: 168.0,
                            height: 18.0,
                        },
                    },
                },
                HandleAnchorRegion {
                    key: Cow::Borrowed("field.foreign_key"),
                    port_key: None,
                    rect: CanvasRect {
                        origin: CanvasPoint { x: 16.0, y: 72.0 },
                        size: CanvasSize {
                            width: 168.0,
                            height: 18.0,
                        },
                    },
                },
            ],
        );
        let input_bounds = handles
            .iter()
            .find(|(handle, _)| handle.port == input)
            .expect("input handle exists");
        let output_bounds = handles
            .iter()
            .find(|(handle, _)| handle.port == output)
            .expect("output handle exists");

        assert_eq!(input_bounds.1.rect.origin.x, -5.0);
        assert_eq!(input_bounds.1.rect.origin.y, 76.0);
        assert_eq!(output_bounds.1.rect.origin.x, 195.0);
        assert_eq!(output_bounds.1.rect.origin.y, 48.0);
    }
}
