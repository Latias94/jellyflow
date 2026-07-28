use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;
use std::sync::Arc;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, Edge, EdgeId, EdgeKind, GraphBuilder, GraphId, NodeId,
    NodeKindKey, Port, PortCapacity, PortDirection, PortId, PortKey, PortKind,
    SYMBOL_REF_NODE_KIND, Symbol, SymbolId, symbol_ref_node_data,
};
use jellyflow_core::interaction::NodeGraphConnectionMode;

use crate::io::{ViewSelectionSeed, ViewSessionSeed};
use crate::rules::EdgeEndpoint;
use crate::runtime::adapter_protocol::{
    DEFAULT_FRAME_REQUIRED_NODE_BUDGET, DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET,
    FrameMaterializationBudget, FrameNode, FrameRequest, NodeLayout, RendererStamp, ViewPoint,
    ViewRect, ViewSize, side_anchor,
};
use crate::runtime::commands::{
    GraphCommand, IncidentEdgePolicy, SymbolEdit, TopologyEdit, TopologyPort,
};
use crate::runtime::geometry::HandlePosition;
use crate::runtime::graph_runtime::{
    GraphRuntime, edge_incarnation_entry_clone_count_for_test,
    port_incarnation_entry_clone_count_for_test, reset_edge_incarnation_entry_clone_count_for_test,
    reset_port_incarnation_entry_clone_count_for_test,
};
use crate::runtime::interaction::{
    CanvasInteractionConfig, CanvasInteractionPhase, CanvasInteractionQueryPageReceipt,
    CanvasInteractionSession, CanvasPressTarget, SelectionMode,
};
use crate::runtime::view_session::{
    NodeIntersectionCursorStaleReason, NodeIntersectionQuery, NodeIntersectionQueryError,
    NodeIntersectionQueryOutcome, ViewSessionEvent, ViewSessionOptions,
};

use super::fixtures::{
    complete_nodes_intersecting, graph_with_nodes, graph_with_routed_edge, node,
    publish_graph_layouts, publish_node_layouts, view_options,
};

#[test]
fn node_intersection_query_never_publishes_a_partial_result() {
    let (graph, nodes) = graph_with_nodes(4);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_graph_layouts(&session);
    let bounds = CanvasRect {
        origin: CanvasPoint { x: -1.0, y: -1.0 },
        size: CanvasSize {
            width: 260.0,
            height: 120.0,
        },
    };

    let cursor = match session
        .nodes_intersecting(NodeIntersectionQuery::new(bounds, 1))
        .unwrap()
    {
        NodeIntersectionQueryOutcome::Incomplete { cursor, work } => {
            assert_eq!(work.entries_examined, 1);
            cursor
        }
        other => panic!("a one-entry page must not expose partial membership: {other:?}"),
    };

    match session
        .nodes_intersecting(NodeIntersectionQuery::resume(cursor, 64))
        .unwrap()
    {
        NodeIntersectionQueryOutcome::Complete {
            nodes: intersecting,
            work,
        } => {
            assert_eq!(intersecting, nodes);
            assert!(work.entries_examined >= 3);
        }
        other => panic!("the resumed query must publish one complete membership: {other:?}"),
    }
}

#[test]
fn marquee_interaction_publishes_only_after_all_query_pages_complete() {
    let (graph, nodes) = graph_with_nodes(4);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_graph_layouts(&view);
    let mut interaction = CanvasInteractionSession::attach(
        &view,
        CanvasInteractionConfig {
            query_entry_budget: 1,
            ..CanvasInteractionConfig::default()
        },
    )
    .unwrap();
    let start = view
        .viewport_pointer_sample(ViewPoint::new(-10.0, -10.0))
        .unwrap();
    let end = view
        .viewport_pointer_sample(ViewPoint::new(260.0, 120.0))
        .unwrap();
    let baseline = view.selection_baseline().unwrap();
    interaction
        .press(
            CanvasPressTarget::Marquee {
                selection_mode: SelectionMode::Replace,
                baseline,
            },
            start,
        )
        .unwrap();

    let mut update = interaction.move_to(end).unwrap();
    let mut pages = 0_u64;
    loop {
        assert!(update.effects().is_empty());
        assert!(update.query_work_pending());
        assert!(interaction.query_work_pending());
        let (next_update, needs_tick, receipt, terminal_error) =
            interaction.advance_query_work(1).into_parts();
        assert!(terminal_error.is_none());
        assert_eq!(receipt.pages_granted(), 1);
        assert_eq!(receipt.pages_attempted(), 1);
        assert!(matches!(
            receipt.page_receipts(),
            [CanvasInteractionQueryPageReceipt::NodeIntersection(_)]
        ));
        pages += 1;
        update = next_update;
        assert_eq!(needs_tick, interaction.query_work_pending());
        assert_eq!(update.query_work_pending(), needs_tick);
        if needs_tick {
            assert!(update.effects().is_empty());
            assert_eq!(
                interaction.presentation().phase(),
                CanvasInteractionPhase::Marquee
            );
            continue;
        }
        assert!(update.effects().is_empty());
        let selected = view.selection().unwrap();
        assert!(selected.nodes().iter().copied().eq(nodes.iter().copied()));
        break;
    }
    assert!(pages > 1);
}

#[test]
fn node_intersection_cursor_cannot_cross_view_boundaries() {
    let (graph, _) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_graph_layouts(&first);
    publish_graph_layouts(&second);
    let bounds = CanvasRect {
        origin: CanvasPoint { x: -1.0, y: -1.0 },
        size: CanvasSize {
            width: 260.0,
            height: 120.0,
        },
    };
    let cursor = match first
        .nodes_intersecting(NodeIntersectionQuery::new(bounds, 1))
        .unwrap()
    {
        NodeIntersectionQueryOutcome::Incomplete { cursor, .. } => cursor,
        other => panic!("one entry must leave a resumable cursor: {other:?}"),
    };

    assert!(matches!(
        second.nodes_intersecting(NodeIntersectionQuery::resume(cursor, 16)),
        Err(NodeIntersectionQueryError::StaleCursor(
            NodeIntersectionCursorStaleReason::ForeignView
        ))
    ));
}

#[test]
fn degenerate_node_intersection_bounds_complete_with_empty_membership() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let view = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_graph_layouts(&view);

    for size in [
        CanvasSize {
            width: 0.0,
            height: 20.0,
        },
        CanvasSize {
            width: 20.0,
            height: 0.0,
        },
    ] {
        assert!(matches!(
            view.nodes_intersecting(NodeIntersectionQuery::new(
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size,
                },
                1,
            )),
            Ok(NodeIntersectionQueryOutcome::Complete { nodes, work })
                if nodes.is_empty() && work == Default::default()
        ));
    }
}

