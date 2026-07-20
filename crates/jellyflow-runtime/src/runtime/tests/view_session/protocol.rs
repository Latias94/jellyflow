use std::cell::Cell;
use std::rc::Rc;

use jellyflow_core::core::{
    CanvasPoint, CanvasRect, CanvasSize, GraphBuilder, GraphId, NodeId, NodeKindKey, Port,
    PortCapacity, PortDirection, PortId, PortKey, PortKind,
};

use crate::io::{ViewSelectionSeed, ViewSessionSeed, ViewStackingSeed};
use crate::runtime::adapter_protocol::{
    AdapterFrame, FeedbackSubmissionError, FrameBoundsSource, FrameRequest, LayoutNodeStatus,
    NodeLayout, RendererStamp, StaleFrameReason, ViewPoint, ViewRect, ViewSize,
};
use crate::runtime::commands::{GraphCommand, IncidentEdgePolicy, TopologyEdit};
use crate::runtime::graph_runtime::GraphRuntime;
use crate::runtime::view_session::{ViewSessionError, ViewSessionOptions};

use super::fixtures::{graph_with_nodes, node, view_options};

fn viewport() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint {
            x: -100.0,
            y: -100.0,
        },
        size: CanvasSize {
            width: 1_000.0,
            height: 600.0,
        },
    }
}

fn measured_rect() -> CanvasRect {
    CanvasRect {
        origin: CanvasPoint { x: 12.0, y: 24.0 },
        size: CanvasSize {
            width: 180.0,
            height: 120.0,
        },
    }
}

fn measured_view_rect() -> ViewRect {
    let rect = measured_rect();
    ViewRect::new(
        ViewPoint::new(rect.origin.x, rect.origin.y),
        ViewSize::new(rect.size.width, rect.size.height),
    )
}

fn layout_for(frame: &AdapterFrame) -> NodeLayout {
    frame.nodes()[0]
        .begin_layout(measured_view_rect())
        .finish()
        .unwrap()
}

#[test]
fn frame_exposes_the_viewport_transform_bound_to_its_feedback_token() {
    let (graph, _) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let pan = CanvasPoint { x: 12.0, y: -8.0 };
    session.set_viewport(pan, 1.75).unwrap();

    let frame = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();

    assert_eq!(frame.transform().pan, pan);
    assert_eq!(frame.transform().zoom, 1.75);
    assert_eq!(session.viewport_transform().unwrap(), frame.transform());
}

#[test]
fn foreign_and_replayed_feedback_have_zero_side_effects() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let first = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let second = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let frame = first
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();
    let feedback = collector.finish().unwrap();

    let error = second
        .submit_feedback(feedback.duplicate_for_test())
        .unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::ForeignFrame { .. }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
    assert!(second.geometry_snapshot().unwrap().node(nodes[0]).is_none());

    let receipt = first
        .submit_feedback(feedback.duplicate_for_test())
        .unwrap();
    assert_eq!(receipt.operations().len(), 2);
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    let after_first = first.stats().unwrap();

    let error = first.submit_feedback(feedback).unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::ReplayedFrame { .. }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(first.stats().unwrap(), after_first);
}

#[test]
fn cross_runtime_feedback_is_rejected_before_consuming_the_local_frame() {
    let (graph, nodes) = graph_with_nodes(1);
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
    assert_eq!(first.id().get(), second.id().get());
    assert_ne!(first.id(), second.id());

    let first_frame = first
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let first_collector = first_frame.collector();
    first_collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "foreign": true }),
        ))
        .unwrap();
    first_collector.layout(layout_for(&first_frame)).unwrap();

    let second_frame = second
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let second_collector = second_frame.collector();
    second_collector.layout(layout_for(&second_frame)).unwrap();
    let second_before_foreign = second.stats().unwrap();

    let error = second
        .submit_feedback(first_collector.finish().unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::ForeignFrame { .. }
    ));
    assert_eq!(second.stats().unwrap(), second_before_foreign);
    assert_eq!(second_runtime.snapshot().unwrap().revision(), 0);
    assert_eq!(
        second_runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&nodes[0])
            .unwrap()
            .data,
        serde_json::Value::Null
    );
    assert!(second.geometry_snapshot().unwrap().nodes().is_empty());

    let receipt = second
        .submit_feedback(second_collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Published)
    );
    assert!(second.geometry_snapshot().unwrap().node(nodes[0]).is_some());
}

