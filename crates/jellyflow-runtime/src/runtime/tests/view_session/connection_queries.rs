use jellyflow_core::core::{
    Edge, EdgeId, EdgeKind, EdgeReconnectable, Graph, GraphBuilder, GraphId, NodeId, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::EdgeEndpoint;
use crate::runtime::commands::{ConnectionEdit, GraphCommand};
use crate::runtime::graph_runtime::{GraphRuntime, GraphRuntimePolicy, PortConnectionDragIntent};
use crate::runtime::tests::fixtures::GraphFixtureMutExt;
use crate::runtime::view_session::{
    ViewCloseReason, ViewCreateFromWireQueryError, ViewSessionError, ViewSessionOptions,
};
use crate::schema::{NodeCatalog, NodeKindSpec, NodePortSpec};

use super::fixtures::{graph_with_routed_edge, node};

#[test]
fn strong_and_weak_views_expose_symmetric_connection_queries() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .port_mut(&ids.target_port)
                .map(|port| port.capacity = PortCapacity::Single)
        })
        .expect("target port");
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak = view.downgrade();
    let edit = ConnectionEdit::connect(
        EdgeId::from_u128(90),
        ids.alternate_source_port,
        ids.target_port,
        NodeGraphConnectionMode::Strict,
    );

    let strong_evaluation = view.evaluate_connection(edit).unwrap();
    let weak_evaluation = weak.evaluate_connection(edit).unwrap();
    assert_eq!(strong_evaluation.revision(), weak_evaluation.revision());
    assert_eq!(strong_evaluation.decision(), weak_evaluation.decision());

    let strong_facts = view
        .port_connection_facts(ids.target_port)
        .unwrap()
        .expect("target facts");
    let weak_facts = weak
        .port_connection_facts(ids.target_port)
        .unwrap()
        .expect("target facts");
    assert_eq!(strong_facts, weak_facts);
    assert_eq!(strong_facts.revision(), 0);
    assert_eq!(strong_facts.port(), ids.target_port);
    assert_eq!(strong_facts.node(), ids.target);
    assert_eq!(strong_facts.direction(), PortDirection::In);
    assert_eq!(strong_facts.capacity(), PortCapacity::Single);
    assert!(strong_facts.can_begin_connection());
    assert!(strong_facts.can_accept_connection());
    assert_eq!(strong_facts.incident().len(), 1);
    let incident = strong_facts.incident()[0];
    assert_eq!(incident.edge(), ids.edge);
    assert_eq!(incident.endpoint(), EdgeEndpoint::To);
    assert_eq!(incident.opposite_port(), ids.source_port);
    assert!(incident.can_reconnect());

    assert!(view.port_connection_facts(PortId::new()).unwrap().is_none());
    assert!(weak.port_connection_facts(PortId::new()).unwrap().is_none());

    let strong_summary = view
        .port_connection_summary(ids.target_port)
        .unwrap()
        .expect("target summary");
    let weak_summary = weak
        .port_connection_summary(ids.target_port)
        .unwrap()
        .expect("target summary");
    assert_eq!(strong_summary, weak_summary);
    assert_eq!(strong_summary.revision(), 0);
    assert_eq!(strong_summary.port(), ids.target_port);
    assert_eq!(strong_summary.node(), ids.target);
    assert_eq!(strong_summary.direction(), PortDirection::In);
    assert_eq!(strong_summary.capacity(), PortCapacity::Single);
    assert!(strong_summary.can_begin_connection());
    assert!(strong_summary.can_accept_connection());
    assert!(strong_summary.is_connected());
    assert_eq!(strong_summary.incident_count(), 1);
    assert!(
        view.port_connection_summary(PortId::new())
            .unwrap()
            .is_none()
    );
    assert!(
        weak.port_connection_summary(PortId::new())
            .unwrap()
            .is_none()
    );

    let strong_drag_intent = view
        .port_connection_drag_intent(ids.target_port)
        .unwrap()
        .expect("target drag intent");
    let weak_drag_intent = weak
        .port_connection_drag_intent(ids.target_port)
        .unwrap()
        .expect("target drag intent");
    assert_eq!(strong_drag_intent, weak_drag_intent);
    assert_eq!(strong_drag_intent.revision(), 0);
    assert_eq!(strong_drag_intent.port(), ids.target_port);
    assert!(matches!(
        strong_drag_intent,
        PortConnectionDragIntent::Reconnect { candidate, .. }
            if candidate.edge() == ids.edge
                && candidate.endpoint() == EdgeEndpoint::To
                && candidate.opposite_port() == ids.source_port
    ));
    assert!(
        view.port_connection_drag_intent(PortId::new())
            .unwrap()
            .is_none()
    );
    assert!(
        weak.port_connection_drag_intent(PortId::new())
            .unwrap()
            .is_none()
    );
}

