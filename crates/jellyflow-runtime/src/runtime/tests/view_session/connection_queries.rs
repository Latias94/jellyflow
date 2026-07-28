use jellyflow_core::core::{
    CanvasPoint, Edge, EdgeId, EdgeKind, EdgeReconnectable, Graph, GraphBuilder, GraphId, NodeId,
    Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::rules::EdgeEndpoint;
use crate::runtime::commands::{
    ConnectionEdit, GraphCommand, IncidentEdgePolicy, TopologyEdit, TopologyPort,
};
use crate::runtime::geometry::HandlePosition;
use crate::runtime::graph_runtime::{
    GraphRuntime, GraphRuntimePolicy, PortConnectionDragIntent, PortConnectionDragQueryOutcome,
};
use crate::runtime::interaction::{ConnectionGestureBinding, ConnectionInteractionStart};
use crate::runtime::tests::fixtures::{
    GraphFixtureMutExt, current_port_instance, port_connection_drag_intent_by_id,
};
use crate::runtime::view_session::{
    ViewCloseReason, ViewCreateFromWireQueryError, ViewSession, ViewSessionError,
    ViewSessionOptions,
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
    assert_eq!(
        strong_evaluation.planning_work(),
        weak_evaluation.planning_work()
    );
    assert!(strong_evaluation.planning_work().fact_reads() > 0);
    assert_eq!(strong_evaluation.planning_work().operations_emitted(), 2);

    let strong_facts = view
        .port_connection_facts(ids.target_port)
        .unwrap()
        .into_outcome()
        .expect("target facts");
    let weak_facts = weak
        .port_connection_facts(ids.target_port)
        .unwrap()
        .into_outcome()
        .expect("target facts");
    assert_eq!(strong_facts, weak_facts);
    assert_eq!(strong_facts.revision(), 0);
    assert_eq!(strong_facts.port(), ids.target_port);
    assert_eq!(strong_facts.node(), ids.target);
    let target_instance = runtime
        .port_instance_key(ids.target_port)
        .unwrap()
        .expect("target port identity");
    assert_eq!(strong_facts.instance(), target_instance);
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

    let missing_facts = view.port_connection_facts(PortId::new()).unwrap();
    assert_eq!(missing_facts.work().fact_reads(), 1);
    assert_eq!(missing_facts.work().incident_bucket_reads(), 0);
    assert_eq!(missing_facts.work().incident_entries_read(), 0);
    assert!(missing_facts.into_outcome().is_none());
    assert!(
        weak.port_connection_facts(PortId::new())
            .unwrap()
            .into_outcome()
            .is_none()
    );

    let strong_summary = view
        .port_connection_summary(ids.target_port)
        .unwrap()
        .into_outcome()
        .expect("target summary");
    let weak_summary = weak
        .port_connection_summary(ids.target_port)
        .unwrap()
        .into_outcome()
        .expect("target summary");
    assert_eq!(strong_summary, weak_summary);
    assert_eq!(strong_summary.revision(), 0);
    assert_eq!(strong_summary.port(), ids.target_port);
    assert_eq!(strong_summary.node(), ids.target);
    assert_eq!(strong_summary.instance(), target_instance);
    assert_eq!(strong_summary.direction(), PortDirection::In);
    assert_eq!(strong_summary.capacity(), PortCapacity::Single);
    assert!(strong_summary.can_begin_connection());
    assert!(strong_summary.can_accept_connection());
    assert!(strong_summary.is_connected());
    assert_eq!(strong_summary.incident_count(), 1);
    let missing_summary = view.port_connection_summary(PortId::new()).unwrap();
    assert_eq!(missing_summary.work().fact_reads(), 1);
    assert_eq!(missing_summary.work().incident_bucket_reads(), 0);
    assert_eq!(missing_summary.work().incident_entries_read(), 0);
    assert!(missing_summary.into_outcome().is_none());
    assert!(
        weak.port_connection_summary(PortId::new())
            .unwrap()
            .into_outcome()
            .is_none()
    );

    let strong_drag_intent = view
        .port_connection_drag_intent(target_instance)
        .expect("target drag intent")
        .into_outcome()
        .ready()
        .expect("target port remains current");
    let weak_drag_intent = weak
        .port_connection_drag_intent(target_instance)
        .expect("target drag intent")
        .into_outcome()
        .ready()
        .expect("target port remains current");
    assert_eq!(strong_drag_intent, weak_drag_intent);
    assert_eq!(strong_drag_intent.revision(), 0);
    assert_eq!(strong_drag_intent.port(), ids.target_port);
    assert_eq!(strong_drag_intent.instance(), target_instance);
    assert!(matches!(
        strong_drag_intent,
        PortConnectionDragIntent::Reconnect { candidate, .. }
            if candidate.edge() == ids.edge
                && candidate.endpoint() == EdgeEndpoint::To
                && candidate.opposite_port() == ids.source_port
    ));
}

#[test]
fn same_id_port_replacement_refreshes_every_connection_fact_identity() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let initial_snapshot = runtime.snapshot().unwrap();
    let initial_facts = view
        .port_connection_facts(ids.target_port)
        .unwrap()
        .into_outcome()
        .expect("initial facts");
    let initial_summary = view
        .port_connection_summary(ids.target_port)
        .unwrap()
        .into_outcome()
        .expect("initial summary");
    let initial_drag =
        port_connection_drag_intent_by_id(&view, ids.target_port).expect("initial drag intent");
    assert_eq!(initial_facts.instance(), initial_summary.instance());
    assert_eq!(initial_facts.instance(), initial_drag.instance());

    let target_node = initial_snapshot.graph().node(&ids.target).unwrap();
    let replacement_ports = target_node
        .ports
        .iter()
        .copied()
        .map(|port_id| {
            let mut record = initial_snapshot.graph().port(&port_id).unwrap().clone();
            if port_id == ids.target_port {
                record.key = PortKey::new("replacement-target");
            }
            TopologyPort::new(port_id, record)
        })
        .collect::<Vec<_>>();
    runtime
        .execute(GraphCommand::topology(
            initial_snapshot.version(),
            TopologyEdit::new().replace_node_ports(
                ids.target,
                replacement_ports,
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("semantic replacement applies");

    let current_instance = runtime
        .port_instance_key(ids.target_port)
        .unwrap()
        .expect("replacement identity");
    assert_ne!(current_instance, initial_facts.instance());
    let stale_drag = view
        .port_connection_drag_intent(initial_facts.instance())
        .expect("stale identity is a negative query outcome");
    assert_eq!(stale_drag.work().fact_reads(), 1);
    assert_eq!(stale_drag.work().incident_bucket_reads(), 0);
    assert_eq!(stale_drag.work().incident_entries_read(), 0);
    assert_eq!(
        stale_drag.into_outcome(),
        PortConnectionDragQueryOutcome::Stale
    );
    assert_eq!(
        view.port_connection_facts(ids.target_port)
            .unwrap()
            .into_outcome()
            .unwrap()
            .instance(),
        current_instance,
    );
    assert_eq!(
        view.port_connection_summary(ids.target_port)
            .unwrap()
            .into_outcome()
            .unwrap()
            .instance(),
        current_instance,
    );
    assert_eq!(
        view.port_connection_drag_intent(current_instance)
            .unwrap()
            .into_outcome()
            .ready()
            .unwrap()
            .instance(),
        current_instance,
    );

    runtime
        .execute(GraphCommand::undo(runtime.snapshot().unwrap().version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("undo restores port semantics");
    let restored_instance = runtime
        .port_instance_key(ids.target_port)
        .unwrap()
        .expect("restored identity");
    assert_ne!(restored_instance, initial_facts.instance());
    assert_ne!(restored_instance, current_instance);
    assert_eq!(
        view.port_connection_summary(ids.target_port)
            .unwrap()
            .into_outcome()
            .unwrap()
            .instance(),
        restored_instance,
    );
}

#[test]
fn occupied_input_drag_facts_carry_the_edge_lifetime_not_only_its_id() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let initial = runtime.snapshot().unwrap();
    let initial_drag = port_connection_drag_intent_by_id(&view, ids.target_port)
        .expect("occupied input drag facts");
    let PortConnectionDragIntent::Reconnect {
        candidate: initial_candidate,
        ..
    } = initial_drag
    else {
        panic!("occupied input must issue a reconnect candidate");
    };
    assert_eq!(initial_candidate.edge(), ids.edge);
    assert_eq!(
        initial_candidate.edge_instance(),
        initial
            .edge_instance_key(ids.edge)
            .expect("initial edge lifetime")
    );

    runtime
        .execute(GraphCommand::disconnect(initial.version(), ids.edge))
        .into_result()
        .unwrap()
        .applied()
        .expect("edge retirement applies");
    let retired = runtime.snapshot().unwrap();
    assert_eq!(retired.edge_instance_key(ids.edge), None);

    runtime
        .execute(GraphCommand::undo(retired.version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("undo restores the edge id");
    let restored = runtime.snapshot().unwrap();
    let restored_drag = port_connection_drag_intent_by_id(&view, ids.target_port)
        .expect("restored occupied input drag facts");
    let PortConnectionDragIntent::Reconnect {
        candidate: restored_candidate,
        ..
    } = restored_drag
    else {
        panic!("restored input must issue a reconnect candidate");
    };
    assert_eq!(restored_candidate.edge(), ids.edge);
    assert_eq!(
        restored_candidate.edge_instance(),
        restored
            .edge_instance_key(ids.edge)
            .expect("restored edge lifetime")
    );
    assert_ne!(
        restored_candidate.edge_instance(),
        initial_candidate.edge_instance(),
        "a restored edge id must not revalidate an old reconnect drag"
    );
}

#[test]
fn port_drag_facts_are_accepted_only_by_the_issuing_runtime() {
    let (graph, ids) = graph_with_routed_edge();
    let first_runtime = GraphRuntime::builder(Default::default())
        .build(graph.clone())
        .unwrap();
    let second_runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = first_runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    let second = second_runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    let drag =
        port_connection_drag_intent_by_id(&first, ids.source_port).expect("source port drag facts");
    let foreign_query = second
        .port_connection_drag_intent(drag.instance())
        .expect("foreign identity is a negative query outcome");
    assert_eq!(foreign_query.work().fact_reads(), 0);
    assert_eq!(foreign_query.work().incident_bucket_reads(), 0);
    assert_eq!(foreign_query.work().incident_entries_read(), 0);
    assert_eq!(
        foreign_query.into_outcome(),
        PortConnectionDragQueryOutcome::Foreign
    );

    assert!(
        first
            .connection_start_from_port_drag_intent(
                drag,
                EdgeId::from_u128(90),
                NodeGraphConnectionMode::Strict,
                CanvasPoint::default(),
                HandlePosition::Right,
                HandlePosition::Left,
                false,
            )
            .unwrap()
            .is_some()
    );
    assert!(matches!(
        second.connection_start_from_port_drag_intent(
            drag,
            EdgeId::from_u128(90),
            NodeGraphConnectionMode::Strict,
            CanvasPoint::default(),
            HandlePosition::Right,
            HandlePosition::Left,
            false,
        ),
        Err(ViewSessionError::ForeignInteractionFacts)
    ));
}

#[test]
fn batched_port_summaries_preserve_order_missing_positions_and_one_revision() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let weak = view.downgrade();
    let missing = PortId::from_u128(u128::MAX - 1);
    let requested = [ids.target_port, missing, ids.source_port, ids.target_port];

    let strong_query = view.port_connection_summaries(&requested).unwrap();
    assert_eq!(strong_query.work().fact_reads(), 7);
    assert_eq!(strong_query.work().incident_bucket_reads(), 3);
    assert_eq!(strong_query.work().incident_entries_read(), 0);
    let strong = strong_query.into_outcome();

    let weak_query = weak.port_connection_summaries(&requested).unwrap();
    assert_eq!(weak_query.work().fact_reads(), 7);
    assert_eq!(weak_query.work().incident_bucket_reads(), 3);
    assert_eq!(weak_query.work().incident_entries_read(), 0);
    let weak = weak_query.into_outcome();

    assert_eq!(strong, weak);
    assert_eq!(strong.revision(), 0);
    assert_eq!(strong.len(), requested.len());
    assert!(!strong.is_empty());
    let entries = strong.iter().collect::<Vec<_>>();
    assert_eq!(
        entries.iter().map(|(port, _)| *port).collect::<Vec<_>>(),
        requested
    );
    assert!(entries[1].1.is_none());
    assert_eq!(entries[0].1, entries[3].1);
    for (requested_port, summary) in entries {
        if let Some(summary) = summary {
            assert_eq!(summary.revision(), strong.revision());
            assert_eq!(summary.port(), requested_port);
        }
    }

    view.execute(GraphCommand::set_node_data(
        view.graph_snapshot().unwrap().version(),
        ids.source,
        serde_json::json!({ "revision": 1 }),
    ))
    .unwrap()
    .into_result()
    .unwrap()
    .applied()
    .expect("unrelated node update applies");
    let updated_query = view.port_connection_summaries(&requested).unwrap();
    assert_eq!(updated_query.work().fact_reads(), 7);
    assert_eq!(updated_query.work().incident_bucket_reads(), 3);
    assert_eq!(updated_query.work().incident_entries_read(), 0);
    let updated = updated_query.into_outcome();
    assert_eq!(updated.revision(), 1);
    assert!(
        updated
            .iter()
            .filter_map(|(_, summary)| summary)
            .all(|summary| summary.revision() == updated.revision()),
    );
    let empty_query = view.port_connection_summaries(&[]).unwrap();
    assert_eq!(empty_query.work().fact_reads(), 0);
    assert_eq!(empty_query.work().incident_bucket_reads(), 0);
    assert_eq!(empty_query.work().incident_entries_read(), 0);
    let empty = empty_query.into_outcome();
    assert!(empty.is_empty());
    assert_eq!(empty.revision(), updated.revision());
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
        .into_outcome()
        .unwrap();
    let second_facts = second
        .port_connection_facts(ids.target_port)
        .unwrap()
        .into_outcome()
        .unwrap();

    assert_eq!(first_facts, second_facts);
    assert!(!first_facts.can_begin_connection());
    assert!(!first_facts.can_accept_connection());
    assert!(!first_facts.incident()[0].can_reconnect());

    let disabled_binding = ConnectionGestureBinding::connect(
        runtime
            .port_instance_key(ids.alternate_source_port)
            .unwrap()
            .expect("disabled source still has an exact lifetime"),
        EdgeEndpoint::From,
        EdgeId::from_u128(90),
        NodeGraphConnectionMode::Strict,
    );
    assert!(matches!(
        first.create_from_wire_candidates(&disabled_binding),
        Err(ViewCreateFromWireQueryError::Session(
            ViewSessionError::StaleInteractionFacts
        ))
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
    assert!(evaluation.planning_work().fact_reads() > 0);
    assert_eq!(evaluation.planning_work().operations_emitted(), 0);
    assert_eq!(
        evaluation.diagnostics()[0].key,
        "connect.source_not_connectable"
    );

    let outcome = first
        .execute(GraphCommand::connect(
            evaluation.version(),
            edge,
            ids.alternate_source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .unwrap()
        .into_result()
        .unwrap();
    let rejection = outcome.rejection().expect("policy rejects the command");
    assert_eq!(
        rejection.diagnostics()[0].key,
        "connect.source_not_connectable"
    );
    assert_eq!(rejection.planning_work(), evaluation.planning_work());
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
    let binding = connection_binding(&view, ids.alternate_source_port, EdgeId::from_u128(0xcafe));

    let strong = view.create_from_wire_candidates(&binding).unwrap();
    let weak_result = weak.create_from_wire_candidates(&binding).unwrap();
    assert_eq!(strong.revision(), weak_result.revision());
    assert_eq!(strong.source_instance(), binding.fixed_port_instance());
    assert_eq!(strong.candidates(), weak_result.candidates());

    let snapshot = runtime.snapshot().unwrap();
    let source = snapshot.graph().node(&ids.alternate_source).unwrap();
    let replacement_ports = source
        .ports
        .iter()
        .copied()
        .map(|port_id| {
            let mut record = snapshot.graph().port(&port_id).unwrap().clone();
            record.key = PortKey::new("replacement-source");
            TopologyPort::new(port_id, record)
        })
        .collect::<Vec<_>>();
    runtime
        .execute(GraphCommand::topology(
            snapshot.version(),
            TopologyEdit::new().replace_node_ports(
                ids.alternate_source,
                replacement_ports,
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("source replacement applies");
    assert!(matches!(
        view.create_from_wire_candidates(&binding),
        Err(ViewCreateFromWireQueryError::Session(
            ViewSessionError::StaleInteractionFacts
        ))
    ));
    assert!(matches!(
        weak.create_from_wire_candidates(&binding),
        Err(ViewCreateFromWireQueryError::Session(
            ViewSessionError::StaleInteractionFacts
        ))
    ));

    view.close().unwrap();
    assert!(matches!(
        weak.create_from_wire_candidates(&binding),
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

    let query = view.port_connection_facts(target_port).unwrap();
    assert_eq!(query.work().incident_bucket_reads(), 1);
    assert_eq!(query.work().incident_entries_read(), 1);
    assert_eq!(query.work().fact_reads(), 4);
    let facts = query.into_outcome().expect("target facts");

    assert_eq!(facts.incident().len(), 1);
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

    let summary_query = view.port_connection_summary(output).unwrap();
    assert_eq!(summary_query.work().incident_bucket_reads(), 1);
    assert_eq!(summary_query.work().incident_entries_read(), 0);
    assert_eq!(summary_query.work().fact_reads(), 2);
    let summary = summary_query.into_outcome().expect("output summary");

    assert!(summary.is_connected());
    assert_eq!(summary.incident_count(), INCIDENT_EDGES);

    let intent_query = view
        .port_connection_drag_intent(current_port_instance(&view, output))
        .expect("output drag intent");
    assert_eq!(intent_query.work().incident_bucket_reads(), 0);
    assert_eq!(intent_query.work().incident_entries_read(), 0);
    assert_eq!(intent_query.work().fact_reads(), 2);
    let intent = intent_query
        .into_outcome()
        .ready()
        .expect("output remains current");

    assert!(matches!(intent, PortConnectionDragIntent::Connect { .. }));
    assert_eq!(intent.revision(), 0);
    assert_eq!(intent.port(), output);
    assert_eq!(intent.node(), source_node);
    assert_eq!(intent.direction(), PortDirection::Out);
    assert_eq!(intent.capacity(), PortCapacity::Multi);
    assert!(intent.can_begin_connection());
}

#[test]
fn input_drag_intent_is_explicit_and_selects_the_first_reconnectable_edge() {
    let vacant_runtime = GraphRuntime::builder(Default::default())
        .build(input_drag_graph(&[]))
        .unwrap();
    let vacant_view = vacant_runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    let vacant_query = vacant_view
        .port_connection_drag_intent(current_port_instance(&vacant_view, PortId::from_u128(10)))
        .expect("vacant input drag intent");
    assert_eq!(vacant_query.work().incident_bucket_reads(), 1);
    assert_eq!(vacant_query.work().incident_entries_read(), 0);
    assert_eq!(vacant_query.work().fact_reads(), 2);
    let vacant = vacant_query
        .into_outcome()
        .ready()
        .expect("vacant input remains current");
    assert!(matches!(vacant, PortConnectionDragIntent::Connect { .. }));
    assert_eq!(vacant.direction(), PortDirection::In);
    assert!(vacant.can_begin_connection());

    let reconnect_runtime = GraphRuntime::builder(Default::default())
        .build(input_drag_graph(&[(300, true), (100, false), (200, true)]))
        .unwrap();
    let reconnect_view = reconnect_runtime
        .attach_view(ViewSessionOptions::default())
        .unwrap();
    let reconnect_query = reconnect_view
        .port_connection_drag_intent(current_port_instance(
            &reconnect_view,
            PortId::from_u128(10),
        ))
        .expect("reconnect drag intent");
    assert_eq!(reconnect_query.work().incident_bucket_reads(), 1);
    assert_eq!(
        reconnect_query.work().incident_entries_read(),
        2,
        "the stable EdgeId scan stops after the first reconnectable candidate",
    );
    assert_eq!(reconnect_query.work().fact_reads(), 6);
    let reconnect = reconnect_query
        .into_outcome()
        .ready()
        .expect("reconnect input remains current");
    assert!(matches!(
        reconnect,
        PortConnectionDragIntent::Reconnect { candidate, .. }
            if candidate.edge() == EdgeId::from_u128(200)
                && candidate.endpoint() == EdgeEndpoint::To
                && candidate.opposite_port() == PortId::from_u128(22)
    ));
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
    let blocked_query = blocked_view
        .port_connection_drag_intent(current_port_instance(&blocked_view, PortId::from_u128(10)))
        .expect("blocked drag intent");
    assert_eq!(blocked_query.work().incident_bucket_reads(), 1);
    assert_eq!(blocked_query.work().incident_entries_read(), 3);
    assert_eq!(blocked_query.work().fact_reads(), 8);
    let blocked = blocked_query
        .into_outcome()
        .ready()
        .expect("blocked input remains current");
    assert!(matches!(
        blocked,
        PortConnectionDragIntent::BlockedOccupied { .. }
    ));
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

fn connection_binding(view: &ViewSession, port: PortId, edge: EdgeId) -> ConnectionGestureBinding {
    let drag = view
        .port_connection_drag_intent(current_port_instance(view, port))
        .expect("connection facts are readable")
        .into_outcome()
        .ready()
        .expect("connection port remains current");
    ConnectionInteractionStart::binding_from_port_drag(drag, edge, NodeGraphConnectionMode::Strict)
        .expect("source port can begin a connection")
}