#[test]
fn cross_runtime_layout_target_cannot_publish_through_a_local_collector() {
    let (graph, nodes) = graph_with_nodes(1);
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

    let first_frame = first
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let first_layout = layout_for(&first_frame);
    let second_frame = second
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let second_collector = second_frame.collector();
    second_collector.layout(first_layout).unwrap();

    let receipt = second
        .submit_feedback(second_collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Stale)
    );
    assert!(receipt.publication().is_none());
    assert!(second.geometry_snapshot().unwrap().node(nodes[0]).is_none());
}

#[test]
fn closed_session_rejects_originating_feedback_with_zero_side_effects() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let runtime_patch_notifications = Rc::new(Cell::new(0));
    let callback_runtime_patch_notifications = runtime_patch_notifications.clone();
    runtime
        .subscribe(move |event| {
            if matches!(
                event,
                crate::runtime::graph_runtime::GraphRuntimeEvent::Patched { .. }
            ) {
                callback_runtime_patch_notifications
                    .set(callback_runtime_patch_notifications.get() + 1);
            }
        })
        .unwrap();
    let layout_notifications = Rc::new(Cell::new(0));
    let callback_layout_notifications = layout_notifications.clone();
    session
        .subscribe(move |event| {
            if matches!(
                event,
                crate::runtime::view_session::ViewSessionEvent::LayoutPublished { .. }
            ) {
                callback_layout_notifications.set(callback_layout_notifications.get() + 1);
            }
        })
        .unwrap();
    let frame = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let collector = frame.collector();
    collector
        .command(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "value": 1 }),
        ))
        .unwrap();
    collector.layout(layout_for(&frame)).unwrap();
    let feedback = collector.finish().unwrap();

    let close = session.close().unwrap();
    assert_eq!(
        close.reason(),
        crate::runtime::view_session::ViewCloseReason::HostClosed
    );
    assert!(close.receipts().is_empty());
    assert_eq!(session.layout_revision_for_test(), 0);
    assert!(session.retired_geometry_is_empty_for_test());
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    assert_eq!(runtime_patch_notifications.get(), 0);
    assert_eq!(layout_notifications.get(), 0);

    let error = session.submit_feedback(feedback).unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::Session(ViewSessionError::Closed {
            reason: crate::runtime::view_session::ViewCloseReason::HostClosed,
        })
    ));
    assert_eq!(session.layout_revision_for_test(), 0);
    assert!(session.retired_geometry_is_empty_for_test());
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&nodes[0])
            .unwrap()
            .data,
        serde_json::Value::Null
    );
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(runtime.history_state().unwrap().redo_len(), 0);
    assert_eq!(runtime_patch_notifications.get(), 0);
    assert_eq!(layout_notifications.get(), 0);
}

#[test]
fn superseded_and_future_frames_are_rejected_before_commands_execute() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let old_frame = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let old = old_frame.collector();
    old.command(GraphCommand::set_node_data(
        0,
        nodes[0],
        serde_json::json!({ "source": "old" }),
    ))
    .unwrap();
    let old = old.finish().unwrap();

    let latest = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let future = latest
        .collector()
        .finish()
        .unwrap()
        .with_sequence_for_test(u64::MAX);

    let error = session.submit_feedback(old).unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::StaleFrame {
            reason: crate::runtime::adapter_protocol::StaleFrameReason::Superseded,
            ..
        }
    ));
    let error = session.submit_feedback(future).unwrap_err();
    assert!(matches!(error, FeedbackSubmissionError::FutureFrame { .. }));
    assert_eq!(runtime.snapshot().unwrap().revision(), 0);
}

