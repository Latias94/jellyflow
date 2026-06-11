use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphId, Node, NodeId, NodeKindKey,
    Port, PortCapacity, PortDirection, PortId, PortKey,
};

use crate::{
    LayoutContext, LayoutDirection, LayoutEngine, LayoutEngineId, LayoutEngineRequest,
    LayoutOptions, LayoutRequest, LayoutResult, LayoutSpacing, MindMapFreeformLayoutEngine,
    builtin_layout_engine_registry, layout_graph_with_mind_map_freeform,
};

#[test]
fn builtin_registry_contains_freeform_engine() {
    let registry = builtin_layout_engine_registry();
    let request = LayoutEngineRequest::mind_map_freeform(LayoutRequest::all());

    assert!(registry.get(&LayoutEngineId::mind_map_freeform()).is_some());
    assert_eq!(request.engine, LayoutEngineId::mind_map_freeform());
    assert_eq!(
        LayoutEngineId::mind_map_freeform().as_str(),
        crate::MIND_MAP_FREEFORM_LAYOUT_ENGINE_ID
    );
}

#[test]
fn wrapper_matches_engine_entry_point() {
    let (graph, _root, _branch_a, _branch_b, _separate) = freeform_fixture_graph();
    let request = freeform_request();

    let wrapper = layout_graph_with_mind_map_freeform(&graph, &request).expect("wrapper");
    let engine = MindMapFreeformLayoutEngine
        .layout(&graph, &request, &LayoutContext::default())
        .expect("engine");

    assert_eq!(wrapper, engine);
}

#[test]
fn freeform_fixture_snapshot_stays_stable() {
    let (graph, root, branch_a, branch_b, separate) = freeform_fixture_graph();
    let request = freeform_request();
    let context = LayoutContext::new().with_pinned_nodes([root]);

    let result = MindMapFreeformLayoutEngine
        .layout(&graph, &request, &context)
        .expect("layout");

    assert_eq!(
        result,
        freeform_expected_snapshot(root, branch_a, branch_b, separate)
    );
}

fn freeform_request() -> LayoutRequest {
    LayoutRequest::all().with_options(LayoutOptions {
        direction: LayoutDirection::TopToBottom,
        spacing: LayoutSpacing {
            nodesep: 24.0,
            ranksep: 24.0,
            edgesep: 24.0,
        },
        margin: CanvasSize {
            width: 0.0,
            height: 0.0,
        },
        default_node_size: CanvasSize {
            width: 172.0,
            height: 36.0,
        },
        node_origin: (0.0, 0.0),
    })
}

fn freeform_fixture_graph() -> (Graph, NodeId, NodeId, NodeId, NodeId) {
    let mut graph = Graph::new(GraphId::from_u128(1));
    let root = NodeId::from_u128(1);
    let branch_a = NodeId::from_u128(2);
    let branch_b = NodeId::from_u128(3);
    let separate = NodeId::from_u128(4);
    let root_out = PortId::from_u128(10);
    let branch_a_in = PortId::from_u128(11);
    let branch_b_in = PortId::from_u128(12);
    let branch_a_out = PortId::from_u128(13);
    let separate_in = PortId::from_u128(14);

    graph.nodes.insert(root, node("demo.root", vec![root_out]));
    graph.nodes.insert(
        branch_a,
        node("demo.branch-a", vec![branch_a_in, branch_a_out]),
    );
    graph
        .nodes
        .insert(branch_b, node("demo.branch-b", vec![branch_b_in]));
    graph
        .nodes
        .insert(separate, node("demo.separate", vec![separate_in]));
    graph.nodes.get_mut(&separate).unwrap().pos = CanvasPoint { x: 150.0, y: 0.0 };
    graph.nodes.get_mut(&separate).unwrap().size = Some(CanvasSize {
        width: 80.0,
        height: 80.0,
    });

    graph
        .ports
        .insert(root_out, port(root, "out", PortDirection::Out));
    graph
        .ports
        .insert(branch_a_in, port(branch_a, "in", PortDirection::In));
    graph
        .ports
        .insert(branch_a_out, port(branch_a, "out", PortDirection::Out));
    graph
        .ports
        .insert(branch_b_in, port(branch_b, "in", PortDirection::In));
    graph
        .ports
        .insert(separate_in, port(separate, "in", PortDirection::In));

    graph
        .edges
        .insert(EdgeId::from_u128(20), edge(root_out, branch_a_in));
    graph
        .edges
        .insert(EdgeId::from_u128(21), edge(root_out, branch_b_in));
    graph
        .edges
        .insert(EdgeId::from_u128(22), edge(branch_a_out, separate_in));

    (graph, root, branch_a, branch_b, separate)
}

fn freeform_expected_snapshot(
    root: NodeId,
    branch_a: NodeId,
    branch_b: NodeId,
    separate: NodeId,
) -> LayoutResult {
    LayoutResult {
        nodes: vec![
            position(
                root,
                CanvasPoint { x: 0.0, y: 0.0 },
                CanvasSize {
                    width: 100.0,
                    height: 60.0,
                },
            ),
            position(
                branch_a,
                CanvasPoint { x: 0.0, y: 84.0 },
                CanvasSize {
                    width: 100.0,
                    height: 60.0,
                },
            ),
            position(
                branch_b,
                CanvasPoint { x: 0.0, y: 168.0 },
                CanvasSize {
                    width: 100.0,
                    height: 60.0,
                },
            ),
            position(
                separate,
                CanvasPoint { x: 150.0, y: 0.0 },
                CanvasSize {
                    width: 80.0,
                    height: 80.0,
                },
            ),
        ],
        edge_routes: vec![
            route(
                EdgeId::from_u128(20),
                CanvasPoint { x: 50.0, y: 30.0 },
                CanvasPoint { x: 50.0, y: 114.0 },
            ),
            route(
                EdgeId::from_u128(21),
                CanvasPoint { x: 50.0, y: 30.0 },
                CanvasPoint { x: 50.0, y: 198.0 },
            ),
            route(
                EdgeId::from_u128(22),
                CanvasPoint { x: 50.0, y: 114.0 },
                CanvasPoint { x: 190.0, y: 40.0 },
            ),
        ],
        bounds: Some(jellyflow_core::CanvasRect {
            origin: CanvasPoint { x: 0.0, y: 0.0 },
            size: CanvasSize {
                width: 230.0,
                height: 228.0,
            },
        }),
    }
}

fn position(node: NodeId, pos: CanvasPoint, size: CanvasSize) -> crate::LayoutNodePosition {
    crate::LayoutNodePosition {
        node,
        pos,
        center: CanvasPoint {
            x: pos.x + size.width * 0.5,
            y: pos.y + size.height * 0.5,
        },
        size,
    }
}

fn route(edge: EdgeId, from: CanvasPoint, to: CanvasPoint) -> crate::LayoutEdgeRoute {
    crate::LayoutEdgeRoute {
        edge,
        points: vec![from, to],
    }
}

fn node(kind: &str, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint { x: 0.0, y: 0.0 },
        origin: None,
        selectable: None,
        focusable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 100.0,
            height: 60.0,
        }),
        hidden: false,
        collapsed: false,
        ports,
        data: serde_json::Value::Null,
    }
}

fn port(node: NodeId, key: &str, dir: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir,
        kind: jellyflow_core::PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn edge(from: PortId, to: PortId) -> Edge {
    Edge {
        kind: EdgeKind::Data,
        from,
        to,
        hidden: false,
        selectable: None,
        focusable: None,
        interaction_width: None,
        deletable: None,
        reconnectable: None,
    }
}
