use jellyflow_core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    GraphMutationFootprint, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};

use crate::{
    LayoutContext, LayoutCoordinateFrame, LayoutEdgeRoute, LayoutEngine, LayoutEngineDescriptor,
    LayoutEngineId, LayoutEngineMetadata, LayoutEngineRegistry, LayoutEngineRequest, LayoutError,
    LayoutFamilyMetadata, LayoutNodePlacement, LayoutOptions, LayoutPlacements, LayoutRequest,
    LayoutScope, LayoutSpacing, PreparedLayoutInput,
};

#[derive(Clone, Copy)]
struct TestEngine;

impl LayoutEngine for TestEngine {
    fn place(&self, input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        Ok(LayoutPlacements::preserve_canvas_coordinates(
            input.nodes().map(|node| LayoutNodePlacement {
                node: node.id(),
                center: CanvasPoint { x: 60.0, y: 40.0 },
            }),
        ))
    }
}

#[derive(Clone)]
struct StaticPlacementsEngine {
    placements: LayoutPlacements,
}

impl LayoutEngine for StaticPlacementsEngine {
    fn place(&self, _input: &PreparedLayoutInput<'_>) -> Result<LayoutPlacements, LayoutError> {
        Ok(self.placements.clone())
    }
}

#[test]
fn registry_executes_custom_engine_and_returns_canvas_positions() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry
        .register(test_descriptor(engine.clone(), TestEngine))
        .unwrap();

    let node = NodeId::from_u128(1);
    let graph = graph_with_one_node(node);
    let request = LayoutEngineRequest::new(
        engine.clone(),
        LayoutRequest::all().with_options(LayoutOptions {
            default_node_size: CanvasSize {
                width: 100.0,
                height: 40.0,
            },
            ..LayoutOptions::default()
        }),
    );
    let result = registry
        .layout(&graph, &request, &LayoutContext::default())
        .unwrap();

    assert_eq!(registry.engine_ids().collect::<Vec<_>>(), vec![&engine]);
    assert!(registry.contains(&engine));
    assert_eq!(
        result.node_position(node).expect("node position").pos,
        CanvasPoint { x: 10.0, y: 20.0 }
    );
}

#[test]
fn registry_rejects_duplicate_engine_ids() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry
        .register(test_descriptor(engine.clone(), TestEngine))
        .unwrap();

    let err = registry
        .register(test_descriptor(engine.clone(), TestEngine))
        .expect_err("duplicate engine");

    assert_eq!(err, LayoutError::DuplicateLayoutEngine(engine));
}