#[test]
fn runtime_owned_policy_keeps_connection_queries_and_commands_consistent() {
    let (mut graph, ids) = graph_with_routed_edge();
    graph
        .with_detached_builder(|builder| {
            builder
                .edge_mut(&ids.edge)
                .map(|edge| edge.reconnectable = None)
        })
        .expect("edge");
    let policy = GraphRuntimePolicy::default()
        .with_node_connections(false)
        .with_edge_reconnection(false);
    let runtime = GraphRuntime::builder(Default::default())
        .with_policy(policy)
        .build(graph)
        .unwrap();
    let first = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let first_facts = first
        .port_connection_facts(ids.target_port)
        .unwrap()
        .unwrap();
    let second_facts = second
        .port_connection_facts(ids.target_port)
        .unwrap()
        .unwrap();

    assert_eq!(first_facts, second_facts);
    assert!(!first_facts.can_begin_connection());
    assert!(!first_facts.can_accept_connection());
    assert!(!first_facts.incident()[0].can_reconnect());

    assert!(matches!(
        first.create_from_wire_candidates(
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ),
        Err(ViewCreateFromWireQueryError::Query(
            crate::runtime::graph_runtime::CreateFromWireQueryError::SourceNotConnectable {
                port,
            }
        )) if port == ids.alternate_source_port
    ));

    let edge = EdgeId::from_u128(91);
    let edit = ConnectionEdit::connect(
        edge,
        ids.alternate_source_port,
        ids.target_port,
        NodeGraphConnectionMode::Strict,
    );
    let evaluation = second.evaluate_connection(edit).unwrap();
    assert!(!evaluation.is_accept());
    assert_eq!(
        evaluation.diagnostics()[0].key,
        "connect.source_not_connectable"
    );

    let outcome = first
        .execute(GraphCommand::connect(
            0,
            edge,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .unwrap();
    assert_eq!(
        outcome.rejection().unwrap().diagnostics()[0].key,
        "connect.source_not_connectable"
    );
    assert!(runtime.snapshot().unwrap().graph().edge(&edge).is_none());
}

#[test]
fn create_from_wire_query_preserves_results_and_errors_across_view_capabilities() {
    let (graph, ids) = graph_with_routed_edge();
    let mut catalog = NodeCatalog::new();
    catalog
        .register(
            NodeKindSpec::builder("test.target", "Target", "test.target")
                .port(NodePortSpec::new(
                    "input",
                    PortDirection::In,
                    PortKind::Data,
                    PortCapacity::Single,
                ))
                .build(),
        )
        .unwrap();
    let runtime = GraphRuntime::builder(catalog).build(graph).unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak = view.downgrade();

    let strong = view
        .create_from_wire_candidates(ids.alternate_source_port, NodeGraphConnectionMode::Strict)
        .unwrap();
    let weak_result = weak
        .create_from_wire_candidates(ids.alternate_source_port, NodeGraphConnectionMode::Strict)
        .unwrap();
    assert_eq!(strong.revision(), weak_result.revision());
    assert_eq!(strong.candidates(), weak_result.candidates());

    let missing = PortId::new();
    assert!(matches!(
        weak.create_from_wire_candidates(missing, NodeGraphConnectionMode::Strict),
        Err(ViewCreateFromWireQueryError::Query(
            crate::runtime::graph_runtime::CreateFromWireQueryError::MissingSourcePort { port }
        )) if port == missing
    ));

    view.close().unwrap();
    assert!(matches!(
        weak.create_from_wire_candidates(
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ),
        Err(ViewCreateFromWireQueryError::Session(
            ViewSessionError::Closed {
                reason: ViewCloseReason::HostClosed,
            }
        ))
    ));
    assert!(matches!(
        weak.evaluate_connection(ConnectionEdit::disconnect(ids.edge)),
        Err(ViewSessionError::Closed {
            reason: ViewCloseReason::HostClosed,
        })
    ));
}

#[test]
fn port_connection_facts_read_only_the_requested_incident_edge_bucket() {
    const UNRELATED_EDGES: usize = 10_000;

    let source_node = NodeId::from_u128(1);
    let target_node = NodeId::from_u128(2);
    let source_port = PortId::from_u128(10);
    let target_port = PortId::from_u128(11);
    let edge = EdgeId::from_u128(20);
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x220));
    let mut source = node("test.source", 0.0);
    source.ports.push(source_port);
    builder.insert_node(source_node, source);
    builder.insert_port(source_port, port(source_node, "source", PortDirection::Out));
    let mut target = node("test.target", 240.0);
    target.ports.push(target_port);
    builder.insert_node(target_node, target);
    builder.insert_port(target_port, port(target_node, "target", PortDirection::In));
    builder.insert_edge(edge, Edge::new(EdgeKind::Data, source_port, target_port));

    let mut unrelated_source = node("test.unrelated-source", 480.0);
    let mut unrelated_target = node("test.unrelated-target", 720.0);
    let unrelated_source_node = NodeId::from_u128(3);
    let unrelated_target_node = NodeId::from_u128(4);
    for offset in 0..UNRELATED_EDGES {
        let offset = offset as u128;
        let from = PortId::from_u128(100_000 + offset);
        let to = PortId::from_u128(200_000 + offset);
        unrelated_source.ports.push(from);
        unrelated_target.ports.push(to);
        builder.insert_port(
            from,
            port(unrelated_source_node, "unrelated-out", PortDirection::Out),
        );
        builder.insert_port(
            to,
            port(unrelated_target_node, "unrelated-in", PortDirection::In),
        );
        builder.insert_edge(
            EdgeId::from_u128(300_000 + offset),
            Edge::new(EdgeKind::Data, from, to),
        );
    }
    builder.insert_node(unrelated_source_node, unrelated_source);
    builder.insert_node(unrelated_target_node, unrelated_target);
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let before = runtime.stats().unwrap();

    let facts = view
        .port_connection_facts(target_port)
        .unwrap()
        .expect("target facts");

    assert_eq!(facts.incident().len(), 1);
    let after = runtime.stats().unwrap();
    assert_eq!(
        after.connection_query_index_bucket_reads - before.connection_query_index_bucket_reads,
        1
    );
    assert_eq!(
        after.connection_query_index_entries_read - before.connection_query_index_entries_read,
        1
    );
    assert_eq!(
        after.connection_query_fact_reads - before.connection_query_fact_reads,
        3
    );
}