#[test]
fn opt_in_incident_routing_dependencies_materialize_the_other_edge_endpoint() {
    let (graph, ids) = graph_with_routed_edge();
    let mut builder = GraphBuilder::new(graph.graph_id());
    for (id, record) in graph.nodes() {
        builder.insert_node(*id, record.clone());
    }
    for (id, record) in graph.ports() {
        builder.insert_port(*id, record.clone());
    }
    for (id, record) in graph.edges() {
        builder.insert_edge(*id, record.clone());
    }
    let unrelated_target = NodeId::from_u128(4);
    let unrelated_target_port = PortId::from_u128(14);
    let mut unrelated_target_record = node("test.unrelated", 720.0);
    unrelated_target_record.ports = vec![unrelated_target_port];
    builder.insert_node(unrelated_target, unrelated_target_record);
    builder.insert_port(
        unrelated_target_port,
        chain_port_record(unrelated_target, PortDirection::In),
    );
    builder.insert_edge(
        EdgeId::from_u128(22),
        Edge::new(
            EdgeKind::Data,
            ids.alternate_source_port,
            unrelated_target_port,
        ),
    );
    let graph = builder.build().unwrap();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let viewport = CanvasRect {
        origin: CanvasPoint::default(),
        size: CanvasSize {
            width: 200.0,
            height: 120.0,
        },
    };

    let default_frame = session
        .frame(FrameRequest::new(
            viewport,
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    assert_eq!(
        default_frame
            .nodes()
            .iter()
            .map(FrameNode::id)
            .collect::<Vec<_>>(),
        vec![ids.source],
        "incident dependency derivation remains opt-in by default",
    );

    let before_incident_query = runtime.stats().unwrap();
    let incident_frame = session
        .frame(
            FrameRequest::new(viewport, RendererStamp::new("test.node", 1))
                .with_incident_routing_dependencies(),
        )
        .unwrap();
    let materialized = incident_frame
        .nodes()
        .iter()
        .map(FrameNode::id)
        .collect::<BTreeSet<_>>();
    assert_eq!(materialized, BTreeSet::from([ids.source, ids.target]));
    assert!(!materialized.contains(&ids.alternate_source));
    assert!(!materialized.contains(&unrelated_target));
    let after_incident_query = runtime.stats().unwrap();
    assert_eq!(
        after_incident_query.frame_incident_routing_index_bucket_reads
            - before_incident_query.frame_incident_routing_index_bucket_reads,
        1,
    );
    assert_eq!(
        after_incident_query.frame_incident_routing_index_entries_read
            - before_incident_query.frame_incident_routing_index_entries_read,
        1,
    );
}

#[test]
fn incident_routing_ignores_broad_phase_candidates_that_fail_exact_admission() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let viewport = CanvasRect {
        origin: CanvasPoint { x: 1_000.0, y: 0.0 },
        size: CanvasSize {
            width: 120.0,
            height: 120.0,
        },
    };
    publish_node_layouts(&session, [(ids.source, viewport)]);
    let before = runtime.stats().unwrap();

    let frame = session
        .frame(
            FrameRequest::new(viewport, RendererStamp::new("test.node", 1))
                .with_incident_routing_dependencies(),
        )
        .unwrap();

    assert!(frame.nodes().is_empty());
    let after = runtime.stats().unwrap();
    assert_eq!(
        after.frame_incident_routing_index_bucket_reads
            - before.frame_incident_routing_index_bucket_reads,
        0,
    );
    assert_eq!(
        after.frame_incident_routing_index_entries_read
            - before.frame_incident_routing_index_entries_read,
        0,
    );
}

#[test]
fn high_fanout_incident_routing_has_bounded_scan_and_materialization() {
    const NODE_COUNT: usize = 10_000;
    let (graph, hub) = graph_with_star(NODE_COUNT);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 200.0,
                    height: 120.0,
                },
            },
            RendererStamp::new("star.node", 1),
        )
        .with_incident_routing_dependencies()
    };

    let before = runtime.stats().unwrap();
    let first = session.frame(request()).unwrap();
    let after_first = runtime.stats().unwrap();
    let first_ids = first.nodes().iter().map(FrameNode::id).collect::<Vec<_>>();
    let overflow = first.materialization_overflow();

    assert_eq!(first_ids.first(), Some(&hub));
    assert_eq!(first_ids.len(), DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET + 1,);
    assert_eq!(overflow.visible_nodes(), 0);
    assert_eq!(overflow.required_nodes(), 0);
    assert_eq!(overflow.routing_dependency_nodes(), 0);
    assert!(overflow.incident_scan_truncated());
    assert_eq!(
        after_first.frame_incident_routing_index_bucket_reads
            - before.frame_incident_routing_index_bucket_reads,
        1,
    );
    assert_eq!(
        after_first.frame_incident_routing_index_entries_read
            - before.frame_incident_routing_index_entries_read,
        DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET as u64,
    );

    let second = session.frame(request()).unwrap();
    let after_second = runtime.stats().unwrap();
    assert_eq!(
        second.nodes().iter().map(FrameNode::id).collect::<Vec<_>>(),
        first_ids,
        "the same stable draw/ID prefix must win every frame",
    );
    assert!(second.materialization_overflow().incident_scan_truncated(),);
    assert_eq!(
        after_second.frame_incident_routing_index_entries_read
            - after_first.frame_incident_routing_index_entries_read,
        DEFAULT_FRAME_ROUTING_DEPENDENCY_BUDGET as u64,
    );
}

#[test]
fn high_fanout_equal_frame_feedback_reuses_incident_snapshot_handles() {
    const NODE_COUNT: usize = 10_000;
    const EDGE_COUNT: u64 = NODE_COUNT as u64 - 1;
    let (graph, hub) = graph_with_star(NODE_COUNT);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 200.0,
                    height: 120.0,
                },
            },
            RendererStamp::new("star.node", 1),
        )
    };

    let before_initial = runtime.stats().unwrap();
    let initial = session.frame(request()).unwrap();
    let initial_hub = initial
        .nodes()
        .iter()
        .find(|node| node.id() == hub)
        .unwrap();
    let collector = initial.collector();
    collector.layout(chain_layout(initial_hub, 0.0)).unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    let after_initial = runtime.stats().unwrap();
    assert_eq!(
        after_initial.frame_incident_snapshot_entries_read
            - before_initial.frame_incident_snapshot_entries_read,
        0
    );
    assert_eq!(
        after_initial.frame_incident_snapshot_materializations
            - before_initial.frame_incident_snapshot_materializations,
        0
    );
    assert_eq!(
        after_initial.feedback_incident_snapshot_entries_read
            - before_initial.feedback_incident_snapshot_entries_read,
        EDGE_COUNT
    );
    assert_eq!(
        after_initial.feedback_incident_snapshot_materializations
            - before_initial.feedback_incident_snapshot_materializations,
        1
    );

    for _ in 0..3 {
        let before_frame = runtime.stats().unwrap();
        let frame = session.frame(request()).unwrap();
        let after_frame = runtime.stats().unwrap();
        assert_eq!(
            after_frame.frame_incident_snapshot_entries_read
                - before_frame.frame_incident_snapshot_entries_read,
            0
        );
        assert_eq!(
            after_frame.frame_incident_snapshot_materializations
                - before_frame.frame_incident_snapshot_materializations,
            0
        );

        let hub_node = frame.nodes().iter().find(|node| node.id() == hub).unwrap();
        let collector = frame.collector();
        collector.layout(chain_layout(hub_node, 0.0)).unwrap();
        let receipt = session
            .submit_feedback(collector.finish().unwrap())
            .unwrap();
        assert!(receipt.publication().is_none());
        let after_feedback = runtime.stats().unwrap();
        assert_eq!(
            after_feedback.feedback_incident_snapshot_entries_read
                - after_frame.feedback_incident_snapshot_entries_read,
            0
        );
        assert_eq!(
            after_feedback.feedback_incident_snapshot_materializations
                - after_frame.feedback_incident_snapshot_materializations,
            0
        );
    }

    let before_replacement = runtime.stats().unwrap();
    let replacement = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 200.0,
                    height: 120.0,
                },
            },
            RendererStamp::new("star.node", 2),
        ))
        .unwrap();
    let after_replacement_frame = runtime.stats().unwrap();
    assert_eq!(
        after_replacement_frame.frame_incident_snapshot_entries_read
            - before_replacement.frame_incident_snapshot_entries_read,
        EDGE_COUNT
    );
    assert_eq!(
        after_replacement_frame.frame_incident_snapshot_materializations
            - before_replacement.frame_incident_snapshot_materializations,
        1
    );
    let replacement_hub = replacement
        .nodes()
        .iter()
        .find(|node| node.id() == hub)
        .unwrap();
    let collector = replacement.collector();
    collector
        .layout(chain_layout(replacement_hub, 0.0))
        .unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    let before_culling = runtime.stats().unwrap();
    let culled = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: 0.0,
                    y: 10_000.0,
                },
                size: CanvasSize {
                    width: 200.0,
                    height: 120.0,
                },
            },
            RendererStamp::new("star.node", 2),
        ))
        .unwrap();
    assert!(culled.nodes().is_empty());
    let after_culling = runtime.stats().unwrap();
    assert_eq!(
        after_culling.frame_incident_snapshot_entries_read
            - before_culling.frame_incident_snapshot_entries_read,
        EDGE_COUNT
    );
    assert_eq!(
        after_culling.frame_incident_snapshot_materializations
            - before_culling.frame_incident_snapshot_materializations,
        1
    );
}