#[test]
fn registry_reports_missing_engine() {
    let registry = LayoutEngineRegistry::new();
    let missing = LayoutEngineId::new("missing");
    let graph = GraphBuilder::new(GraphId::from_u128(1));
    let request = LayoutEngineRequest::new(missing.clone(), LayoutRequest::all());

    let err = registry
        .layout(&graph, &request, &LayoutContext::default())
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
fn registry_rejects_incomplete_custom_placements() {
    let mut registry = LayoutEngineRegistry::new();
    let engine = LayoutEngineId::new("custom.test");
    registry
        .register(test_descriptor(
            engine.clone(),
            StaticPlacementsEngine {
                placements: LayoutPlacements::normalize_to_margin([]),
            },
        ))
        .unwrap();
    let node = NodeId::from_u128(1);
    let graph = graph_with_one_node(node);
    let request = LayoutEngineRequest::new(engine, LayoutRequest::all());

    let err = registry
        .layout(&graph, &request, &LayoutContext::default())
        .expect_err("custom engines must return every scoped node");

    assert_eq!(err, LayoutError::MissingNodePosition(node));
}

#[test]
fn registry_rejects_non_finite_custom_result_geometry() {
    let engine = LayoutEngineId::new("custom.invalid");
    let node = NodeId::from_u128(1);
    let mut registry = LayoutEngineRegistry::new();
    registry
        .register(test_descriptor(
            engine.clone(),
            StaticPlacementsEngine {
                placements: LayoutPlacements::preserve_canvas_coordinates([LayoutNodePlacement {
                    node,
                    center: CanvasPoint {
                        x: f32::NAN,
                        y: 20.0,
                    },
                }]),
            },
        ))
        .unwrap();

    let err = registry
        .layout(
            &graph_with_one_node(node),
            &LayoutEngineRequest::new(engine, LayoutRequest::all()),
            &LayoutContext::default(),
        )
        .expect_err("non-finite custom geometry must be rejected");

    assert_eq!(err, LayoutError::InvalidResultNodeGeometry(node));
}

#[test]
fn layout_resolves_node_size_precedence_for_every_engine() {
    let nodes = [
        NodeId::from_u128(1),
        NodeId::from_u128(2),
        NodeId::from_u128(3),
        NodeId::from_u128(4),
    ];
    let mut graph = graph_with_nodes(nodes);
    let graph_size = CanvasSize {
        width: 300.0,
        height: 70.0,
    };
    let request_size = CanvasSize {
        width: 80.0,
        height: 50.0,
    };
    let context_size = CanvasSize {
        width: 60.0,
        height: 30.0,
    };
    let default_size = CanvasSize {
        width: 20.0,
        height: 20.0,
    };
    graph.node_mut(&nodes[0]).expect("node").size = Some(graph_size);
    let request = LayoutRequest::all()
        .with_measured_node_sizes([
            (
                nodes[0],
                CanvasSize {
                    width: 1.0,
                    height: 1.0,
                },
            ),
            (nodes[1], request_size),
        ])
        .with_options(LayoutOptions {
            default_node_size: default_size,
            ..LayoutOptions::default()
        });
    let context = LayoutContext::new().with_measured_node_sizes([
        (
            nodes[1],
            CanvasSize {
                width: 2.0,
                height: 2.0,
            },
        ),
        (nodes[2], context_size),
    ]);
    let (registry, engine) = test_registry("custom.sizes", TestEngine);

    let result = registry
        .layout(&graph, &LayoutEngineRequest::new(engine, request), &context)
        .expect("layout");

    assert_eq!(
        result.node_position(nodes[0]).expect("graph").size,
        graph_size
    );
    assert_eq!(
        result.node_position(nodes[1]).expect("request").size,
        request_size
    );
    assert_eq!(
        result.node_position(nodes[2]).expect("context").size,
        context_size
    );
    assert_eq!(
        result.node_position(nodes[3]).expect("default").size,
        default_size
    );
}

#[test]
fn layout_applies_origin_bounds_and_routes_in_one_coordinate_frame() {
    let mut graph = connected_graph();
    let source = NodeId::from_u128(1);
    let target = NodeId::from_u128(2);
    let edge = EdgeId::from_u128(5);
    for node in [source, target] {
        graph.node_mut(&node).expect("node").size = Some(CanvasSize {
            width: 100.0,
            height: 40.0,
        });
    }
    graph.node_mut(&source).expect("source").origin =
        Some(jellyflow_core::NodeOrigin { x: 0.5, y: 0.5 });
    graph.node_mut(&target).expect("target").origin =
        Some(jellyflow_core::NodeOrigin { x: 1.0, y: 1.0 });
    let placements = LayoutPlacements {
        nodes: vec![
            LayoutNodePlacement {
                node: source,
                center: CanvasPoint { x: 100.0, y: 100.0 },
            },
            LayoutNodePlacement {
                node: target,
                center: CanvasPoint { x: 250.0, y: 100.0 },
            },
        ],
        edge_routes: vec![LayoutEdgeRoute {
            edge,
            points: vec![
                CanvasPoint { x: 100.0, y: 100.0 },
                CanvasPoint { x: 250.0, y: 100.0 },
            ],
        }],
        coordinate_frame: LayoutCoordinateFrame::NormalizeToMargin,
    };
    let (registry, engine) =
        test_registry("custom.finalizer", StaticPlacementsEngine { placements });
    let request = LayoutRequest::all().with_options(LayoutOptions {
        margin: CanvasSize {
            width: 10.0,
            height: 20.0,
        },
        ..LayoutOptions::default()
    });

    let result = registry
        .layout(
            &graph,
            &LayoutEngineRequest::new(engine, request),
            &LayoutContext::default(),
        )
        .expect("layout");
    let source_position = result.node_position(source).expect("source");
    let target_position = result.node_position(target).expect("target");
    let bounds = result.bounds.expect("bounds");
    let route = result.edge_routes.first().expect("route");

    assert_eq!(bounds.origin, CanvasPoint { x: 10.0, y: 20.0 });
    assert_eq!(source_position.pos, source_position.center);
    assert_eq!(
        target_position.pos,
        CanvasPoint {
            x: target_position.center.x + 50.0,
            y: target_position.center.y + 20.0,
        }
    );
    assert_eq!(
        route.points,
        vec![source_position.center, target_position.center]
    );
}

#[test]
fn layout_rejects_invalid_requests_and_geometry() {
    let node = NodeId::from_u128(1);
    let graph = graph_with_one_node(node);
    let (registry, engine) = test_registry("custom.validation", TestEngine);

    let spacing = LayoutSpacing {
        nodesep: -1.0,
        ..LayoutSpacing::default()
    };
    let err = registry
        .layout(
            &graph,
            &LayoutEngineRequest::new(
                engine.clone(),
                LayoutRequest::all().with_options(LayoutOptions {
                    spacing,
                    ..LayoutOptions::default()
                }),
            ),
            &LayoutContext::default(),
        )
        .expect_err("spacing");
    assert_eq!(err, LayoutError::InvalidSpacing(spacing));

    let margin = CanvasSize {
        width: f32::INFINITY,
        height: 0.0,
    };
    let err = registry
        .layout(
            &graph,
            &LayoutEngineRequest::new(
                engine.clone(),
                LayoutRequest::all().with_options(LayoutOptions {
                    margin,
                    ..LayoutOptions::default()
                }),
            ),
            &LayoutContext::default(),
        )
        .expect_err("margin");
    assert_eq!(err, LayoutError::InvalidMargin(margin));

    let missing = NodeId::from_u128(99);
    let err = registry
        .layout(
            &graph,
            &LayoutEngineRequest::new(engine.clone(), LayoutRequest::nodes([missing])),
            &LayoutContext::default(),
        )
        .expect_err("scope");
    assert_eq!(err, LayoutError::MissingScopeNode(missing));

    let invalid_size = CanvasSize {
        width: 0.0,
        height: 1.0,
    };
    let err = registry
        .layout(
            &graph,
            &LayoutEngineRequest::new(
                engine.clone(),
                LayoutRequest::all().with_measured_node_sizes([(node, invalid_size)]),
            ),
            &LayoutContext::default(),
        )
        .expect_err("size");
    assert_eq!(
        err,
        LayoutError::InvalidNodeSize {
            node,
            size: invalid_size
        }
    );

    let mut extreme = graph_with_one_node(node);
    let extreme_node = extreme.node_mut(&node).expect("node");
    extreme_node.pos.x = f32::MAX;
    extreme_node.size = Some(CanvasSize {
        width: f32::MAX,
        height: 36.0,
    });
    let err = registry
        .layout(
            &extreme,
            &LayoutEngineRequest::new(engine, LayoutRequest::all()),
            &LayoutContext::default(),
        )
        .expect_err("extreme geometry");
    assert_eq!(err, LayoutError::InvalidResultNodeGeometry(node));
}

#[test]
fn layout_filters_projection_and_validates_edge_endpoints() {
    let source = NodeId::from_u128(1);
    let target = NodeId::from_u128(2);
    let edge = EdgeId::from_u128(5);
    let (registry, engine) = test_registry("custom.projection", TestEngine);

    let mut hidden = connected_graph();
    hidden.node_mut(&target).expect("target").hidden = true;
    let result = registry
        .layout(
            &hidden,
            &LayoutEngineRequest::new(engine.clone(), LayoutRequest::all()),
            &LayoutContext::default(),
        )
        .expect("hidden projection");
    assert!(result.node_position(source).is_some());
    assert!(result.node_position(target).is_none());
    assert!(result.edge_routes.is_empty());

    let scoped = connected_graph();
    let result = registry
        .layout(
            &scoped,
            &LayoutEngineRequest::new(engine.clone(), LayoutRequest::nodes([source])),
            &LayoutContext::default(),
        )
        .expect("scoped projection");
    assert_eq!(result.nodes.len(), 1);
    assert!(result.edge_routes.is_empty());

    let mut missing_source = connected_graph();
    let source_port = missing_source.edges().get(&edge).expect("edge").from;
    missing_source.remove_port(&source_port);
    let err = registry
        .layout(
            &missing_source,
            &LayoutEngineRequest::new(engine.clone(), LayoutRequest::all()),
            &LayoutContext::default(),
        )
        .expect_err("missing source port");
    assert_eq!(err, LayoutError::MissingSourcePort(edge));

    let mut missing_target = connected_graph();
    missing_target.remove_node(&target);
    let err = registry
        .layout(
            &missing_target,
            &LayoutEngineRequest::new(engine, LayoutRequest::all()),
            &LayoutContext::default(),
        )
        .expect_err("missing target node");
    assert_eq!(err, LayoutError::MissingTargetNode { edge });
}

#[test]
fn dirty_scope_expands_reconnect_footprint_to_old_and_new_endpoint_nodes() {
    let graph = connected_graph_with_third_node();
    let mut footprint = GraphMutationFootprint::new();
    footprint.touch_edge(EdgeId::from_u128(5));
    footprint.touch_port(PortId::from_u128(4));

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

#[test]
fn graph_wide_footprint_requires_all_nodes() {
    let graph = connected_graph();
    let mut footprint = GraphMutationFootprint::new();
    footprint.touch_graph();

    assert_eq!(
        LayoutScope::from_footprint(&graph, &footprint),
        LayoutScope::All
    );
}

fn test_descriptor<E>(id: LayoutEngineId, engine: E) -> LayoutEngineDescriptor
where
    E: LayoutEngine + 'static,
{
    let family = LayoutFamilyMetadata::new("custom", "Custom");
    let metadata = LayoutEngineMetadata::new(id, family.id.clone(), "Custom engine");
    LayoutEngineDescriptor::new(family, metadata, engine)
}

fn test_registry<E>(id: &str, engine: E) -> (LayoutEngineRegistry, LayoutEngineId)
where
    E: LayoutEngine + 'static,
{
    let id = LayoutEngineId::new(id);
    let mut registry = LayoutEngineRegistry::new();
    registry
        .register(test_descriptor(id.clone(), engine))
        .expect("register test engine");
    (registry, id)
}

fn graph_after_source_removed() -> Graph {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    let node_id = NodeId::from_u128(2);
    let port_id = PortId::from_u128(4);

    graph.insert_node(node_id, node("demo.b", vec![port_id]));
    graph.insert_port(port_id, port(node_id, "in", PortDirection::In));
    graph.build_unchecked()
}

fn graph_with_one_node(node: NodeId) -> GraphBuilder {
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
    graph
}

fn graph_with_nodes(nodes: impl IntoIterator<Item = NodeId>) -> GraphBuilder {
    let mut graph = GraphBuilder::new(GraphId::from_u128(1));
    for node_id in nodes {
        graph.insert_node(node_id, node("demo.node", Vec::new()));
    }
    graph
}

fn connected_graph() -> GraphBuilder {
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
    graph
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