#[test]
fn layout_targets_cannot_cross_frame_boundaries() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let first = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let old_layout = layout_for(&first);
    let second = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let collector = second.collector();
    collector.layout(old_layout).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert!(matches!(
        receipt.operations(),
        [
            crate::runtime::adapter_protocol::FeedbackOperationReceipt::Layout {
                status: LayoutNodeStatus::Stale,
                ..
            }
        ]
    ));
    assert!(receipt.request_next_frame());
    assert!(receipt.publication().is_none());
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .node(nodes[0])
            .is_none()
    );
}

#[test]
fn delete_then_undo_assigns_a_new_incarnation_and_never_revives_old_targets() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let initial = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let old_incarnation = initial.nodes()[0].incarnation();
    let old_layout = layout_for(&initial);
    session
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(nodes[0], IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    session.execute(GraphCommand::undo(1)).unwrap();

    let restored = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    assert!(restored.nodes()[0].incarnation() > old_incarnation);
    let collector = restored.collector();
    collector.layout(old_layout).unwrap();

    let receipt = session
        .submit_feedback(collector.finish().unwrap())
        .unwrap();

    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Stale)
    );
    assert!(receipt.publication().is_none());
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .node(nodes[0])
            .is_none()
    );
}

#[test]
fn provisional_admission_uses_document_catalog_and_protocol_sizes_in_order() {
    let persisted_id = NodeId::from_u128(101);
    let catalog_id = NodeId::from_u128(102);
    let fallback_id = NodeId::from_u128(103);
    let persisted_port = PortId::from_u128(201);
    let mut persisted = node("persisted", 0.0);
    persisted.ports = vec![persisted_port];
    let mut catalog = node("catalog", 240.0);
    catalog.size = None;
    let mut fallback = node("fallback", 520.0);
    fallback.size = None;
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x205));
    builder.insert_node(persisted_id, persisted);
    builder.insert_node(catalog_id, catalog);
    builder.insert_node(fallback_id, fallback);
    builder.insert_port(
        persisted_port,
        Port {
            node: persisted_id,
            key: PortKey::new("out"),
            dir: PortDirection::Out,
            kind: PortKind::Data,
            capacity: PortCapacity::Multi,
            connectable: None,
            connectable_start: None,
            connectable_end: None,
            ty: None,
            data: serde_json::Value::Null,
        },
    );
    let runtime = GraphRuntime::builder(Default::default())
        .build(builder.build().unwrap())
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let catalog_size = CanvasSize {
        width: 220.0,
        height: 110.0,
    };
    let fallback_size = CanvasSize {
        width: 144.0,
        height: 88.0,
    };
    let frame = session
        .frame(
            FrameRequest::new(viewport(), RendererStamp::new("test.node", 1))
                .with_kind_default(NodeKindKey::new("catalog"), catalog_size)
                .with_fallback_size(fallback_size),
        )
        .unwrap();
    let record = |node_id| {
        frame
            .nodes()
            .iter()
            .find(|node| node.id() == node_id)
            .unwrap()
    };

    assert_eq!(
        record(persisted_id).bounds_source(),
        FrameBoundsSource::Persisted
    );
    assert_eq!(
        record(persisted_id).bounds().size,
        CanvasSize {
            width: 160.0,
            height: 96.0,
        }
    );
    assert_eq!(
        record(catalog_id).bounds_source(),
        FrameBoundsSource::KindDefault
    );
    assert_eq!(record(catalog_id).bounds().size, catalog_size);
    assert_eq!(
        record(fallback_id).bounds_source(),
        FrameBoundsSource::ProtocolFallback
    );
    assert_eq!(record(fallback_id).bounds().size, fallback_size);
    assert_eq!(record(persisted_id).ports().len(), 1);
    let geometry = session.geometry_snapshot().unwrap();
    assert!(geometry.node(persisted_id).is_none());
    assert!(geometry.port(persisted_port).is_none());
}

