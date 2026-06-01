use super::harness::{HarnessEvent, InteractionHarness};

use crate::io::NodeGraphViewState;
use crate::runtime::selection::{SelectionBoxOptions, SelectionBoxResult};
use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

#[test]
fn selection_box_replaces_selection_with_policy_filtered_sorted_result() {
    let fixture = selection_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(
        vec![fixture.outside],
        vec![fixture.non_selectable_edge],
        Vec::new(),
    );
    let mut harness =
        InteractionHarness::with_view_state("selection box replacement", fixture.graph, view_state);

    let result = harness
        .store_mut()
        .apply_selection_box(selection_rect(), SelectionBoxOptions::default());

    let expected = SelectionBoxResult {
        nodes: vec![fixture.low, fixture.high],
        edges: vec![fixture.connected_edge, fixture.connected_outside_edge],
        groups: Vec::new(),
    };
    assert_eq!(result, expected);
    harness.assert_events(&[HarnessEvent::selection(
        expected.nodes,
        expected.edges,
        expected.groups,
    )]);
}

#[test]
fn selection_box_additive_mode_unions_with_existing_selection_and_sorts() {
    let fixture = selection_fixture();
    let mut view_state = NodeGraphViewState::default();
    view_state.set_selection(vec![fixture.outside], Vec::new(), Vec::new());
    let mut harness =
        InteractionHarness::with_view_state("selection box additive", fixture.graph, view_state);

    let result = harness.store_mut().apply_selection_box(
        selection_rect(),
        SelectionBoxOptions {
            additive: true,
            ..SelectionBoxOptions::default()
        },
    );

    let expected = SelectionBoxResult {
        nodes: vec![fixture.low, fixture.high, fixture.outside],
        edges: vec![fixture.connected_edge, fixture.connected_outside_edge],
        groups: Vec::new(),
    };
    assert_eq!(result, expected);
    harness.assert_events(&[HarnessEvent::selection(
        expected.nodes,
        expected.edges,
        expected.groups,
    )]);
}

struct SelectionFixture {
    graph: Graph,
    low: NodeId,
    high: NodeId,
    outside: NodeId,
    connected_edge: EdgeId,
    connected_outside_edge: EdgeId,
    non_selectable_edge: EdgeId,
}

fn selection_fixture() -> SelectionFixture {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let low = NodeId::from_u128(10);
    let hidden = NodeId::from_u128(20);
    let high = NodeId::from_u128(30);
    let disabled = NodeId::from_u128(40);
    let outside = NodeId::from_u128(50);

    let (low_out, low_port) = out_port(low, 100);
    let (high_in, high_port) = in_port(high, 101);
    let (outside_in, outside_port) = in_port(outside, 102);

    graph.nodes.insert(
        high,
        node(
            CanvasPoint { x: 12.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            false,
            vec![high_in],
        ),
    );
    graph.nodes.insert(
        hidden,
        node(
            CanvasPoint { x: 24.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            true,
            Vec::new(),
        ),
    );
    graph.nodes.insert(
        low,
        node(
            CanvasPoint { x: 0.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            false,
            vec![low_out],
        ),
    );
    graph.nodes.insert(
        disabled,
        node(
            CanvasPoint { x: 36.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            Some(false),
            false,
            Vec::new(),
        ),
    );
    graph.nodes.insert(
        outside,
        node(
            CanvasPoint { x: 80.0, y: 0.0 },
            Some(CanvasSize {
                width: 10.0,
                height: 10.0,
            }),
            None,
            false,
            vec![outside_in],
        ),
    );
    graph.ports.insert(low_out, low_port);
    graph.ports.insert(high_in, high_port);
    graph.ports.insert(outside_in, outside_port);

    let connected_edge = EdgeId::from_u128(200);
    graph.edges.insert(
        connected_edge,
        Edge {
            kind: EdgeKind::Data,
            from: low_out,
            to: high_in,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    let non_selectable_edge = EdgeId::from_u128(201);
    graph.edges.insert(
        non_selectable_edge,
        Edge {
            kind: EdgeKind::Data,
            from: low_out,
            to: outside_in,
            selectable: Some(false),
            deletable: None,
            reconnectable: None,
        },
    );

    let connected_outside_edge = EdgeId::from_u128(202);
    graph.edges.insert(
        connected_outside_edge,
        Edge {
            kind: EdgeKind::Data,
            from: low_out,
            to: outside_in,
            selectable: None,
            deletable: None,
            reconnectable: None,
        },
    );

    SelectionFixture {
        graph,
        low,
        high,
        outside,
        connected_edge,
        connected_outside_edge,
        non_selectable_edge,
    }
}

fn selection_rect() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint { x: -1.0, y: -1.0 },
        size: CanvasSize {
            width: 50.0,
            height: 20.0,
        },
    }
}

fn node(
    pos: CanvasPoint,
    size: Option<CanvasSize>,
    selectable: Option<bool>,
    hidden: bool,
    ports: Vec<PortId>,
) -> Node {
    Node {
        kind: NodeKindKey::new("test.node"),
        kind_version: 1,
        pos,
        selectable,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size,
        hidden,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn out_port(node: NodeId, stable: u128) -> (PortId, Port) {
    port(node, stable, PortDirection::Out, PortCapacity::Multi)
}

fn in_port(node: NodeId, stable: u128) -> (PortId, Port) {
    port(node, stable, PortDirection::In, PortCapacity::Single)
}

fn port(node: NodeId, stable: u128, dir: PortDirection, capacity: PortCapacity) -> (PortId, Port) {
    let id = PortId::from_u128(stable);
    (
        id,
        Port {
            node,
            key: PortKey::new(format!("p{stable}")),
            dir,
            kind: PortKind::Data,
            capacity,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    )
}
