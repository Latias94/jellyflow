use super::support::assert_conformance_trace;
use crate::runtime::conformance::{ConformanceAction, ConformanceScenario, ConformanceTraceEvent};
use crate::runtime::connection::ConnectEdgeRequest;
use crate::runtime::tests::fixtures::make_store;
use jellyflow_core::core::{
    CanvasPoint, CanvasSize, Edge, EdgeId, EdgeKind, EdgeLabelAnchor, EdgeViewDescriptor, Graph,
    GraphBuilder, GraphId, Node, NodeId, NodeKindKey, Port, PortCapacity, PortDirection, PortId,
    PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;
use serde_json::json;

#[test]
fn product_fixture_automation_connect_delete_path_uses_headless_runtime_actions() {
    let mut fixture = ProductFixtureBuilder::new("automation");
    let trigger = fixture.node(
        "automation.trigger",
        -360.0,
        0.0,
        [FixturePort::exec_out("out")],
    );
    let llm = fixture.node(
        "automation.llm",
        -80.0,
        0.0,
        [
            FixturePort::exec_in("in"),
            FixturePort::exec_out("yes"),
            FixturePort::exec_out("no"),
        ],
    );
    let tool = fixture.node(
        "automation.tool",
        220.0,
        -80.0,
        [FixturePort::exec_in("in")],
    );
    let error = fixture.node(
        "automation.error",
        220.0,
        120.0,
        [FixturePort::exec_in("error")],
    );
    let request = ConnectEdgeRequest::new(
        fixture.port(trigger, "out"),
        fixture.port(llm, "in"),
        NodeGraphConnectionMode::Strict,
    )
    .with_edge_id(EdgeId::from_u128(500));

    let mut view_state = crate::io::NodeGraphViewState::default();
    view_state.set_selection(vec![error], Vec::new(), Vec::new());
    let scenario = ConformanceScenario::new("automation product fixture", fixture.graph())
        .with_view_state(view_state)
        .with_actions([
            ConformanceAction::apply_connect_edge(request),
            ConformanceAction::apply_delete_selection(),
        ])
        .with_expected_trace([
            ConformanceTraceEvent::graph_commit(Some("connect edge"), ["add_edge"]),
            ConformanceTraceEvent::graph_commit(Some("delete selection"), ["remove_node"]),
            ConformanceTraceEvent::selection([], [], []),
        ]);

    let _ = (tool, llm);
    assert_conformance_trace(&scenario);
}

#[test]
fn product_fixture_erd_edges_keep_relation_metadata_on_edge() {
    let mut fixture = ProductFixtureBuilder::new("erd");
    let customer = fixture.node(
        "erd.table",
        -240.0,
        0.0,
        [FixturePort::data_out("pk"), FixturePort::data_in("fk")],
    );
    let order = fixture.node(
        "erd.table",
        60.0,
        0.0,
        [FixturePort::data_out("pk"), FixturePort::data_in("fk")],
    );
    let edge = EdgeId::from_u128(510);
    fixture.edge(edge, customer, "pk", order, "fk", "1:N");

    let graph = fixture.graph();
    assert_eq!(graph.edges()[&edge].data["label"], "1:N");
    assert_eq!(graph.edges()[&edge].view.label.as_deref(), Some("1:N"));

    let scenario = ConformanceScenario::new("erd product fixture", graph)
        .with_actions([ConformanceAction::set_selection([], [edge], [])])
        .with_expected_trace([ConformanceTraceEvent::selection([], [edge], [])]);
    assert_conformance_trace(&scenario);
}

#[test]
fn product_fixture_hierarchies_run_through_same_graph_runtime() {
    for (name, root_kind, child_kind) in [
        ("mind map fixture", "mind.topic", "mind.idea"),
        ("org chart fixture", "org.person", "org.department"),
    ] {
        let mut fixture = ProductFixtureBuilder::new(name);
        let root = fixture.node(root_kind, 0.0, 0.0, [FixturePort::data_out("out")]);
        let child_a = fixture.node(
            child_kind,
            240.0,
            -80.0,
            [FixturePort::data_in("in"), FixturePort::data_out("out")],
        );
        let child_b = fixture.node(
            child_kind,
            240.0,
            80.0,
            [FixturePort::data_in("in"), FixturePort::data_out("out")],
        );
        fixture.edge(EdgeId::new(), root, "out", child_a, "in", "parent");
        fixture.edge(EdgeId::new(), root, "out", child_b, "in", "parent");

        let scenario = ConformanceScenario::new(name, fixture.graph())
            .with_actions([ConformanceAction::apply_node_drag(
                child_a,
                CanvasPoint {
                    x: 320.0,
                    y: -120.0,
                },
            )])
            .with_expected_trace([ConformanceTraceEvent::graph_commit(
                Some("node drag"),
                ["set_node_pos"],
            )]);
        assert_conformance_trace(&scenario);
    }
}

#[test]
fn product_fixture_knowledge_board_keeps_source_claim_edges_headless() {
    let mut fixture = ProductFixtureBuilder::new("knowledge board");
    let source = fixture.node(
        "knowledge.source",
        -260.0,
        0.0,
        [FixturePort::data_out("out")],
    );
    let claim = fixture.node(
        "knowledge.claim",
        0.0,
        0.0,
        [FixturePort::data_in("in"), FixturePort::data_out("out")],
    );
    let action = fixture.node("knowledge.action", 260.0, 0.0, [FixturePort::data_in("in")]);
    fixture.edge(EdgeId::new(), source, "out", claim, "in", "supports");
    fixture.edge(EdgeId::new(), claim, "out", action, "in", "next");
    let store = make_store(fixture.graph());

    assert_eq!(store.graph().nodes().len(), 3);
    assert!(store.graph().edges().values().any(|edge| {
        edge.data.get("label").and_then(|value| value.as_str()) == Some("supports")
    }));
}

struct ProductFixtureBuilder {
    graph: GraphBuilder,
    ports: Vec<(NodeId, PortKey, PortId)>,
}

impl ProductFixtureBuilder {
    fn new(_name: &str) -> Self {
        Self {
            graph: GraphBuilder::new(GraphId::new()),
            ports: Vec::new(),
        }
    }

    fn node(
        &mut self,
        kind: &str,
        x: f32,
        y: f32,
        ports: impl IntoIterator<Item = FixturePort>,
    ) -> NodeId {
        let id = NodeId::new();
        let mut node = Node {
            kind: NodeKindKey::new(kind),
            kind_version: 1,
            pos: CanvasPoint { x, y },
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
                width: 180.0,
                height: 88.0,
            }),
            hidden: false,
            collapsed: false,
            ports: Vec::new(),
            data: json!({ "title": kind }),
        };

        for spec in ports {
            let port_id = PortId::new();
            let key = PortKey::new(spec.key);
            node.ports.push(port_id);
            self.ports.push((id, key.clone(), port_id));
            self.graph.insert_port(
                port_id,
                Port {
                    node: id,
                    key,
                    dir: spec.direction,
                    kind: spec.kind,
                    capacity: PortCapacity::Multi,
                    connectable: None,
                    connectable_start: None,
                    connectable_end: None,
                    ty: None,
                    data: serde_json::Value::Null,
                },
            );
        }
        self.graph.insert_node(id, node);
        id
    }

    fn edge(
        &mut self,
        id: EdgeId,
        source_node: NodeId,
        source_key: &str,
        target_node: NodeId,
        target_key: &str,
        label: &str,
    ) {
        let mut edge = Edge::new(
            EdgeKind::Data,
            self.port(source_node, source_key),
            self.port(target_node, target_key),
        );
        edge.data = json!({ "label": label });
        edge.view = EdgeViewDescriptor::new()
            .with_renderer_key("product-edge")
            .with_label(label)
            .with_label_anchor(EdgeLabelAnchor::Center)
            .with_target_marker_key("arrow")
            .with_style_token("fixture")
            .with_hit_target_width(24.0);
        self.graph.insert_edge(id, edge);
    }

    fn port(&self, node: NodeId, key: &str) -> PortId {
        self.ports
            .iter()
            .find_map(|(found_node, found_key, port)| {
                (*found_node == node && found_key.0 == key).then_some(*port)
            })
            .expect("fixture port exists")
    }

    fn graph(self) -> Graph {
        self.graph.build_unchecked()
    }
}

#[derive(Debug, Clone, Copy)]
struct FixturePort {
    key: &'static str,
    direction: PortDirection,
    kind: PortKind,
}

impl FixturePort {
    fn data_in(key: &'static str) -> Self {
        Self {
            key,
            direction: PortDirection::In,
            kind: PortKind::Data,
        }
    }

    fn data_out(key: &'static str) -> Self {
        Self {
            key,
            direction: PortDirection::Out,
            kind: PortKind::Data,
        }
    }

    fn exec_in(key: &'static str) -> Self {
        Self {
            key,
            direction: PortDirection::In,
            kind: PortKind::Exec,
        }
    }

    fn exec_out(key: &'static str) -> Self {
        Self {
            key,
            direction: PortDirection::Out,
            kind: PortKind::Exec,
        }
    }
}
