use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, NodeOrigin, Port, PortCapacity, PortDirection, PortId, PortKey,
};

use crate::{
    LayoutContext, LayoutDirection, LayoutEngineRequest, LayoutOptions, LayoutRequest,
    LayoutResult, LayoutSpacing, builtin_layout_engine_registry,
};

#[test]
fn freeform_fixture_is_deterministic_and_preserves_the_pinned_root() {
    let (graph, root, branch_a, branch_b, separate) = freeform_fixture_graph();
    let graph = graph.build_unchecked();
    let request = freeform_request();
    let context = LayoutContext::new().with_pinned_nodes([root]);

    let first = run_freeform_layout(&graph, &request, &context).expect("first layout");
    let second = run_freeform_layout(&graph, &request, &context).expect("second layout");

    assert_eq!(first, second);
    assert_eq!(
        first.node_position(root).expect("root").pos,
        CanvasPoint::default()
    );
    assert!(
        first.node_position(branch_b).expect("branch b").center.y
            > first.node_position(branch_a).expect("branch a").center.y
    );
    assert!(first.node_position(separate).is_some());
    assert_eq!(first.edge_routes.len(), 3);
    assert!(first.bounds.is_some());
}

#[test]
fn freeform_mixed_board_separates_overlaps_and_uses_context_sizes() {
    let (graph, root, note, image, detached) = mixed_freeform_fixture_graph();
    let request = freeform_request();
    let measured_note_size = CanvasSize {
        width: 280.0,
        height: 180.0,
    };
    let context = LayoutContext::new()
        .with_measured_node_sizes([(note, measured_note_size)])
        .with_pinned_nodes([root]);

    let result = run_freeform_layout(&graph, &request, &context).expect("layout");

    let root = result.node_position(root).expect("root");
    let note = result.node_position(note).expect("note");
    let image = result.node_position(image).expect("image");
    let detached = result.node_position(detached).expect("detached");

    assert_eq!(note.size, measured_note_size);
    assert!(image.center.y > root.center.y);
    assert!(note.center.y > image.center.y);
    assert_eq!(detached.pos, CanvasPoint { x: 520.0, y: 40.0 });
    assert_pairwise_separated(&[root, note, image, detached], 24.0);
}

#[test]
fn freeform_resolves_more_than_sixty_four_coincident_nodes() {
    let mut graph = GraphBuilder::new(GraphId::from_u128(3));
    for index in 0..66_u128 {
        let node_id = NodeId::from_u128(index + 1);
        graph.insert_node(node_id, node("demo.coincident", Vec::new()));
    }
    let graph = graph.build_unchecked();

    let result = run_freeform_layout(&graph, &freeform_request(), &LayoutContext::default())
        .expect("the placement bound must scale with the existing nodes");

    assert_eq!(result.nodes.len(), 66);
    assert_pairwise_separated(&result.nodes, 24.0);
}

#[test]
fn freeform_keeps_a_nonzero_pinned_anchor_in_canvas_coordinates() {
    let (mut graph, root, _, _, _) = freeform_fixture_graph();
    graph.node_mut(&root).expect("root exists").pos = CanvasPoint { x: 200.0, y: 100.0 };
    graph.node_mut(&root).expect("root exists").origin = Some(NodeOrigin { x: 0.5, y: 0.5 });
    let graph = graph.build_unchecked();

    let result = run_freeform_layout(
        &graph,
        &freeform_request(),
        &LayoutContext::new().with_pinned_nodes([root]),
    )
    .expect("layout");

    let root = result.node_position(root).expect("root placement");
    assert_eq!(root.pos, CanvasPoint { x: 200.0, y: 100.0 });
    assert_eq!(root.center, CanvasPoint { x: 200.0, y: 100.0 });
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
    })
}

fn freeform_fixture_graph() -> (GraphBuilder, NodeId, NodeId, NodeId, NodeId) {
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
        .node_mut(&separate)
        .map(|node| node.pos = CanvasPoint { x: 150.0, y: 0.0 })
        .expect("node exists");
    graph
        .node_mut(&separate)
        .map(|node| {
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

    (graph, root, branch_a, branch_b, separate)
}

fn mixed_freeform_fixture_graph() -> (Graph, NodeId, NodeId, NodeId, NodeId) {
    let mut graph = GraphBuilder::new(GraphId::from_u128(2));
    let root = NodeId::from_u128(11);
    let note = NodeId::from_u128(12);
    let image = NodeId::from_u128(13);
    let detached = NodeId::from_u128(14);
    let root_out = PortId::from_u128(30);
    let note_in = PortId::from_u128(31);
    let note_out = PortId::from_u128(32);
    let image_in = PortId::from_u128(33);
    let image_out = PortId::from_u128(34);
    let detached_in = PortId::from_u128(35);

    graph.insert_node(root, node("demo.root", vec![root_out]));
    graph
        .node_mut(&root)
        .map(|node| {
            node.size = Some(CanvasSize {
                width: 140.0,
                height: 72.0,
            })
        })
        .expect("node exists");
    graph.insert_node(note, node("demo.note", vec![note_in, note_out]));
    graph
        .node_mut(&note)
        .map(|node| node.size = None)
        .expect("node exists");
    graph.insert_node(image, node("demo.image", vec![image_in, image_out]));
    graph
        .node_mut(&image)
        .map(|node| node.pos = CanvasPoint { x: 60.0, y: 20.0 })
        .expect("node exists");
    graph
        .node_mut(&image)
        .map(|node| {
            node.size = Some(CanvasSize {
                width: 96.0,
                height: 96.0,
            })
        })
        .expect("node exists");
    graph.insert_node(detached, node("demo.detached", vec![detached_in]));
    graph
        .node_mut(&detached)
        .map(|node| node.pos = CanvasPoint { x: 520.0, y: 40.0 })
        .expect("node exists");
    graph
        .node_mut(&detached)
        .map(|node| {
            node.size = Some(CanvasSize {
                width: 180.0,
                height: 110.0,
            })
        })
        .expect("node exists");

    graph.insert_port(root_out, port(root, "out", PortDirection::Out));
    graph.insert_port(note_in, port(note, "in", PortDirection::In));
    graph.insert_port(note_out, port(note, "out", PortDirection::Out));
    graph.insert_port(image_in, port(image, "in", PortDirection::In));
    graph.insert_port(image_out, port(image, "out", PortDirection::Out));
    graph.insert_port(detached_in, port(detached, "in", PortDirection::In));

    graph.insert_edge(EdgeId::from_u128(40), edge(root_out, note_in));
    graph.insert_edge(EdgeId::from_u128(41), edge(note_out, image_in));
    graph.insert_edge(EdgeId::from_u128(42), edge(image_out, detached_in));
    (graph.build_unchecked(), root, note, image, detached)
}

fn run_freeform_layout(
    graph: &Graph,
    request: &LayoutRequest,
    context: &LayoutContext,
) -> Result<LayoutResult, crate::LayoutError> {
    builtin_layout_engine_registry().layout(
        graph,
        &LayoutEngineRequest::mind_map_freeform(request.clone()),
        context,
    )
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
    Edge::new(EdgeKind::Data, from, to)
}
