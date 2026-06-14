use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeEndpoints, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    GraphOp, GraphOpBuilderExt, GraphTransaction, Node, NodeId, NodeKindKey, Port, PortCapacity,
    PortDirection, PortId, PortKey, PortKind,
};

use crate::{
    LayoutContext, LayoutEngine, LayoutEngineId, LayoutEngineRegistry, LayoutEngineRequest,
    LayoutError, LayoutNodePosition, LayoutRequest, LayoutResult, LayoutScope,
    layout_graph_with_engine,
};

#[derive(Clone)]
struct TestEngine {
    id: LayoutEngineId,
}

impl TestEngine {
    fn new(id: impl Into<LayoutEngineId>) -> Self {
        Self { id: id.into() }
    }
}

impl LayoutEngine for TestEngine {
    fn id(&self) -> LayoutEngineId {
        self.id.clone()
    }

    fn layout(
        &self,
        graph: &Graph,
        _request: &LayoutRequest,
        context: &LayoutContext,
    ) -> Result<LayoutResult, LayoutError> {
        let nodes = graph
            .nodes()
            .keys()
            .copied()
            .filter(|node| !context.pinned_nodes.contains(node))
            .map(|node| LayoutNodePosition {
                node,
                pos: CanvasPoint { x: 10.0, y: 20.0 },
                center: CanvasPoint { x: 60.0, y: 40.0 },
                size: CanvasSize {
                    width: 100.0,
                    height: 40.0,
                },
            })
            .collect();

        Ok(LayoutResult {
            nodes,
            edge_routes: Vec::new(),
            bounds: None,
        })
    }
}

#[test]
fn registry_resolves_custom_engine_by_stable_id() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();

    let graph = graph_with_one_node(NodeId::from_u128(1));
    let request = LayoutEngineRequest::new(engine.clone(), LayoutRequest::all());
    let result =
        layout_graph_with_engine(&graph, &request, &registry, &LayoutContext::default()).unwrap();

    assert_eq!(registry.engine_ids().collect::<Vec<_>>(), vec![&engine]);
    assert_eq!(result.nodes.len(), 1);
}

#[test]
fn registry_rejects_duplicate_engine_ids() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();

    let err = registry
        .insert(TestEngine::new(engine.clone()))
        .expect_err("duplicate engine");

    assert_eq!(err, LayoutError::DuplicateLayoutEngine(engine));
}

#[test]
fn registry_reports_missing_engine() {
    let registry = LayoutEngineRegistry::new();
    let missing = LayoutEngineId::new("missing");
    let graph = GraphBuilder::new(GraphId::from_u128(1));
    let request = LayoutEngineRequest::new(missing.clone(), LayoutRequest::all());

    let err = layout_graph_with_engine(&graph, &request, &registry, &LayoutContext::default())
        .expect_err("missing engine");

    assert_eq!(err, LayoutError::MissingLayoutEngine(missing));
}

#[test]
fn engine_request_serializes_engine_id() {
    let request = LayoutEngineRequest::new("custom.test", LayoutRequest::all());

    let encoded = serde_json::to_string(&request).expect("serialize");
    let decoded: LayoutEngineRequest = serde_json::from_str(&encoded).expect("deserialize");

    assert_eq!(decoded.engine.as_str(), "custom.test");
}

#[test]
fn engine_request_constructors_use_builtin_ids() {
    assert_eq!(
        LayoutEngineRequest::tidy_tree(LayoutRequest::all()).engine,
        LayoutEngineId::tidy_tree()
    );
}

#[test]
fn context_pinned_nodes_are_visible_to_engines() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();
    let pinned = NodeId::from_u128(1);
    let graph = graph_with_one_node(pinned);
    let request = LayoutEngineRequest::new(engine, LayoutRequest::all());
    let context = LayoutContext::new().with_pinned_nodes([pinned]);

    let result = layout_graph_with_engine(&graph, &request, &registry, &context).unwrap();
    let tx = result.to_transaction(&graph).unwrap();

    assert!(result.nodes.is_empty());
    assert!(tx.is_empty());
}

#[test]
fn generic_result_converts_to_transaction() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();
    let node = NodeId::from_u128(1);
    let graph = graph_with_one_node(node);
    let request = LayoutEngineRequest::new(engine, LayoutRequest::all());

    let result =
        layout_graph_with_engine(&graph, &request, &registry, &LayoutContext::default()).unwrap();
    let tx = result.to_transaction(&graph).unwrap();

    assert!(matches!(tx.ops()[0], GraphOp::SetNodePos { id, .. } if id == node));
}