#[test]
fn topology_changes_version_only_affected_incident_snapshots() {
    let (graph, ids) = graph_with_routed_edge();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let source_initial = runtime.incident_snapshot_for_test(ids.source).unwrap();
    let target_initial = runtime.incident_snapshot_for_test(ids.target).unwrap();
    let alternate_initial = runtime
        .incident_snapshot_for_test(ids.alternate_source)
        .unwrap();

    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            ids.source,
            serde_json::json!({ "value": 1 }),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("payload update applies");
    let source_after_payload = runtime.incident_snapshot_for_test(ids.source).unwrap();
    let target_after_payload = runtime.incident_snapshot_for_test(ids.target).unwrap();
    let alternate_after_payload = runtime
        .incident_snapshot_for_test(ids.alternate_source)
        .unwrap();
    assert!(Arc::ptr_eq(&source_initial, &source_after_payload));
    assert!(Arc::ptr_eq(&target_initial, &target_after_payload));
    assert!(Arc::ptr_eq(&alternate_initial, &alternate_after_payload));

    runtime
        .execute(GraphCommand::reconnect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
            EdgeEndpoint::From,
            ids.alternate_source_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("reconnect applies");
    let source_after_reconnect = runtime.incident_snapshot_for_test(ids.source).unwrap();
    let target_after_reconnect = runtime.incident_snapshot_for_test(ids.target).unwrap();
    let alternate_after_reconnect = runtime
        .incident_snapshot_for_test(ids.alternate_source)
        .unwrap();
    assert!(!Arc::ptr_eq(&source_after_payload, &source_after_reconnect));
    assert!(Arc::ptr_eq(&target_after_payload, &target_after_reconnect));
    assert!(!Arc::ptr_eq(
        &alternate_after_payload,
        &alternate_after_reconnect
    ));
    assert_eq!(source_after_payload.as_ref(), &BTreeSet::from([ids.edge]));
    assert!(source_after_reconnect.is_empty());
    assert!(alternate_after_payload.is_empty());
    assert_eq!(
        alternate_after_reconnect.as_ref(),
        &BTreeSet::from([ids.edge])
    );

    runtime
        .execute(GraphCommand::disconnect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("disconnect applies");
    let source_after_disconnect = runtime.incident_snapshot_for_test(ids.source).unwrap();
    let target_after_disconnect = runtime.incident_snapshot_for_test(ids.target).unwrap();
    let alternate_after_disconnect = runtime
        .incident_snapshot_for_test(ids.alternate_source)
        .unwrap();
    assert!(Arc::ptr_eq(
        &source_after_reconnect,
        &source_after_disconnect
    ));
    assert!(!Arc::ptr_eq(
        &target_after_reconnect,
        &target_after_disconnect
    ));
    assert!(!Arc::ptr_eq(
        &alternate_after_reconnect,
        &alternate_after_disconnect
    ));
    assert_eq!(target_after_reconnect.as_ref(), &BTreeSet::from([ids.edge]));
    assert_eq!(
        alternate_after_reconnect.as_ref(),
        &BTreeSet::from([ids.edge])
    );
    assert!(target_after_disconnect.is_empty());
    assert!(alternate_after_disconnect.is_empty());

    runtime
        .execute(GraphCommand::connect(
            runtime.snapshot().unwrap().version(),
            ids.edge,
            ids.source_port,
            ids.target_port,
            NodeGraphConnectionMode::Strict,
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("connect applies");
    let source_after_connect = runtime.incident_snapshot_for_test(ids.source).unwrap();
    let target_after_connect = runtime.incident_snapshot_for_test(ids.target).unwrap();
    let alternate_after_connect = runtime
        .incident_snapshot_for_test(ids.alternate_source)
        .unwrap();
    assert!(!Arc::ptr_eq(
        &source_after_disconnect,
        &source_after_connect
    ));
    assert!(!Arc::ptr_eq(
        &target_after_disconnect,
        &target_after_connect
    ));
    assert!(Arc::ptr_eq(
        &alternate_after_disconnect,
        &alternate_after_connect
    ));

    let replacement_port = PortId::from_u128(14);
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().replace_node_ports(
                ids.source,
                [TopologyPort::new(
                    replacement_port,
                    chain_port_record(ids.source, PortDirection::Out),
                )],
                IncidentEdgePolicy::Cascade,
            ),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("port replacement applies");
    let source_after_replace = runtime.incident_snapshot_for_test(ids.source).unwrap();
    let target_after_replace = runtime.incident_snapshot_for_test(ids.target).unwrap();
    assert!(!Arc::ptr_eq(&source_after_connect, &source_after_replace));
    assert!(!Arc::ptr_eq(&target_after_connect, &target_after_replace));
    assert_eq!(source_after_connect.as_ref(), &BTreeSet::from([ids.edge]));
    assert_eq!(target_after_connect.as_ref(), &BTreeSet::from([ids.edge]));
    assert!(source_after_replace.is_empty());
    assert!(target_after_replace.is_empty());

    let replacement_graph = runtime.snapshot().unwrap().graph().clone();
    let replacement = runtime
        .catalog()
        .canonicalize_graph(&replacement_graph)
        .unwrap();
    runtime.replace_document(replacement).unwrap();
    let source_after_reset = runtime.incident_snapshot_for_test(ids.source).unwrap();
    assert!(!Arc::ptr_eq(&source_after_replace, &source_after_reset));
    assert!(source_after_replace.is_empty());
    assert!(source_after_reset.is_empty());
}

#[test]
fn frame_budget_reports_visible_required_and_explicit_routing_overflow_separately() {
    let (graph, nodes) = graph_with_nodes(30);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let budget = FrameMaterializationBudget::new(3, 2, 2);
    let frame = session
        .frame(
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: CanvasSize {
                        width: 100_000.0,
                        height: 1_000.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            )
            .with_materialization_budget(budget)
            .with_required_nodes(nodes[10..15].iter().copied())
            .with_routing_dependencies(nodes[15..20].iter().copied()),
        )
        .unwrap();
    let overflow = frame.materialization_overflow();

    assert_eq!(FrameMaterializationBudget::default().required_nodes(), 64);
    assert_eq!(DEFAULT_FRAME_REQUIRED_NODE_BUDGET, 64);
    assert_eq!(frame.nodes().len(), 7);
    assert_eq!(overflow.visible_nodes(), 27);
    assert_eq!(overflow.required_nodes(), 3);
    assert_eq!(overflow.routing_dependency_nodes(), 3);
    assert!(!overflow.visible_scan_truncated());
    assert!(!overflow.incident_scan_truncated());
    assert!(!overflow.is_empty());
    assert_eq!(session.stats().unwrap().frame_nodes_materialized, 7);
}

#[test]
fn oversized_all_visible_frames_bound_spatial_and_candidate_work() {
    const NODE_COUNT: usize = 10_000;
    const FRAME_COUNT: usize = 4;
    const VISIBLE_LIMIT: usize = 32;
    const EXPECTED_SCAN_LIMIT: usize = 64;

    let (graph, _) = graph_with_nodes(NODE_COUNT);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 2_000_000.0,
                    height: 128.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
        .with_materialization_budget(
            FrameMaterializationBudget::new(VISIBLE_LIMIT, 0, 0)
                .with_visible_node_scan_limit(EXPECTED_SCAN_LIMIT),
        )
    };

    let before = session.stats().unwrap();
    let mut stable_ids = None;
    for _ in 0..FRAME_COUNT {
        let frame = session.frame(request()).unwrap();
        let ids = frame.nodes().iter().map(FrameNode::id).collect::<Vec<_>>();
        assert_eq!(ids.len(), VISIBLE_LIMIT);
        assert_eq!(frame.materialization_overflow().visible_nodes(), 32);
        assert!(frame.materialization_overflow().visible_scan_truncated());
        if let Some(stable_ids) = &stable_ids {
            assert_eq!(&ids, stable_ids, "bounded admission must be deterministic");
        } else {
            stable_ids = Some(ids);
        }
    }
    let after = session.stats().unwrap();

    assert!(
        after.admission_query_entries_examined - before.admission_query_entries_examined
            <= FRAME_COUNT as u64 * EXPECTED_SCAN_LIMIT as u64,
        "the visible-node budget must bound raw spatial admission scans",
    );
    assert!(
        after.frame_selection_membership_lookups - before.frame_selection_membership_lookups
            <= FRAME_COUNT as u64 * EXPECTED_SCAN_LIMIT as u64,
        "candidate construction must remain bounded before ordering and truncation",
    );
}

#[test]
fn frame_binding_revisions_advance_only_for_affected_nodes() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let changed = nodes[42];
    let unchanged = nodes[43];
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 1.0,
                    height: 1.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
        .with_required_nodes([changed, unchanged])
    };

    let before = session.frame(request()).unwrap();
    let changed_before = before
        .nodes()
        .iter()
        .find(|node| node.id() == changed)
        .unwrap();
    let unchanged_before = before
        .nodes()
        .iter()
        .find(|node| node.id() == unchanged)
        .unwrap();
    assert_eq!(changed_before.binding_revision(), 0);
    assert_eq!(unchanged_before.binding_revision(), 0);
    assert!(
        changed_before
            .graph_snapshot()
            .ptr_eq(&runtime.snapshot().unwrap())
    );

    runtime
        .execute(GraphCommand::set_node_data(
            runtime.snapshot().unwrap().version(),
            changed,
            serde_json::json!({ "value": 42 }),
        ))
        .into_result()
        .unwrap();

    let after = session.frame(request()).unwrap();
    let changed_after = after
        .nodes()
        .iter()
        .find(|node| node.id() == changed)
        .unwrap();
    let unchanged_after = after
        .nodes()
        .iter()
        .find(|node| node.id() == unchanged)
        .unwrap();
    assert_eq!(changed_after.binding_revision(), 1);
    assert_eq!(unchanged_after.binding_revision(), 0);
    assert!(
        changed_after
            .graph_snapshot()
            .ptr_eq(&runtime.snapshot().unwrap())
    );
}

#[test]
fn symbol_changes_advance_only_referrer_bindings_and_notifications() {
    let (graph, nodes) = graph_with_nodes(2);
    let symbol_id = SymbolId::from_u128(0x51_000);
    let referenced = nodes[0];
    let unrelated = nodes[1];
    let mut builder = GraphBuilder::new(graph.graph_id()).with_symbol(
        symbol_id,
        Symbol {
            name: "input".to_owned(),
            ty: Some(jellyflow_core::types::TypeDesc::String),
            default_value: None,
            meta: serde_json::Value::Null,
        },
    );
    for (id, node) in graph.nodes() {
        let mut node = node.clone();
        if *id == referenced {
            node.kind = NodeKindKey::new(SYMBOL_REF_NODE_KIND);
            node.data = symbol_ref_node_data(symbol_id);
        }
        builder.insert_node(*id, node);
    }
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().expect("valid symbol-ref graph"))
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 1.0,
                    height: 1.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
        .with_required_nodes([referenced, unrelated])
    };
    let before = session.frame(request()).unwrap();
    let referenced_revision = before
        .nodes()
        .iter()
        .find(|node| node.id() == referenced)
        .expect("referenced frame node")
        .binding_revision();
    let unrelated_revision = before
        .nodes()
        .iter()
        .find(|node| node.id() == unrelated)
        .expect("unrelated frame node")
        .binding_revision();
    let referenced_notifications = Rc::new(Cell::new(0_u64));
    let callback_referenced_notifications = referenced_notifications.clone();
    session
        .subscribe_node(referenced, move |_| {
            callback_referenced_notifications
                .set(callback_referenced_notifications.get().saturating_add(1));
        })
        .unwrap();
    let unrelated_notifications = Rc::new(Cell::new(0_u64));
    let callback_unrelated_notifications = unrelated_notifications.clone();
    session
        .subscribe_node(unrelated, move |_| {
            callback_unrelated_notifications
                .set(callback_unrelated_notifications.get().saturating_add(1));
        })
        .unwrap();
    let affected_nodes = Rc::new(RefCell::new(Vec::new()));
    let callback_affected_nodes = affected_nodes.clone();
    session
        .subscribe(move |event| {
            if let ViewSessionEvent::RuntimePatched { patch } = event {
                callback_affected_nodes
                    .borrow_mut()
                    .extend_from_slice(patch.affected_nodes());
            }
        })
        .unwrap();
    let mut changed = runtime
        .snapshot()
        .unwrap()
        .graph()
        .symbol(&symbol_id)
        .expect("symbol")
        .clone();
    changed.name = "renamed input".to_owned();

    runtime
        .execute(GraphCommand::symbols(
            runtime.snapshot().unwrap().version(),
            SymbolEdit::new().update(symbol_id, changed),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("symbol update applies");

    assert_eq!(affected_nodes.borrow().as_slice(), [referenced]);
    assert_eq!(referenced_notifications.get(), 1);
    assert_eq!(unrelated_notifications.get(), 0);
    let after = session.frame(request()).unwrap();
    assert_eq!(
        after
            .nodes()
            .iter()
            .find(|node| node.id() == referenced)
            .expect("updated referenced frame node")
            .binding_revision(),
        referenced_revision + 1,
    );
    assert_eq!(
        after
            .nodes()
            .iter()
            .find(|node| node.id() == unrelated)
            .expect("updated unrelated frame node")
            .binding_revision(),
        unrelated_revision,
    );
}

#[test]
fn replacement_binding_revision_belongs_to_the_new_node_incarnation() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let old_lifetime = runtime.node_lifetime_key(nodes[0]).unwrap().unwrap();
    let mut replacement = runtime
        .snapshot()
        .unwrap()
        .graph()
        .node(&nodes[0])
        .unwrap()
        .clone();
    replacement.kind = NodeKindKey::new("replacement.node");

    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().remove_node(nodes[0], IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap()
        .applied()
        .unwrap();
    runtime
        .execute(GraphCommand::topology(
            runtime.snapshot().unwrap().version(),
            TopologyEdit::new().add_node(nodes[0], replacement, []),
        ))
        .into_result()
        .unwrap()
        .applied()
        .unwrap();

    let frame = session
        .frame(
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: CanvasSize {
                        width: 1.0,
                        height: 1.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            )
            .with_required_nodes([nodes[0]]),
        )
        .unwrap();

    assert_eq!(frame.nodes()[0].binding_revision(), 2);
    assert_ne!(frame.nodes()[0].instance().lifetime(), old_lifetime);
}

#[test]
fn document_reset_binding_revision_belongs_to_the_reused_node_id() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let old_lifetime = runtime.node_lifetime_key(nodes[0]).unwrap().unwrap();
    let (replacement, replacement_nodes) = graph_with_nodes(1);
    assert_eq!(replacement_nodes, nodes);
    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();

    runtime.replace_document(replacement).unwrap();

    let frame = session
        .frame(
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: CanvasSize {
                        width: 1.0,
                        height: 1.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            )
            .with_required_nodes([nodes[0]]),
        )
        .unwrap();

    assert_eq!(frame.nodes()[0].binding_revision(), 1);
    assert_ne!(frame.nodes()[0].instance().lifetime(), old_lifetime);
}

