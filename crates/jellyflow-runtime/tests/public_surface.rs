use std::collections::BTreeSet;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, EdgeId, Graph, GraphBuilder, GraphId, Node, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use jellyflow_core::types::TypeDesc;
use jellyflow_runtime::commands::{
    ConnectionEdit, GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort,
};
use jellyflow_runtime::geometry::{HandlePosition, ViewPoint, ViewRect, ViewSize};
use jellyflow_runtime::graph::{Diagnostic, DiagnosticTarget, GraphRuntime, GraphRuntimePolicy};
use jellyflow_runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionSession, InteractionDamage, InteractionTickDemand,
};
use jellyflow_runtime::io::{GraphFile, PersistenceLimits, ViewSessionSeed, ViewSessionSeedFile};
use jellyflow_runtime::profile::GraphProfile;
use jellyflow_runtime::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};
use jellyflow_runtime::view::{
    AdapterEnvironment, EdgePickQuery, FeedbackCollector, FrameRequest, NodeLayoutTarget,
    RendererStamp, ViewSessionOptions, VisibleRouteQuery,
};
use jellyflow_runtime::viewport::ViewportTransform;

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
    let initial_snapshot = view.graph_snapshot().unwrap();
    assert_eq!(initial_snapshot.revision(), 0);
    let node_lifetime = initial_snapshot
        .node_lifetime_key(node)
        .expect("public snapshot exposes exact node lifetime identity");
    assert_eq!(
        runtime.node_lifetime_key(node).unwrap(),
        Some(node_lifetime),
    );
    let port_instance = runtime
        .port_instance_key(port)
        .unwrap()
        .expect("public runtime exposes port lifetime identity");
    assert_eq!(port_instance.owner(), node_lifetime);
    assert_eq!(port_instance.port(), port);

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
    assert_eq!(frame_node.instance().lifetime(), node_lifetime);
    let frame_port = frame_node
        .ports()
        .iter()
        .find(|candidate| candidate.id() == port)
        .expect("port frame");
    assert_eq!(frame_port.instance(), port_instance);

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
    assert_eq!(receipt.layout_receipts().count(), 1);
    assert!(receipt.publication().is_some());
    assert_eq!(view.geometry_snapshot().unwrap().layout_revision(), 1);
    let _visible_routes = VisibleRouteQuery::new(
        ViewRect::new(ViewPoint::new(0.0, 0.0), ViewSize::new(800.0, 600.0)),
        64,
    )
    .with_margin(24.0);
    let _edge_pick = EdgePickQuery::new(ViewPoint::new(160.0, 186.0), 4.0, 64);
    let mut interaction =
        CanvasInteractionSession::attach(&view, CanvasInteractionConfig::default()).unwrap();
    let sample = view
        .viewport_pointer_sample(ViewPoint::new(160.0, 186.0))
        .unwrap();
    let edge_query = interaction.request_edge_pick(sample).unwrap();
    let dispatch = edge_query.into_dispatch();
    let effects = dispatch.effects;
    let damage = dispatch.damage;
    let tick_demand = dispatch.tick_demand;
    assert!(
        dispatch.query_work_pending,
        "edge picking leaves its query continuation inside the session"
    );
    assert!(interaction.query_work_pending());
    assert!(effects.is_empty());
    assert_eq!(damage, InteractionDamage::default());
    assert_eq!(tick_demand, InteractionTickDemand::None);
    let (resolved, needs_tick, receipt, terminal_error) =
        interaction.advance_query_work(1).into_parts();
    assert_eq!(receipt.pages_granted(), 1);
    assert_eq!(receipt.pages_attempted(), 1);
    assert_eq!(receipt.page_receipts().len(), 1);
    assert!(terminal_error.is_none());
    assert!(!needs_tick);
    assert!(resolved.effects().is_empty());
    assert!(!resolved.query_work_pending());
    assert!(!interaction.query_work_pending());

    assert!(
        view.set_adapter_environment(AdapterEnvironment::new(2, 3, 4, 5))
            .unwrap()
    );
    assert!(
        !view
            .set_adapter_environment(AdapterEnvironment::new(2, 3, 4, 5))
            .unwrap()
    );

    let version = runtime.snapshot().expect("snapshot").version();
    let outcome = runtime
        .execute(GraphCommand::topology(version, TopologyEdit::new()))
        .into_result()
        .expect("noop command");
    assert!(outcome.applied().is_none());
}

#[test]
fn replace_node_ports_declares_retained_and_new_port_ids() {
    let (graph, node, retained_port) = graph_with_one_port();
    let runtime = GraphRuntime::builder(NodeCatalog::new())
        .build(graph)
        .expect("public graph runtime");
    let new_port = PortId::from_u128(0x504);
    let command = GraphCommand::topology(
        runtime.snapshot().expect("snapshot").version(),
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
        .execute(GraphCommand::connection(evaluation.version(), edit))
        .into_result()
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
fn semantic_catalog_and_viewport_planning_are_framework_neutral() {
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("public.value")
                .port(NodePortSpec::new(
                    "value",
                    PortDirection::Out,
                    PortKind::Data,
                    PortCapacity::Multi,
                ))
                .build(),
        )
        .expect("catalog registration");
    let creation = catalog
        .instantiate_node(
            &NodeKindKey::new("public.value"),
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 120.0,
                    height: 72.0,
                },
            },
        )
        .expect("catalog creation");
    let runtime = GraphRuntime::builder(catalog)
        .build(Graph::new(GraphId::from_u128(0x601)))
        .expect("runtime");
    let version = runtime.snapshot().expect("snapshot").version();
    runtime
        .execute(creation.into_command(version))
        .into_result()
        .expect("creation command")
        .applied()
        .expect("creation applies");

    assert_eq!(runtime.snapshot().unwrap().revision(), 1);

    let transform = ViewportTransform::new(CanvasPoint::default(), 1.0).unwrap();
    assert!(transform.is_valid());
    let _ = std::mem::size_of::<FeedbackCollector>();
    let _ = std::mem::size_of::<NodeLayoutTarget>();
}
