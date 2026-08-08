use egui::{Sense, Ui, Vec2};
use egui_kittest::Harness;
use jellyflow_core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, Graph, GraphBuilder, GraphId,
    Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_egui::{EguiNativeFrameHost, EguiNativeFrameOutcome};
use jellyflow_runtime::geometry::HandlePosition;
use jellyflow_runtime::graph::GraphRuntime;
use jellyflow_runtime::view::{FrameRequest, LayoutNodeStatus, RendererStamp, ViewSessionOptions};

const STRUCTURED_RENDERER: &str = "test.egui.structured-native";

struct StructuredFixture {
    graph: Graph,
    nodes: Vec<NodeId>,
    ports: Vec<PortId>,
    edges: Vec<EdgeId>,
}

struct StructuredState {
    _runtime: GraphRuntime,
    host: EguiNativeFrameHost,
    nodes: Vec<NodeId>,
    ports: Vec<PortId>,
    edges: Vec<EdgeId>,
    outcome: Option<EguiNativeFrameOutcome>,
}

impl StructuredState {
    fn new(fixture: StructuredFixture) -> Self {
        let runtime = GraphRuntime::builder(Default::default())
            .build(fixture.graph)
            .expect("structured runtime");
        let view = runtime
            .attach_view(ViewSessionOptions::default())
            .expect("structured view");
        Self {
            _runtime: runtime,
            host: EguiNativeFrameHost::new(view),
            nodes: fixture.nodes,
            ports: fixture.ports,
            edges: fixture.edges,
            outcome: None,
        }
    }
}

fn render_structured_graph(ui: &mut Ui, state: &mut StructuredState) {
    state.outcome = Some(
        state
            .host
            .show(ui, structured_request(), |ui, native| {
                let record = native.frame_node().node().expect("structured node");
                let root = ui.group(|ui| {
                    ui.set_min_width(150.0);
                    ui.strong(&record.kind.0);
                    for frame_port in native.frame_node().ports() {
                        let port = frame_port.port().expect("structured port");
                        ui.horizontal(|ui| {
                            ui.monospace(&port.key.0);
                            let (_, response) =
                                ui.allocate_exact_size(Vec2::splat(14.0), Sense::drag());
                            let outward = match port.dir {
                                PortDirection::In => HandlePosition::Left,
                                PortDirection::Out => HandlePosition::Right,
                            };
                            native
                                .measure_port(frame_port.id(), &response, outward)
                                .expect("structured real port is measured");
                        });
                    }
                });
                root.response
            })
            .expect("structured frame submits"),
    );
}

#[test]
fn erd_fixture_keeps_table_facts_real_ports_and_relation_route() {
    let harness = Harness::builder()
        .with_size(Vec2::new(920.0, 680.0))
        .build_ui_state(render_structured_graph, StructuredState::new(erd_fixture()));
    assert_structured_native_evidence(harness.state());

    let state = harness.state();
    let snapshot = state.host.view().graph_snapshot().expect("ERD graph");
    assert_eq!(
        snapshot
            .graph()
            .node(&state.nodes[0])
            .expect("users table")
            .data,
        serde_json::json!({ "table": "users", "columns": ["id", "email"] }),
    );
    assert_eq!(
        snapshot
            .graph()
            .node(&state.nodes[1])
            .expect("orders table")
            .data,
        serde_json::json!({ "table": "orders", "columns": ["id", "user_id"] }),
    );
    let relation = snapshot
        .graph()
        .edge(&state.edges[0])
        .expect("ERD relation");
    assert_eq!(
        (relation.from, relation.to),
        (state.ports[0], state.ports[1])
    );
}

#[test]
fn mind_map_fixture_keeps_branch_order_real_ports_and_both_routes() {
    let harness = Harness::builder()
        .with_size(Vec2::new(920.0, 680.0))
        .build_ui_state(
            render_structured_graph,
            StructuredState::new(mind_map_fixture()),
        );
    assert_structured_native_evidence(harness.state());

    let state = harness.state();
    let snapshot = state.host.view().graph_snapshot().expect("mind-map graph");
    let root = snapshot
        .graph()
        .node(&state.nodes[0])
        .expect("mind-map root");
    assert_eq!(root.ports, state.ports[..2]);
    assert_eq!(root.data, serde_json::json!({ "topic": "Headless UI" }));
    assert_eq!(snapshot.graph().edges().len(), 2);
    assert_eq!(
        snapshot
            .graph()
            .edges()
            .values()
            .map(|edge| edge.from)
            .collect::<Vec<_>>(),
        state.ports[..2],
    );
}

fn assert_structured_native_evidence(state: &StructuredState) {
    let outcome = state.outcome.as_ref().expect("structured outcome");
    assert!(outcome.diagnostics().is_empty());
    assert_eq!(outcome.rendered_instances().len(), state.nodes.len());
    let layout_receipts = outcome.receipt().layout_receipts().collect::<Vec<_>>();
    assert_eq!(layout_receipts.len(), state.nodes.len());
    assert!(layout_receipts.iter().all(|receipt| {
        matches!(
            receipt.status(),
            Some(LayoutNodeStatus::Published | LayoutNodeStatus::Unchanged),
        )
    }));

    let geometry = state
        .host
        .view()
        .geometry_snapshot()
        .expect("structured geometry");
    assert_eq!(geometry.nodes().len(), state.nodes.len());
    assert_eq!(geometry.ports().len(), state.ports.len());
    assert_eq!(geometry.routes().len(), state.edges.len());
    for node in &state.nodes {
        assert!(geometry.node(*node).is_some_and(|node| node.is_fresh()));
    }
    for port in &state.ports {
        assert!(geometry.port(*port).is_some());
    }
    for edge in &state.edges {
        assert!(geometry.route(*edge).is_some());
    }
}

