use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    GraphMutationFootprint, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
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

    let encoded = serde_json::to_value(&request).expect("serialize");
    assert!(encoded["layout"]["options"].get("node_origin").is_none());
    let decoded: LayoutEngineRequest = serde_json::from_value(encoded).expect("deserialize");

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

    assert!(result.nodes.is_empty());
}

#[test]
fn generic_engine_returns_layout_result() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry.insert(TestEngine::new(engine.clone())).unwrap();
    let node = NodeId::from_u128(1);
    let graph = graph_with_one_node(node);
    let request = LayoutEngineRequest::new(engine, LayoutRequest::all());

    let result =
        layout_graph_with_engine(&graph, &request, &registry, &LayoutContext::default()).unwrap();

    assert_eq!(
        result.node_position(node).expect("node position").pos,
        CanvasPoint { x: 10.0, y: 20.0 }
    );
}

#[test]
fn dirty_scope_expands_touched_edge_to_endpoint_nodes() {
    let graph = connected_graph();
    let mut footprint = GraphMutationFootprint::new();
    footprint.touch_edge(EdgeId::from_u128(5));

    let scope = LayoutScope::from_footprint(&graph, &footprint);

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
fn dirty_scope_expands_reconnect_footprint_to_old_and_new_endpoint_nodes() {
    let graph = connected_graph_with_third_node();
    let mut footprint = GraphMutationFootprint::new();
    footprint.touch_edge(EdgeId::from_u128(5));
    footprint.touch_port(PortId::from_u128(3));
    footprint.touch_port(PortId::from_u128(4));
    footprint.touch_port(PortId::from_u128(7));

    let request = LayoutRequest::all().with_dirty_scope_from_footprint(&graph, &footprint);

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
    let after = graph_after_source_removed();
    let mut footprint = GraphMutationFootprint::new();
    footprint.touch_node(NodeId::from_u128(1));
    footprint.touch_port(PortId::from_u128(3));
    footprint.touch_port(PortId::from_u128(4));
    footprint.touch_edge(EdgeId::from_u128(5));

    let scope = LayoutScope::from_footprint(&after, &footprint);

    assert_eq!(
        scope.nodes(),
        Some(&[NodeId::from_u128(2)].into_iter().collect())
    );
}

#[test]
fn dirty_scope_can_be_empty_for_non_layout_metadata_changes() {
    let graph = connected_graph();
    let mut footprint = GraphMutationFootprint::new();
    footprint.imports.insert(GraphId::from_u128(2));

    let scope = LayoutScope::from_footprint(&graph, &footprint);

    assert!(scope.is_empty());
}

fn graph_after_source_removed() -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let node_id = NodeId::from_u128(2);
    let port_id = PortId::from_u128(4);

    graph.insert_node(node_id, node("demo.b", vec![port_id]));
    graph.insert_port(port_id, port(node_id, "in", PortDirection::In));
    graph.build_unchecked()
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
    Edge::new(EdgeKind::Data, from, to)
}