#[test]
fn dirty_scope_expands_edge_change_to_endpoint_nodes() {
    let graph = connected_graph();
    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeHidden {
        id: EdgeId::from_u128(5),
        from: false,
        to: true,
    }]);

    let scope = LayoutScope::from_transaction(&graph, &tx);

    assert_eq!(
        scope.nodes(),
        Some(
            &[NodeId::from_u128(1), NodeId::from_u128(2)]
                .into_iter()
                .collect()
        )
    );
}

#[test]
fn dirty_scope_expands_reconnect_to_old_and_new_endpoint_nodes() {
    let graph = connected_graph_with_third_node();
    let tx = GraphTransaction::from_ops([GraphOp::SetEdgeEndpoints {
        id: EdgeId::from_u128(5),
        from: EdgeEndpoints::new(PortId::from_u128(3), PortId::from_u128(4)),
        to: EdgeEndpoints::new(PortId::from_u128(3), PortId::from_u128(7)),
    }]);

    let request = LayoutRequest::all().with_dirty_scope_from_transaction(&graph, &tx);

    assert_eq!(
        request.scope.nodes(),
        Some(
            &[
                NodeId::from_u128(1),
                NodeId::from_u128(2),
                NodeId::from_u128(6),
            ]
            .into_iter()
            .collect()
        )
    );
}

#[test]
fn dirty_scope_omits_removed_node_ids_and_keeps_remaining_neighbors() {
    let before = connected_graph();
    let tx = before
        .build_remove_node_tx(NodeId::from_u128(1), "remove source")
        .expect("remove tx");
    let mut after = before.clone();
    tx.apply_to(&mut after).expect("apply tx");

    let scope = LayoutScope::from_transaction(&after, &tx);

    assert_eq!(
        scope.nodes(),
        Some(&[NodeId::from_u128(2)].into_iter().collect())
    );
}

#[test]
fn dirty_scope_can_be_empty_for_non_layout_metadata_changes() {
    let graph = connected_graph();
    let tx = GraphTransaction::from_ops([GraphOp::SetImportAlias {
        id: GraphId::from_u128(2),
        from: None,
        to: Some("demo".to_string()),
    }]);

    let scope = LayoutScope::from_transaction(&graph, &tx);

    assert!(scope.is_empty());
}

fn graph_with_one_node(node: NodeId) -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    graph.insert_node(
        node,
        Node {
            kind: NodeKindKey::new("demo.node"),
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
            size: None,
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: serde_json::Value::Null,
        },
    );
    graph.build_unchecked()
}

fn connected_graph() -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::from_u128(1);
    let b = NodeId::from_u128(2);
    let out = PortId::from_u128(3);
    let inn = PortId::from_u128(4);
    let edge = EdgeId::from_u128(5);

    graph.insert_node(a, node("demo.a", vec![out]));
    graph.insert_node(b, node("demo.b", vec![inn]));
    graph.insert_port(out, port(a, "out", PortDirection::Out));
    graph.insert_port(inn, port(b, "in", PortDirection::In));
    graph.insert_edge(edge, data_edge(out, inn));
    graph.build_unchecked()
}

fn connected_graph_with_third_node() -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let a = NodeId::from_u128(1);
    let b = NodeId::from_u128(2);
    let c = NodeId::from_u128(6);
    let out = PortId::from_u128(3);
    let old_inn = PortId::from_u128(4);
    let new_inn = PortId::from_u128(7);
    let edge = EdgeId::from_u128(5);

    graph.insert_node(a, node("demo.a", vec![out]));
    graph.insert_node(b, node("demo.b", vec![old_inn]));
    graph.insert_node(c, node("demo.c", vec![new_inn]));
    graph.insert_port(out, port(a, "out", PortDirection::Out));
    graph.insert_port(old_inn, port(b, "old-in", PortDirection::In));
    graph.insert_port(new_inn, port(c, "new-in", PortDirection::In));
    graph.insert_edge(edge, data_edge(out, new_inn));
    graph.build_unchecked()
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
        size: None,
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
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}

fn data_edge(from: PortId, to: PortId) -> Edge {
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