#[test]
fn commands_own_graph_freshness_while_transform_and_environment_precede_execution() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let frame = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let stale_graph = frame.collector();
    stale_graph
        .command(GraphCommand::set_node_data(
            1,
            nodes[0],
            serde_json::json!({ "should_not_apply": true }),
        ))
        .unwrap();
    runtime
        .execute(GraphCommand::set_node_data(
            0,
            nodes[0],
            serde_json::json!({ "revision": 1 }),
        ))
        .unwrap();
    let receipt = session
        .submit_feedback(stale_graph.finish().unwrap())
        .unwrap();
    assert!(matches!(
        receipt.operations()[0].command_outcome(),
        Some(Ok(crate::runtime::commands::GraphCommandOutcome::Applied(
            _
        )))
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);

    let frame = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let stale_transform = frame.collector();
    stale_transform
        .command(GraphCommand::set_node_data(
            2,
            nodes[0],
            serde_json::json!({ "should_not_apply": true }),
        ))
        .unwrap();
    session
        .set_viewport(CanvasPoint { x: 10.0, y: 20.0 }, 1.25)
        .unwrap();
    let error = session
        .submit_feedback(stale_transform.finish().unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::StaleFrame {
            reason: crate::runtime::adapter_protocol::StaleFrameReason::Transform,
            ..
        }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);

    let frame = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let stale_environment = frame.collector();
    stale_environment
        .command(GraphCommand::set_node_data(
            2,
            nodes[0],
            serde_json::json!({ "should_not_apply": true }),
        ))
        .unwrap();
    session
        .set_adapter_environment(crate::runtime::adapter_protocol::AdapterEnvironment::new(
            1, 2, 3, 4,
        ))
        .unwrap();
    let error = session
        .submit_feedback(stale_environment.finish().unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::StaleFrame {
            reason: crate::runtime::adapter_protocol::StaleFrameReason::Environment,
            ..
        }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 2);
}

#[test]
fn document_replacement_retires_frames_targets_and_geometry() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let initial = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let initial_collector = initial.collector();
    initial_collector.layout(layout_for(&initial)).unwrap();
    session
        .submit_feedback(initial_collector.finish().unwrap())
        .unwrap();
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .node(nodes[0])
            .is_some()
    );
    assert_eq!(
        session.nodes_intersecting(measured_rect()).unwrap(),
        vec![nodes[0]]
    );
    let geometry_revision_before_reset = session.geometry_snapshot().unwrap().layout_revision();

    let retired = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let retired_layout = layout_for(&retired);
    let retired_collector = retired.collector();
    retired_collector
        .command(GraphCommand::set_node_data(
            1,
            nodes[0],
            serde_json::json!({ "must_not_apply": true }),
        ))
        .unwrap();
    retired_collector.layout(retired_layout.clone()).unwrap();

    let (replacement, replacement_nodes) = graph_with_nodes(1);
    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();
    runtime.replace_document(replacement).unwrap();
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.snapshot().unwrap().document_epoch(), 1);
    let reset_geometry = session.geometry_snapshot().unwrap();
    assert_eq!(
        reset_geometry.layout_revision(),
        geometry_revision_before_reset + 1
    );
    assert!(reset_geometry.node(nodes[0]).is_none());
    assert!(
        session
            .nodes_intersecting(measured_rect())
            .unwrap()
            .is_empty()
    );

    let error = session
        .submit_feedback(retired_collector.finish().unwrap())
        .unwrap_err();
    assert!(matches!(
        error,
        FeedbackSubmissionError::StaleFrame {
            reason: StaleFrameReason::DocumentEpoch,
            ..
        }
    ));
    assert_eq!(runtime.snapshot().unwrap().revision(), 1);
    assert_eq!(runtime.history_state().unwrap().undo_len(), 0);
    assert_eq!(
        runtime
            .snapshot()
            .unwrap()
            .graph()
            .node(&replacement_nodes[0])
            .unwrap()
            .data,
        serde_json::Value::Null
    );

    let current = session
        .frame(FrameRequest::new(
            viewport(),
            RendererStamp::new("test.node", 1),
        ))
        .unwrap();
    let current_collector = current.collector();
    current_collector.layout(retired_layout).unwrap();
    let receipt = session
        .submit_feedback(current_collector.finish().unwrap())
        .unwrap();
    assert_eq!(
        receipt.operations()[0].layout_status(),
        Some(LayoutNodeStatus::Stale)
    );
    assert!(
        session
            .geometry_snapshot()
            .unwrap()
            .node(replacement_nodes[0])
            .is_none()
    );
}

