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
) -> Vec<(ConnectionHandleRef, HandleBounds)> {
    let mut by_side: BTreeMap<PortViewSide, Vec<(SideOrderKey<'a>, ConnectionHandleRef)>> =
        BTreeMap::new();

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
        by_side.entry(side).or_default().push((
            SideOrderKey {
                group: view.and_then(|view| view.group.as_deref()),
                explicit_order: view.and_then(|view| view.order),
                port_index,
            },
            handle,
        ));
    }

    let mut handles = Vec::new();
    for (side, mut side_ports) in by_side {
        side_ports.sort_by(|(a_key, a_handle), (b_key, b_handle)| {
            (a_key, a_handle.port).cmp(&(b_key, b_handle.port))
        });
        let count = side_ports.len().max(1) as f32;
        for (index, (_key, handle)) in side_ports.into_iter().enumerate() {
            handles.push((
                handle,
                HandleBounds {
                    rect: handle_rect_for_side(side, index, count, node_size),
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
        );

        assert_eq!(handles[0].1.position, HandlePosition::Left);
    }
}