fn chain_node(index: usize) -> NodeId {
    NodeId::from_u128(index as u128 + 1)
}

fn graph_with_star(node_count: usize) -> (jellyflow_core::core::Graph, NodeId) {
    assert!(node_count > 1);
    let hub = NodeId::from_u128(1);
    let hub_port = PortId::from_u128(100_000);
    let mut hub_record = node("star.node", 0.0);
    hub_record.ports = vec![hub_port];
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x20a));
    builder.insert_node(hub, hub_record);
    builder.insert_port(hub_port, chain_port_record(hub, PortDirection::Out));

    for index in 1..node_count {
        let target = NodeId::from_u128(index as u128 + 1);
        let target_port = PortId::from_u128(100_000 + index as u128);
        let mut target_record = node("star.node", 10_000.0 + index as f32 * 200.0);
        target_record.ports = vec![target_port];
        builder.insert_node(target, target_record);
        builder.insert_port(target_port, chain_port_record(target, PortDirection::In));
        builder.insert_edge(
            EdgeId::from_u128(200_000 + index as u128),
            Edge::new(EdgeKind::Data, hub_port, target_port),
        );
    }

    (builder.build().unwrap(), hub)
}

fn chain_port(index: usize, direction: PortDirection) -> PortId {
    let offset = match direction {
        PortDirection::In => 0,
        PortDirection::Out => 1,
    };
    PortId::from_u128(100_000 + index as u128 * 2 + offset)
}