#[test]
fn frame_request_resolves_renderer_by_node_kind_without_per_node_registration() {
    let (graph, nodes) = graph_with_nodes(1);
    let kind = graph.node(&nodes[0]).unwrap().kind.clone();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();

    let frame = session
        .frame(
            FrameRequest::new(viewport(), RendererStamp::new("fallback", 1))
                .with_kind_renderer(kind, RendererStamp::new("workflow.llm", 7)),
        )
        .unwrap();

    assert_eq!(frame.nodes()[0].renderer().key(), "workflow.llm");
    assert_eq!(frame.nodes()[0].renderer().incarnation(), 7);
}

#[test]
fn frame_request_materializes_explicit_required_nodes_outside_the_viewport() {
    let (graph, nodes) = graph_with_nodes(1);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let offscreen = CanvasRect {
        origin: CanvasPoint {
            x: 50_000.0,
            y: 50_000.0,
        },
        size: CanvasSize {
            width: 800.0,
            height: 600.0,
        },
    };

    let frame = session
        .frame(
            FrameRequest::new(offscreen, RendererStamp::new("test.node", 1))
                .with_required_nodes([nodes[0]]),
        )
        .unwrap();

    assert_eq!(frame.nodes().len(), 1);
    assert_eq!(frame.nodes()[0].id(), nodes[0]);
}

#[test]
fn frame_nodes_follow_unified_back_to_front_order_after_admission() {
    let ids = (1..=6).map(NodeId::from_u128).collect::<Vec<_>>();
    let mut builder = GraphBuilder::new(GraphId::from_u128(0x20a));
    for (id, x) in [
        (ids[0], 0.0),
        (ids[1], 180.0),
        (ids[2], 360.0),
        (ids[3], 10_000.0),
        (ids[4], 20_000.0),
        (ids[5], 80.0),
    ] {
        builder.insert_node(id, node("test.node", x));
    }
    let graph = builder.build().unwrap();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime
        .attach_view(view_options(
            ViewSessionSeed::new(graph_id)
                .with_selection(ViewSelectionSeed::new(vec![ids[2]], vec![], vec![]))
                .with_stacking(ViewStackingSeed::new(
                    vec![ids[4], ids[3], ids[2], ids[1]],
                    vec![],
                    vec![],
                )),
        ))
        .unwrap();

    let frame = session
        .frame(
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint { x: -10.0, y: -10.0 },
                    size: CanvasSize {
                        width: 560.0,
                        height: 200.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            )
            .with_required_nodes([ids[4]]),
        )
        .unwrap();

    assert_eq!(
        frame
            .nodes()
            .iter()
            .map(|node| node.id())
            .collect::<Vec<_>>(),
        vec![ids[4], ids[1], ids[0], ids[5], ids[2]],
        "required nodes retain explicit rank, culled nodes stay absent, and selected nodes paint last",
    );

    session
        .set_selection(vec![ids[4]], Vec::new(), Vec::new())
        .unwrap();
    let reselection = session
        .frame(
            FrameRequest::new(
                CanvasRect {
                    origin: CanvasPoint { x: -10.0, y: -10.0 },
                    size: CanvasSize {
                        width: 560.0,
                        height: 200.0,
                    },
                },
                RendererStamp::new("test.node", 1),
            )
            .with_required_nodes([ids[4]]),
        )
        .unwrap();

    assert_eq!(
        reselection
            .nodes()
            .iter()
            .map(|node| node.id())
            .collect::<Vec<_>>(),
        vec![ids[2], ids[1], ids[0], ids[5], ids[4]],
        "selection changes update elevation without rebuilding the explicit draw-order rank",
    );
}

