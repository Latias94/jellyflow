use std::collections::BTreeSet;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::TypeDesc;
use jellyflow_layout::{
    LayoutContext, LayoutEngineRequest, LayoutRequest, builtin_layout_engine_registry,
};
use jellyflow_runtime::io::{GraphFile, PersistenceLimits, ViewSessionSeed, ViewSessionSeedFile};
use jellyflow_runtime::runtime::{layout, viewport};
use jellyflow_runtime::schema::{
    NodeCatalog, NodeKindSpec, NodePortSpec, PortPresentationHint, PortSide,
};
use jellyflow_runtime::{
    AdapterEnvironment, CanvasInteractionConfig, CanvasInteractionQuery, CanvasInteractionSession,
    ConnectionEdit, Diagnostic, DiagnosticTarget, EdgePickQuery, FeedbackCollector, FrameRequest,
    GraphCommand, GraphProfile, GraphRuntime, GraphRuntimePolicy, HandlePosition,
    IncidentEdgePolicy, InteractionPosition, NodeLayoutTarget, RendererStamp, TopologyEdit,
    TopologyPort, ViewPoint, ViewRect, ViewSessionOptions, ViewSize, VisibleRouteQuery,
};

fn graph_with_one_port() -> (Graph, NodeId, PortId) {
    let graph_id = GraphId::from_u128(0x501);
    let node = NodeId::from_u128(0x502);
    let port = PortId::from_u128(0x503);
    let graph = GraphBuilder::new(graph_id)
        .with_node(
            node,
            Node {
                kind: NodeKindKey::new("public.native"),
                kind_version: 1,
                pos: CanvasPoint { x: 40.0, y: 60.0 },
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
                ports: vec![port],
                data: serde_json::json!({"label": "native"}),
            },
        )
        .with_port(
            port,
            Port {
                node,
                key: PortKey::new("output"),
                dir: PortDirection::Out,
                kind: PortKind::Data,
                capacity: PortCapacity::Multi,
                connectable: None,
                connectable_start: None,
                connectable_end: None,
                ty: None,
                data: serde_json::Value::Null,
            },
        )
        .build()
        .expect("public graph fixture");
    (graph, node, port)
}

#[test]
fn view_seed_public_surface_returns_only_the_sanitized_seed() {
    let seed = ViewSessionSeed::new(GraphId::from_u128(0x500));
    let bytes = ViewSessionSeedFile::encode_json(&seed).expect("encode public seed");

    let decoded: ViewSessionSeed =
        ViewSessionSeedFile::decode_json(&bytes).expect("decode public seed");

    assert_eq!(decoded, seed);
}

#[test]
fn bounded_graph_save_is_part_of_the_public_persistence_surface() {
    let directory = tempfile::tempdir().expect("temporary public persistence directory");
    let path = directory.path().join("graph.json");
    let graph_id = GraphId::from_u128(0x505);
    let limits = PersistenceLimits::default().with_max_bytes(1024 * 1024);

    GraphFile::from_graph(Graph::new(graph_id))
        .save_json_with_limits(&path, limits)
        .expect("bounded public graph save");
    let loaded = GraphFile::load_canonical_json_with_limits(&path, &NodeCatalog::new(), limits)
        .expect("bounded public graph load");

    assert_eq!(loaded.graph().graph_id(), graph_id);
}

#[test]
fn runtime_manifest_stays_framework_free() {
    let manifest = include_str!("../Cargo.toml");
    for forbidden in [
        "fret-core",
        "fret-ui",
        "fret-runtime",
        "egui",
        "open-gpui",
        "open_gpui",
        "wgpu",
        "winit",
    ] {
        assert!(
            !manifest.contains(forbidden),
            "forbidden dependency: {forbidden}"
        );
    }
}