fn chain_edge(index: usize) -> EdgeId {
    EdgeId::from_u128(200_000 + index as u128)
}

fn chain_port_record(node: NodeId, direction: PortDirection) -> Port {
    Port {
        node,
        key: PortKey::new(match direction {
            PortDirection::In => "in",
            PortDirection::Out => "out",
        }),
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

fn chain_layout(record: &FrameNode, offset_x: f32) -> NodeLayout {
    let bounds = ViewRect {
        origin: ViewPoint {
            x: record.bounds().origin.x + offset_x,
            y: record.bounds().origin.y,
        },
        size: ViewSize {
            width: 160.0,
            height: 96.0,
        },
    };
    let mut layout = record.begin_layout(bounds);
    for port in record.ports() {
        let direction = port.port().expect("frame port belongs to its snapshot").dir;
        let (position, x) = match direction {
            PortDirection::In => (HandlePosition::Left, bounds.origin.x),
            PortDirection::Out => (
                HandlePosition::Right,
                bounds.origin.x + bounds.size.width - 12.0,
            ),
        };
        let visual_bounds = ViewRect {
            origin: ViewPoint { x, y: 42.0 },
            size: ViewSize {
                width: 12.0,
                height: 12.0,
            },
        };
        let route_anchor =
            side_anchor(visual_bounds, position).expect("fixture port bounds are valid");
        layout
            .port_rendered(
                port.layout_target(),
                visual_bounds,
                visual_bounds,
                route_anchor,
                position,
            )
            .unwrap();
    }
    layout.finish().unwrap()
}

#[test]
fn equal_layout_feedback_does_not_churn_layout_revision_or_spatial_work() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let viewport = CanvasRect {
        origin: CanvasPoint { x: -10.0, y: -10.0 },
        size: CanvasSize {
            width: 300.0,
            height: 200.0,
        },
    };
    let measured = ViewRect {
        origin: ViewPoint::default(),
        size: ViewSize {
            width: 160.0,
            height: 96.0,
        },
    };

    let frame = session
        .frame(FrameRequest::new(
            viewport,
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let materialized = frame.nodes().len();
    assert!(materialized < 100);
    let frame_node = frame
        .nodes()
        .iter()
        .find(|node| node.id() == nodes[0])
        .unwrap();
    let collector = frame.collector();
    collector
        .layout(frame_node.begin_layout(measured).finish().unwrap())
        .unwrap();
    let first = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    assert_eq!(first.publication().unwrap().layout_revision(), 1);
    assert_eq!(first.publication().unwrap().spatial_updates(), 1);
    assert_eq!(
        complete_nodes_intersecting(
            &session,
            CanvasRect {
                origin: CanvasPoint { x: -1.0, y: -1.0 },
                size: CanvasSize {
                    width: 2.0,
                    height: 2.0,
                },
            },
        ),
        vec![nodes[0]]
    );
    for _ in 0..120 {
        assert_eq!(session.geometry_snapshot().unwrap().layout_revision(), 1);
    }
    let after_first = session.stats().unwrap();
    assert_eq!(after_first.frame_nodes_materialized, materialized as u64);
    assert_eq!(after_first.geometry_snapshot_builds, 1);

    for index in 0..120 {
        let frame = session
            .frame(FrameRequest::new(
                viewport,
                RendererStamp::new("test.node", 1),
            ))
            .unwrap();
        let frame_node = frame
            .nodes()
            .iter()
            .find(|node| node.id() == nodes[0])
            .unwrap();
        let collector = frame.collector();
        let mut equivalent = measured;
        equivalent.origin.x += if index % 2 == 0 { 0.005 } else { -0.005 };
        collector
            .layout(frame_node.begin_layout(equivalent).finish().unwrap())
            .unwrap();
        let receipt = session
            .submit_feedback(collector.finish().unwrap())
            .unwrap();
        assert!(receipt.publication().is_none());
        assert_eq!(session.geometry_snapshot().unwrap().layout_revision(), 1);
    }

    let after_equal_samples = session.stats().unwrap();
    assert_eq!(
        after_equal_samples.spatial_updates,
        after_first.spatial_updates
    );
    assert_eq!(
        after_equal_samples.route_invalidations,
        after_first.route_invalidations
    );
    assert_eq!(
        after_equal_samples.layout_publications,
        after_first.layout_publications
    );
    assert_eq!(
        after_equal_samples.refresh_requests,
        after_first.refresh_requests
    );
    assert_eq!(
        after_equal_samples.geometry_snapshot_builds,
        after_first.geometry_snapshot_builds
    );
}

#[test]
fn frame_materializes_only_visible_overscan_and_explicit_routing_dependencies() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let far_node = *nodes.last().unwrap();
    let frame = session
        .frame(
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint::default(),
                    size: CanvasSize {
                        width: 300.0,
                        height: 200.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            )
            .with_overscan(64.0)
            .with_routing_dependencies([far_node]),
        )
        .unwrap();

    assert!(frame.nodes().len() < 100);
    assert!(frame.nodes().iter().any(|node| node.id() == far_node));
    let snapshot = session.graph_snapshot().unwrap();
    for record in frame.nodes() {
        assert!(std::ptr::eq(
            record.node().unwrap(),
            snapshot.graph().node(&record.id()).unwrap(),
        ));
    }
    let stats = session.stats().unwrap();
    assert_eq!(stats.frame_nodes_materialized, frame.nodes().len() as u64);
    assert_eq!(
        stats.frame_order_candidates_ranked,
        frame.nodes().len() as u64,
        "paint ordering must visit only the admitted candidate subset",
    );
    assert_eq!(stats.admission_index_rebuilds, 1);
    assert!(stats.admission_nodes_visited >= 10_000);
    assert!(stats.admission_query_cells_examined > 0);
    assert!(stats.admission_query_entries_examined < 100);

    let before_sweep = stats;
    for step in 0..120 {
        let frame = session
            .frame(FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint {
                        x: step as f32 * 1_900.0,
                        y: 0.0,
                    },
                    size: CanvasSize {
                        width: 300.0,
                        height: 200.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            ))
            .unwrap();
        assert!(frame.nodes().len() < 100);
    }
    let after_sweep = session.stats().unwrap();
    assert_eq!(after_sweep.admission_index_rebuilds, 1);
    assert_eq!(
        after_sweep.admission_nodes_visited,
        before_sweep.admission_nodes_visited
    );
    assert_eq!(
        after_sweep.frame_order_candidates_ranked - before_sweep.frame_order_candidates_ranked,
        after_sweep.frame_nodes_materialized - before_sweep.frame_nodes_materialized,
        "each steady-state frame ranks exactly the nodes it materializes",
    );
    assert!(
        after_sweep.admission_query_cells_examined - before_sweep.admission_query_cells_examined
            <= 120 * 8
    );
    assert!(
        after_sweep.admission_query_entries_examined
            - before_sweep.admission_query_entries_examined
            < 120 * 100
    );
}

#[test]
fn frame_order_ranks_only_admitted_candidates_with_a_large_selection() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime
        .attach_view(view_options(
            ViewSessionSeed::new(graph_id).with_selection(ViewSelectionSeed::new(
                nodes,
                vec![],
                vec![],
            )),
        ))
        .unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 300.0,
                    height: 200.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };

    let first = session.frame(request()).unwrap();
    assert!(first.nodes().len() < 100);
    assert!(first.nodes().iter().all(FrameNode::selected));
    assert_eq!(
        session.export_seed().unwrap().selection().nodes().len(),
        10_000
    );
    let before = session.stats().unwrap();
    assert_eq!(
        before.frame_order_candidates_ranked,
        first.nodes().len() as u64
    );
    assert_eq!(
        before.frame_selection_membership_lookups,
        first.nodes().len() as u64,
        "selection membership is read exactly once for each admitted candidate",
    );

    for _ in 0..20 {
        assert!(session.frame(request()).unwrap().nodes().len() < 100);
    }
    let after = session.stats().unwrap();
    assert_eq!(
        after.frame_order_candidates_ranked - before.frame_order_candidates_ranked,
        after.frame_nodes_materialized - before.frame_nodes_materialized,
        "large selection membership must be cached; steady-state order work follows admitted nodes",
    );
    assert_eq!(
        after.frame_selection_membership_lookups - before.frame_selection_membership_lookups,
        after.frame_nodes_materialized - before.frame_nodes_materialized,
        "frame-local selection facts must not scan the complete graph or selection",
    );
    assert_eq!(
        after.admission_nodes_visited,
        before.admission_nodes_visited
    );
}