#[test]
fn selection_is_a_frame_local_fact_that_does_not_wake_node_bindings() {
    let (graph, nodes) = graph_with_nodes(2);
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime.attach_view(ViewSessionOptions::default()).unwrap();
    let selected = nodes[0];
    let unselected = nodes[1];
    let binding_calls = Rc::new(Cell::new(0));
    let callback_calls = Rc::clone(&binding_calls);
    session
        .subscribe_node(selected, move |_| {
            callback_calls.set(callback_calls.get() + 1);
        })
        .unwrap();
    let request = || FrameRequest::new(viewport(), RendererStamp::new("test.node", 1));

    let initial = session.frame(request()).unwrap();
    let initial_selected = initial
        .nodes()
        .iter()
        .find(|node| node.id() == selected)
        .unwrap();
    let initial_unselected = initial
        .nodes()
        .iter()
        .find(|node| node.id() == unselected)
        .unwrap();
    assert!(!initial_selected.selected());
    assert!(!initial_unselected.selected());
    let selected_binding_revision = initial_selected.binding_revision();
    let unselected_binding_revision = initial_unselected.binding_revision();
    let before_selection = session.stats().unwrap();

    session
        .set_selection(vec![selected], Vec::new(), Vec::new())
        .unwrap();
    let selected_frame = session.frame(request()).unwrap();
    let selected_record = selected_frame
        .nodes()
        .iter()
        .find(|node| node.id() == selected)
        .unwrap();
    let unselected_record = selected_frame
        .nodes()
        .iter()
        .find(|node| node.id() == unselected)
        .unwrap();

    assert!(selected_record.selected());
    assert!(!unselected_record.selected());
    assert_eq!(
        selected_record.binding_revision(),
        selected_binding_revision
    );
    assert_eq!(
        unselected_record.binding_revision(),
        unselected_binding_revision
    );
    assert_eq!(selected_frame.graph_revision(), initial.graph_revision());
    assert_eq!(binding_calls.get(), 0);
    assert_eq!(
        session.stats().unwrap().node_binding_notifications,
        before_selection.node_binding_notifications,
    );

    session
        .set_selection(Vec::new(), Vec::new(), Vec::new())
        .unwrap();
    let cleared_frame = session.frame(request()).unwrap();
    let cleared_record = cleared_frame
        .nodes()
        .iter()
        .find(|node| node.id() == selected)
        .unwrap();
    assert!(!cleared_record.selected());
    assert_eq!(cleared_record.binding_revision(), selected_binding_revision);
    assert_eq!(cleared_frame.graph_revision(), initial.graph_revision());
    assert_eq!(binding_calls.get(), 0);
}

#[test]
fn frame_order_rank_survives_hard_retirement_and_rebuilds_on_document_reset() {
    let (graph, nodes) = graph_with_nodes(3);
    let replacement = graph.clone();
    let graph_id = graph.graph_id();
    let runtime = GraphRuntime::builder(Default::default())
        .build(graph)
        .unwrap();
    let session = runtime
        .attach_view(view_options(ViewSessionSeed::new(graph_id).with_stacking(
            ViewStackingSeed::new(vec![nodes[2], nodes[0]], vec![], vec![]),
        )))
        .unwrap();
    let request = || FrameRequest::new(viewport(), RendererStamp::new("test.node", 1));
    let ids = |frame: &crate::runtime::adapter_protocol::AdapterFrame| {
        frame
            .nodes()
            .iter()
            .map(|node| node.id())
            .collect::<Vec<_>>()
    };

    assert_eq!(
        ids(&session.frame(request()).unwrap()),
        vec![nodes[2], nodes[0], nodes[1]]
    );

    session
        .execute(GraphCommand::topology(
            0,
            TopologyEdit::new().remove_node(nodes[2], IncidentEdgePolicy::Reject),
        ))
        .unwrap();
    assert_eq!(
        ids(&session.frame(request()).unwrap()),
        vec![nodes[0], nodes[1]]
    );

    session.execute(GraphCommand::undo(1)).unwrap();
    assert_eq!(
        ids(&session.frame(request()).unwrap()),
        vec![nodes[2], nodes[0], nodes[1]]
    );

    let replacement = runtime.catalog().canonicalize_graph(&replacement).unwrap();
    runtime.replace_document(replacement).unwrap();
    assert_eq!(
        ids(&session.frame(request()).unwrap()),
        vec![nodes[2], nodes[0], nodes[1]]
    );
}