#[test]
fn graph_runtime_and_view_session_are_the_public_adapter_seam() {
    let (graph, node, port) = graph_with_one_port();
    let policy = GraphRuntimePolicy::default().with_edge_reconnection(false);
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .with_policy(policy)
        .build(graph)
        .expect("runtime");
    assert_eq!(runtime.policy().unwrap(), policy);
    let view = runtime
        .attach_view(ViewSessionOptions::default())
        .expect("view");
    assert_eq!(view.graph_snapshot().unwrap().revision(), 0);

    let frame = view
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 800.0,
                    height: 600.0,
                },
            },
            RendererStamp::new("public.native", 1),
        ))
        .expect("frame");
    let frame_node = frame
        .nodes()
        .iter()
        .find(|candidate| candidate.id() == node)
        .expect("node frame");
    let frame_port = frame_node
        .ports()
        .iter()
        .find(|candidate| candidate.id() == port)
        .expect("port frame");

    let node_bounds = ViewRect::new(ViewPoint::new(80.0, 120.0), ViewSize::new(240.0, 144.0));
    let port_bounds = ViewRect::new(ViewPoint::new(308.0, 180.0), ViewSize::new(12.0, 12.0));
    let port_hit_bounds = ViewRect::new(ViewPoint::new(300.0, 172.0), ViewSize::new(28.0, 28.0));
    let mut layout = frame_node.begin_layout(node_bounds);
    layout
        .port_rendered(
            frame_port.layout_target(),
            port_bounds,
            port_hit_bounds,
            ViewPoint::new(320.0, 186.0),
            HandlePosition::Right,
        )
        .expect("complete port layout");
    let collector = frame.collector();
    collector
        .layout(layout.finish().expect("complete node layout"))
        .expect("layout");
    let receipt = view
        .submit_feedback(collector.finish().expect("feedback"))
        .expect("submit");
    assert_eq!(receipt.operations().len(), 1);
    assert!(receipt.publication().is_some());
    assert_eq!(view.geometry_snapshot().unwrap().layout_revision(), 1);
    let _visible_routes = VisibleRouteQuery::new(
        ViewRect::new(ViewPoint::new(0.0, 0.0), ViewSize::new(800.0, 600.0)),
        64,
    )
    .with_margin(24.0);
    let _edge_pick = EdgePickQuery::new(ViewPoint::new(160.0, 186.0), 4.0, 64);
    let mut interaction = CanvasInteractionSession::new(
        view.id(),
        view.generation().unwrap(),
        CanvasInteractionConfig::default(),
    )
    .unwrap();
    let position = InteractionPosition::new(
        ViewPoint::new(160.0, 186.0),
        CanvasPoint { x: 80.0, y: 93.0 },
    )
    .unwrap();
    let edge_query = interaction.request_edge_pick(position).unwrap();
    let (queries, effects, damage, tick_demand) = edge_query.into_parts();
    assert!(matches!(
        queries.as_slice(),
        [CanvasInteractionQuery::EdgePick { .. }]
    ));
    assert!(effects.is_empty());
    assert_eq!(damage, jellyflow_runtime::InteractionDamage::default());
    assert_eq!(tick_demand, jellyflow_runtime::InteractionTickDemand::None);

    assert!(
        view.set_adapter_environment(AdapterEnvironment::new(2, 3, 4, 5))
            .unwrap()
    );
    assert!(
        !view
            .set_adapter_environment(AdapterEnvironment::new(2, 3, 4, 5))
            .unwrap()
    );

    let outcome = runtime
        .execute(GraphCommand::topology(
            0,
            jellyflow_runtime::TopologyEdit::new(),
        ))
        .expect("noop command");
    assert!(outcome.applied().is_none());
}

#[test]
fn replace_node_ports_declares_retained_and_new_port_ids() {
    let (_graph, node, retained_port) = graph_with_one_port();
    let new_port = PortId::from_u128(0x504);
    let command = GraphCommand::topology(
        0,
        TopologyEdit::new().replace_node_ports(
            node,
            [
                TopologyPort::new(
                    retained_port,
                    public_port(node, "output", PortDirection::Out),
                ),
                TopologyPort::new(new_port, public_port(node, "input", PortDirection::In)),
            ],
            IncidentEdgePolicy::Reject,
        ),
    );

    let declared = command.declared_footprint();

    assert_eq!(declared.nodes, BTreeSet::from([node]));
    assert_eq!(declared.ports, BTreeSet::from([retained_port, new_port]));
}