#[test]
fn ten_thousand_selected_edges_do_not_rebuild_membership_on_geometry_hot_paths() {
    const EDGE_COUNT: usize = 10_000;
    let (graph, hub) = graph_with_star(EDGE_COUNT + 1);
    let graph_id = graph.graph_id();
    let selected_edges = graph.edges().keys().copied().collect::<Vec<_>>();
    assert_eq!(selected_edges.len(), EDGE_COUNT);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id).with_selection(
            ViewSelectionSeed::new(vec![], selected_edges.clone(), vec![]),
        )))
        .unwrap();
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 200.0,
                    height: 120.0,
                },
            },
            RendererStamp::new("star.node", 1),
        )
    };
    let before = session.stats().unwrap();

    let frame = session.frame(request()).unwrap();
    let hub_frame = frame
        .nodes()
        .iter()
        .find(|node| node.id() == hub)
        .expect("hub is visible");
    let hub_instance = hub_frame.instance().clone();
    let collector = frame.collector();
    collector.layout(chain_layout(hub_frame, 0.0)).unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    session
        .execute(GraphCommand::set_node_data(
            session.graph_snapshot().unwrap().version(),
            NodeId::from_u128((EDGE_COUNT + 1) as u128),
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap()
        .into_result()
        .unwrap()
        .applied()
        .expect("unrelated payload patch applies");
    assert!(session.invalidate_node_layout(&hub_instance).unwrap());
    let _ = session.frame(request()).unwrap();

    let after = session.stats().unwrap();
    assert_eq!(
        after.edge_selection_membership_rebuilds - before.edge_selection_membership_rebuilds,
        0,
        "frame, layout publication, patch, and invalidation must borrow the authoritative non-Clone membership",
    );
    assert_eq!(
        after.edge_selection_membership_entries_rebuilt
            - before.edge_selection_membership_entries_rebuilt,
        0,
        "geometry hot paths must not copy work proportional to the complete edge selection",
    );
    assert_eq!(
        session.export_seed().unwrap().selection().edges(),
        selected_edges,
        "geometry work must preserve authoritative selection order",
    );
}

#[test]
fn one_edge_remove_and_restore_in_ten_thousand_edges_has_bounded_identity_copy_work() {
    const EDGE_COUNT: usize = 10_000;
    const MAX_IDENTITY_ENTRY_CLONES: u64 = 64;
    let (graph, _) = graph_with_star(EDGE_COUNT + 1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let before = session.graph_snapshot().unwrap();
    let edges = before.graph().edges().keys().copied().collect::<Vec<_>>();
    assert_eq!(edges.len(), EDGE_COUNT);
    let removed = edges[EDGE_COUNT / 2];
    let removed_incarnation = before.edge_incarnation(removed).unwrap();

    reset_edge_incarnation_entry_clone_count_for_test();
    session
        .execute(GraphCommand::disconnect(before.version(), removed))
        .unwrap()
        .into_result()
        .unwrap()
        .applied()
        .expect("disconnect applies");
    let identity_entry_clones = edge_incarnation_entry_clone_count_for_test();
    let after = session.graph_snapshot().unwrap();

    assert!(
        identity_entry_clones <= MAX_IDENTITY_ENTRY_CLONES,
        "one edge removal must copy bounded tree-path work, not all {EDGE_COUNT} identities; copied {identity_entry_clones}",
    );
    assert_eq!(
        before.edge_incarnation(removed),
        Some(removed_incarnation),
        "the retained snapshot must preserve the removed edge identity",
    );
    assert_eq!(after.edge_incarnation(removed), None);

    reset_edge_incarnation_entry_clone_count_for_test();
    session
        .execute(GraphCommand::undo(after.version()))
        .unwrap()
        .into_result()
        .unwrap()
        .applied()
        .expect("undo restores the edge");
    let restore_identity_entry_clones = edge_incarnation_entry_clone_count_for_test();
    let restored = session.graph_snapshot().unwrap();
    assert!(
        restore_identity_entry_clones <= MAX_IDENTITY_ENTRY_CLONES,
        "one edge restoration must copy bounded tree-path work, not all {EDGE_COUNT} identities; copied {restore_identity_entry_clones}",
    );
    let restored_incarnation = restored
        .edge_incarnation(removed)
        .expect("undo restores the removed edge identity");
    assert_ne!(
        restored_incarnation, removed_incarnation,
        "restoring an edge ID must allocate a new lifetime identity",
    );

    for edge in edges.iter().copied().filter(|edge| *edge != removed) {
        assert_eq!(
            after.edge_incarnation(edge),
            before.edge_incarnation(edge),
            "unchanged edges must retain their lifetime identity",
        );
        assert_eq!(
            restored.edge_incarnation(edge),
            before.edge_incarnation(edge),
            "restoring one edge must not replace unrelated edge identities",
        );
    }
}

#[test]
fn one_port_remove_and_restore_in_ten_thousand_ports_has_bounded_identity_copy_work() {
    const PORT_COUNT: usize = 10_000;
    const MAX_IDENTITY_ENTRY_CLONES: u64 = 64;
    let node_id = NodeId::from_u128(0x20b_000);
    let ports = (0..PORT_COUNT)
        .map(|index| PortId::from_u128(0x20c_000 + index as u128))
        .collect::<Vec<_>>();
    let mut node_record = node("ports.node", 0.0);
    node_record.ports = ports.clone();
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x20b));
    builder.insert_node(node_id, node_record);
    for (index, port_id) in ports.iter().copied().enumerate() {
        let mut record = chain_port_record(node_id, PortDirection::Out);
        record.key = PortKey::new(format!("out-{index}"));
        builder.insert_port(port_id, record);
    }
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let before = runtime.snapshot().unwrap();
    let removed = ports[PORT_COUNT / 2];
    let removed_incarnation = before.port_incarnation(removed).unwrap();
    let desired = ports
        .iter()
        .copied()
        .filter(|port| *port != removed)
        .map(|port| {
            TopologyPort::new(
                port,
                before.graph().port(&port).expect("port exists").clone(),
            )
        })
        .collect::<Vec<_>>();

    reset_port_incarnation_entry_clone_count_for_test();
    runtime
        .execute(GraphCommand::topology(
            before.version(),
            TopologyEdit::new().replace_node_ports(node_id, desired, IncidentEdgePolicy::Reject),
        ))
        .into_result()
        .unwrap()
        .applied()
        .expect("port removal applies");
    let identity_entry_clones = port_incarnation_entry_clone_count_for_test();
    let after = runtime.snapshot().unwrap();
    assert!(
        identity_entry_clones <= MAX_IDENTITY_ENTRY_CLONES,
        "one port removal must copy bounded tree-path work, not all {PORT_COUNT} identities; copied {identity_entry_clones}",
    );
    assert_eq!(before.port_incarnation(removed), Some(removed_incarnation));
    assert_eq!(after.port_incarnation(removed), None);

    reset_port_incarnation_entry_clone_count_for_test();
    runtime
        .execute(GraphCommand::undo(after.version()))
        .into_result()
        .unwrap()
        .applied()
        .expect("undo restores the port");
    let restore_identity_entry_clones = port_incarnation_entry_clone_count_for_test();
    let restored = runtime.snapshot().unwrap();
    assert!(
        restore_identity_entry_clones <= MAX_IDENTITY_ENTRY_CLONES,
        "one port restoration must copy bounded tree-path work, not all {PORT_COUNT} identities; copied {restore_identity_entry_clones}",
    );
    let restored_incarnation = restored
        .port_incarnation(removed)
        .expect("undo restores the port identity");
    assert_ne!(restored_incarnation, removed_incarnation);
    for port in ports.iter().copied().filter(|port| *port != removed) {
        assert_eq!(after.port_incarnation(port), before.port_incarnation(port));
        assert_eq!(
            restored.port_incarnation(port),
            before.port_incarnation(port)
        );
    }
}

