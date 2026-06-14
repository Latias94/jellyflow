use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey,
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

#[test]
fn freeform_mixed_board_separates_overlaps_and_uses_context_sizes() {
    let (graph, root, note, image, detached, hidden) = mixed_freeform_fixture_graph();
    let request = freeform_request();
    let measured_note_size = CanvasSize {
        width: 280.0,
        height: 180.0,
    };
    let context = LayoutContext::new()
        .with_measured_node_sizes([(note, measured_note_size)])
        .with_pinned_nodes([root]);

    let result = MindMapFreeformLayoutEngine
        .layout(&graph, &request, &context)
        .expect("layout");

    let root = result.node_position(root).expect("root");
    let note = result.node_position(note).expect("note");
    let image = result.node_position(image).expect("image");
    let detached = result.node_position(detached).expect("detached");

    assert!(result.node_position(hidden).is_none());
    assert_eq!(root.pos, CanvasPoint { x: 0.0, y: 0.0 });
    assert_eq!(note.size, measured_note_size);
    assert!(image.center.y > root.center.y);
    assert!(note.center.y > image.center.y);
    assert_eq!(detached.pos, CanvasPoint { x: 520.0, y: 40.0 });
    assert_pairwise_separated(&[root, note, image, detached], 24.0);
    assert_eq!(result.edge_routes.len(), 3);
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
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let root = NodeId::from_u128(1);
    let branch_a = NodeId::from_u128(2);
    let branch_b = NodeId::from_u128(3);
    let separate = NodeId::from_u128(4);
    let root_out = PortId::from_u128(10);
    let branch_a_in = PortId::from_u128(11);
    let branch_b_in = PortId::from_u128(12);
    let branch_a_out = PortId::from_u128(13);
    let separate_in = PortId::from_u128(14);

    graph.insert_node(root, node("demo.root", vec![root_out]));
    graph.insert_node(
        branch_a,
        node("demo.branch-a", vec![branch_a_in, branch_a_out]),
    );
    graph.insert_node(branch_b, node("demo.branch-b", vec![branch_b_in]));
    graph.insert_node(separate, node("demo.separate", vec![separate_in]));
    graph
        .update_node(&separate, |node| {
            node.pos = CanvasPoint { x: 150.0, y: 0.0 }
        })
        .expect("node exists");
    graph
        .update_node(&separate, |node| {
            node.size = Some(CanvasSize {
                width: 80.0,
                height: 80.0,
            })
        })
        .expect("node exists");

    graph.insert_port(root_out, port(root, "out", PortDirection::Out));
    graph.insert_port(branch_a_in, port(branch_a, "in", PortDirection::In));
    graph.insert_port(branch_a_out, port(branch_a, "out", PortDirection::Out));
    graph.insert_port(branch_b_in, port(branch_b, "in", PortDirection::In));
    graph.insert_port(separate_in, port(separate, "in", PortDirection::In));

    graph.insert_edge(EdgeId::from_u128(20), edge(root_out, branch_a_in));
    graph.insert_edge(EdgeId::from_u128(21), edge(root_out, branch_b_in));
    graph.insert_edge(EdgeId::from_u128(22), edge(branch_a_out, separate_in));

    (graph.build_unchecked(), root, branch_a, branch_b, separate)
}

fn mixed_freeform_fixture_graph() -> (Graph, NodeId, NodeId, NodeId, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    let root = NodeId::from_u128(11);
    let note = NodeId::from_u128(12);
    let image = NodeId::from_u128(13);
    let detached = NodeId::from_u128(14);
    let hidden = NodeId::from_u128(15);
    let root_out = PortId::from_u128(30);
    let note_in = PortId::from_u128(31);
    let note_out = PortId::from_u128(32);
    let image_in = PortId::from_u128(33);
    let image_out = PortId::from_u128(34);
    let detached_in = PortId::from_u128(35);
    let hidden_in = PortId::from_u128(36);

    graph.insert_node(root, node("demo.root", vec![root_out]));
    graph
        .update_node(&root, |node| {
            node.size = Some(CanvasSize {
                width: 140.0,
                height: 72.0,
            })
        })
        .expect("node exists");
    graph.insert_node(note, node("demo.note", vec![note_in, note_out]));
    graph
        .update_node(&note, |node| node.size = None)
        .expect("node exists");
    graph.insert_node(image, node("demo.image", vec![image_in, image_out]));
    graph
        .update_node(&image, |node| node.pos = CanvasPoint { x: 60.0, y: 20.0 })
        .expect("node exists");
    graph
        .update_node(&image, |node| {
            node.size = Some(CanvasSize {
                width: 96.0,
                height: 96.0,
            })
        })
        .expect("node exists");
    graph.insert_node(detached, node("demo.detached", vec![detached_in]));
    graph
        .update_node(&detached, |node| {
            node.pos = CanvasPoint { x: 520.0, y: 40.0 }
        })
        .expect("node exists");
    graph
        .update_node(&detached, |node| {
            node.size = Some(CanvasSize {
                width: 180.0,
                height: 110.0,
            })
        })
        .expect("node exists");
    graph.insert_node(hidden, node("demo.hidden", vec![hidden_in]));
    graph
        .update_node(&hidden, |node| node.pos = CanvasPoint { x: 30.0, y: 30.0 })
        .expect("node exists");
    graph
        .update_node(&hidden, |node| node.hidden = true)
        .expect("node exists");

    graph.insert_port(root_out, port(root, "out", PortDirection::Out));
    graph.insert_port(note_in, port(note, "in", PortDirection::In));
    graph.insert_port(note_out, port(note, "out", PortDirection::Out));
    graph.insert_port(image_in, port(image, "in", PortDirection::In));
    graph.insert_port(image_out, port(image, "out", PortDirection::Out));
    graph.insert_port(detached_in, port(detached, "in", PortDirection::In));
    graph.insert_port(hidden_in, port(hidden, "in", PortDirection::In));

    graph.insert_edge(EdgeId::from_u128(40), edge(root_out, note_in));
    graph.insert_edge(EdgeId::from_u128(41), edge(note_out, image_in));
    graph.insert_edge(EdgeId::from_u128(42), edge(image_out, detached_in));
    graph.insert_edge(EdgeId::from_u128(43), edge(root_out, hidden_in));

    (graph.build_unchecked(), root, note, image, detached, hidden)
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

fn assert_pairwise_separated(nodes: &[crate::LayoutNodePosition], gap: f32) {
    for (left_index, left) in nodes.iter().enumerate() {
        for right in nodes.iter().skip(left_index + 1) {
            assert!(
                !rects_overlap_with_gap(left, right, gap),
                "nodes {:?} and {:?} still overlap within gap {gap}",
                left.node,
                right.node
            );
        }
    }
}

fn rects_overlap_with_gap(
    left: &crate::LayoutNodePosition,
    right: &crate::LayoutNodePosition,
    gap: f32,
) -> bool {
    let right_origin_x = right.pos.x - gap;
    let right_origin_y = right.pos.y - gap;
    let right_extent_x = right.pos.x + right.size.width + gap;
    let right_extent_y = right.pos.y + right.size.height + gap;
    let left_extent_x = left.pos.x + left.size.width;
    let left_extent_y = left.pos.y + left.size.height;

    left.pos.x < right_extent_x
        && left_extent_x > right_origin_x
        && left.pos.y < right_extent_y
        && left_extent_y > right_origin_y
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
