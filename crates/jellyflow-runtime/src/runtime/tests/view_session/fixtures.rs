use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::io::ViewSessionSeed;
use crate::runtime::adapter_protocol::{
    FrameMaterializationBudget, FrameRequest, RendererStamp, ViewPoint, ViewRect, ViewSize,
    side_anchor,
};
use crate::runtime::geometry::HandlePosition;
use crate::runtime::view_session::{
    NodeIntersectionQuery, NodeIntersectionQueryOutcome, ViewSession, ViewSessionOptions,
};

pub(super) fn node(kind: &str, x: f32) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x, y: 0.0 },
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 160.0,
            height: 96.0,
        }),
        hidden: false,
        collapsed: false,
        ports: Vec::new(),
        data: serde_json::Value::Null,
    }
}

pub(in crate::runtime::tests) fn graph_with_nodes(count: usize) -> (Graph, Vec<NodeId>) {
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x200));
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        let id = NodeId::from_u128(index as u128 + 1);
        builder.insert_node(id, node("test.node", index as f32 * 24.0));
        ids.push(id);
    }
    (builder.build().expect("valid fixture graph"), ids)
}

#[derive(Clone, Copy)]
pub(in crate::runtime::tests) struct RoutedGraphIds {
    pub(in crate::runtime::tests) source: NodeId,
    pub(in crate::runtime::tests) target: NodeId,
    pub(in crate::runtime::tests) alternate_source: NodeId,
    pub(in crate::runtime::tests) source_port: PortId,
    pub(in crate::runtime::tests) target_port: PortId,
    pub(in crate::runtime::tests) alternate_source_port: PortId,
    pub(in crate::runtime::tests) edge: EdgeId,
}

pub(in crate::runtime::tests) fn graph_with_routed_edge() -> (Graph, RoutedGraphIds) {
    let ids = RoutedGraphIds {
        source: NodeId::from_u128(1),
        target: NodeId::from_u128(2),
        alternate_source: NodeId::from_u128(3),
        source_port: PortId::from_u128(11),
        target_port: PortId::from_u128(12),
        alternate_source_port: PortId::from_u128(13),
        edge: EdgeId::from_u128(21),
    };
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x201));
    for (id, port, direction, x) in [
        (ids.source, ids.source_port, PortDirection::Out, 0.0),
        (ids.target, ids.target_port, PortDirection::In, 240.0),
        (
            ids.alternate_source,
            ids.alternate_source_port,
            PortDirection::Out,
            480.0,
        ),
    ] {
        let mut current = node("test.node", x);
        current.ports = vec![port];
        builder.insert_node(id, current);
        builder.insert_port(port, port_for(id, direction));
    }
    builder.insert_edge(
        ids.edge,
        Edge::new(EdgeKind::Data, ids.source_port, ids.target_port),
    );
    (builder.build().expect("valid routed graph"), ids)
}

fn port_for(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(match direction {
            PortDirection::In => "in",
            PortDirection::Out => "out",
        }),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

pub(super) fn view_options(seed: ViewSessionSeed) -> ViewSessionOptions {
    ViewSessionOptions { seed: Some(seed) }
}

pub(super) fn publish_graph_layouts(session: &ViewSession) {
    let snapshot = session.graph_snapshot().expect("fixture graph snapshot");
    let layouts = snapshot
        .graph()
        .nodes()
        .iter()
        .map(|(node_id, node)| {
            (
                *node_id,
                jellyflow_core::core::CanvasRect {
                    origin: node.pos,
                    size: node.size.unwrap_or(CanvasSize {
                        width: 160.0,
                        height: 96.0,
                    }),
                },
            )
        })
        .collect::<Vec<_>>();
    publish_node_layouts(session, layouts);
}

pub(super) fn publish_node_layouts(
    session: &ViewSession,
    layouts: impl IntoIterator<Item = (NodeId, jellyflow_core::core::CanvasRect)>,
) {
    let layouts = layouts
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert!(
        !layouts.is_empty(),
        "fixture must publish at least one node"
    );
    let node_count = layouts.len();
    let request = FrameRequest::new(
        jellyflow_core::core::CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 1.0,
                height: 1.0,
            },
        },
        RendererStamp::new("test.seed", 1),
    )
    .with_required_nodes(layouts.keys().copied())
    .with_materialization_budget(
        FrameMaterializationBudget::new(0, node_count, 0).with_visible_node_scan_limit(0),
    );
    let frame = session.frame(request).expect("fixture adapter frame");
    assert_eq!(
        frame.nodes().len(),
        node_count,
        "every requested fixture node must be materialized"
    );
    let collector = frame.collector();
    for record in frame.nodes() {
        let canvas_bounds = layouts
            .get(&record.id())
            .copied()
            .expect("frame must contain only requested fixture nodes");
        let transform = frame.transform();
        let view_origin = transform
            .screen_point_for_canvas(canvas_bounds.origin)
            .expect("fixture node origin is finite");
        let view_bounds = ViewRect::new(
            ViewPoint::new(view_origin.x, view_origin.y),
            ViewSize::new(
                canvas_bounds.size.width * transform.zoom,
                canvas_bounds.size.height * transform.zoom,
            ),
        );
        let mut layout = record.begin_layout(view_bounds);
        for (index, port) in record.ports().iter().enumerate() {
            let outward = match port.port().expect("fixture real port exists").dir {
                PortDirection::In => HandlePosition::Left,
                PortDirection::Out => HandlePosition::Right,
            };
            let x = match outward {
                HandlePosition::Left => view_bounds.origin.x - 6.0,
                HandlePosition::Right => view_bounds.origin.x + view_bounds.size.width - 6.0,
                HandlePosition::Top | HandlePosition::Bottom => unreachable!(),
            };
            let visual_bounds = ViewRect::new(
                ViewPoint::new(x, view_bounds.origin.y + 20.0 + index as f32 * 18.0),
                ViewSize::new(12.0, 12.0),
            );
            layout
                .port_rendered(
                    port.layout_target(),
                    visual_bounds,
                    visual_bounds,
                    side_anchor(visual_bounds, outward).expect("fixture port bounds are valid"),
                    outward,
                )
                .expect("fixture port layout is valid");
        }
        collector
            .layout(layout.finish().expect("fixture covers every real port"))
            .expect("fixture collector remains open");
    }
    session
        .submit_feedback(collector.finish().expect("fixture feedback is complete"))
        .expect("fixture feedback is accepted");
}

pub(super) fn complete_nodes_intersecting(
    session: &ViewSession,
    bounds: CanvasRect,
) -> Vec<NodeId> {
    match session
        .nodes_intersecting(NodeIntersectionQuery::new(bounds, usize::MAX))
        .expect("fixture node-intersection query is valid")
    {
        NodeIntersectionQueryOutcome::Complete { nodes, .. } => nodes,
        other => panic!("fixture node-intersection query must complete: {other:?}"),
    }
}