#[test]
fn admission_query_stats_count_index_entries_before_node_deduplication() {
    let node_id = NodeId::from_u128(1);
    let mut large_node = node("test.node", 0.0);
    large_node.size = Some(CanvasSize {
        width: 511.0,
        height: 511.0,
    });
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x204));
    builder.insert_node(node_id, large_node);
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 511.0,
                    height: 511.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let stats = session.stats().unwrap();

    assert_eq!(frame.nodes().len(), 1);
    assert_eq!(stats.admission_nodes_visited, 1);
    assert_eq!(stats.admission_query_cells_examined, 8);
    assert_eq!(stats.admission_query_entries_examined, 4);

    let collector = frame.collector();
    collector
        .layout(
            frame.nodes()[0]
                .begin_layout(ViewRect {
                    origin: ViewPoint::default(),
                    size: ViewSize {
                        width: 511.0,
                        height: 511.0,
                    },
                })
                .finish()
                .unwrap(),
        )
        .unwrap();
    session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();
    let before_union_query = session.stats().unwrap();

    let next_frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 511.0,
                    height: 511.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let after_union_query = session.stats().unwrap();

    assert_eq!(next_frame.nodes().len(), 1);
    assert_eq!(
        after_union_query.admission_query_cells_examined
            - before_union_query.admission_query_cells_examined,
        8
    );
    assert_eq!(
        after_union_query.admission_query_entries_examined
            - before_union_query.admission_query_entries_examined,
        8
    );
}

#[test]
fn oversized_viewport_query_reports_linear_index_fallback_cost() {
    let (graph, nodes) = graph_with_nodes(100);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint::default(),
                size: CanvasSize {
                    width: 256.0 * 4_096.0,
                    height: 128.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let stats = session.stats().unwrap();

    assert_eq!(frame.nodes().len(), nodes.len());
    assert_eq!(stats.admission_query_cells_examined, 0);
    assert_eq!(stats.admission_query_entries_examined, nodes.len() as u64);
}

#[test]
fn oversized_node_remains_admitted_through_the_overflow_index() {
    let node_id = NodeId::from_u128(1);
    let mut oversized_node = node("test.node", 0.0);
    oversized_node.size = Some(CanvasSize {
        width: 256.0 * 4_096.0,
        height: 96.0,
    });
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x205));
    builder.insert_node(node_id, oversized_node);
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let frame = session
        .frame(FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: 900_000.0,
                    y: 0.0,
                },
                size: CanvasSize {
                    width: 128.0,
                    height: 96.0,
                },
            },
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let stats = session.stats().unwrap();

    assert_eq!(frame.nodes().len(), 1);
    assert_eq!(frame.nodes()[0].id(), node_id);
    assert_eq!(stats.admission_query_entries_examined, 1);
}