#[test]
fn output_drag_intent_skips_a_high_fanout_incident_bucket() {
    const INCIDENT_EDGES: usize = 9_999;

    let source_node = NodeId::from_u128(1);
    let target_node = NodeId::from_u128(2);
    let output = PortId::from_u128(10);
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x221));
    let mut source = node("test.source", 0.0);
    source.ports.push(output);
    builder.insert_node(source_node, source);
    builder.insert_port(output, port(source_node, "source", PortDirection::Out));
    let mut target = node("test.target", 240.0);
    for offset in 0..INCIDENT_EDGES {
        let input = PortId::from_u128(200_000 + offset as u128);
        target.ports.push(input);
        builder.insert_port(input, port(target_node, "target", PortDirection::In));
        builder.insert_edge(
            EdgeId::from_u128(100_000 + offset as u128),
            Edge::new(EdgeKind::Data, output, input),
        );
    }
    builder.insert_node(target_node, target);
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let summary_before = runtime.stats().unwrap();

    let summary = view
        .port_connection_summary(output)
        .unwrap()
        .expect("output summary");

    assert!(summary.is_connected());
    assert_eq!(summary.incident_count(), INCIDENT_EDGES);
    let summary_after = runtime.stats().unwrap();
    assert_eq!(
        summary_after.connection_query_index_bucket_reads
            - summary_before.connection_query_index_bucket_reads,
        0
    );
    assert_eq!(
        summary_after.connection_query_index_entries_read
            - summary_before.connection_query_index_entries_read,
        0
    );
    assert_eq!(
        summary_after.connection_query_fact_reads - summary_before.connection_query_fact_reads,
        2
    );
    let before = runtime.stats().unwrap();

    let intent = view
        .port_connection_drag_intent(output)
        .unwrap()
        .expect("output drag intent");

    assert!(matches!(intent, PortConnectionDragIntent::Connect { .. }));
    assert_eq!(intent.revision(), 0);
    assert_eq!(intent.port(), output);
    assert_eq!(intent.node(), source_node);
    assert_eq!(intent.direction(), PortDirection::Out);
    assert_eq!(intent.capacity(), PortCapacity::Multi);
    assert!(intent.can_begin_connection());
    let after = runtime.stats().unwrap();
    assert_eq!(
        after.connection_query_index_bucket_reads - before.connection_query_index_bucket_reads,
        0
    );
    assert_eq!(
        after.connection_query_index_entries_read - before.connection_query_index_entries_read,
        0
    );
    assert_eq!(
        after.connection_query_fact_reads - before.connection_query_fact_reads,
        2
    );
}