fn erd_fixture() -> StructuredFixture {
    let users = NodeId::from_u128(0xe001);
    let orders = NodeId::from_u128(0xe002);
    let users_id = PortId::from_u128(0xe011);
    let orders_user_id = PortId::from_u128(0xe012);
    let relation = EdgeId::from_u128(0xe021);
    let mut builder = GraphBuilder::new(GraphId::from_u128(0xe000));
    builder.insert_node(
        users,
        structured_node(
            "fixture.erd.table",
            CanvasPoint { x: 24.0, y: 40.0 },
            vec![users_id],
            serde_json::json!({ "table": "users", "columns": ["id", "email"] }),
        ),
    );
    builder.insert_node(
        orders,
        structured_node(
            "fixture.erd.table",
            CanvasPoint { x: 380.0, y: 40.0 },
            vec![orders_user_id],
            serde_json::json!({ "table": "orders", "columns": ["id", "user_id"] }),
        ),
    );
    builder.insert_port(users_id, structured_port(users, "id", PortDirection::Out));
    builder.insert_port(
        orders_user_id,
        structured_port(orders, "user_id", PortDirection::In),
    );
    builder.insert_edge(
        relation,
        Edge::new(EdgeKind::Data, users_id, orders_user_id),
    );
    StructuredFixture {
        graph: builder.build().expect("valid ERD fixture"),
        nodes: vec![users, orders],
        ports: vec![users_id, orders_user_id],
        edges: vec![relation],
    }
}

fn mind_map_fixture() -> StructuredFixture {
    let root = NodeId::from_u128(0xd001);
    let runtime = NodeId::from_u128(0xd002);
    let adapters = NodeId::from_u128(0xd003);
    let root_runtime = PortId::from_u128(0xd011);
    let root_adapters = PortId::from_u128(0xd012);
    let runtime_input = PortId::from_u128(0xd013);
    let adapters_input = PortId::from_u128(0xd014);
    let runtime_edge = EdgeId::from_u128(0xd021);
    let adapters_edge = EdgeId::from_u128(0xd022);
    let mut builder = GraphBuilder::new(GraphId::from_u128(0xd000));
    builder.insert_node(
        root,
        structured_node(
            "fixture.mind-map.topic",
            CanvasPoint { x: 300.0, y: 30.0 },
            vec![root_runtime, root_adapters],
            serde_json::json!({ "topic": "Headless UI" }),
        ),
    );
    builder.insert_node(
        runtime,
        structured_node(
            "fixture.mind-map.topic",
            CanvasPoint { x: 80.0, y: 280.0 },
            vec![runtime_input],
            serde_json::json!({ "topic": "Runtime" }),
        ),
    );
    builder.insert_node(
        adapters,
        structured_node(
            "fixture.mind-map.topic",
            CanvasPoint { x: 540.0, y: 280.0 },
            vec![adapters_input],
            serde_json::json!({ "topic": "Native adapters" }),
        ),
    );
    builder.insert_port(
        root_runtime,
        structured_port(root, "runtime", PortDirection::Out),
    );
    builder.insert_port(
        root_adapters,
        structured_port(root, "adapters", PortDirection::Out),
    );
    builder.insert_port(
        runtime_input,
        structured_port(runtime, "parent", PortDirection::In),
    );
    builder.insert_port(
        adapters_input,
        structured_port(adapters, "parent", PortDirection::In),
    );
    builder.insert_edge(
        runtime_edge,
        Edge::new(EdgeKind::Data, root_runtime, runtime_input),
    );
    builder.insert_edge(
        adapters_edge,
        Edge::new(EdgeKind::Data, root_adapters, adapters_input),
    );
    StructuredFixture {
        graph: builder.build().expect("valid mind-map fixture"),
        nodes: vec![root, runtime, adapters],
        ports: vec![root_runtime, root_adapters, runtime_input, adapters_input],
        edges: vec![runtime_edge, adapters_edge],
    }
}

fn structured_node(
    kind: &str,
    pos: CanvasPoint,
    ports: Vec<PortId>,
    data: serde_json::Value,
) -> Node {
    Node {
        kind: NodeKindKey::new(kind),
        kind_version: 1,
        pos,
        origin: None,
        selectable: None,
        draggable: None,
        connectable: None,
        deletable: None,
        parent: None,
        extent: None,
        expand_parent: None,
        size: Some(CanvasSize {
            width: 190.0,
            height: 110.0,
        }),
        hidden: false,
        collapsed: false,
        ports,
        data,
    }
}

fn structured_port(node: NodeId, key: &str, dir: PortDirection) -> Port {
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

fn structured_request() -> FrameRequest {
    FrameRequest::new(
        CanvasRect {
            origin: CanvasPoint::default(),
            size: CanvasSize {
                width: 920.0,
                height: 680.0,
            },
        },
        RendererStamp::new(STRUCTURED_RENDERER, 1),
    )
    .with_incident_routing_dependencies()
}