#[test]
fn admission_index_applies_graph_patches_without_a_full_rebuild() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let request_at = |x| {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint { x, y: -20.0 },
                size: CanvasSize {
                    width: 220.0,
                    height: 140.0,
                },
            },
            RendererStamp::new("test.node", 1),
        )
    };
    let initial = session.frame(request_at(0.0)).unwrap();
    assert!(initial.nodes().iter().any(|node| node.id() == nodes[0]));
    let before = session.stats().unwrap();
    assert_eq!(before.admission_index_rebuilds, 1);

    session
        .execute(GraphCommand::set_node_position(
            session.graph_snapshot().unwrap().version(),
            nodes[0],
            CanvasPoint {
                x: 500_000.0,
                y: 0.0,
            },
        ))
        .unwrap()
        .into_result()
        .unwrap();

    let moved = session.frame(request_at(499_980.0)).unwrap();
    assert!(moved.nodes().iter().any(|node| node.id() == nodes[0]));
    let after = session.stats().unwrap();
    assert_eq!(
        after.admission_index_rebuilds,
        before.admission_index_rebuilds
    );
    assert_eq!(
        after.admission_nodes_visited - before.admission_nodes_visited,
        1
    );
    assert!(after.admission_query_entries_examined - before.admission_query_entries_examined < 10);
}

#[test]
fn one_node_change_structurally_shares_ten_thousand_node_geometry_snapshots() {
    let (graph, nodes) = graph_with_nodes(10_000);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    publish_graph_layouts(&session);

    let changed = nodes[4_321];
    let unchanged = nodes[8_765];
    let before = session.geometry_snapshot().unwrap();
    let before_stats = session.stats().unwrap();
    let previous_position = before.node(changed).unwrap().bounds().origin;
    assert_eq!(before.nodes().len(), 10_000);

    session
        .execute(GraphCommand::set_node_position(
            session.graph_snapshot().unwrap().version(),
            changed,
            CanvasPoint {
                x: previous_position.x + 17.0,
                y: previous_position.y + 11.0,
            },
        ))
        .unwrap()
        .into_result()
        .unwrap();

    let after_patch = session.stats().unwrap();
    assert_eq!(
        after_patch.geometry_snapshot_fact_updates - before_stats.geometry_snapshot_fact_updates,
        1
    );
    let after = session.geometry_snapshot().unwrap();
    let after_snapshot = session.stats().unwrap();
    assert_eq!(
        after_snapshot.geometry_snapshot_builds - before_stats.geometry_snapshot_builds,
        1
    );
    assert_eq!(after.nodes().len(), 10_000);
    assert!(!std::ptr::eq(
        before.node(changed).unwrap(),
        after.node(changed).unwrap()
    ));
    assert!(std::ptr::eq(
        before.node(unchanged).unwrap(),
        after.node(unchanged).unwrap()
    ));
    assert_eq!(
        before.node(changed).unwrap().bounds().origin,
        previous_position
    );
    assert_eq!(
        after.node(changed).unwrap().bounds().origin,
        CanvasPoint {
            x: previous_position.x + 17.0,
            y: previous_position.y + 11.0,
        }
    );
}

#[test]
fn one_measurement_in_a_ten_thousand_node_chain_updates_only_incident_facts() {
    const NODE_COUNT: usize = 10_000;
    const MIDDLE: usize = NODE_COUNT / 2;
    const SPACING: f32 = 400.0;

    let mut builder = GraphBuilder::new(GraphId::from_u128(0x206));
    for index in 0..NODE_COUNT {
        let node_id = chain_node(index);
        let input = chain_port(index, PortDirection::In);
        let output = chain_port(index, PortDirection::Out);
        let mut record = node("chain", index as f32 * SPACING);
        record.ports = vec![input, output];
        builder.insert_node(node_id, record);
        builder.insert_port(input, chain_port_record(node_id, PortDirection::In));
        builder.insert_port(output, chain_port_record(node_id, PortDirection::Out));
    }
    for index in 0..NODE_COUNT - 1 {
        builder.insert_edge(
            chain_edge(index),
            Edge::new(
                EdgeKind::Data,
                chain_port(index, PortDirection::Out),
                chain_port(index + 1, PortDirection::In),
            ),
        );
    }
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let first = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let dependencies = [
        chain_node(MIDDLE - 1),
        chain_node(MIDDLE),
        chain_node(MIDDLE + 1),
    ];
    let request = || {
        FrameRequest::new(
            CanvasRect {
                origin: CanvasPoint {
                    x: MIDDLE as f32 * SPACING - 20.0,
                    y: -20.0,
                },
                size: CanvasSize {
                    width: 200.0,
                    height: 140.0,
                },
            },
            RendererStamp::new("chain.node", 1),
        )
        .with_routing_dependencies(dependencies)
    };

    let frame = first.frame(request()).unwrap();
    assert_eq!(frame.nodes().len(), 3);
    let snapshot = first.graph_snapshot().unwrap();
    for record in frame.nodes() {
        for port in record.ports() {
            assert!(std::ptr::eq(
                port.port().unwrap(),
                snapshot.graph().port(&port.id()).unwrap(),
            ));
        }
    }
    let collector = frame.collector();
    for record in frame.nodes() {
        collector.layout(chain_layout(record, 0.0)).unwrap();
    }
    first.submit_feedback(collector.finish().unwrap()).unwrap();
    let second_before = second.stats().unwrap();

    let frame = first.frame(request()).unwrap();
    let middle = frame
        .nodes()
        .iter()
        .find(|record| record.id() == chain_node(MIDDLE))
        .unwrap();
    let collector = frame.collector();
    collector.layout(chain_layout(middle, 10.0)).unwrap();
    let receipt = first.submit_feedback(collector.finish().unwrap()).unwrap();
    let publication = receipt.publication().unwrap();
    assert_eq!(publication.spatial_updates(), 1);
    assert_eq!(publication.route_recomputations(), 2);
    assert_eq!(second.stats().unwrap(), second_before);
    let stable = first.stats().unwrap();
    assert_eq!(stable.admission_index_rebuilds, 1);

    for index in 0..120 {
        let frame = first.frame(request()).unwrap();
        let middle = frame
            .nodes()
            .iter()
            .find(|record| record.id() == chain_node(MIDDLE))
            .unwrap();
        let offset = if index % 2 == 0 { 0.005 } else { -0.005 };
        let collector = frame.collector();
        collector.layout(chain_layout(middle, offset)).unwrap();
        assert!(
            first
                .submit_feedback(collector.finish().unwrap())
                .unwrap()
                .publication()
                .is_none()
        );
    }

    let after_epsilon = first.stats().unwrap();
    assert_eq!(after_epsilon.spatial_updates, stable.spatial_updates);
    assert_eq!(
        after_epsilon.route_invalidations,
        stable.route_invalidations
    );
    assert_eq!(
        after_epsilon.route_recomputations,
        stable.route_recomputations
    );
    assert_eq!(
        after_epsilon.layout_publications,
        stable.layout_publications
    );
    assert_eq!(after_epsilon.refresh_requests, stable.refresh_requests);
    assert_eq!(after_epsilon.admission_index_rebuilds, 1);
    assert_eq!(second.stats().unwrap(), second_before);
}