#[test]
fn input_drag_intent_is_explicit_and_selects_the_first_reconnectable_edge() {
    let vacant_runtime = GraphRuntime::builder(Default::default())
        .build(input_drag_graph(&[]))
        .unwrap();
    let vacant_view = vacant_runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    let vacant = vacant_view
        .port_connection_drag_intent(PortId::from_u128(10))
        .unwrap()
        .expect("vacant input drag intent");
    assert!(matches!(vacant, PortConnectionDragIntent::Connect { .. }));
    assert_eq!(vacant.direction(), PortDirection::In);
    assert!(vacant.can_begin_connection());

    let reconnect_runtime = GraphRuntime::builder(Default::default())
        .build(input_drag_graph(&[(300, true), (100, false), (200, true)]))
        .unwrap();
    let reconnect_view = reconnect_runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    let reconnect_before = reconnect_runtime.stats().unwrap();
    let reconnect = reconnect_view
        .port_connection_drag_intent(PortId::from_u128(10))
        .unwrap()
        .expect("reconnect drag intent");
    assert!(matches!(
        reconnect,
        PortConnectionDragIntent::Reconnect { candidate, .. }
            if candidate.edge() == EdgeId::from_u128(200)
                && candidate.endpoint() == EdgeEndpoint::To
                && candidate.opposite_port() == PortId::from_u128(22)
    ));
    let reconnect_after = reconnect_runtime.stats().unwrap();
    assert_eq!(
        reconnect_after.connection_query_index_bucket_reads
            - reconnect_before.connection_query_index_bucket_reads,
        1
    );
    assert_eq!(
        reconnect_after.connection_query_index_entries_read
            - reconnect_before.connection_query_index_entries_read,
        2,
        "the stable EdgeId scan stops after the first reconnectable candidate",
    );

    let blocked_runtime = GraphRuntime::builder(Default::default())
        .build(input_drag_graph(&[
            (300, false),
            (100, false),
            (200, false),
        ]))
        .unwrap();
    let blocked_view = blocked_runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    let blocked_before = blocked_runtime.stats().unwrap();
    let blocked = blocked_view
        .port_connection_drag_intent(PortId::from_u128(10))
        .unwrap()
        .expect("blocked drag intent");
    assert!(matches!(
        blocked,
        PortConnectionDragIntent::BlockedOccupied { .. }
    ));
    let blocked_after = blocked_runtime.stats().unwrap();
    assert_eq!(
        blocked_after.connection_query_index_bucket_reads
            - blocked_before.connection_query_index_bucket_reads,
        1
    );
    assert_eq!(
        blocked_after.connection_query_index_entries_read
            - blocked_before.connection_query_index_entries_read,
        3
    );
}

fn input_drag_graph(edges: &[(u128, bool)]) -> Graph {
    let input_node = NodeId::from_u128(1);
    let input = PortId::from_u128(10);
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x222));
    let mut target = node("test.target", 240.0);
    target.ports.push(input);
    builder.insert_node(input_node, target);
    builder.insert_port(input, port(input_node, "target", PortDirection::In));

    for (index, (edge_id, reconnectable)) in edges.iter().copied().enumerate() {
        let source_node = NodeId::from_u128(10 + index as u128);
        let output = PortId::from_u128(20 + index as u128);
        let mut source = node("test.source", index as f32 * 120.0);
        source.ports.push(output);
        builder.insert_node(source_node, source);
        builder.insert_port(output, port(source_node, "source", PortDirection::Out));
        let mut edge = Edge::new(EdgeKind::Data, output, input);
        edge.reconnectable = Some(EdgeReconnectable::Bool(reconnectable));
        builder.insert_edge(EdgeId::from_u128(edge_id), edge);
    }

    builder.build().expect("valid input drag graph")
}

fn port(node: NodeId, key: &str, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(key),
        dir: direction,
        kind: PortKind::Data,
        capacity: PortCapacity::Multi,
        connectable: None,
        connectable_start: None,
        connectable_end: None,
        ty: None,
        data: serde_json::Value::Null,
    }
}