#[test]
fn graph_profile_keeps_type_policy_and_structured_connection_diagnostics_public() {
    let graph_id = GraphId::from_u128(0x551);
    let source_node = NodeId::from_u128(0x552);
    let target_node = NodeId::from_u128(0x553);
    let source_port = PortId::from_u128(0x554);
    let target_port = PortId::from_u128(0x555);
    let graph = GraphBuilder::new(graph_id)
        .with_node(source_node, public_node("public.source", vec![source_port]))
        .with_node(target_node, public_node("public.target", vec![target_port]))
        .with_port(
            source_port,
            public_port(source_node, "output", PortDirection::Out),
        )
        .with_port(
            target_port,
            public_port(target_node, "input", PortDirection::In),
        )
        .build()
        .expect("typed public graph fixture");
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .with_profile(Box::new(PublicTypedProfile {
            source: source_port,
            target: target_port,
        }))
        .build(graph)
        .expect("profiled runtime");
    let edit = ConnectionEdit::connect(
        EdgeId::from_u128(0x556),
        source_port,
        target_port,
        NodeGraphConnectionMode::Strict,
    );

    let evaluation = runtime.evaluate_connection(edit).expect("evaluation");
    assert!(!evaluation.is_accept());
    assert_eq!(evaluation.diagnostics()[0].key, "connect.type_mismatch");
    assert!(matches!(
        evaluation.diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == target_port
    ));

    let outcome = runtime
        .execute(GraphCommand::connection(evaluation.revision(), edit))
        .expect("connection command");
    let rejection = outcome.rejection().expect("profile rejection");
    assert_eq!(
        rejection.diagnostics()[0].key,
        evaluation.diagnostics()[0].key
    );
    assert_eq!(
        rejection.diagnostics()[0].message,
        evaluation.diagnostics()[0].message
    );
    assert!(matches!(
        rejection.diagnostics()[0].target,
        DiagnosticTarget::Port { id } if id == target_port
    ));
}

struct PublicTypedProfile {
    source: PortId,
    target: PortId,
}

impl GraphProfile for PublicTypedProfile {
    fn type_of_port(&self, _graph: &Graph, port: PortId) -> Option<TypeDesc> {
        if port == self.source {
            Some(TypeDesc::String)
        } else if port == self.target {
            Some(TypeDesc::Int)
        } else {
            None
        }
    }

    fn validate_graph(&self, _graph: &Graph) -> Vec<Diagnostic> {
        Vec::new()
    }
}

fn public_node(kind: &str, ports: Vec<PortId>) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos: CanvasPoint::default(),
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

fn public_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
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

#[test]
fn semantic_catalog_and_layout_planning_are_framework_neutral() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("public.value", "Value", "public.value")
                .category(["Workflow"])
                .default_size(CanvasSize {
                    width: 120.0,
                    height: 72.0,
                })
                .port(
                    NodePortSpec::new(
                        "value",
                        PortDirection::Out,
                        PortKind::Data,
                        PortCapacity::Multi,
                    )
                    .with_presentation(PortPresentationHint::new().with_side(PortSide::Right)),
                )
                .build(),
        )
        .expect("catalog registration");
    let creation = catalog
        .instantiate_node(&NodeKindKey::new("public.value"), CanvasPoint::default())
        .expect("catalog creation");
    let runtime = GraphRuntime::builder(catalog)
        .build(Graph::new(GraphId::from_u128(0x601)))
        .expect("runtime");
    runtime
        .execute(creation.into_command(0))
        .expect("creation command")
        .applied()
        .expect("creation applies");

    let graph = runtime.snapshot().unwrap().graph().clone();
    let request = LayoutEngineRequest::dugong(LayoutRequest::all());
    let result = builtin_layout_engine_registry()
        .layout(&graph, &request, &LayoutContext::new())
        .expect("layout result");
    let command = layout::layout_result_to_command(1, &graph, &result).expect("layout command");
    assert_eq!(command.expected_revision(), 1);

    let transform = viewport::ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    assert!(transform.is_valid());
    let _ = std::mem::size_of::<FeedbackCollector>();
    let _ = std::mem::size_of::<NodeLayoutTarget>();
}
